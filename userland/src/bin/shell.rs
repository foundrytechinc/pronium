// Copyright (C) 2026 Pronin. All rights reserved.
// ProniumOS Userland Shell

#![no_std]
#![no_main]

extern crate alloc;
extern crate sdk;

use alloc::string::String;
use sdk::{print, println};

// ============================================================================
// Entry point
// ============================================================================

#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    // Heap starts right above the code region (0x400000 + 1 MiB)
    sdk::init_heap(0x400000 + 0x100000, 2 * 1024 * 1024);

    println!("===========================================");
    println!(" ProniumOS Userland Shell (IUM Executable) ");
    println!("===========================================");
    println!("Type 'help' for a list of commands.");
    println!();

    let mut kb_handle = 0usize;
    if !sdk::nt_open_file(r"\Device\Keyboard", &mut kb_handle).is_success() {
        println!("FATAL: Failed to open \\Device\\Keyboard");
        sdk::sys_exit();
    }

    let mut input_buffer = String::new();

    loop {
        // Print prompt with current user
        let mut user_buf = [0u8; 32];
        let mut user_len = 0usize;
        sdk::sys_get_user(&mut user_buf, &mut user_len);
        let user = core::str::from_utf8(&user_buf[..user_len]).unwrap_or("user");
        print!("{}@pronium:~# ", user);

        input_buffer.clear();

        // Read line from keyboard
        loop {
            let mut buf = [0u8; 1];
            let mut bytes_read = 0usize;
            if sdk::nt_read_file(kb_handle, &mut buf, 0, &mut bytes_read).is_success()
                && bytes_read == 1
            {
                let scancode = buf[0];
                if let Some(c) = scancode_to_char(scancode) {
                    if c == '\n' {
                        println!();
                        break;
                    } else if c == '\x08' {
                        if !input_buffer.is_empty() {
                            input_buffer.pop();
                            print!("\x08 \x08");
                        }
                    } else {
                        input_buffer.push(c);
                        print!("{}", c);
                    }
                }
            }
        }

        execute_command(input_buffer.trim());
    }
}

// ============================================================================
// Command dispatcher — 1:1 port of commands.rs
// ============================================================================

fn execute_command(input: &str) {
    if input.is_empty() {
        return;
    }

    let mut parts = input.split_whitespace();
    let cmd = parts.next().unwrap_or("");

    match cmd {
        "help" => {
            print!("Available commands:\n");
            print!("  help          Show this help\n");
            print!("  clear         Clear screen\n");
            print!("  echo <text>   Print text\n");
            print!("  stat          System info\n");
            print!("  ls            List files\n");
            print!("  mkdir <name>  Create directory\n");
            print!("  touch <name>  Create file\n");
            print!("  write <name> <text> Write to file (FAT32)\n");
            print!("  cat <name>    Show file contents\n");
            print!("  edit <name>   Text editor (ESC to save)\n");
            print!("  whoami        Current user\n");
            print!("  su <user>     Switch user\n");
            print!("  ifconfig      Network interfaces\n");
            print!("  ping <ip>     ICMP echo (real)\n");
            print!("  setdns <ip>   Set global DNS server\n");
            print!("  pronfetch     OS Information\n");
            print!("  iumstart <p>  Execute .ium binary\n");
            print!("  httpget <ip> [port] [path]  HTTP GET\n");
            print!("  beep <hz> <ms> Play sound via speaker server (IPC)\n");
            print!("  cr <W>x<H>    Change resolution (1024x768 to 1920x1080)\n");
        }
        "beep" => {
            let hz_str = parts.next().unwrap_or("440");
            let ms_str = parts.next().unwrap_or("500");
            let hz = parse_u32(hz_str).unwrap_or(440);
            let ms = parse_u64(ms_str).unwrap_or(500);
            let _ = sdk::sys_beep(hz, ms);
        }
        "cr" => {
            if let Some(res_str) = parts.next() {
                let mut dims = res_str.split('x');
                let width_str = dims.next();
                let height_str = dims.next();
                if let (Some(w_str), Some(h_str)) = (width_str, height_str) {
                    if let (Some(w), Some(h)) = (parse_u32(w_str), parse_u32(h_str)) {
                        if w < 1024 || w > 1920 || h < 768 || h > 1080 {
                            print_err("Invalid resolution");
                        } else {
                            if sdk::sys_change_res(w as u16, h as u16).is_success() {
                                print!("\x1b[92mResolution changed successfully!\n\x1b[97m");
                            } else {
                                print_err("Failed to change resolution via Bochs VBE");
                            }
                        }
                    } else {
                        print_err("Invalid resolution format. Use WxH (e.g. 1024x768)");
                    }
                } else {
                    print_err("Invalid resolution format. Use WxH (e.g. 1024x768)");
                }
            } else {
                print!("Usage: cr <width>x<height>\n");
            }
        }
        "clear" => {
            print!("\x1b[2J\x1b[H");
        }
        "echo" => {
            let remainder = if input.len() > cmd.len() {
                input[cmd.len()..].trim_start()
            } else {
                ""
            };
            print!("{}\n", remainder);
        }
        "stat" => {
            let mut info = sdk::SysStatInfo {
                ram_mb: 0, monitor_w: 0, monitor_h: 0, uptime_secs: 0,
                pci_dev_count: 0, nic_type: 0, padding: 0,
            };
            sdk::sys_get_stat(&mut info);
            print!("\x1b[96mOS: Pronium OS [Proprietary Core]\n");
            print!("Status: Proprietary\n");
            print!("RamFS Nodes: ?/64\n"); // userland sys_get_stat doesn't give RamFS nodes
            print!("NIC: {}\n", if info.nic_type > 0 { "UP" } else { "none" });
            print!("\x1b[97m");
        }
        "pronfetch" => {
            let mut info = sdk::SysStatInfo {
                ram_mb: 0, monitor_w: 0, monitor_h: 0, uptime_secs: 0,
                pci_dev_count: 0, nic_type: 0, padding: 0,
            };
            sdk::sys_get_stat(&mut info);
            
            print!("\x1b[96m               @@@@@@@@@@@@@@@     \x1b[97mOS: Pronium OS\n");
            print!("\x1b[96m                            @@@    \x1b[97mBuild: 260726\n");
            print!("\x1b[96m                            @@@    \x1b[97mUptime: {}s\n", info.uptime_secs);
            print!("\x1b[96m               @@@@@@@@@@@@@@@@    \x1b[97mRAM (Usable): {} MB\n", info.ram_mb);
            print!("\x1b[96m               @@@@@@@@@@@@@@      \x1b[97mNIC: {}\n", nic_name(info.nic_type));
            print!("\x1b[96m               @@@                 \x1b[97mDevices: {} PCI\n", info.pci_dev_count);
            print!("\x1b[96m               @@@                 \x1b[97mMonitor: VGA Text Mode {}x{}\n", info.monitor_w, info.monitor_h);
        }
        "ls" => {
            let path = parts.next().unwrap_or("/");
            if path.starts_with("/ram") {
                let mut buf = [0u8; 4096];
                let (status, count) = sdk::sys_ramfs_ls(&mut buf);
                if status.is_success() && count > 0 {
                    let mut pos = 0usize;
                    let mut found = false;
                    while pos < buf.len() && buf[pos] != 0 {
                        let type_byte = buf[pos];
                        pos += 1;
                        let name_start = pos;
                        while pos < buf.len() && buf[pos] != 0 { pos += 1; }
                        if let Ok(name) = core::str::from_utf8(&buf[name_start..pos]) {
                            if type_byte == b'd' {
                                print!("\x1b[94m");
                            } else {
                                print!("\x1b[97m");
                            }
                            print!("{}  ", name);
                            found = true;
                        }
                        pos += 1;
                    }
                    if found { print!("\n"); }
                    print!("\x1b[97m");
                }
            } else {
                let mut buf = [0u8; 4096];
                let (status, count) = sdk::sys_fat_ls(path, &mut buf);
                if status.is_success() {
                    const ENTRY_SIZE: usize = 20;
                    for i in 0..(count as usize) {
                        let off = i * ENTRY_SIZE;
                        if off + ENTRY_SIZE > buf.len() { break; }
                        let name_bytes = &buf[off..off + 11];
                        let attr = buf[off + 11];
                        let is_dir = (attr & 0x10) != 0;
                        let mut name_str = String::new();
                        let base = &name_bytes[..8];
                        let ext = &name_bytes[8..11];
                        for &b in base { if b != 0x20 && b != 0 { name_str.push(b as char); } }
                        let mut has_ext = false;
                        for &b in ext { if b != 0x20 && b != 0 { has_ext = true; break; } }
                        if has_ext {
                            name_str.push('.');
                            for &b in ext { if b != 0x20 && b != 0 { name_str.push(b as char); } }
                        }
                        if is_dir {
                            print!("\x1b[94m");
                        } else {
                            print!("\x1b[97m");
                        }
                        print!("{}  ", name_str);
                    }
                    print!("\n");
                } else {
                    if path == "/" {
                        print_err("Failed to read FAT32 root directory");
                    } else {
                        print_err("FAT32 not mounted");
                    }
                }
                print!("\x1b[97m");
            }
        }
        "mkdir" => {
            if let Some(path) = parts.next() {
                if path.starts_with("/ram") {
                    let name = path.trim_start_matches("/ram/").trim_start_matches("/ram");
                    let status = sdk::sys_ramfs_mkdir(name);
                    if !status.is_success() {
                        print_err("Error creating directory");
                    }
                } else {
                    let status = sdk::sys_fat_mkdir(path);
                    if status.is_success() {
                        print!("Directory created.\n");
                    } else {
                        print_err("Failed to create directory (or FAT32 not mounted)");
                    }
                }
            } else {
                print!("Usage: mkdir <path>\n");
            }
        }
        "touch" => {
            if let Some(path) = parts.next() {
                if path.starts_with("/ram") {
                    let name = path.trim_start_matches("/ram/").trim_start_matches("/ram");
                    let status = sdk::sys_ramfs_touch(name);
                    if !status.is_success() {
                        print_err("Error creating file");
                    }
                } else {
                    let status = sdk::sys_fat_touch(path);
                    if status.is_success() {
                        print!("File created.\n");
                    } else {
                        print_err("FAT32 not mounted");
                    }
                }
            } else {
                print!("Usage: touch <path>\n");
            }
        }
        "write" => {
            if let Some(path) = parts.next() {
                let remainder = if input.len() > cmd.len() + 1 + path.len() {
                    input[cmd.len() + 1 + path.len()..].trim_start()
                } else {
                    ""
                };
                if path.starts_with("/ram") {
                    let name = path.trim_start_matches("/ram/").trim_start_matches("/ram");
                    let status = sdk::sys_ramfs_write(name, remainder.as_bytes());
                    if status.is_success() {
                        print!("File written.\n");
                    } else {
                        print_err("Error writing file");
                    }
                } else {
                    let status = sdk::sys_fat_write(path, remainder.as_bytes());
                    if status.is_success() {
                        print!("File written.\n");
                    } else {
                        print_err("FAT32 not mounted");
                    }
                }
            } else {
                print!("Usage: write <path> <text>\n");
            }
        }
        "cat" => {
            if let Some(path) = parts.next() {
                if path.starts_with("/ram") {
                    let name = path.trim_start_matches("/ram/").trim_start_matches("/ram");
                    let mut buf = [0u8; 512];
                    let bytes = sdk::sys_ramfs_cat(name, &mut buf);
                    if bytes <= 7 {
                        if bytes == 3 {
                            print_err("File not found in RamFS");
                        } else if bytes == 6 {
                            print_err("Not a file");
                        }
                    } else {
                        let len = bytes as usize;
                        if let Ok(s) = core::str::from_utf8(&buf[..len]) {
                            print!("{}\n", s);
                        }
                    }
                } else {
                    let mut buf = alloc::vec![0u8; 8192];
                    let bytes = sdk::sys_fat_cat(path, &mut buf);
                    match bytes {
                        0 => print!("\n"),
                        3 => print_err("File not found on FAT32"),
                        5 => print_err("FAT32 not mounted"),
                        6 => print_err("Is a directory"),
                        n if n < 8 => print_err("FAT32 error"),
                        n => {
                            if let Ok(s) = core::str::from_utf8(&buf[..n as usize]) {
                                print!("{}\n", s);
                            } else {
                                print_err("\nInvalid UTF-8 in stream");
                            }
                        }
                    }
                }
            } else {
                print!("Usage: cat <path>\n");
            }
        }
        "edit" => {
            if let Some(path) = parts.next() {
                let mut data_buf = alloc::vec![0u8; 4096];
                let mut data_len = 0;
                let mut is_ramfs = false;
                let mut name = path;
                
                if path.starts_with("/ram") {
                    is_ramfs = true;
                    name = path.trim_start_matches("/ram/").trim_start_matches("/ram");
                    let bytes = sdk::sys_ramfs_cat(name, &mut data_buf);
                    if bytes > 7 {
                        data_len = bytes as usize;
                    }
                } else {
                    let bytes = sdk::sys_fat_cat(path, &mut data_buf);
                    if bytes > 7 {
                        data_len = bytes as usize;
                    } else if bytes == 5 {
                        print_err("FAT32 not mounted");
                        return;
                    }
                }

                print!("\x1b[2J\x1b[H\x1b[36m--- Pronium Editor (ESC to save) ---\n\x1b[97m");
                if data_len > 0 {
                    if let Ok(s) = core::str::from_utf8(&data_buf[..data_len]) {
                        print!("{}", s);
                    }
                }

                let mut kb_handle = 0usize;
                sdk::nt_open_file(r"\Device\Keyboard", &mut kb_handle);

                let mut edit_buf = alloc::string::String::new();
                if data_len > 0 {
                    if let Ok(s) = core::str::from_utf8(&data_buf[..data_len]) {
                        edit_buf.push_str(s);
                    }
                }

                loop {
                    let mut key_buf = [0u8; 1];
                    let mut br = 0usize;
                    if sdk::nt_read_file(kb_handle, &mut key_buf, 0, &mut br).is_success() && br == 1 {
                        let sc = key_buf[0];
                        if sc == 0x01 { // ESC
                            break;
                        }
                        if let Some(c) = scancode_to_char(sc) {
                            if c == '\x08' {
                                if !edit_buf.is_empty() {
                                    edit_buf.pop();
                                    print!("\x08 \x08");
                                }
                            } else if c == '\n' {
                                edit_buf.push(c);
                                print!("\n");
                            } else {
                                edit_buf.push(c);
                                print!("{}", c);
                            }
                        }
                    }
                }
                sdk::nt_close(kb_handle);
                print!("\x1b[2J\x1b[H"); // Clear screen after exit
                
                // Save
                if is_ramfs {
                    sdk::sys_ramfs_write(name, edit_buf.as_bytes());
                } else {
                    sdk::sys_fat_write(path, edit_buf.as_bytes());
                }
            } else {
                print!("Usage: edit <path>\n");
            }
        }
        "iumstart" => {
            if let Some(path) = parts.next() {
                let mut data_buf = alloc::vec![0u8; 1024];
                let bytes = sdk::sys_fat_cat(path, &mut data_buf);
                if bytes == 6 {
                    print_err("Cannot execute a directory");
                } else if bytes > 7 {
                    print!("Executing {} ({} bytes)...\n", path, bytes);
                    let status = sdk::sys_iumstart(path);
                    match status {
                        sdk::NtStatus::Success => {
                            print!("\x1b[92mValid IUM binary detected! Jumping to entry point...\n\x1b[97m");
                            print!("\x1b[92mProcess spawned in Ring 3!\n\x1b[97m");
                        }
                        sdk::NtStatus::NotFound => {
                            print_err("File not found on FAT32");
                        }
                        sdk::NtStatus::InvalidParameter => {
                            print_err("Invalid IUM magic bytes");
                        }
                        _ => {
                            print_err("File too small to be a valid IUM binary");
                        }
                    }
                } else {
                    print_err("File not found on FAT32");
                }
            } else {
                print!("Usage: iumstart <path>\n");
            }
        }
        "whoami" => {
            let mut user_buf = [0u8; 32];
            let mut user_len = 0usize;
            sdk::sys_get_user(&mut user_buf, &mut user_len);
            if let Ok(name) = core::str::from_utf8(&user_buf[..user_len]) {
                print!("{}\n", name);
            }
        }
        "su" => {
            if let Some(name) = parts.next() {
                let canonical = match name {
                    "root" => "root",
                    "pronin" => "pronin",
                    _ => "guest",
                };
                sdk::sys_set_user(canonical);
            } else {
                print!("Usage: su <user>\n");
            }
        }
        "setdns" => {
            if let Some(ip_str) = parts.next() {
                if let Some(ip) = parse_ip(ip_str) {
                    sdk::sys_net_setdns(&ip);
                    print!("DNS server set to ");
                    print_ip(&ip);
                    print!("\n");
                } else {
                    print_err("Invalid IP address");
                }
            } else {
                print!("Usage: setdns <ip>\n");
            }
        }
        "ifconfig" => {
            let mut info = sdk::SysIfconfigInfo::zeroed();
            if sdk::sys_net_ifconfig(&mut info).is_success() && info.has_nic != 0 {
                print!("\x1b[96meth0: flags=UP\n");
                print!("      ether ");
                print_mac(&info.mac);
                print!("\n      inet  ");
                print_ip(&info.our_ip);
                print!("\n      mask  ");
                print_ip(&info.subnet_mask);
                print!("\n      gw    ");
                print_ip(&info.gateway_ip);
                print!("\n      dns   ");
                print_ip(&info.dns_ip);
                print!("\n\x1b[97m");
            } else {
                print_err("No network interface");
            }
        }
        "ping" => {
            if let Some(ip_str) = parts.next() {
                if let Some(_ip) = parse_ip(ip_str) {
                    // Actual kernel ping prints natively in the kernel.
                    // But here we use sys_net_ping which returns a reply string.
                    let mut reply_buf = [0u8; 256];
                    // The old code prints via net::ping. Let's try to simulate.
                    // Actually, if we just call sys_net_ping, it does the work.
                    // Let's match the old output which just did the ping and didn't print stats.
                    let mut ip = [0u8; 4];
                    if let Some(i) = parse_ip(ip_str) { ip = i; }
                    let _ = sdk::sys_net_ping(&ip, &mut reply_buf);
                    // It seems the kernel sys_net_ping already might print, or we should print.
                    // Let's just output what's in reply_buf
                    if let Ok(s) = core::str::from_utf8(&reply_buf) {
                        let msg = s.trim_end_matches('\0');
                        if !msg.is_empty() {
                            print!("{}", msg);
                        }
                    }
                } else {
                    print_err("Invalid IP address");
                }
            } else {
                print!("Usage: ping <ip>\n");
            }
        }
        "httpget" => {
            if let Some(ip_str) = parts.next() {
                if let Some(ip) = parse_ip(ip_str) {
                    let second = parts.next();
                    let (port, path) = match second {
                        Some(s) => match parse_u32(s) {
                            Some(p) => (p as u16, parts.next().unwrap_or("/")),
                            None => (80u16, s),
                        },
                        None => (80u16, "/"),
                    };

                    let mut out_buf = alloc::vec![0u8; 8192];
                    let _ = sdk::sys_net_httpget(&ip, port, path, &mut out_buf);
                    if let Ok(s) = core::str::from_utf8(&out_buf) {
                        let msg = s.trim_end_matches('\0');
                        if !msg.is_empty() {
                            print!("{}", msg);
                        }
                    }
                } else {
                    print_err("Invalid IP address");
                }
            } else {
                print!("Usage: httpget <ip> [port] [path]\n");
            }
        }
        _ => {
            print!("\x1b[91mError: Unknown command: {}. Type 'help'.\n\x1b[97m", cmd);
        }
    }
}

// ============================================================================
// Helper functions
// ============================================================================

fn print_err(msg: &str) {
    print!("\x1b[91mError: {}\n\x1b[97m", msg);
}

fn nic_name(nic_type: u32) -> &'static str {
    match nic_type {
        1 => "Intel PRO/1000",
        2 => "Realtek RTL8139",
        _ => "None",
    }
}

fn parse_ip(s: &str) -> Option<[u8; 4]> {
    let mut ip = [0u8; 4];
    let mut idx = 0usize;
    for part in s.split('.') {
        if idx >= 4 { return None; }
        let mut n: u16 = 0;
        if part.is_empty() { return None; }
        for c in part.bytes() {
            if !(b'0'..=b'9').contains(&c) { return None; }
            n = n * 10 + (c - b'0') as u16;
            if n > 255 { return None; }
        }
        ip[idx] = n as u8;
        idx += 1;
    }
    if idx == 4 { Some(ip) } else { None }
}

fn parse_u32(s: &str) -> Option<u32> {
    let mut n: u64 = 0;
    if s.is_empty() { return None; }
    for b in s.bytes() {
        if !(b'0'..=b'9').contains(&b) { return None; }
        n = n * 10 + (b - b'0') as u64;
        if n > u32::MAX as u64 { return None; }
    }
    Some(n as u32)
}

fn parse_u64(s: &str) -> Option<u64> {
    let mut n: u128 = 0;
    if s.is_empty() { return None; }
    for b in s.bytes() {
        if !(b'0'..=b'9').contains(&b) { return None; }
        n = n * 10 + (b - b'0') as u128;
        if n > u64::MAX as u128 { return None; }
    }
    Some(n as u64)
}

fn print_ip(ip: &[u8; 4]) {
    print!("{}.{}.{}.{}", ip[0], ip[1], ip[2], ip[3]);
}

fn print_mac(mac: &[u8; 6]) {
    for (i, &b) in mac.iter().enumerate() {
        if i > 0 { print!(":"); }
        print!("{:02x}", b);
    }
}

fn scancode_to_char(scancode: u8) -> Option<char> {
    match scancode {
        0x02 => Some('1'), 0x03 => Some('2'), 0x04 => Some('3'), 0x05 => Some('4'),
        0x06 => Some('5'), 0x07 => Some('6'), 0x08 => Some('7'), 0x09 => Some('8'),
        0x0A => Some('9'), 0x0B => Some('0'), 0x0C => Some('-'), 0x0D => Some('='),
        0x0E => Some('\x08'), // Backspace
        0x0F => Some('\t'),
        0x10 => Some('q'), 0x11 => Some('w'), 0x12 => Some('e'), 0x13 => Some('r'),
        0x14 => Some('t'), 0x15 => Some('y'), 0x16 => Some('u'), 0x17 => Some('i'),
        0x18 => Some('o'), 0x19 => Some('p'), 0x1A => Some('['), 0x1B => Some(']'),
        0x1C => Some('\n'), // Enter
        0x1E => Some('a'), 0x1F => Some('s'), 0x20 => Some('d'), 0x21 => Some('f'),
        0x22 => Some('g'), 0x23 => Some('h'), 0x24 => Some('j'), 0x25 => Some('k'),
        0x26 => Some('l'), 0x27 => Some(';'), 0x28 => Some('\''), 0x29 => Some('`'),
        0x2B => Some('\\'), 0x2C => Some('z'), 0x2D => Some('x'), 0x2E => Some('c'),
        0x2F => Some('v'), 0x30 => Some('b'), 0x31 => Some('n'), 0x32 => Some('m'),
        0x33 => Some(','), 0x34 => Some('.'), 0x35 => Some('/'), 0x39 => Some(' '),
        _ => None,
    }
}
