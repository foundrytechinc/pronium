



#![allow(dead_code)]


pub static mut PAGE_BITMAP: [u8; 128] = [0; 128]; 


/// Marks a range of virtual memory as executable by clearing the NX bit
/// in every page-table entry that covers `[ptr, ptr + byte_count)`.
///
/// The Pronium kernel uses an identity-mapped physical-memory window
/// (`PHYS_MEM_OFFSET`).  For driver pages allocated via `sys_allocate_pages`
/// the virtual address **is** the physical address (they live in the low
/// 0x10000000+ region that is identity-mapped by Limine).
///
/// # Safety
/// Caller must ensure `ptr` points to memory allocated by `sys_allocate_pages`.
pub fn make_executable(ptr: *mut u8, byte_count: usize) {
    // Walk every page in the range and clear bit 63 (NX) in the PTE.
    // We use the HHDM offset from `PHYS_MEM_OFFSET` to reach the page tables.
    let phys_offset = unsafe { crate::PHYS_MEM_OFFSET };

    let start = ptr as usize;
    let end   = start + byte_count;

    // Round down to page boundary
    let mut page = start & !0xFFF;

    while page < end {
        // Resolve PTE for this virtual address using the recursive walk
        // through HHDM-mapped page tables.
        if let Some(pte_ptr) = resolve_pte(page, phys_offset) {
            unsafe {
                let pte = core::ptr::read_volatile(pte_ptr);
                // Bit 63 = NX (No-Execute). Clear it to allow execution.
                let new_pte = pte & !(1u64 << 63);
                core::ptr::write_volatile(pte_ptr, new_pte);
            }
        }
        page += 4096;
    }

    // Flush TLB for the modified range
    let mut addr = start & !0xFFF;
    while addr < end {
        unsafe {
            core::arch::asm!(
                "invlpg [{0}]",
                in(reg) addr,
                options(nostack, preserves_flags)
            );
        }
        addr += 4096;
    }
}

/// Walk the 4-level page table to find the PTE for a virtual address.
/// Returns a pointer to the PTE (inside the HHDM-mapped physical memory) or
/// `None` if any level is not present.
fn resolve_pte(vaddr: usize, hhdm: u64) -> Option<*mut u64> {
    #[inline]
    unsafe fn read_cr3() -> u64 {
        let cr3: u64;
        core::arch::asm!("mov {}, cr3", out(reg) cr3, options(nomem, nostack));
        cr3
    }

    let hhdm = hhdm as usize;

    let cr3      = unsafe { read_cr3() };
    let pml4_phys = (cr3 & !0xFFF) as usize;

    let pml4_idx = (vaddr >> 39) & 0x1FF;
    let pdpt_idx = (vaddr >> 30) & 0x1FF;
    let pd_idx   = (vaddr >> 21) & 0x1FF;
    let pt_idx   = (vaddr >> 12) & 0x1FF;

    // Safety: HHDM gives us access to any physical page as a virtual address.
    let pml4 = (pml4_phys + hhdm) as *const u64;
    let pml4e = unsafe { core::ptr::read_volatile(pml4.add(pml4_idx)) };
    if pml4e & 1 == 0 { return None; }

    let pdpt_phys = (pml4e & 0x000F_FFFF_FFFF_F000) as usize;
    let pdpt = (pdpt_phys + hhdm) as *const u64;
    let pdpte = unsafe { core::ptr::read_volatile(pdpt.add(pdpt_idx)) };
    if pdpte & 1 == 0 { return None; }
    if pdpte & (1 << 7) != 0 { return None; } // 1 GB page — skip

    let pd_phys = (pdpte & 0x000F_FFFF_FFFF_F000) as usize;
    let pd = (pd_phys + hhdm) as *const u64;
    let pde = unsafe { core::ptr::read_volatile(pd.add(pd_idx)) };
    if pde & 1 == 0 { return None; }
    if pde & (1 << 7) != 0 { return None; } // 2 MB page — skip

    let pt_phys = (pde & 0x000F_FFFF_FFFF_F000) as usize;
    let pt = (pt_phys + hhdm) as *mut u64;
    Some(unsafe { pt.add(pt_idx) })
}

pub fn sys_allocate_pages(count: usize) -> *mut u8 {
    unsafe {
        let bitmap = &mut *(&raw mut PAGE_BITMAP);
        let mut consecutive = 0;
        let mut start_idx = 0;
        
        for i in 0..1024 {
            let byte_idx = i / 8;
            let bit_idx = i % 8;
            if (bitmap[byte_idx] & (1 << bit_idx)) == 0 {
                if consecutive == 0 {
                    start_idx = i;
                }
                consecutive += 1;
                if consecutive == count {
                    
                    for j in start_idx..start_idx + count {
                        bitmap[j / 8] |= 1 << (j % 8);
                    }
                    
                    let addr = 0x1000_0000 + (start_idx * 4096);
                    return addr as *mut u8;
                }
            } else {
                consecutive = 0;
            }
        }
    }
    core::ptr::null_mut()
}
