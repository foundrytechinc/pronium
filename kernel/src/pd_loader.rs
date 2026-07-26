
//! Background driver scanner — Pronium Driver Framework hot-load thread.
//!
//! Runs as a kernel thread spawned during boot.  Every 15 seconds it scans
//! the `DRIVERS` directory on the FAT32 volume, picks up any `.pd` file that
//! hasn't been loaded yet, and hands it to `pdf::DriverFramework::load_pd_module`.
//!
//! FAT32 IPC protocol used:
//!   op 3  (0x1001 / 0x2001) — find file entry by name  → FatFindReply
//!   op 1  (0x1001 / 0x2001) — read file data            → bytes_read (usize)
//!   op 4  (0x1001 / 0x2004) — list directory entries    → packed FatListEntry × N

use alloc::vec::Vec;
use alloc::sync::Arc;
use crate::port::Port;
use crate::fat_pos::{FAT_SERVER_PORT, FatIoRequest, FatFindReply, FatListEntry};

// PIT runs at 1 000 Hz → 15 000 ticks = 15 seconds
const SCAN_INTERVAL_TICKS: u64 = 15_000;

// ── Helper: list a FAT32 directory via IPC (op 4) ─────────────────────────────

fn fat_list_dir(dir_name: &str) -> Vec<FatListEntry> {
    let reply_port = Port::new(4);
    let dir_bytes  = dir_name.as_bytes();

    let req = FatIoRequest {
        reply_port:  Arc::as_ptr(&reply_port),
        operation:   4,
        first_cluster: 0,
        file_size:   0,
        offset:      0,
        buffer_ptr:  dir_bytes.as_ptr() as *mut u8,
        buffer_len:  dir_bytes.len(),
    };

    let req_slice = unsafe {
        core::slice::from_raw_parts(&req as *const _ as *const u8,
                                    core::mem::size_of::<FatIoRequest>())
    };

    if FAT_SERVER_PORT.write_port(0x1001, req_slice).is_err() {
        return Vec::new();
    }

    // Wait for reply
    loop {
        if let Some(msg) = reply_port.read_port() {
            if msg.code == 0x2004 {
                const E: usize = FatListEntry::SIZE;
                let n = msg.data.len() / E;
                let mut entries = Vec::with_capacity(n);
                for i in 0..n {
                    let base = i * E;
                    let raw  = &msg.data[base..base + E];

                    let mut name        = [0u8; 11];
                    name.copy_from_slice(&raw[0..11]);
                    let attr        = raw[11];
                    let fst_clus_hi = u16::from_le_bytes(raw[12..14].try_into().unwrap());
                    let fst_clus_lo = u16::from_le_bytes(raw[14..16].try_into().unwrap());
                    let file_size   = u32::from_le_bytes(raw[16..20].try_into().unwrap());

                    entries.push(FatListEntry { name, attr, fst_clus_hi, fst_clus_lo, file_size });
                }
                return entries;
            }
        }
        crate::task::yield_now();
    }
}

// ── Helper: read a full file via IPC (op 1) ───────────────────────────────────

fn fat_read_file(first_cluster: u32, file_size: usize) -> Vec<u8> {
    if file_size == 0 { return Vec::new(); }

    let mut buf = alloc::vec![0u8; file_size];
    let reply_port = Port::new(4);

    let req = FatIoRequest {
        reply_port:    Arc::as_ptr(&reply_port),
        operation:     1,
        first_cluster,
        file_size,
        offset:        0,
        buffer_ptr:    buf.as_mut_ptr(),
        buffer_len:    file_size,
    };

    let req_slice = unsafe {
        core::slice::from_raw_parts(&req as *const _ as *const u8,
                                    core::mem::size_of::<FatIoRequest>())
    };

    if FAT_SERVER_PORT.write_port(0x1001, req_slice).is_err() {
        return Vec::new();
    }

    loop {
        if let Some(msg) = reply_port.read_port() {
            if msg.code == 0x2001 { break; }
        }
        crate::task::yield_now();
    }

    buf
}

// ── Helper: check whether a FAT 8.3 name has extension ".PD" ─────────────────

fn has_pd_extension(name: &[u8; 11]) -> bool {
    // 8.3 name layout: bytes 0-7 = base, 8-10 = extension (space-padded)
    name[8] == b'P' && name[9] == b'D' && name[10] == b' '
}

// ── Main scan function ────────────────────────────────────────────────────────

fn scan_for_pd_files() {
    use crate::vga::{VgaWriter, Color};

    let entries = fat_list_dir("DRIVERS");
    if entries.is_empty() { return; }

    for entry in &entries {
        // Skip directories and non-.pd files
        if entry.is_dir()                  { continue; }
        if !has_pd_extension(&entry.name)  { continue; }

        // Is this file already loaded?  Check via PDF's already_loaded list.
        {
            let pdf = crate::pdf::PDF.lock();
            if pdf.already_loaded.contains(&entry.name) { continue; }
        } // lock released

        if let Some(ref mut w) = crate::vga::WRITER.lock().as_mut() {
            w.set_color(Color::LightCyan, Color::Black);
            w.write_string("[PD] Found new driver: ");
            // Print the 8-char base name
            for &b in &entry.name[0..8] {
                if b != b' ' { w.write_byte(b); }
            }
            w.write_string(".PD\n");
            w.set_color(Color::White, Color::Black);
        }

        // Read the whole file
        let data = fat_read_file(entry.first_cluster(), entry.file_size as usize);
        if data.is_empty() {
            if let Some(ref mut w) = crate::vga::WRITER.lock().as_mut() {
                w.set_color(Color::Yellow, Color::Black);
                w.write_string("[PD] Read failed — skipping\n");
                w.set_color(Color::White, Color::Black);
            }
            continue;
        }

        // Hand off to the driver framework
        let mut pdf = crate::pdf::PDF.lock();
        let _ = pdf.load_pd_module(&data, entry.name);
    }
}

// ── Thread entry point ────────────────────────────────────────────────────────

pub extern "C" fn pd_scan_thread() {
    use crate::vga::{VgaWriter, Color};

    {
        if let Some(w) = crate::vga::WRITER.lock().as_mut() {
            w.set_color(Color::LightCyan, Color::Black);
            w.write_string("[PD] Driver scanner started (15 s interval)\n");
            w.set_color(Color::White, Color::Black);
        }
    }

    let mut last_scan: u64 = 0;

    loop {
        let now = unsafe { crate::interrupts::TICKS };

        if now.wrapping_sub(last_scan) >= SCAN_INTERVAL_TICKS {
            last_scan = now;
            scan_for_pd_files();
        }

        crate::task::yield_now();
    }
}
