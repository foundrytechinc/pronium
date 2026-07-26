
//! ELF64 relocatable-object loader for Pronium Driver modules (.pd files).
//!
//! Supports ET_REL (relocatable), EM_X86_64 only.
//! Relocation types handled: R_X86_64_64, PC32, PLT32, 32, 32S.
//!
//! The loader:
//!   1. Validates the ELF header.
//!   2. Allocates physical pages for every SHF_ALLOC section.
//!   3. Copies section data (or zero-fills BSS) into those pages.
//!   4. Applies all .rela.* relocations, resolving undefined symbols via
//!      `kernel_syms::resolve_symbol`.
//!   5. Finds the `pd_init` symbol and returns its address.

use crate::memory::sys_allocate_pages;

// ─── ELF constants ────────────────────────────────────────────────────────────

const ELFMAG:      [u8; 4] = [0x7f, b'E', b'L', b'F'];
const ELFCLASS64:  u8  = 2;
const ELFDATA2LSB: u8  = 1;   // little-endian
const ET_REL:      u16 = 1;   // relocatable object
const EM_X86_64:   u16 = 62;

// Section types
const SHT_SYMTAB:  u32 = 2;
const SHT_RELA:    u32 = 4;
const SHT_NOBITS:  u32 = 8;   // BSS

// Section flags
const SHF_ALLOC:   u64 = 0x2;

// Special section indices
const SHN_UNDEF:   u16 = 0;
const SHN_ABS:     u16 = 0xFFF1;

// x86-64 relocation types
const R_X86_64_NONE:  u32 = 0;
const R_X86_64_64:    u32 = 1;
const R_X86_64_PC32:  u32 = 2;
const R_X86_64_PLT32: u32 = 4;
const R_X86_64_32:    u32 = 10;
const R_X86_64_32S:   u32 = 11;

// Maximum sections we will track (keeps stack/BSS size bounded)
const MAX_SECTIONS: usize = 64;

// ─── Public API ───────────────────────────────────────────────────────────────

#[derive(Debug)]
pub enum ElfError {
    TooSmall,
    BadMagic,
    NotRelocatable,
    WrongArch,
    TooManySections,
    AllocationFailed,
    NoEntryPoint,
    BadRelocation,
    BadSymbol,
    UnresolvedSymbol,
}

/// A successfully loaded module ready to call.
pub struct LoadedModule {
    /// Virtual address of `pd_init`.
    pub entry: usize,
    /// Address of the first allocated page (for logging / future unload).
    pub base: usize,
}

// ─── Raw ELF helpers (unaligned reads) ────────────────────────────────────────

#[inline] fn ru16(d: &[u8], o: usize) -> u16 { u16::from_le_bytes(d[o..o+2].try_into().unwrap()) }
#[inline] fn ru32(d: &[u8], o: usize) -> u32 { u32::from_le_bytes(d[o..o+4].try_into().unwrap()) }
#[inline] fn ru64(d: &[u8], o: usize) -> u64 { u64::from_le_bytes(d[o..o+8].try_into().unwrap()) }
#[inline] fn ri64(d: &[u8], o: usize) -> i64 { i64::from_le_bytes(d[o..o+8].try_into().unwrap()) }

// ─── ELF header offsets (ELF64) ───────────────────────────────────────────────
// e_ident[0..16], e_type@16, e_machine@18, e_version@20, e_entry@24,
// e_phoff@32, e_shoff@40, e_flags@48, e_ehsize@52, e_phentsize@54,
// e_phnum@56, e_shentsize@58, e_shnum@60, e_shstrndx@62

// Section header offsets (within each Elf64_Shdr, size = 64 bytes):
// sh_name@0, sh_type@4, sh_flags@8, sh_addr@16, sh_offset@24, sh_size@32,
// sh_link@40, sh_info@44, sh_addralign@48, sh_entsize@56

// Symbol entry offsets (Elf64_Sym, size = 24 bytes):
// st_name@0, st_info@4, st_other@5, st_shndx@6, st_value@8, st_size@16

// Rela entry offsets (Elf64_Rela, size = 24 bytes):
// r_offset@0, r_info@8, r_addend@16

// ─── Loader ───────────────────────────────────────────────────────────────────

/// Parse and load an ELF64 relocatable object.
/// Returns a `LoadedModule` with the address of `pd_init` on success.
pub fn load_elf(data: &[u8]) -> Result<LoadedModule, ElfError> {
    // ── 1. Validate header ────────────────────────────────────────────────────
    if data.len() < 64 { return Err(ElfError::TooSmall); }

    if &data[0..4] != &ELFMAG           { return Err(ElfError::BadMagic); }
    if data[4] != ELFCLASS64            { return Err(ElfError::BadMagic); }
    if data[5] != ELFDATA2LSB           { return Err(ElfError::BadMagic); }
    if ru16(data, 16) != ET_REL         { return Err(ElfError::NotRelocatable); }
    if ru16(data, 18) != EM_X86_64      { return Err(ElfError::WrongArch); }

    let e_shoff    = ru64(data, 40) as usize;
    let e_shentsz  = ru16(data, 58) as usize;
    let e_shnum    = ru16(data, 60) as usize;
    let e_shstrndx = ru16(data, 62) as usize;

    if e_shnum > MAX_SECTIONS { return Err(ElfError::TooManySections); }

    // Helper: read one section header field set by index.
    // Returns (sh_name, sh_type, sh_flags, sh_offset, sh_size, sh_link, sh_info)
    let shdr = |i: usize| -> Option<(u32, u32, u64, u64, u64, u32, u32)> {
        let base = e_shoff + i * e_shentsz;
        if base + e_shentsz > data.len() { return None; }
        Some((
            ru32(data, base + 0),  // sh_name
            ru32(data, base + 4),  // sh_type
            ru64(data, base + 8),  // sh_flags
            ru64(data, base + 24), // sh_offset
            ru64(data, base + 32), // sh_size
            ru32(data, base + 40), // sh_link
            ru32(data, base + 44), // sh_info
        ))
    };

    // ── 2. Allocate & populate ALLOC sections ─────────────────────────────────
    // section_addrs[i] = virtual address where section i was loaded (0 = not loaded)
    let mut section_addrs = [0usize; MAX_SECTIONS];
    let mut first_base    = 0usize;

    for i in 0..e_shnum {
        let (_, sh_type, sh_flags, sh_offset, sh_size, _, _) =
            shdr(i).ok_or(ElfError::TooSmall)?;

        if sh_size == 0               { continue; }
        if (sh_flags & SHF_ALLOC) == 0 { continue; }

        let byte_count   = sh_size as usize;
        let pages_needed = (byte_count + 4095) / 4096;
        let ptr = sys_allocate_pages(pages_needed);
        if ptr.is_null() { return Err(ElfError::AllocationFailed); }

        if first_base == 0 { first_base = ptr as usize; }

        if sh_type == SHT_NOBITS {
            // BSS — zero-fill
            unsafe { core::ptr::write_bytes(ptr, 0, byte_count); }
        } else {
            let src = sh_offset as usize;
            if src + byte_count > data.len() { return Err(ElfError::TooSmall); }
            unsafe { core::ptr::copy_nonoverlapping(data[src..].as_ptr(), ptr, byte_count); }
        }

        // Clear the NX bit so the CPU can execute code in this section.
        // Must be done after data is written (irrelevant for BSS, but harmless).
        crate::memory::make_executable(ptr, byte_count);

        section_addrs[i] = ptr as usize;
    }

    // ── 3. Locate SYMTAB + its string table ──────────────────────────────────
    let mut symtab_idx    = 0usize;
    let mut symstrtab_idx = 0usize;

    for i in 0..e_shnum {
        let (_, sh_type, _, _, _, sh_link, _) = shdr(i).ok_or(ElfError::TooSmall)?;
        if sh_type == SHT_SYMTAB {
            symtab_idx    = i;
            symstrtab_idx = sh_link as usize;
            break;
        }
    }

    // ── 4. Symbol resolver closure ────────────────────────────────────────────
    // Given a symbol-table index, return the symbol's runtime virtual address.
    let resolve_sym = |sym_idx: usize| -> Result<usize, ElfError> {
        if symtab_idx == 0 { return Err(ElfError::BadSymbol); }

        let (_, _, _, sym_off, sym_size, _, _) = shdr(symtab_idx).ok_or(ElfError::BadSymbol)?;
        let (_, _, _, str_off, _,        _, _) = shdr(symstrtab_idx).ok_or(ElfError::BadSymbol)?;

        const SYM_SZ: usize = 24; // sizeof(Elf64_Sym)
        let s = sym_off as usize + sym_idx * SYM_SZ;
        if s + SYM_SZ > data.len() { return Err(ElfError::BadSymbol); }

        let st_name  = ru32(data, s + 0);
        let st_shndx = ru16(data, s + 6);
        let st_value = ru64(data, s + 8);

        // Absolute symbol
        if st_shndx == SHN_ABS { return Ok(st_value as usize); }

        // Undefined — look up in kernel symbol table
        if st_shndx == SHN_UNDEF {
            let name_start = str_off as usize + st_name as usize;
            let name_end   = data[name_start..]
                .iter()
                .position(|&b| b == 0)
                .map(|p| name_start + p)
                .unwrap_or(data.len());
            let name = core::str::from_utf8(&data[name_start..name_end])
                .map_err(|_| ElfError::BadSymbol)?;
            return crate::kernel_syms::resolve_symbol(name)
                .ok_or(ElfError::UnresolvedSymbol);
        }

        // Defined in a section
        let sec_addr = section_addrs[st_shndx as usize];
        if sec_addr == 0 { return Err(ElfError::BadSymbol); }
        Ok(sec_addr + st_value as usize)
    };

    // ── 5. Apply relocations ──────────────────────────────────────────────────
    for i in 0..e_shnum {
        let (_, sh_type, _, sh_offset, sh_size, _, sh_info) =
            shdr(i).ok_or(ElfError::TooSmall)?;

        if sh_type != SHT_RELA { continue; }

        let target_sec  = sh_info as usize;
        let target_base = section_addrs[target_sec];
        if target_base == 0 { continue; } // section not loaded — skip

        const RELA_SZ: usize = 24; // sizeof(Elf64_Rela)
        let n_relas = sh_size as usize / RELA_SZ;

        for r in 0..n_relas {
            let rbase = sh_offset as usize + r * RELA_SZ;
            if rbase + RELA_SZ > data.len() { return Err(ElfError::BadRelocation); }

            let r_offset = ru64(data, rbase + 0);
            let r_info   = ru64(data, rbase + 8);
            let r_addend = ri64(data, rbase + 16);

            let sym_idx  = (r_info >> 32) as usize;
            let rel_type = (r_info & 0xFFFF_FFFF) as u32;

            if rel_type == R_X86_64_NONE { continue; }

            let sym_addr  = resolve_sym(sym_idx)? as i64;
            let patch_ptr = (target_base + r_offset as usize) as *mut u8;
            let p         = patch_ptr as i64;

            unsafe {
                match rel_type {
                    R_X86_64_64 => {
                        let v = (sym_addr + r_addend) as u64;
                        core::ptr::write_unaligned(patch_ptr as *mut u64, v);
                    }
                    R_X86_64_PC32 | R_X86_64_PLT32 => {
                        let v = (sym_addr + r_addend - p) as i32;
                        core::ptr::write_unaligned(patch_ptr as *mut i32, v);
                    }
                    R_X86_64_32 => {
                        let v = (sym_addr + r_addend) as u32;
                        core::ptr::write_unaligned(patch_ptr as *mut u32, v);
                    }
                    R_X86_64_32S => {
                        let v = (sym_addr + r_addend) as i32;
                        core::ptr::write_unaligned(patch_ptr as *mut i32, v);
                    }
                    _ => return Err(ElfError::BadRelocation),
                }
            }
        }
    }

    // ── 6. Find pd_init entry point ───────────────────────────────────────────
    if symtab_idx == 0 { return Err(ElfError::NoEntryPoint); }

    let (_, _, _, sym_off, sym_size, _, _) = shdr(symtab_idx).ok_or(ElfError::NoEntryPoint)?;
    let (_, _, _, str_off, _,        _, _) = shdr(symstrtab_idx).ok_or(ElfError::NoEntryPoint)?;

    const SYM_SZ: usize = 24;
    let n_syms = sym_size as usize / SYM_SZ;

    for s in 0..n_syms {
        let base = sym_off as usize + s * SYM_SZ;
        if base + SYM_SZ > data.len() { break; }

        let st_name  = ru32(data, base + 0);
        let st_shndx = ru16(data, base + 6);
        let st_value = ru64(data, base + 8);

        if st_shndx == SHN_UNDEF || st_shndx == SHN_ABS { continue; }

        let name_start = str_off as usize + st_name as usize;
        let name_end   = data[name_start..]
            .iter()
            .position(|&b| b == 0)
            .map(|p| name_start + p)
            .unwrap_or(data.len());

        if let Ok("pd_init") = core::str::from_utf8(&data[name_start..name_end]) {
            let sec_addr = section_addrs[st_shndx as usize];
            if sec_addr == 0 { return Err(ElfError::NoEntryPoint); }
            return Ok(LoadedModule {
                entry: sec_addr + st_value as usize,
                base:  first_base,
            });
        }
    }

    Err(ElfError::NoEntryPoint)
}
