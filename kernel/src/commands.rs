// Copyright (C) 2026 Pronin. All rights reserved.

use crate::vga::{VgaWriter, Color};
use crate::ramfs::NodeType;
use crate::shell::{Shell, ShellState};
use crate::net;

pub fn execute_command(input: &str, writer: &mut VgaWriter, shell: &mut Shell) {
    let mut parts = input.split_whitespace();
    let cmd = parts.next().unwrap_or("");

    match cmd {
        "help" => {
            writer.write_string("Available commands:\n");
            writer.write_string("  help          Show this help\n");
            writer.write_string("  clear         Clear screen\n");
            writer.write_string("  stat          System info\n");
            writer.write_string("  edit <name>   Text editor (ESC to save)\n");
            writer.write_string("  mousetest     Test PS/2 mouse driver\n");
            writer.write_string("  whoami        Current user\n");
            writer.write_string("  su <user>     Switch user\n");
            writer.write_string("  ifconfig      Network interfaces\n");
            writer.write_string("  ping <ip>     ICMP echo (real)\n");
            writer.write_string("  setdns <ip>   Set global DNS server\n");
            writer.write_string("  pronfetch     OS Information\n");
            writer.write_string("  iumstart <p>  Execute .ium binary\n");
            writer.write_string("  httpget <ip> [port] [path]  HTTP GET\n");
            writer.write_string("  beep <hz> <ms> Play sound via speaker server (IPC)\n");
            writer.write_string("  cr <W>x<H>    Change resolution (1024x768 to 1920x1080)\n");
            writer.set_color(Color::LightCyan, Color::Black);
            writer.write_string("--- PSD Tools (Pronin Software Distribution) ---\n");
            writer.set_color(Color::White, Color::Black);
            writer.write_string("  ls, mkdir, touch, write, cat, echo, cp, mv, rm, rmdir\n");
            writer.write_string("  df, du, head, tail, wc, md5sum, sha256sum, printf\n");
            writer.write_string("  pwd, uname, uptime, date\n");
        }
        "beep" => {
            let hz_str = parts.next().unwrap_or("440");
            let ms_str = parts.next().unwrap_or("500");
            
            let hz = hz_str.parse::<u32>().unwrap_or(440);
            let ms = ms_str.parse::<u64>().unwrap_or(500);
            
            let port = crate::drivers::speaker::SPEAKER_SERVER_PORT.call_once(|| crate::port::Port::new(32));
            let _ = port.write_port(0x4001, &hz.to_le_bytes());
            
            let start = crate::syscall::nt_get_tick_count();
            while crate::syscall::nt_get_tick_count() - start < ms {
                crate::task::yield_now();
            }
            
            let _ = port.write_port(0x4002, &[]);
            crate::task::yield_now();
        }
        "cr" => {
            if let Some(res_str) = parts.next() {
                let mut dims = res_str.split('x');
                let width_str = dims.next();
                let height_str = dims.next();
                if let (Some(w_str), Some(h_str)) = (width_str, height_str) {
                    if let (Ok(w), Ok(h)) = (w_str.parse::<usize>(), h_str.parse::<usize>()) {
                        if w < 1024 || w > 1920 || h < 768 || h > 1080 {
                            print_err(writer, "Invalid resolution");
                        } else {
                            let offset = unsafe { *(&raw const crate::PHYS_MEM_OFFSET) };
                            if crate::drivers::bochs_vga::init_bochs_vga(offset, w as u16, h as u16).is_some() {
                                unsafe { crate::MONITOR_INFO = (w, h); }
                                writer.update_scale_and_clear();
                                writer.set_color(Color::LightGreen, Color::Black);
                                writer.write_string("Resolution changed successfully!\n");
                                writer.set_color(Color::White, Color::Black);
                            } else {
                                print_err(writer, "Failed to change resolution via Bochs VBE");
                            }
                        }
                    } else {
                        print_err(writer, "Invalid resolution format. Use WxH (e.g. 1024x768)");
                    }
                } else {
                    print_err(writer, "Invalid resolution format. Use WxH (e.g. 1024x768)");
                }
            } else {
                writer.write_string("Usage: cr <width>x<height>\n");
            }
        }
        "clear" => {
            writer.clear_screen();
        }
        "mousetest" => {
            writer.set_color(Color::LightCyan, Color::Black);
            writer.write_string("Testing PS/2 Mouse Driver. Press ANY key to exit.\n");
            writer.set_color(Color::White, Color::Black);

            let mut mouse_buf = [0u8; 3];
            let mut mouse_idx = 0;

            loop {
                // Check keyboard
                if crate::drivers::keyboard::poll_scancode().is_some() {
                    break;
                }

                // Check mouse
                let mut b = None;
                {
                    let lock_guard = crate::drivers::ps2_mouse::MOUSE_DEV.lock();
                    if let Some(ref dev) = *lock_guard {
                        if let Some(mut locked_dev) = dev.try_lock() {
                            let mut tmp = [0u8; 1];
                            use crate::pos::PosObject;
                            if let Ok(1) = locked_dev.read(0, &mut tmp) {
                                b = Some(tmp[0]);
                            }
                        }
                    }
                }

                if let Some(byte) = b {
                    mouse_buf[mouse_idx] = byte;
                    mouse_idx += 1;
                    if mouse_idx == 3 {
                        let flags = mouse_buf[0];
                        let dx = mouse_buf[1] as i8 as i32;
                        let dy = mouse_buf[2] as i8 as i32;
                        let left = (flags & 1) != 0;
                        let right = (flags & 2) != 0;
                        let middle = (flags & 4) != 0;
                        
                        writer.write_string(&alloc::format!("Mouse: dx={}, dy={}, L={}, R={}, M={}\n", dx, dy, left, right, middle));
                        mouse_idx = 0;
                    }
                } else {
                    crate::task::yield_now();
                }
            }
            writer.write_string("Exited mouse test.\n");
        }
        "stat" => {
            writer.set_color(Color::LightCyan, Color::Black);
            writer.write_string("OS: Pronium OS [Proprietary Core]\n");
            writer.write_string("Status: Proprietary\n");
            writer.write_string("RamFS Nodes: ");
            print_usize(writer, crate::RAM_FS.lock().occupied_nodes);
            writer.write_string("/64\n");
            let has_nic = unsafe {
                let ptr = &raw const crate::NIC;
                (*ptr).is_some()
            };
            writer.write_string("NIC: ");
            writer.write_string(if has_nic { "UP" } else { "none" });
            writer.write_string("\n");
            writer.set_color(Color::White, Color::Black);
        }
        "pronfetch" => {
            let uptime_secs = unsafe {
                let ptr = &raw const crate::interrupts::TICKS;
                *ptr / 1000
            };
            
            let nic_name = unsafe {
                let ptr = &raw const crate::NIC;
                match &*ptr {
                    Some(crate::net::NetworkCard::E1000(_)) => "Intel PRO/1000",
                    Some(crate::net::NetworkCard::Rtl8139(_)) => "Realtek RTL8139",
                    None => "None",
                }
            };

            let ram_mb = unsafe { crate::TOTAL_RAM_MB };
            let (mon_w, mon_h) = unsafe { crate::MONITOR_INFO };
            
            let mut pci_dev_count = 0;
            for bus in 0u16..=255 {
                for dev in 0..32 {
                    if crate::drivers::pci::check_device(bus as u8, dev).is_some() {
                        pci_dev_count += 1;
                    }
                }
            }

            writer.set_color(Color::LightCyan, Color::Black);
            writer.write_string("               @@@@@@@@@@@@@@@     ");
            writer.set_color(Color::White, Color::Black);
            writer.write_string("OS: Pronium OS\n");
            
            writer.set_color(Color::LightCyan, Color::Black);
            writer.write_string("                            @@@    ");
            writer.set_color(Color::White, Color::Black);
            writer.write_string("Build: 260726\n");
            
            writer.set_color(Color::LightCyan, Color::Black);
            writer.write_string("                            @@@    ");
            writer.set_color(Color::White, Color::Black);
            writer.write_string("Uptime: ");
            print_usize(writer, uptime_secs as usize);
            writer.write_string("s\n");
            
            writer.set_color(Color::LightCyan, Color::Black);
            writer.write_string("               @@@@@@@@@@@@@@@@    ");
            writer.set_color(Color::White, Color::Black);
            writer.write_string("RAM (Usable): ");
            print_usize(writer, ram_mb);
            writer.write_string(" MB\n");
            
            writer.set_color(Color::LightCyan, Color::Black);
            writer.write_string("               @@@@@@@@@@@@@@      ");
            writer.set_color(Color::White, Color::Black);
            writer.write_string("NIC: ");
            writer.write_string(nic_name);
            writer.write_string("\n");

            writer.set_color(Color::LightCyan, Color::Black);
            writer.write_string("               @@@                 ");
            writer.set_color(Color::White, Color::Black);
            writer.write_string("Devices: ");
            print_usize(writer, pci_dev_count);
            writer.write_string(" PCI\n");

            writer.set_color(Color::LightCyan, Color::Black);
            writer.write_string("               @@@                 ");
            writer.set_color(Color::White, Color::Black);
            let (mon_w, mon_h) = unsafe { crate::MONITOR_INFO };
            writer.write_string("Monitor: VGA Text Mode ");
            print_usize(writer, mon_w);
            writer.write_string("x");
            print_usize(writer, mon_h);
            writer.write_string("\n");
        }
        "edit" => {
            if let Some(path) = parts.next() {
                if path.starts_with("/ram") {
                    let name = path.trim_start_matches("/ram/").trim_start_matches("/ram");
                    let index = if let Some(idx) = crate::RAM_FS.lock().find_node(name) {
                        idx
                    } else {
                        match crate::RAM_FS.lock().add_node(name, NodeType::File) {
                            Ok(idx) => idx,
                            Err(e) => {
                                print_err(writer, e);
                                return;
                            }
                        }
                    };

                    if crate::RAM_FS.lock().nodes[index].node_type != NodeType::File {
                        print_err(writer, "Not a file");
                        return;
                    }

                    writer.clear_screen();
                    writer.set_color(Color::Cyan, Color::Black);
                    writer.write_string("--- Pronium Editor (ESC to save) ---\n");
                    writer.set_color(Color::White, Color::Black);

                    let node = &crate::RAM_FS.lock().nodes[index];
                    if let Ok(s) = core::str::from_utf8(&node.data[..node.data_len]) {
                        writer.write_string(s);
                    }
                    shell.state = ShellState::Editor { node_index: index };
                } else {
                    let mut fat_lock = unsafe { crate::FAT_FS.lock() };
                    if let Some(fat) = &mut *fat_lock {
                        let mut data_buf = [0u8; 4096];
                        let mut data_len = 0;
                        
                        // Try to read existing file
                        if let Ok(Some(entry)) = fat.find_entry(fat.root_cluster, path) {
                            if !entry.is_dir() {
                                if let Ok(bytes_read) = fat.read_file_offset(entry.first_cluster(), entry.file_size as usize, 0, &mut data_buf) {
                                    data_len = bytes_read;
                                }
                            }
                        }

                        let mut path_buf = [0u8; 32];
                        let path_len = core::cmp::min(path.len(), 32);
                        path_buf[..path_len].copy_from_slice(path.as_bytes());

                        writer.clear_screen();
                        writer.set_color(Color::Cyan, Color::Black);
                        writer.write_string("--- Pronium FAT32 Editor (ESC to save) ---\n");
                        writer.set_color(Color::White, Color::Black);

                        if let Ok(s) = core::str::from_utf8(&data_buf[..data_len]) {
                            writer.write_string(s);
                        }

                        shell.state = ShellState::FatEditor {
                            path: path_buf,
                            path_len,
                            data: data_buf,
                            data_len,
                        };
                    } else {
                        print_err(writer, "FAT32 not mounted");
                    }
                }
            } else {
                writer.write_string("Usage: edit <path>\n");
            }
        }
        "iumstart" => {
            if let Some(path) = parts.next() {
                let mut fat_lock = unsafe { crate::FAT_FS.lock() };
                if let Some(fat) = &mut *fat_lock {
                    match fat.find_entry(fat.root_cluster, path) {
                        Ok(Some(entry)) => {
                            if entry.is_dir() {
                                print_err(writer, "Cannot execute a directory");
                            } else {
                                let mut data = alloc::vec![0u8; entry.file_size as usize];
                                match fat.read_file_offset(entry.first_cluster(), entry.file_size as usize, 0, &mut data) {
                                    Ok(_) => {
                                        writer.write_string("Executing ");
                                        writer.write_string(path);
                                        writer.write_string(" (");
                                        print_usize(writer, data.len());
                                        writer.write_string(" bytes)...\n");
                                        
                                        if data.len() >= core::mem::size_of::<crate::ium::IumHeader>() {
                                            let header_ptr = data.as_ptr() as *const crate::ium::IumHeader;
                                            let header = unsafe { &*header_ptr };
                                            if header.is_valid() {
                                                writer.set_color(Color::LightGreen, Color::Black);
                                                writer.write_string("Valid IUM binary detected! Jumping to entry point...\n");
                                                writer.set_color(Color::White, Color::Black);
                                                
                                                let entry_offset = header.entry_point as usize;
                                                let payload = &data[core::mem::size_of::<crate::ium::IumHeader>()..];
                                                
                                                // Map the binary payload at the link address 0x400000
                                                const USER_CODE_BASE: u64 = 0x400000;
                                                const USER_STACK_BASE: u64 = 0x800000;
                                                const USER_STACK_SIZE: u64 = 65536;
                                                
                                                // Allocate backing memory (leaked heap) and copy payload
                                                let code_backing = alloc::vec![0u8; payload.len()].into_boxed_slice();
                                                let code_backing = alloc::boxed::Box::leak(code_backing);
                                                code_backing[..payload.len()].copy_from_slice(payload);
                                                
                                                let stack_backing = alloc::vec![0u8; USER_STACK_SIZE as usize].into_boxed_slice();
                                                let stack_backing = alloc::boxed::Box::leak(stack_backing);
                                                
                                                // Map code at USER_CODE_BASE and stack at USER_STACK_BASE
                                                map_user_pages(
                                                    USER_CODE_BASE,
                                                    code_backing.as_ptr() as u64,
                                                    code_backing.len(),
                                                    true, // executable
                                                );
                                                map_user_pages(
                                                    USER_STACK_BASE,
                                                    stack_backing.as_ptr() as u64,
                                                    stack_backing.len(),
                                                    false, // not executable, just writable
                                                );
                                                
                                                let user_entry = USER_CODE_BASE + entry_offset as u64;
                                                let user_stack_top = USER_STACK_BASE + USER_STACK_SIZE;

                                                unsafe {
                                                    crate::syscall::IUM_CODE_PTR = user_entry;
                                                    crate::syscall::IUM_STACK_PTR = user_stack_top;
                                                }

                                                crate::task::SCHEDULER.lock().spawn(crate::syscall::ium_trampoline);
                                                
                                                writer.set_color(Color::LightGreen, Color::Black);
                                                writer.write_string("Process spawned in Ring 3!\n");
                                                writer.set_color(Color::White, Color::Black);
                                            } else {
                                                print_err(writer, "Invalid IUM magic bytes");
                                            }
                                        } else {
                                            print_err(writer, "File too small to be a valid IUM binary");
                                        }
                                    }
                                    Err(_) => print_err(writer, "Failed to read binary data from FAT32"),
                                }
                            }
                        }
                        Ok(None) => print_err(writer, "File not found on FAT32"),
                        Err(_) => print_err(writer, "FAT32 read error"),
                    }
                } else {
                    print_err(writer, "FAT32 not mounted");
                }
            } else {
                writer.write_string("Usage: iumstart <path>\n");
            }
        }
        "whoami" => {
            writer.write_string(shell.current_user);
            writer.write_string("\n");
        }
        "su" => {
            if let Some(name) = parts.next() {
                match name {
                    "root" => shell.current_user = "root",
                    "pronin" => shell.current_user = "pronin",
                    _ => shell.current_user = "guest",
                }
            } else {
                writer.write_string("Usage: su <user>\n");
            }
        }
        "setdns" => {
            if let Some(ip_str) = parts.next() {
                match net::parse_ip(ip_str) {
                    Some(ip) => {
                        net::set_dns(ip);
                        writer.write_string("DNS server set to ");
                        net::print_ip(writer, ip);
                        writer.write_string("\n");
                    }
                    None => print_err(writer, "Invalid IP address"),
                }
            } else {
                writer.write_string("Usage: setdns <ip>\n");
            }
        }
        "ifconfig" => {
            let nic_ref = unsafe {
                let ptr = &raw const crate::NIC;
                &*ptr
            };
            match nic_ref {
                Some(nic) => {
                    writer.set_color(Color::LightCyan, Color::Black);
                    writer.write_string("eth0: flags=UP\n");
                    writer.write_string("      ether ");
                    net::print_mac(writer, nic.mac());
                    writer.write_string("\n      inet  ");
                    net::print_ip(writer, net::OUR_IP);
                    writer.write_string("\n      mask  ");
                    net::print_ip(writer, net::SUBNET_MASK);
                    writer.write_string("\n      gw    ");
                    net::print_ip(writer, net::GATEWAY_IP);
                    writer.write_string("\n      dns   ");
                    net::print_ip(writer, net::get_dns());
                    writer.write_string("\n");
                    writer.set_color(Color::White, Color::Black);
                }
                None => {
                    print_err(writer, "No network interface");
                }
            }
        }
        "ping" => {
            if let Some(ip_str) = parts.next() {
                match net::parse_ip(ip_str) {
                    Some(target_ip) => {
                        let nic_mut = unsafe {
                            let ptr = &raw mut crate::NIC;
                            &mut *ptr
                        };
                        let offset = unsafe {
                            let ptr = &raw const crate::PHYS_MEM_OFFSET;
                            *ptr
                        };
                        match nic_mut {
                            Some(nic) => {
                                net::ping(nic, target_ip, offset, writer);
                            }
                            None => {
                                print_err(writer, "No network interface");
                            }
                        }
                    }
                    None => {
                        print_err(writer, "Invalid IP address");
                    }
                }
            } else {
                writer.write_string("Usage: ping <ip>\n");
            }
        }
        "httpget" => {
            if let Some(ip_str) = parts.next() {
                match net::parse_ip(ip_str) {
                    Some(target_ip) => {
                        let second = parts.next();
                        let (port, path) = match second {
                            Some(s) => match parse_u16(s) {
                                Some(p) => (p, parts.next().unwrap_or("/")),
                                None => (80u16, s),
                            },
                            None => (80u16, "/"),
                        };

                        let mut req = [0u8; 512];
                        let mut p = 0;
                        for chunk in [
                            b"GET " as &[u8], path.as_bytes(),
                            b" HTTP/1.0\r\nHost: " as &[u8], ip_str.as_bytes(),
                            b"\r\nConnection: close\r\n\r\n" as &[u8],
                        ] {
                            req[p..p + chunk.len()].copy_from_slice(chunk);
                            p += chunk.len();
                        }

                        let nic_mut = unsafe { &mut *(&raw mut crate::NIC) };
                        let offset = unsafe { *(&raw const crate::PHYS_MEM_OFFSET) };

                        match nic_mut {
                            Some(nic) => {
                                net::tcp_exchange(nic, target_ip, port, &req[..p], offset, writer);
                            }
                            None => print_err(writer, "No network interface"),
                        }
                    }
                    None => print_err(writer, "Invalid IP address"),
                }
            } else {
                writer.write_string("Usage: httpget <ip> [port] [path]\n");
            }
        }
        _ => {
            // First check if it's a PSD-Tools command
            if !crate::psd_tools::handle_psd_tool(input, cmd, parts.clone(), writer, shell) {
                writer.set_color(Color::LightRed, Color::Black);
                writer.write_string("Unknown command: ");
                writer.write_string(cmd);
                writer.write_string(". Type 'help'.\n");
                writer.set_color(Color::White, Color::Black);
            }
        }
    }
}

/// Maps `size` bytes of memory backed by heap at `backing_virt_addr`
/// to user-accessible virtual addresses starting at `target_virt_addr`.
/// The backing memory must already be allocated in the kernel heap.
fn map_user_pages(target_virt_addr: u64, backing_virt_addr: u64, size: usize, executable: bool) {
    if size == 0 {
        return;
    }
    use x86_64::structures::paging::{PageTable, PageTableFlags, PhysFrame, FrameAllocator, Size4KiB};
    use x86_64::structures::paging::{OffsetPageTable, Mapper, Page};
    use x86_64::registers::control::Cr3;
    use x86_64::{VirtAddr, PhysAddr};

    unsafe {
        let phys_offset = *(&raw const crate::PHYS_MEM_OFFSET);
        let (l4_frame, _) = Cr3::read();
        let l4_table = &mut *((l4_frame.start_address().as_u64() + phys_offset) as *mut PageTable);
        let mut mapper = OffsetPageTable::new(l4_table, VirtAddr::new(phys_offset));

        let mut offset: usize = 0;
        while offset < size {
            // Get the physical address of the backing page
            let backing_page_virt = VirtAddr::new((backing_virt_addr + offset as u64) & !0xFFF);
            
            // Translate backing virtual address to physical
            use x86_64::structures::paging::Translate;
            let phys_addr = match mapper.translate_addr(backing_page_virt) {
                Some(pa) => pa,
                None => { offset += 4096; continue; }
            };
            
            // Map target virtual page to this physical frame
            let target_page = Page::<Size4KiB>::containing_address(
                VirtAddr::new(target_virt_addr + offset as u64)
            );
            let frame = PhysFrame::containing_address(phys_addr);
            
            let mut flags = PageTableFlags::PRESENT
                | PageTableFlags::WRITABLE
                | PageTableFlags::USER_ACCESSIBLE;
            if !executable {
                flags |= PageTableFlags::NO_EXECUTE;
            }
            
            // Use a simple frame allocator for new page table pages
            let result = mapper.map_to(
                target_page,
                frame,
                flags,
                &mut HeapFrameAllocator,
            );
            if let Ok(flusher) = result {
                flusher.flush();
            }
            
            offset += 4096;
        }

        // Also set USER_ACCESSIBLE on all intermediate page table levels
        // (map_to sets it on the leaf, but intermediate levels need it too)
        let start = target_virt_addr & !0xFFF;
        let end = target_virt_addr + size as u64 - 1;
        let mut current = start;
        while current <= end {
            let virt = VirtAddr::new(current);
            let l4e = &mut l4_table[virt.p4_index()];
            if !l4e.is_unused() {
                let mut f = l4e.flags();
                f.insert(PageTableFlags::USER_ACCESSIBLE);
                l4e.set_flags(f);

                let l3_table = &mut *((l4e.addr().as_u64() + phys_offset) as *mut PageTable);
                let l3e = &mut l3_table[virt.p3_index()];
                if !l3e.is_unused() && !l3e.flags().contains(PageTableFlags::HUGE_PAGE) {
                    let mut f = l3e.flags();
                    f.insert(PageTableFlags::USER_ACCESSIBLE);
                    l3e.set_flags(f);

                    let l2_table = &mut *((l3e.addr().as_u64() + phys_offset) as *mut PageTable);
                    let l2e = &mut l2_table[virt.p2_index()];
                    if !l2e.is_unused() && !l2e.flags().contains(PageTableFlags::HUGE_PAGE) {
                        let mut f = l2e.flags();
                        f.insert(PageTableFlags::USER_ACCESSIBLE);
                        l2e.set_flags(f);
                    }
                }
            }
            current += 4096;
            if current == 0 { break; }
        }

        x86_64::instructions::tlb::flush_all();
    }
}

/// Simple frame allocator that uses the kernel heap to allocate page table frames.
struct HeapFrameAllocator;

unsafe impl x86_64::structures::paging::FrameAllocator<x86_64::structures::paging::Size4KiB>
    for HeapFrameAllocator
{
    fn allocate_frame(&mut self) -> Option<x86_64::structures::paging::PhysFrame> {
        use x86_64::structures::paging::{PhysFrame, Size4KiB};
        use x86_64::{PhysAddr, VirtAddr};
        use x86_64::structures::paging::{OffsetPageTable, Translate};

        // Allocate a 4KiB-aligned page from the heap
        let layout = core::alloc::Layout::from_size_align(4096, 4096).ok()?;
        let ptr = unsafe { alloc::alloc::alloc_zeroed(layout) };
        if ptr.is_null() {
            return None;
        }

        // Translate the heap virtual address to physical
        unsafe {
            let phys_offset = *(&raw const crate::PHYS_MEM_OFFSET);
            let (l4_frame, _) = x86_64::registers::control::Cr3::read();
            let l4_table = &mut *((l4_frame.start_address().as_u64() + phys_offset)
                as *mut x86_64::structures::paging::PageTable);
            let mapper = OffsetPageTable::new(l4_table, VirtAddr::new(phys_offset));

            let virt = VirtAddr::new(ptr as u64);
            mapper
                .translate_addr(virt)
                .map(|pa| PhysFrame::containing_address(pa))
        }
    }
}


fn print_err(w: &mut VgaWriter, msg: &str) {
    w.set_color(Color::LightRed, Color::Black);
    w.write_string("Error: ");
    w.write_string(msg);
    w.write_string("\n");
    w.set_color(Color::White, Color::Black);
}

fn print_usize(w: &mut VgaWriter, mut n: usize) {
    if n == 0 {
        w.write_byte(b'0');
        return;
    }
    let mut buf = [0u8; 20];
    let mut i = 0;
    while n > 0 {
        buf[i] = (n % 10) as u8 + b'0';
        n /= 10;
        i += 1;
    }
    while i > 0 {
        i -= 1;
        w.write_byte(buf[i]);
    }
}

fn parse_u16(s: &str) -> Option<u16> {
    let mut n: u32 = 0;
    if s.is_empty() {
        return None;
    }
    for b in s.bytes() {
        if !(b'0'..=b'9').contains(&b) {
            return None;
        }
        n = n * 10 + (b - b'0') as u32;
        if n > 65535 {
            return None;
        }
    }
    Some(n as u16)
}
