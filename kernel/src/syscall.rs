// Copyright (C) 2026 Pronin. All rights reserved.
// Kernel syscall dispatcher — all Ring-3 system calls are routed here.



use crate::pos::{HandleTable, PosError};
use spin::Mutex;

pub static HANDLE_TABLE: Mutex<HandleTable> = Mutex::new(HandleTable::new());

pub type Handle = usize;

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NtStatus {
    Success = 0,
    InvalidHandle = 1,
    AccessDenied = 2,
    NotFound = 3,
    NotSupported = 4,
    IoError = 5,
    InvalidParameter = 6,
    InvalidCommand = 7,
}

impl NtStatus {
    pub fn is_success(&self) -> bool {
        matches!(self, NtStatus::Success)
    }
}

impl From<PosError> for NtStatus {
    fn from(err: PosError) -> Self {
        match err {
            PosError::InvalidHandle => NtStatus::InvalidHandle,
            PosError::AccessDenied => NtStatus::AccessDenied,
            PosError::NotFound => NtStatus::NotFound,
            PosError::NotSupported => NtStatus::NotSupported,
            PosError::InvalidParameter => NtStatus::InvalidParameter,
            PosError::InvalidCommand => NtStatus::InvalidCommand,
            PosError::IoError(_) => NtStatus::IoError,
        }
    }
}

pub fn NtOpenFile(path: &str, out_handle: &mut Handle) -> NtStatus {
    let mut table = HANDLE_TABLE.lock();
    match table.open(path) {
        Ok(h) => {
            *out_handle = h;
            NtStatus::Success
        }
        Err(e) => e.into(),
    }
}

pub fn NtClose(handle: Handle) -> NtStatus {
    let mut table = HANDLE_TABLE.lock();
    match table.close(handle) {
        Ok(_) => NtStatus::Success,
        Err(e) => e.into(),
    }
}

pub fn NtReadFile(handle: Handle, buffer: &mut [u8], offset: u64, bytes_read: &mut usize) -> NtStatus {
    let table = HANDLE_TABLE.lock();
    match table.get_object(handle) {
        Ok(obj) => {
            let mut obj_lock = obj.lock();
            match obj_lock.read(offset, buffer) {
                Ok(read) => {
                    *bytes_read = read;
                    NtStatus::Success
                }
                Err(e) => e.into(),
            }
        }
        Err(e) => e.into(),
    }
}

pub fn NtWriteFile(handle: Handle, buffer: &[u8], offset: u64, bytes_written: &mut usize) -> NtStatus {
    let table = HANDLE_TABLE.lock();
    match table.get_object(handle) {
        Ok(obj) => {
            let mut obj_lock = obj.lock();
            match obj_lock.write(offset, buffer) {
                Ok(written) => {
                    *bytes_written = written;
                    NtStatus::Success
                }
                Err(e) => e.into(),
            }
        }
        Err(e) => e.into(),
    }
}

pub fn NtDeviceIoControlFile(handle: Handle, io_control_code: u32, arg: usize) -> NtStatus {
    let table = HANDLE_TABLE.lock();
    match table.get_object(handle) {
        Ok(obj) => {
            let mut obj_lock = obj.lock();
            match obj_lock.ioctl(io_control_code, arg) {
                Ok(_) => NtStatus::Success,
                Err(e) => e.into(),
            }
        }
        Err(e) => e.into(),
    }
}


pub fn nt_get_tick_count() -> u64 {
    unsafe { core::ptr::read_volatile(&raw const crate::interrupts::TICKS) }
}

// ---------------------------------------------------------------------------
// sys_get_stat (syscall 10)
// Fills a SysStatInfo struct at the pointer provided by arg1.
// ---------------------------------------------------------------------------
#[repr(C)]
pub struct SysStatInfo {
    pub ram_mb: u64,
    pub monitor_w: u64,
    pub monitor_h: u64,
    pub uptime_secs: u64,
    pub pci_dev_count: u64,
    /// NIC type: 0=None, 1=Intel E1000, 2=Realtek RTL8139
    pub nic_type: u32,
    pub padding: u32,
}

pub fn sys_get_stat(out_ptr: u64) -> u64 {
    if out_ptr == 0 {
        return NtStatus::InvalidParameter as u64;
    }
    let uptime_secs = unsafe {
        let ptr = &raw const crate::interrupts::TICKS;
        *ptr / 1000
    };
    let ram_mb = unsafe { crate::TOTAL_RAM_MB } as u64;
    let (mon_w, mon_h) = unsafe { crate::MONITOR_INFO };
    let nic_type: u32 = unsafe {
        let ptr = &raw const crate::NIC;
        match &*ptr {
            Some(crate::net::NetworkCard::E1000(_)) => 1,
            Some(crate::net::NetworkCard::Rtl8139(_)) => 2,
            None => 0,
        }
    };
    let mut pci_dev_count: u64 = 0;
    for bus in 0u8..=255 {
        for dev in 0..32u8 {
            if crate::drivers::pci::check_device(bus, dev).is_some() {
                pci_dev_count += 1;
            }
        }
    }
    let info = SysStatInfo {
        ram_mb,
        monitor_w: mon_w as u64,
        monitor_h: mon_h as u64,
        uptime_secs,
        pci_dev_count,
        nic_type,
        padding: 0,
    };
    unsafe {
        core::ptr::write_volatile(out_ptr as *mut SysStatInfo, info);
    }
    NtStatus::Success as u64
}

// ---------------------------------------------------------------------------
// sys_net_ping (syscall 11)
// arg1 = pointer to [u8; 4] target IP, arg2 = pointer to output buf, arg3 = buf len
// Returns 0 on success (reply received), non-zero on error.
// ---------------------------------------------------------------------------
pub fn sys_net_ping(ip_ptr: u64, out_buf: u64, out_len: u64) -> u64 {
    if ip_ptr == 0 { return NtStatus::InvalidParameter as u64; }
    let target_ip: [u8; 4] = unsafe { core::ptr::read(ip_ptr as *const [u8; 4]) };
    let nic_mut = unsafe { &mut *(&raw mut crate::NIC) };
    let offset = unsafe { *(&raw const crate::PHYS_MEM_OFFSET) };

    match nic_mut {
        Some(nic) => {
            // Send ICMP echo via the existing ping infrastructure (write result to a temp VgaWriter-like buffer)
            // We reuse the kernel net::ping but capture output via a simple byte buffer writer
            let result = net_ping_to_buf(nic, target_ip, offset, out_buf, out_len);
            result
        }
        None => NtStatus::IoError as u64,
    }
}

fn net_ping_to_buf(
    nic: &mut crate::net::NetworkCard,
    target_ip: [u8; 4],
    offset: u64,
    out_buf: u64,
    out_len: u64,
) -> u64 {
    // We use a BufWriter-like approach: build the ICMP reply string into the provided buffer
    struct BufWriter {
        ptr: *mut u8,
        len: usize,
        pos: usize,
    }
    impl BufWriter {
        fn write(&mut self, s: &str) {
            let bytes = s.as_bytes();
            let available = self.len.saturating_sub(self.pos).saturating_sub(1);
            let to_copy = bytes.len().min(available);
            if to_copy > 0 {
                unsafe {
                    core::ptr::copy_nonoverlapping(bytes.as_ptr(), self.ptr.add(self.pos), to_copy);
                }
                self.pos += to_copy;
                unsafe { *self.ptr.add(self.pos) = 0; }
            }
        }
        fn write_ip(&mut self, ip: [u8; 4]) {
            fn u8_to_str(mut n: u8, buf: &mut [u8; 3]) -> usize {
                if n == 0 { buf[0] = b'0'; return 1; }
                let mut i = 0;
                while n > 0 { buf[i] = (n % 10) + b'0'; n /= 10; i += 1; }
                buf[..i].reverse();
                i
            }
            let mut tmp = [0u8; 3];
            for (idx, &b) in ip.iter().enumerate() {
                let len = u8_to_str(b, &mut tmp);
                let s = core::str::from_utf8(&tmp[..len]).unwrap_or("?");
                self.write(s);
                if idx < 3 { self.write("."); }
            }
        }
    }

    let mut bw = BufWriter {
        ptr: out_buf as *mut u8,
        len: out_len as usize,
        pos: 0,
    };

    // Use kernel net::ping_raw that returns bool
    let ok = crate::net::ping_raw(nic, target_ip, offset);

    if ok {
        bw.write("64 bytes from ");
        bw.write_ip(target_ip);
        bw.write(": icmp_seq=1 ttl=64\n");
        NtStatus::Success as u64
    } else {
        bw.write("Request timeout for ");
        bw.write_ip(target_ip);
        bw.write("\n");
        NtStatus::IoError as u64
    }
}

// ---------------------------------------------------------------------------
// sys_net_ifconfig (syscall 12)
// arg1 = pointer to SysIfconfigInfo struct
// ---------------------------------------------------------------------------
#[repr(C)]
pub struct SysIfconfigInfo {
    pub has_nic: u8,
    pub nic_type: u8,  // 1=E1000, 2=RTL8139
    pub padding: [u8; 2],
    pub mac: [u8; 6],
    pub padding2: [u8; 2],
    pub our_ip: [u8; 4],
    pub subnet_mask: [u8; 4],
    pub gateway_ip: [u8; 4],
    pub dns_ip: [u8; 4],
}

impl SysIfconfigInfo {
    pub const fn zeroed() -> Self {
        SysIfconfigInfo {
            has_nic: 0,
            nic_type: 0,
            padding: [0; 2],
            mac: [0; 6],
            padding2: [0; 2],
            our_ip: [0; 4],
            subnet_mask: [0; 4],
            gateway_ip: [0; 4],
            dns_ip: [0; 4],
        }
    }
}

pub fn sys_net_ifconfig(out_ptr: u64) -> u64 {
    if out_ptr == 0 { return NtStatus::InvalidParameter as u64; }
    let nic_ref = unsafe { &*(&raw const crate::NIC) };
    let mut info = SysIfconfigInfo {
        has_nic: 0,
        nic_type: 0,
        padding: [0; 2],
        mac: [0; 6],
        padding2: [0; 2],
        our_ip: crate::net::OUR_IP,
        subnet_mask: crate::net::SUBNET_MASK,
        gateway_ip: crate::net::GATEWAY_IP,
        dns_ip: crate::net::get_dns(),
    };
    match nic_ref {
        Some(nic) => {
            info.has_nic = 1;
            info.mac = nic.mac();
            info.nic_type = match nic {
                crate::net::NetworkCard::E1000(_) => 1,
                crate::net::NetworkCard::Rtl8139(_) => 2,
            };
        }
        None => {}
    }
    unsafe { core::ptr::write_volatile(out_ptr as *mut SysIfconfigInfo, info); }
    NtStatus::Success as u64
}

// ---------------------------------------------------------------------------
// sys_net_setdns (syscall 13)
// arg1 = pointer to [u8; 4] IP
// ---------------------------------------------------------------------------
pub fn sys_net_setdns(ip_ptr: u64) -> u64 {
    if ip_ptr == 0 { return NtStatus::InvalidParameter as u64; }
    let ip: [u8; 4] = unsafe { core::ptr::read(ip_ptr as *const [u8; 4]) };
    crate::net::set_dns(ip);
    NtStatus::Success as u64
}

/// ---------------------------------------------------------------------------
// sys_net_httpget (syscall 14)
// arg1 = [u8;4] ip_ptr, arg2 = port (u16), arg3 = path_ptr, arg4 = path_len,
// arg5 = out_buf_ptr (userland buffer, kernel assumes 8192 bytes max)
// ---------------------------------------------------------------------------
pub fn sys_net_httpget(ip_ptr: u64, port: u64, path_ptr: u64, path_len: u64, out_buf_ptr: u64) -> u64 {
    if ip_ptr == 0 { return NtStatus::InvalidParameter as u64; }
    let target_ip: [u8; 4] = unsafe { core::ptr::read(ip_ptr as *const [u8; 4]) };
    let port16 = port as u16;
    let path = if path_ptr != 0 && path_len > 0 {
        let bytes = unsafe { core::slice::from_raw_parts(path_ptr as *const u8, path_len as usize) };
        core::str::from_utf8(bytes).unwrap_or("/")
    } else {
        "/"
    };

    // Build IP string for Host header
    fn ip_to_buf(ip: [u8; 4], buf: &mut [u8]) -> usize {
        let mut pos = 0usize;
        for (i, &byte) in ip.iter().enumerate() {
            if i > 0 { buf[pos] = b'.'; pos += 1; }
            let mut n = byte;
            let start = pos;
            if n == 0 { buf[pos] = b'0'; pos += 1; }
            else {
                let mut tmp = [0u8; 3];
                let mut len = 0;
                while n > 0 { tmp[len] = (n % 10) + b'0'; n /= 10; len += 1; }
                tmp[..len].reverse();
                buf[pos..pos + len].copy_from_slice(&tmp[..len]);
                pos += len;
            }
        }
        pos
    }

    let mut ip_str_buf = [0u8; 16];
    let ip_str_len = ip_to_buf(target_ip, &mut ip_str_buf);

    let mut req = [0u8; 512];
    let mut p = 0;
    for chunk in [
        b"GET " as &[u8],
        path.as_bytes(),
        b" HTTP/1.0\r\nHost: " as &[u8],
        &ip_str_buf[..ip_str_len],
        b"\r\nConnection: close\r\n\r\n" as &[u8],
    ] {
        let avail = req.len() - p;
        let to_copy = chunk.len().min(avail);
        req[p..p + to_copy].copy_from_slice(&chunk[..to_copy]);
        p += to_copy;
    }

    let nic_mut = unsafe { &mut *(&raw mut crate::NIC) };
    let offset = unsafe { *(&raw const crate::PHYS_MEM_OFFSET) };

    match nic_mut {
        Some(nic) => {
            let out_ptr = out_buf_ptr as *mut u8;
            let out_len = if out_ptr.is_null() { 0 } else { 8192usize };
            let ok = crate::net::tcp_exchange_raw(nic, target_ip, port16, &req[..p], offset, out_ptr, out_len);
            if ok { NtStatus::Success as u64 } else { NtStatus::IoError as u64 }
        }
        None => NtStatus::IoError as u64,
    }
}

// ---------------------------------------------------------------------------
// sys_beep (syscall 15)
// arg1 = hz (u32), arg2 = ms (u64)
// ---------------------------------------------------------------------------
pub fn sys_beep(hz: u64, ms: u64) -> u64 {
    let port = crate::drivers::speaker::SPEAKER_SERVER_PORT.call_once(|| crate::port::Port::new(32));
    let hz32 = hz as u32;
    let _ = port.write_port(0x4001, &hz32.to_le_bytes());
    let start = nt_get_tick_count();
    while nt_get_tick_count().wrapping_sub(start) < ms {
        crate::task::yield_now();
    }
    let _ = port.write_port(0x4002, &[]);
    crate::task::yield_now();
    NtStatus::Success as u64
}

// ---------------------------------------------------------------------------
// sys_change_res (syscall 16)
// arg1 = width (u16), arg2 = height (u16)
// Returns 0 on success.
// ---------------------------------------------------------------------------
pub fn sys_change_res(w: u64, h: u64) -> u64 {
    let width = w as u16;
    let height = h as u16;
    if (width as usize) < 1024 || (width as usize) > 1920
    || (height as usize) < 768 || (height as usize) > 1080
    {
        return NtStatus::InvalidParameter as u64;
    }
    let offset = unsafe { *(&raw const crate::PHYS_MEM_OFFSET) };
    if crate::drivers::bochs_vga::init_bochs_vga(offset, width, height).is_some() {
        unsafe { crate::MONITOR_INFO = (width as usize, height as usize); }
        NtStatus::Success as u64
    } else {
        NtStatus::IoError as u64
    }
}

// ---------------------------------------------------------------------------
// sys_ramfs_op (syscall 17)
// arg1 = opcode, arg2 = name_ptr, arg3 = name_len, arg4 = data_ptr, arg5 = data_len_or_outbuf_size
//
// Opcodes:
//   1 = ls (list root) -> writes null-separated names into data_ptr buffer; returns count in rax high 32
//   2 = mkdir  -> create directory node
//   3 = touch  -> create file node
//   4 = cat    -> read file contents into data_ptr
//   5 = write  -> write data_ptr bytes into file
//   6 = rm     -> remove node (not in commands.rs, but useful)
// ---------------------------------------------------------------------------
pub fn sys_ramfs_op(opcode: u64, name_ptr: u64, name_len: u64, data_ptr: u64, data_len: u64) -> u64 {
    use crate::ramfs::NodeType;

    let name = if name_ptr != 0 && name_len > 0 {
        let bytes = unsafe { core::slice::from_raw_parts(name_ptr as *const u8, name_len as usize) };
        match core::str::from_utf8(bytes) {
            Ok(s) => s,
            Err(_) => return NtStatus::InvalidParameter as u64,
        }
    } else {
        ""
    };

    match opcode {
        1 => {
            // ls: write "name\0name\0..." into data_ptr buffer; return number of entries in high32
            if data_ptr == 0 || data_len == 0 {
                return NtStatus::InvalidParameter as u64;
            }
            let buf = unsafe { core::slice::from_raw_parts_mut(data_ptr as *mut u8, data_len as usize) };
            let mut pos = 0usize;
            let mut count = 0u32;
            let fs = crate::RAM_FS.lock();
            for i in 1..64usize {
                let node = &fs.nodes[i];
                if node.in_use && node.parent_id == Some(0) {
                    if let Ok(s) = core::str::from_utf8(&node.name[..node.name_len]) {
                        let is_dir = node.node_type == NodeType::Directory;
                        let bytes = s.as_bytes();
                        let needed = bytes.len() + 2; // +1 for type byte, +1 for null
                        if pos + needed < buf.len() {
                            buf[pos] = if is_dir { b'd' } else { b'f' }; // type prefix
                            pos += 1;
                            buf[pos..pos + bytes.len()].copy_from_slice(bytes);
                            pos += bytes.len();
                            buf[pos] = 0;
                            pos += 1;
                            count += 1;
                        }
                    }
                }
            }
            // Terminate with double-null
            if pos < buf.len() { buf[pos] = 0; }
            (NtStatus::Success as u64) | ((count as u64) << 32)
        }
        2 => {
            // mkdir
            match crate::RAM_FS.lock().add_node(name, NodeType::Directory) {
                Ok(_) => NtStatus::Success as u64,
                Err(_) => NtStatus::IoError as u64,
            }
        }
        3 => {
            // touch
            match crate::RAM_FS.lock().add_node(name, NodeType::File) {
                Ok(_) => NtStatus::Success as u64,
                Err(_) => NtStatus::IoError as u64,
            }
        }
        4 => {
            // cat: read file into data_ptr
            if data_ptr == 0 { return NtStatus::InvalidParameter as u64; }
            let fs = crate::RAM_FS.lock();
            if let Some(i) = fs.find_node(name) {
                let node = &fs.nodes[i];
                if node.node_type == NodeType::File {
                    let len = node.data_len.min(data_len as usize);
                    unsafe {
                        core::ptr::copy_nonoverlapping(node.data.as_ptr(), data_ptr as *mut u8, len);
                        if (len as u64) < data_len { *(data_ptr as *mut u8).add(len) = 0; }
                    }
                    len as u64
                } else {
                    NtStatus::InvalidParameter as u64
                }
            } else {
                NtStatus::NotFound as u64
            }
        }
        5 => {
            // write: write data_ptr bytes into file
            if data_ptr == 0 || data_len == 0 { return NtStatus::InvalidParameter as u64; }
            let data = unsafe { core::slice::from_raw_parts(data_ptr as *const u8, data_len as usize) };
            let mut fs = crate::RAM_FS.lock();

            // Find or create node
            let idx = if let Some(i) = fs.find_node(name) {
                i
            } else {
                match fs.add_node(name, NodeType::File) {
                    Ok(i) => i,
                    Err(_) => return NtStatus::IoError as u64,
                }
            };
            let len = data.len().min(crate::ramfs::MAX_FILE_SIZE);
            fs.nodes[idx].data[..len].copy_from_slice(&data[..len]);
            fs.nodes[idx].data_len = len;
            NtStatus::Success as u64
        }
        _ => NtStatus::InvalidCommand as u64,
    }
}

// ---------------------------------------------------------------------------
// sys_fat_touch (syscall 18)
// arg1 = filename_ptr, arg2 = filename_len
// ---------------------------------------------------------------------------
pub fn sys_fat_touch(name_ptr: u64, name_len: u64) -> u64 {
    if name_ptr == 0 || name_len == 0 { return NtStatus::InvalidParameter as u64; }
    let bytes = unsafe { core::slice::from_raw_parts(name_ptr as *const u8, name_len as usize) };
    let name = match core::str::from_utf8(bytes) {
        Ok(s) => s,
        Err(_) => return NtStatus::InvalidParameter as u64,
    };
    let name = name.trim_start_matches('/').trim();
    let mut fat_lock = unsafe { crate::FAT_FS.lock() };
    if let Some(fat) = &mut *fat_lock {
        let root = fat.root_cluster;
        match fat.create_file(root, name) {
            Ok(_) => NtStatus::Success as u64,
            Err(_) => NtStatus::IoError as u64,
        }
    } else {
        NtStatus::NotFound as u64
    }
}

// ---------------------------------------------------------------------------
// sys_fat_mkdir (syscall 25)
// arg1 = filename_ptr, arg2 = filename_len
// ---------------------------------------------------------------------------
pub fn sys_fat_mkdir(name_ptr: u64, name_len: u64) -> u64 {
    if name_ptr == 0 || name_len == 0 { return NtStatus::InvalidParameter as u64; }
    let bytes = unsafe { core::slice::from_raw_parts(name_ptr as *const u8, name_len as usize) };
    let name = match core::str::from_utf8(bytes) {
        Ok(s) => s,
        Err(_) => return NtStatus::InvalidParameter as u64,
    };
    let name = name.trim_start_matches('/').trim();
    let mut fat_lock = unsafe { crate::FAT_FS.lock() };
    if let Some(fat) = &mut *fat_lock {
        let root = fat.root_cluster;
        match fat.create_dir(root, name) {
            Ok(_) => NtStatus::Success as u64,
            Err(_) => NtStatus::IoError as u64,
        }
    } else {
        NtStatus::NotFound as u64
    }
}

// ---------------------------------------------------------------------------
// sys_fat_write (syscall 19)
// arg1 = filename_ptr, arg2 = filename_len, arg3 = data_ptr, arg4 = data_len
// ---------------------------------------------------------------------------
pub fn sys_fat_write(name_ptr: u64, name_len: u64, data_ptr: u64, data_len: u64) -> u64 {
    if name_ptr == 0 || name_len == 0 { return NtStatus::InvalidParameter as u64; }
    let name_bytes = unsafe { core::slice::from_raw_parts(name_ptr as *const u8, name_len as usize) };
    let name = match core::str::from_utf8(name_bytes) {
        Ok(s) => s,
        Err(_) => return NtStatus::InvalidParameter as u64,
    };
    let name = name.trim_start_matches('/').trim();
    let data = if data_ptr != 0 && data_len > 0 {
        unsafe { core::slice::from_raw_parts(data_ptr as *const u8, data_len as usize) }
    } else {
        &[]
    };
    let mut fat_lock = unsafe { crate::FAT_FS.lock() };
    if let Some(fat) = &mut *fat_lock {
        let root = fat.root_cluster;
        match fat.write_file(root, name, data) {
            Ok(_) => NtStatus::Success as u64,
            Err(_) => NtStatus::IoError as u64,
        }
    } else {
        NtStatus::NotFound as u64
    }
}

// ---------------------------------------------------------------------------
// sys_get_user / sys_set_user (syscalls 20/21)
// We store a global current user string.
// ---------------------------------------------------------------------------
static USER_LOCK: spin::Mutex<()> = spin::Mutex::new(());
static mut CURRENT_USER: [u8; 32] = *b"pronin\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0";
static mut CURRENT_USER_LEN: usize = 6;

pub fn sys_get_user(out_ptr: u64, out_len_ptr: u64) -> u64 {
    if out_ptr == 0 { return NtStatus::InvalidParameter as u64; }
    let _g = USER_LOCK.lock();
    let len = unsafe { CURRENT_USER_LEN };
    let buf = unsafe { core::slice::from_raw_parts_mut(out_ptr as *mut u8, 32.min(len + 1)) };
    buf[..len].copy_from_slice(unsafe { &CURRENT_USER[..len] });
    if buf.len() > len { buf[len] = 0; }
    if out_len_ptr != 0 {
        unsafe { *(out_len_ptr as *mut usize) = len; }
    }
    NtStatus::Success as u64
}

pub fn sys_set_user(name_ptr: u64, name_len: u64) -> u64 {
    if name_ptr == 0 || name_len == 0 { return NtStatus::InvalidParameter as u64; }
    let bytes = unsafe { core::slice::from_raw_parts(name_ptr as *const u8, name_len as usize) };
    let _g = USER_LOCK.lock();
    let len = bytes.len().min(31);
    unsafe {
        CURRENT_USER[..len].copy_from_slice(&bytes[..len]);
        CURRENT_USER[len] = 0;
        CURRENT_USER_LEN = len;
    }
    NtStatus::Success as u64
}

// ---------------------------------------------------------------------------
// sys_fat_ls (syscall 22)
// arg1 = path_ptr, arg2 = path_len, arg3 = out_buf_ptr, arg4 = out_buf_len
// Writes packed FatListEntry structs (20 bytes each) into the output buffer.
// Returns entry count in high32, NtStatus in low32.
// ---------------------------------------------------------------------------
pub fn sys_fat_ls(path_ptr: u64, path_len: u64, out_buf: u64, out_buf_len: u64) -> u64 {
    let path = if path_ptr != 0 && path_len > 0 {
        let bytes = unsafe { core::slice::from_raw_parts(path_ptr as *const u8, path_len as usize) };
        core::str::from_utf8(bytes).unwrap_or("")
    } else {
        ""
    };

    // Use the FAT server IPC (op=4)
    use crate::fat_pos::{FatIoRequest, FAT_SERVER_PORT};
    use alloc::sync::Arc;

    let reply_port = Arc::new(crate::port::Port::new(2));
    let mut path_buf = alloc::vec::Vec::from(path.as_bytes());
    let req = FatIoRequest {
        reply_port: Arc::as_ptr(&reply_port),
        operation: 4,
        first_cluster: 0,
        file_size: 0,
        offset: 0,
        buffer_ptr: if path_buf.is_empty() { core::ptr::null_mut() } else { path_buf.as_mut_ptr() },
        buffer_len: path_buf.len(),
    };
    let req_bytes = unsafe {
        core::slice::from_raw_parts(&req as *const _ as *const u8, core::mem::size_of::<FatIoRequest>())
    };
    if FAT_SERVER_PORT.write_port(0x1001, req_bytes).is_err() {
        return NtStatus::IoError as u64;
    }
    loop {
        if let Some(msg) = reply_port.read_port() {
            if msg.code == 0x2004 {
                let entry_size = 20usize;
                let count = msg.data.len() / entry_size;
                let buf = unsafe { core::slice::from_raw_parts_mut(out_buf as *mut u8, out_buf_len as usize) };
                let bytes_to_copy = (count * entry_size).min(buf.len());
                buf[..bytes_to_copy].copy_from_slice(&msg.data[..bytes_to_copy]);
                return (NtStatus::Success as u64) | ((count as u64) << 32);
            }
        }
        crate::task::yield_now();
    }
}

// ---------------------------------------------------------------------------
// sys_fat_cat (syscall 23)
// arg1=path_ptr, arg2=path_len, arg3=out_buf, arg4=out_buf_len
// Reads a file from FAT32 into the output buffer; returns bytes read.
// ---------------------------------------------------------------------------
pub fn sys_fat_cat(path_ptr: u64, path_len: u64, out_buf: u64, out_buf_len: u64) -> u64 {
    if path_ptr == 0 || path_len == 0 || out_buf == 0 { return NtStatus::InvalidParameter as u64; }
    let path_bytes = unsafe { core::slice::from_raw_parts(path_ptr as *const u8, path_len as usize) };
    let path = match core::str::from_utf8(path_bytes) {
        Ok(s) => s,
        Err(_) => return NtStatus::InvalidParameter as u64,
    };
    let path = path.trim_start_matches('/').trim();

    let (first_cluster, file_size) = {
        let mut fat_lock = unsafe { crate::FAT_FS.lock() };
        if let Some(fat) = &mut *fat_lock {
            let root = fat.root_cluster;
            match fat.find_entry(root, path) {
                Ok(Some(entry)) if !entry.is_dir() => (entry.first_cluster(), entry.file_size as usize),
                Ok(Some(_)) => return NtStatus::InvalidParameter as u64,
                Ok(None) => return NtStatus::NotFound as u64,
                Err(_) => return NtStatus::IoError as u64,
            }
        } else {
            return NtStatus::NotFound as u64;
        }
    };

    // Now read in chunks
    let out_buf_slice = unsafe { core::slice::from_raw_parts_mut(out_buf as *mut u8, out_buf_len as usize) };
    let mut total_read = 0usize;
    let max_read = file_size.min(out_buf_len as usize);

    while total_read < max_read {
        let chunk_size = (max_read - total_read).min(1024);
        let mut chunk = [0u8; 1024];
        let mut bytes_read = 0;
        {
            let mut fat_lock = unsafe { crate::FAT_FS.lock() };
            if let Some(fat) = &mut *fat_lock {
                if let Ok(br) = fat.read_file_offset(first_cluster, file_size, total_read, &mut chunk[..chunk_size]) {
                    bytes_read = br;
                }
            }
        }
        if bytes_read == 0 { break; }
        out_buf_slice[total_read..total_read + bytes_read].copy_from_slice(&chunk[..bytes_read]);
        total_read += bytes_read;
        crate::task::yield_now();
    }

    (NtStatus::Success as u64) | ((total_read as u64) << 32)
}

// ---------------------------------------------------------------------------
// sys_iumstart (syscall 24)
// arg1 = path_ptr, arg2 = path_len
// Loads and spawns an IUM binary from FAT32 in Ring 3.
// ---------------------------------------------------------------------------
// sys_iumstart: load IUM/ELF executable and run in Ring 3
// IUM format: [IumHeader 24 bytes][ELF64 payload]
// ---------------------------------------------------------------------------
pub fn sys_iumstart(path_ptr: u64, path_len: u64) -> u64 {
    use commands_impl::map_user_pages;

    // Helper: write a string directly to VGA for debugging (bypasses locks)
    macro_rules! dbg_print {
        ($s:expr) => {{
            crate::print!("{}", $s);
        }};
    }

    if path_ptr == 0 || path_len == 0 { return NtStatus::InvalidParameter as u64; }
    let path_bytes = unsafe { core::slice::from_raw_parts(path_ptr as *const u8, path_len as usize) };
    let path = match core::str::from_utf8(path_bytes) {
        Ok(s) => s,
        Err(_) => return NtStatus::InvalidParameter as u64,
    };
    let path = path.trim_start_matches('/').trim();

    dbg_print!("[IUM] Step 1: acquiring FAT lock...\n");

    // Read file from FAT32 (release FAT lock before doing page mapping)
    let data: alloc::vec::Vec<u8> = {
        let mut fat_lock = unsafe { crate::FAT_FS.lock() };
        dbg_print!("[IUM] Step 2: FAT lock acquired, finding entry...\n");
        if let Some(fat) = &mut *fat_lock {
            let root = fat.root_cluster;
            match fat.find_entry(root, path) {
                Ok(Some(entry)) if !entry.is_dir() => {
                    let fsz = entry.file_size as usize;
                    dbg_print!("[IUM] Step 3: file found, reading bytes...\n");
                    {
                        crate::println!("[IUM]   file_size = {} bytes", fsz);
                    }
                    let mut buf = alloc::vec![0u8; fsz];
                    match fat.read_file_offset(entry.first_cluster(), fsz, 0, &mut buf) {
                        Ok(_) => {
                            dbg_print!("[IUM] Step 4: file read OK.\n");
                            buf
                        }
                        Err(_) => {
                            dbg_print!("[IUM] Step 3 FAIL: read_file_offset returned error!\n");
                            return NtStatus::IoError as u64;
                        }
                    }
                }
                Ok(Some(_)) => {
                    dbg_print!("[IUM] Step 2 FAIL: path is a directory!\n");
                    return NtStatus::InvalidParameter as u64;
                }
                Ok(None) => {
                    dbg_print!("[IUM] Step 2 FAIL: file not found on FAT32!\n");
                    return NtStatus::NotFound as u64;
                }
                Err(_) => {
                    dbg_print!("[IUM] Step 2 FAIL: find_entry returned I/O error!\n");
                    return NtStatus::IoError as u64;
                }
            }
        } else {
            dbg_print!("[IUM] Step 2 FAIL: FAT32 volume is None!\n");
            return NtStatus::NotFound as u64;
        }
    };

    dbg_print!("[IUM] Step 5: validating IUM header...\n");

    // Validate IUM header
    let ium_hdr_size = core::mem::size_of::<crate::ium::IumHeader>();
    if data.len() < ium_hdr_size {
        dbg_print!("[IUM] Step 5 FAIL: data too small for IUM header!\n");
        return NtStatus::InvalidParameter as u64;
    }
    let header = unsafe { &*(data.as_ptr() as *const crate::ium::IumHeader) };
    if !header.is_valid() {
        dbg_print!("[IUM] Step 5 FAIL: bad IUM magic (not 'IUM!')!\n");
        {
            crate::println!("[IUM]   magic bytes = {} {} {} {}", header.magic[0], header.magic[1], header.magic[2], header.magic[3]);
        }
        return NtStatus::InvalidParameter as u64;
    }

    dbg_print!("[IUM] Step 6: validating ELF header...\n");

    // ELF starts right after IUM header
    let elf = &data[ium_hdr_size..];
    if elf.len() < 64 {
        dbg_print!("[IUM] Step 6 FAIL: ELF payload too small!\n");
        return NtStatus::InvalidParameter as u64;
    }
    if &elf[0..4] != b"\x7fELF" {
        dbg_print!("[IUM] Step 6 FAIL: missing ELF magic!\n");
        return NtStatus::InvalidParameter as u64;
    }

    // Parse ELF64 header
    let e_entry     = u64::from_le_bytes(elf[24..32].try_into().unwrap_or([0;8]));
    let e_phoff     = u64::from_le_bytes(elf[32..40].try_into().unwrap_or([0;8])) as usize;
    let e_phentsize = u16::from_le_bytes(elf[54..56].try_into().unwrap_or([0;2])) as usize;
    let e_phnum     = u16::from_le_bytes(elf[56..58].try_into().unwrap_or([0;2])) as usize;

    {
        crate::println!("[IUM] ELF: entry={:#X} phnum={} phentsize={}", e_entry, e_phnum, e_phentsize);
    }

    if e_entry == 0 || e_phoff == 0 || e_phnum == 0 || e_phentsize < 56 {
        dbg_print!("[IUM] Step 6 FAIL: invalid ELF header fields!\n");
        return NtStatus::InvalidParameter as u64;
    }

    dbg_print!("[IUM] Step 7: mapping ELF segments...\n");

    // Map each PT_LOAD segment at its requested virtual address
    const PT_LOAD: u32 = 1;
    for i in 0..e_phnum {
        let ph = e_phoff + i * e_phentsize;
        if ph + e_phentsize > elf.len() { break; }

        let p_type   = u32::from_le_bytes(elf[ph..ph+4].try_into().unwrap_or([0;4]));
        let p_flags  = u32::from_le_bytes(elf[ph+4..ph+8].try_into().unwrap_or([0;4]));
        let p_offset = u64::from_le_bytes(elf[ph+8..ph+16].try_into().unwrap_or([0;8])) as usize;
        let p_vaddr  = u64::from_le_bytes(elf[ph+16..ph+24].try_into().unwrap_or([0;8]));
        let p_filesz = u64::from_le_bytes(elf[ph+32..ph+40].try_into().unwrap_or([0;8])) as usize;
        let p_memsz  = u64::from_le_bytes(elf[ph+40..ph+48].try_into().unwrap_or([0;8])) as usize;

        if p_type != PT_LOAD || p_memsz == 0 { continue; }

        {
            crate::println!("[IUM]   seg[{}] vaddr={:#X} memsz={}", i, p_vaddr, p_memsz);
        }

        let page_base   = p_vaddr & !0xFFF;
        let page_offset = (p_vaddr - page_base) as usize;
        let mapped_size = (page_offset + p_memsz + 4095) & !4095;

        let seg_layout = core::alloc::Layout::from_size_align(mapped_size, 4096).unwrap();
        let seg_ptr = unsafe { alloc::alloc::alloc_zeroed(seg_layout) };
        if seg_ptr.is_null() {
            dbg_print!("[IUM] Step 7 FAIL: alloc_zeroed returned null!\n");
            return NtStatus::IoError as u64;
        }

        if p_filesz > 0 && p_offset + p_filesz <= elf.len() {
            unsafe {
                core::ptr::copy_nonoverlapping(
                    elf[p_offset..].as_ptr(),
                    seg_ptr.add(page_offset),
                    p_filesz,
                );
            }
        }

        let executable = (p_flags & 1) != 0; // PF_X
        map_user_pages(page_base, seg_ptr as u64, mapped_size, executable);
    }

    dbg_print!("[IUM] Step 8: mapping stack...\n");

    // User stack: 1 MiB at 0x7FFF_0000
    const USER_STACK_BASE: u64 = 0x7FFF_0000;
    const USER_STACK_SIZE: usize = 1024 * 1024;
    let stack_layout = core::alloc::Layout::from_size_align(USER_STACK_SIZE, 4096).unwrap();
    let stack_ptr = unsafe { alloc::alloc::alloc_zeroed(stack_layout) };
    if !stack_ptr.is_null() {
        map_user_pages(USER_STACK_BASE, stack_ptr as u64, USER_STACK_SIZE, false);
    } else {
        dbg_print!("[IUM] Step 8 WARN: stack alloc failed!\n");
    }

    dbg_print!("[IUM] Step 9: mapping heap...\n");

    // User heap: 2 MiB at 0x500000
    map_user_pages(0x500000, 0, 2 * 1024 * 1024, false);

    dbg_print!("[IUM] Step 10: spawning ium_trampoline...\n");

    let user_entry     = e_entry;
    let user_stack_top = USER_STACK_BASE + USER_STACK_SIZE as u64;

    unsafe {
        crate::syscall::IUM_CODE_PTR  = user_entry;
        crate::syscall::IUM_STACK_PTR = user_stack_top;
    }

    x86_64::instructions::interrupts::without_interrupts(|| {
        crate::task::SCHEDULER.lock().spawn(crate::syscall::ium_trampoline);
    });

    dbg_print!("[IUM] Step 10: done — returning Success.\n");
    NtStatus::Success as u64
}

// ---------------------------------------------------------------------------
// Page mapping helper (extracted from commands.rs, used by sys_iumstart)
// ---------------------------------------------------------------------------
mod commands_impl {
    pub fn map_user_pages(target_virt_addr: u64, backing_virt_addr: u64, size: usize, executable: bool) {
        if size == 0 { return; }
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
                use x86_64::structures::paging::{Translate, FrameAllocator};
                let phys_addr = if backing_virt_addr != 0 {
                    let backing_page_virt = VirtAddr::new((backing_virt_addr + offset as u64) & !0xFFF);
                    match mapper.translate_addr(backing_page_virt) {
                        Some(pa) => pa,
                        None => { offset += 4096; continue; }
                    }
                } else {
                    let mut allocator = HeapFrameAllocator;
                    if let Some(frame) = allocator.allocate_frame() {
                        frame.start_address()
                    } else {
                        offset += 4096; continue;
                    }
                };
                let target_page = Page::<Size4KiB>::containing_address(VirtAddr::new(target_virt_addr + offset as u64));
                let frame = PhysFrame::containing_address(phys_addr);
                let mut flags = PageTableFlags::PRESENT | PageTableFlags::WRITABLE | PageTableFlags::USER_ACCESSIBLE;
                if !executable { flags |= PageTableFlags::NO_EXECUTE; }
                let result = mapper.map_to(target_page, frame, flags, &mut HeapFrameAllocator);
                if let Ok(flusher) = result { flusher.flush(); }
                offset += 4096;
            }

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

    struct HeapFrameAllocator;
    unsafe impl x86_64::structures::paging::FrameAllocator<x86_64::structures::paging::Size4KiB> for HeapFrameAllocator {
        fn allocate_frame(&mut self) -> Option<x86_64::structures::paging::PhysFrame> {
            use x86_64::structures::paging::{PhysFrame, Size4KiB};
            use x86_64::{PhysAddr, VirtAddr};
            use x86_64::structures::paging::{OffsetPageTable, Translate};
            let layout = core::alloc::Layout::from_size_align(4096, 4096).ok()?;
            let ptr = unsafe { alloc::alloc::alloc_zeroed(layout) };
            if ptr.is_null() { return None; }
            unsafe {
                let phys_offset = *(&raw const crate::PHYS_MEM_OFFSET);
                let (l4_frame, _) = x86_64::registers::control::Cr3::read();
                let l4_table = &mut *((l4_frame.start_address().as_u64() + phys_offset) as *mut x86_64::structures::paging::PageTable);
                let mapper = OffsetPageTable::new(l4_table, VirtAddr::new(phys_offset));
                let virt = VirtAddr::new(ptr as u64);
                mapper.translate_addr(virt).map(|pa| PhysFrame::containing_address(pa))
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Main syscall dispatch table
// ---------------------------------------------------------------------------
extern "C" {
    fn syscall_handler_stub();
}

pub fn init_syscalls() {
    use x86_64::registers::model_specific::{Efer, EferFlags, Star, LStar, SFMask};
    use x86_64::registers::rflags::RFlags;
    use x86_64::VirtAddr;

    let selectors = crate::gdt::get_selectors();
    unsafe {
        Efer::update(|flags| flags.insert(EferFlags::SYSTEM_CALL_EXTENSIONS));
        Star::write(
            selectors.user_code_selector,
            selectors.user_data_selector,
            selectors.code_selector,
            selectors.data_selector
        ).unwrap();
        LStar::write(VirtAddr::new(syscall_handler_stub as *const () as u64));
        SFMask::write(RFlags::INTERRUPT_FLAG);
    }
}

pub static mut IUM_CODE_PTR: u64 = 0;
pub static mut IUM_STACK_PTR: u64 = 0;

pub extern "C" fn ium_trampoline() {
    let code_ptr = unsafe { IUM_CODE_PTR };
    let stack_ptr = unsafe { IUM_STACK_PTR };
    unsafe { jump_to_ring3(code_ptr, stack_ptr) }
}

pub unsafe fn jump_to_ring3(entry_point: u64, stack_ptr: u64) -> ! {
    let selectors = crate::gdt::get_selectors();
    let data_sel = (selectors.user_data_selector.0 | 3) as u64;
    let code_sel = (selectors.user_code_selector.0 | 3) as u64;
    x86_64::instructions::interrupts::disable();
    core::arch::asm!(
        "mov ds, ax",
        "mov es, ax",
        "mov fs, ax",
        "mov gs, ax",
        "push rax",
        "push rcx",
        "push 0x202",
        "push rdx",
        "push {entry}",
        "iretq",
        entry = in(reg) entry_point,
        in("ax") data_sel,
        in("cx") stack_ptr,
        in("dx") code_sel,
        options(noreturn)
    );
}

#[no_mangle]
pub extern "C" fn syscall_dispatch(
    sys_no: u64, arg1: u64, arg2: u64, arg3: u64, arg4: u64, arg5: u64
) -> u64 {
    match sys_no {
        // --- Base POS syscalls ---
        1 => {
            let path_bytes = unsafe { core::slice::from_raw_parts(arg1 as *const u8, arg2 as usize) };
            if let Ok(path) = core::str::from_utf8(path_bytes) {
                let out_handle = unsafe { &mut *(arg3 as *mut Handle) };
                crate::syscall::NtOpenFile(path, out_handle) as u64
            } else {
                NtStatus::InvalidParameter as u64
            }
        }
        2 => crate::syscall::NtClose(arg1 as usize) as u64,
        3 => {
            let buffer = unsafe { core::slice::from_raw_parts_mut(arg2 as *mut u8, arg3 as usize) };
            let bytes_read = unsafe { &mut *(arg5 as *mut usize) };
            crate::syscall::NtReadFile(arg1 as usize, buffer, arg4, bytes_read) as u64
        }
        4 => {
            let buffer = unsafe { core::slice::from_raw_parts(arg2 as *const u8, arg3 as usize) };
            let bytes_written = unsafe { &mut *(arg5 as *mut usize) };
            crate::syscall::NtWriteFile(arg1 as usize, buffer, arg4, bytes_written) as u64
        }
        5 => crate::syscall::NtDeviceIoControlFile(arg1 as usize, arg2 as u32, arg3 as usize) as u64,

        // --- Extended syscalls (commands.rs port) ---
        10 => sys_get_stat(arg1),
        11 => sys_net_ping(arg1, arg2, arg3),
        12 => sys_net_ifconfig(arg1),
        13 => sys_net_setdns(arg1),
        14 => sys_net_httpget(arg1, arg2, arg3, arg4, arg5),
        15 => sys_beep(arg1, arg2),
        16 => sys_change_res(arg1, arg2),
        17 => sys_ramfs_op(arg1, arg2, arg3, arg4, arg5),
        18 => sys_fat_touch(arg1, arg2),
        19 => sys_fat_write(arg1, arg2, arg3, arg4),
        20 => sys_get_user(arg1, arg2),
        21 => sys_set_user(arg1, arg2),
        22 => sys_fat_ls(arg1, arg2, arg3, arg4),
        23 => sys_fat_cat(arg1, arg2, arg3, arg4),
        24 => sys_iumstart(arg1, arg2),
        25 => sys_fat_mkdir(arg1, arg2),

        // --- Process exit ---
        100 => crate::task::terminate_current_thread(),

        _ => 0,
    }
}

static mut SYSCALL_STACK: [u8; 4096 * 8] = [0; 4096 * 8];

#[no_mangle]
static mut SYSCALL_KERNEL_STACK_TOP: u64 = 0;

pub fn set_syscall_kernel_stack_top(top: u64) {
    unsafe { *(&raw mut SYSCALL_KERNEL_STACK_TOP) = top; }
}

pub fn init_syscall_stack() {
    unsafe {
        let top = (&raw mut SYSCALL_STACK) as *mut u8 as u64 + (4096 * 8);
        let ptr = &raw mut SYSCALL_KERNEL_STACK_TOP;
        *ptr = top;
    }
}

core::arch::global_asm!(
    r#"
    .global syscall_handler_stub
    syscall_handler_stub:
        // rcx = return RIP, r11 = saved RFLAGS
        // rsp = user stack — we MUST NOT push to it in kernel mode

        // Save user rsp in a scratch register, load kernel stack
        mov [rip + SYSCALL_USER_RSP], rsp
        mov rsp, [rip + SYSCALL_KERNEL_STACK_TOP]

        // Now on kernel stack — safe to push
        push r11        // saved RFLAGS
        push rcx        // saved RIP
        push rbp
        push rbx
        push r12
        push r13
        push r14
        push r15

        // Save user rsp on kernel stack too
        mov rbx, [rip + SYSCALL_USER_RSP]
        push rbx

        // Save original args for restore
        push r9
        push r8
        push r10
        push rdx
        push rsi
        push rdi

        // Set up args for syscall_dispatch(sys_no, arg1, arg2, arg3, arg4, arg5)
        mov r9, r8
        mov r8, r10
        mov rcx, rdx
        mov rdx, rsi
        mov rsi, rdi
        mov rdi, rax

        // Align stack to 16 bytes
        mov rbp, rsp
        and rsp, -16

        call syscall_dispatch

        mov rsp, rbp

        // Restore saved args
        pop rdi
        pop rsi
        pop rdx
        pop r10
        pop r8
        pop r9

        // Restore user rsp
        pop rbx
        mov [rip + SYSCALL_USER_RSP], rbx

        pop r15
        pop r14
        pop r13
        pop r12
        pop rbx
        pop rbp
        pop rcx        // return RIP
        pop r11        // saved RFLAGS

        // Restore user stack pointer
        mov rsp, [rip + SYSCALL_USER_RSP]

        sysretq

    .global SYSCALL_USER_RSP
    SYSCALL_USER_RSP:
        .quad 0
    "#
);
