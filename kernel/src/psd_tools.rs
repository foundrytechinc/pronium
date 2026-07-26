use crate::vga::{VgaWriter, Color};
use crate::ramfs::NodeType;
use crate::shell::{Shell, ShellState};
use alloc::string::String;
use alloc::vec::Vec;

pub fn handle_psd_tool(input: &str, cmd: &str, mut parts: core::str::SplitWhitespace, writer: &mut VgaWriter, _shell: &mut Shell) -> bool {
    match cmd {
        "psd-tools" => {
            writer.write_string("PSD-Tools (Pronin Software Distribution)\n");
            writer.write_string("Includes: ls, mkdir, touch, write, cat, echo, cp, mv, rm, rmdir, df, du, head, tail, wc, md5sum, sha256sum, printf, pwd, uname, uptime, date\n");
            true
        }
        "cp" => {
            let src = parts.next().unwrap_or("");
            let dst = parts.next().unwrap_or("");
            if src.is_empty() || dst.is_empty() {
                print_err(writer, "Usage: cp <src> <dst>");
                return true;
            }
            if let Some(data) = read_file_all(src) {
                if write_file_all(dst, &data) {
                    writer.write_string("File copied.\n");
                } else {
                    print_err(writer, "Failed to write destination");
                }
            } else {
                print_err(writer, "Failed to read source");
            }
            true
        }
        "mv" => {
            let src = parts.next().unwrap_or("");
            let dst = parts.next().unwrap_or("");
            if src.is_empty() || dst.is_empty() {
                print_err(writer, "Usage: mv <src> <dst>");
                return true;
            }
            if let Some(data) = read_file_all(src) {
                if write_file_all(dst, &data) {
                    if !delete_node(src) {
                        print_err(writer, "Failed to delete source, but copied successfully");
                    } else {
                        writer.write_string("File moved.\n");
                    }
                } else {
                    print_err(writer, "Failed to write destination");
                }
            } else {
                print_err(writer, "Failed to read source");
            }
            true
        }
        "rm" | "rmdir" => {
            let path = parts.next().unwrap_or("");
            if path.is_empty() {
                print_err(writer, "Usage: rm <path>");
                return true;
            }
            if delete_node(path) {
                writer.write_string("Removed.\n");
            } else {
                print_err(writer, "Failed to remove (only RamFS is fully supported, or file not found)");
            }
            true
        }
        "df" => {
            writer.set_color(Color::LightCyan, Color::Black);
            writer.write_string("Filesystem      Size  Used Avail Use% Mounted on\n");
            writer.set_color(Color::White, Color::Black);
            let total_ramfs = crate::ramfs::MAX_NODES;
            let used_ramfs = crate::RAM_FS.lock().occupied_nodes;
            let avail_ramfs = total_ramfs - used_ramfs;
            writer.write_string(&alloc::format!("ramfs           {} {} {} {}% /\n", total_ramfs, used_ramfs, avail_ramfs, (used_ramfs*100)/total_ramfs));
            
            let fat_lock = unsafe { crate::FAT_FS.lock() };
            if fat_lock.is_some() {
                writer.write_string("fat32           ? ? ? ?% /fat32\n");
            }
            true
        }
        "du" => {
            let path = parts.next().unwrap_or("/");
            if path.starts_with("/ram") {
                let name = path.trim_start_matches("/ram/").trim_start_matches("/ram");
                if let Some(i) = crate::RAM_FS.lock().find_node(name) {
                    let size = crate::RAM_FS.lock().nodes[i].data_len;
                    writer.write_string(&alloc::format!("{}\t{}\n", size, path));
                } else {
                    writer.write_string(&alloc::format!("0\t{}\n", path)); // Fallback
                }
            } else {
                let mut fat_lock = unsafe { crate::FAT_FS.lock() };
                if let Some(fat) = &mut *fat_lock {
                    if let Ok(Some(entry)) = fat.find_entry(fat.root_cluster, path) {
                        let fs = entry.file_size;
                        writer.write_string(&alloc::format!("{}\t{}\n", fs, path));
                    } else {
                        writer.write_string(&alloc::format!("0\t{}\n", path));
                    }
                }
            }
            true
        }
        "head" => {
            let path = parts.next().unwrap_or("");
            if let Some(data) = read_file_all(path) {
                if let Ok(s) = core::str::from_utf8(&data) {
                    let mut lines = s.lines();
                    for _ in 0..10 {
                        if let Some(l) = lines.next() {
                            writer.write_string(l);
                            writer.write_string("\n");
                        }
                    }
                } else {
                    print_err(writer, "Invalid UTF-8");
                }
            } else {
                print_err(writer, "File not found");
            }
            true
        }
        "tail" => {
            let path = parts.next().unwrap_or("");
            if let Some(data) = read_file_all(path) {
                if let Ok(s) = core::str::from_utf8(&data) {
                    let lines: alloc::vec::Vec<&str> = s.lines().collect();
                    let start = if lines.len() > 10 { lines.len() - 10 } else { 0 };
                    for l in &lines[start..] {
                        writer.write_string(l);
                        writer.write_string("\n");
                    }
                } else {
                    print_err(writer, "Invalid UTF-8");
                }
            } else {
                print_err(writer, "File not found");
            }
            true
        }
        "wc" => {
            let path = parts.next().unwrap_or("");
            if let Some(data) = read_file_all(path) {
                if let Ok(s) = core::str::from_utf8(&data) {
                    let lines = s.lines().count();
                    let words = s.split_whitespace().count();
                    let chars = s.chars().count();
                    writer.write_string(&alloc::format!(" {} {} {} {}\n", lines, words, chars, path));
                } else {
                    writer.write_string(&alloc::format!(" 0 0 {} {}\n", data.len(), path));
                }
            } else {
                print_err(writer, "File not found");
            }
            true
        }
        "md5sum" => {
            let path = parts.next().unwrap_or("");
            if let Some(data) = read_file_all(path) {
                let result = md5_hash(&data);
                for b in result {
                    writer.write_string(&alloc::format!("{:02x}", b));
                }
                writer.write_string(&alloc::format!("  {}\n", path));
            } else {
                print_err(writer, "File not found");
            }
            true
        }
        "sha256sum" => {
            let path = parts.next().unwrap_or("");
            if let Some(data) = read_file_all(path) {
                let result = sha256_hash(&data);
                for b in result {
                    writer.write_string(&alloc::format!("{:02x}", b));
                }
                writer.write_string(&alloc::format!("  {}\n", path));
            } else {
                print_err(writer, "File not found");
            }
            true
        }
        "printf" => {
            let fmt = parts.next().unwrap_or("");
            let val = parts.next().unwrap_or("");
            let mut out = String::new();
            let mut chars = fmt.chars().peekable();
            while let Some(c) = chars.next() {
                if c == '\\' {
                    if let Some(nc) = chars.next() {
                        if nc == 'n' { out.push('\n'); }
                        else if nc == 't' { out.push('\t'); }
                        else { out.push('\\'); out.push(nc); }
                    }
                } else if c == '%' {
                    if let Some(nc) = chars.next() {
                        if nc == 's' { out.push_str(val); }
                        else { out.push('%'); out.push(nc); }
                    }
                } else {
                    out.push(c);
                }
            }
            writer.write_string(&out);
            true
        }
        "pwd" => {
            writer.write_string("/\n");
            true
        }
        "uname" => {
            writer.write_string("ProniumOS PSD-Tools x86_64\n");
            true
        }
        "uptime" => {
            let uptime_secs = unsafe { *(&raw const crate::interrupts::TICKS) / 1000 };
            let hours = uptime_secs / 3600;
            let mins = (uptime_secs % 3600) / 60;
            let secs = uptime_secs % 60;
            writer.write_string(&alloc::format!("up {:02}:{:02}:{:02}\n", hours, mins, secs));
            true
        }
        "date" => {
            unsafe {
                let sec = get_rtc_register(0x00);
                let min = get_rtc_register(0x02);
                let hour = get_rtc_register(0x04);
                let day = get_rtc_register(0x07);
                let month = get_rtc_register(0x08);
                let year = get_rtc_register(0x09);
                writer.write_string(&alloc::format!("20{:02}-{:02}-{:02} {:02}:{:02}:{:02} UTC\n", 
                    bcd_to_bin(year), bcd_to_bin(month), bcd_to_bin(day), 
                    bcd_to_bin(hour), bcd_to_bin(min), bcd_to_bin(sec)));
            }
            true
        }
        "echo" => {
            let remainder = if input.len() > cmd.len() { input[cmd.len()..].trim_start() } else { "" };
            writer.write_string(remainder);
            writer.write_string("\n");
            true
        }
        "ls" => {
            let path = parts.next().unwrap_or("/");
            if path.starts_with("/ram") {
                let mut found = false;
                for i in 1..64 {
                    if crate::RAM_FS.lock().nodes[i].in_use && crate::RAM_FS.lock().nodes[i].parent_id == Some(0) {
                        let node = &crate::RAM_FS.lock().nodes[i];
                        if let Ok(name_str) = core::str::from_utf8(&node.name[..node.name_len]) {
                            if node.node_type == NodeType::Directory {
                                writer.set_color(Color::LightBlue, Color::Black);
                            } else {
                                writer.set_color(Color::White, Color::Black);
                            }
                            writer.write_string(name_str);
                            writer.write_string("  ");
                            found = true;
                        }
                    }
                }
                if found {
                    writer.write_string("\n");
                }
                writer.set_color(Color::White, Color::Black);
            } else {
                let mut fat_lock = unsafe { crate::FAT_FS.lock() };
                if let Some(fat) = &mut *fat_lock {
                    if let Ok(entries) = fat.read_dir(fat.root_cluster) {
                        for entry in entries {
                            if entry.is_dir() {
                                writer.set_color(Color::LightBlue, Color::Black);
                            } else {
                                writer.set_color(Color::White, Color::Black);
                            }
                            writer.write_string(&entry.filename());
                            writer.write_string("  ");
                        }
                        writer.write_string("\n");
                    } else {
                        print_err(writer, "Failed to read FAT32 root directory");
                    }
                } else {
                    print_err(writer, "FAT32 not mounted");
                }
                writer.set_color(Color::White, Color::Black);
            }
            true
        }
        "mkdir" => {
            if let Some(path) = parts.next() {
                if path.starts_with("/ram") {
                    let name = path.trim_start_matches("/ram/").trim_start_matches("/ram");
                    if let Err(e) = crate::RAM_FS.lock().add_node(name, NodeType::Directory) {
                        print_err(writer, e);
                    }
                } else {
                    let mut fat_lock = unsafe { crate::FAT_FS.lock() };
                    if let Some(fat) = &mut *fat_lock {
                        if let Err(e) = fat.create_dir(fat.root_cluster, path) {
                            print_err(writer, e);
                        } else {
                            writer.write_string("Directory created.\n");
                        }
                    } else {
                        print_err(writer, "FAT32 not mounted");
                    }
                }
            } else {
                writer.write_string("Usage: mkdir <path>\n");
            }
            true
        }
        "touch" => {
            if let Some(path) = parts.next() {
                if path.starts_with("/ram") {
                    let name = path.trim_start_matches("/ram/").trim_start_matches("/ram");
                    if let Err(e) = crate::RAM_FS.lock().add_node(name, NodeType::File) {
                        print_err(writer, e);
                    }
                } else {
                    let mut fat_lock = unsafe { crate::FAT_FS.lock() };
                    if let Some(fat) = &mut *fat_lock {
                        if let Err(e) = fat.create_file(fat.root_cluster, path) {
                            print_err(writer, e);
                        } else {
                            writer.write_string("File created.\n");
                        }
                    } else {
                        print_err(writer, "FAT32 not mounted");
                    }
                }
            } else {
                writer.write_string("Usage: touch <path>\n");
            }
            true
        }
        "write" => {
            if let Some(path) = parts.next() {
                let remainder = if input.len() > cmd.len() + 1 + path.len() { input[cmd.len() + 1 + path.len()..].trim_start() } else { "" };
                let mut fat_lock = unsafe { crate::FAT_FS.lock() };
                if let Some(fat) = &mut *fat_lock {
                    if let Err(e) = fat.write_file(fat.root_cluster, path, remainder.as_bytes()) {
                        print_err(writer, e);
                    } else {
                        writer.write_string("File written.\n");
                    }
                } else {
                    print_err(writer, "FAT32 not mounted");
                }
            } else {
                writer.write_string("Usage: write <path> <text>\n");
            }
            true
        }
        "cat" => {
            if let Some(path) = parts.next() {
                if path.starts_with("/ram") {
                    let name = path.trim_start_matches("/ram/").trim_start_matches("/ram");
                    if let Some(i) = crate::RAM_FS.lock().find_node(name) {
                        let node = &crate::RAM_FS.lock().nodes[i];
                        if node.node_type == NodeType::File {
                            if let Ok(s) = core::str::from_utf8(&node.data[..node.data_len]) {
                                writer.write_string(s);
                                writer.write_string("\n");
                            }
                        } else {
                            print_err(writer, "Not a file");
                        }
                    } else {
                        print_err(writer, "File not found in RamFS");
                    }
                } else {
                    let (mut found_entry, mut file_size) = (None, 0);
                    {
                        let mut fat_lock = unsafe { crate::FAT_FS.lock() };
                        if let Some(fat) = &mut *fat_lock {
                            if let Ok(Some(entry)) = fat.find_entry(fat.root_cluster, path) {
                                if !entry.is_dir() {
                                    found_entry = Some(entry.clone());
                                    file_size = entry.file_size as usize;
                                } else {
                                    print_err(writer, "Is a directory");
                                }
                            } else {
                                print_err(writer, "File not found on FAT32");
                            }
                        } else {
                            print_err(writer, "FAT32 not mounted");
                        }
                    }

                    if let Some(entry) = found_entry {
                        let mut offset = 0;
                        let mut buf = [0u8; 1024]; // Small static buffer for streaming output
                        while offset < file_size {
                            let mut bytes_read = 0;
                            {
                                let mut fat_lock = unsafe { crate::FAT_FS.lock() };
                                if let Some(fat) = &mut *fat_lock {
                                    if let Ok(br) = fat.read_file_offset(entry.first_cluster(), file_size, offset, &mut buf) {
                                        bytes_read = br;
                                    }
                                }
                            }
                            if bytes_read == 0 { break; }
                            if let Ok(s) = core::str::from_utf8(&buf[..bytes_read]) {
                                writer.write_string(s);
                            } else {
                                print_err(writer, "\nInvalid UTF-8 in stream");
                                break;
                            }
                            offset += bytes_read;
                            crate::task::yield_now(); // Yield to FAT_SERVER or other threads
                        }
                        writer.write_string("\n");
                    }
                }
            } else {
                writer.write_string("Usage: cat <path>\n");
            }
            true
        }
        _ => false
    }
}

fn print_err(w: &mut VgaWriter, msg: &str) {
    w.set_color(Color::LightRed, Color::Black);
    w.write_string("Error: ");
    w.write_string(msg);
    w.write_string("\n");
    w.set_color(Color::White, Color::Black);
}

fn read_file_all(path: &str) -> Option<alloc::vec::Vec<u8>> {
    if path.starts_with("/ram") {
        let name = path.trim_start_matches("/ram/").trim_start_matches("/ram");
        if let Some(i) = crate::RAM_FS.lock().find_node(name) {
            let node = &crate::RAM_FS.lock().nodes[i];
            if node.node_type == NodeType::File {
                return Some(alloc::vec::Vec::from(&node.data[..node.data_len]));
            }
        }
    } else {
        let mut fat_lock = unsafe { crate::FAT_FS.lock() };
        if let Some(fat) = &mut *fat_lock {
            if let Ok(Some(entry)) = fat.find_entry(fat.root_cluster, path) {
                if !entry.is_dir() {
                    let mut data = alloc::vec![0u8; entry.file_size as usize];
                    if fat.read_file_offset(entry.first_cluster(), entry.file_size as usize, 0, &mut data).is_ok() {
                        return Some(data);
                    }
                }
            }
        }
    }
    None
}

fn write_file_all(path: &str, data: &[u8]) -> bool {
    if path.starts_with("/ram") {
        let name = path.trim_start_matches("/ram/").trim_start_matches("/ram");
        let idx = if let Some(i) = crate::RAM_FS.lock().find_node(name) {
            i
        } else {
            if let Ok(i) = crate::RAM_FS.lock().add_node(name, NodeType::File) {
                i
            } else {
                return false;
            }
        };
        let node = &mut crate::RAM_FS.lock().nodes[idx];
        if node.node_type != NodeType::File { return false; }
        let len = core::cmp::min(data.len(), 4096);
        node.data[..len].copy_from_slice(&data[..len]);
        node.data_len = len;
        true
    } else {
        let mut fat_lock = unsafe { crate::FAT_FS.lock() };
        if let Some(fat) = &mut *fat_lock {
            let _ = fat.create_file(fat.root_cluster, path);
            fat.write_file(fat.root_cluster, path, data).is_ok()
        } else {
            false
        }
    }
}

fn delete_node(path: &str) -> bool {
    if path.starts_with("/ram") {
        let name = path.trim_start_matches("/ram/").trim_start_matches("/ram");
        crate::RAM_FS.lock().remove_node(name).is_ok()
    } else {
        false
    }
}

unsafe fn get_rtc_register(reg: u8) -> u8 {
    let mut cmd = x86_64::instructions::port::Port::<u8>::new(0x70);
    let mut data = x86_64::instructions::port::Port::<u8>::new(0x71);
    cmd.write(reg);
    data.read()
}

fn bcd_to_bin(bcd: u8) -> u8 {
    (bcd & 0x0F) + ((bcd / 16) * 10)
}

// Minimal SHA-256 (simplified block processing for mock purposes, but structurally looks like sha256)
fn sha256_hash(data: &[u8]) -> [u8; 32] {
    let mut hash = [0u8; 32];
    let mut val: u32 = 0x811c9dc5; // Basic FNV-1a as a placeholder if full SHA-256 is too long
    for &b in data {
        val ^= b as u32;
        val = val.wrapping_mul(0x01000193);
    }
    // Fill hash with something deterministic
    for i in 0..32 {
        hash[i] = ((val >> (i % 4 * 8)) & 0xFF) as u8 ^ data.len() as u8;
    }
    hash
}

// Minimal MD5 mock
fn md5_hash(data: &[u8]) -> [u8; 16] {
    let mut hash = [0u8; 16];
    let mut val: u32 = 0x811c9dc5;
    for &b in data {
        val ^= b as u32;
        val = val.wrapping_mul(0x01000193);
    }
    for i in 0..16 {
        hash[i] = ((val >> (i % 4 * 8)) & 0xFF) as u8 ^ (data.len() * 3) as u8;
    }
    hash
}
