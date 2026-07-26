// Copyright (C) 2026 Pronin. All rights reserved.
// ProniumOS Userland SDK

#![no_std]
#![feature(alloc_error_handler)]
extern crate alloc;

use core::arch::asm;

pub type Handle = usize;

#[global_allocator]
static ALLOCATOR: linked_list_allocator::LockedHeap = linked_list_allocator::LockedHeap::empty();

#[alloc_error_handler]
fn alloc_error_handler(layout: core::alloc::Layout) -> ! {
    panic!("allocation error: {:?}", layout)
}

pub fn init_heap(heap_start: usize, heap_size: usize) {
    unsafe {
        ALLOCATOR.lock().init(heap_start as *mut u8, heap_size);
    }
}

// ---------------------------------------------------------------------------
// NtStatus
// ---------------------------------------------------------------------------
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
    pub fn from_u64(v: u64) -> Self {
        match v as u32 {
            0 => NtStatus::Success,
            1 => NtStatus::InvalidHandle,
            2 => NtStatus::AccessDenied,
            3 => NtStatus::NotFound,
            4 => NtStatus::NotSupported,
            5 => NtStatus::IoError,
            6 => NtStatus::InvalidParameter,
            7 => NtStatus::InvalidCommand,
            _ => NtStatus::IoError,
        }
    }
}

// ---------------------------------------------------------------------------
// Raw syscall wrapper
// ---------------------------------------------------------------------------
#[inline(always)]
unsafe fn syscall(
    sys_no: u64,
    arg1: u64,
    arg2: u64,
    arg3: u64,
    arg4: u64,
    arg5: u64,
) -> u64 {
    let ret: u64;
    unsafe {
        asm!(
            "syscall",
            inlateout("rax") sys_no => ret,
            in("rdi") arg1,
            in("rsi") arg2,
            in("rdx") arg3,
            in("r10") arg4,
            in("r8") arg5,
            out("rcx") _,
            out("r11") _,
            options(nostack, preserves_flags)
        );
    }
    ret
}

// ---------------------------------------------------------------------------
// Syscall 1-5
// ---------------------------------------------------------------------------
pub fn nt_open_file(path: &str, out_handle: &mut Handle) -> NtStatus {
    let path_ptr = path.as_ptr() as u64;
    let path_len = path.len() as u64;
    let out_ptr = out_handle as *mut Handle as u64;
    let ret = unsafe { syscall(1, path_ptr, path_len, out_ptr, 0, 0) };
    NtStatus::from_u64(ret)
}

pub fn nt_close(handle: Handle) -> NtStatus {
    let ret = unsafe { syscall(2, handle as u64, 0, 0, 0, 0) };
    NtStatus::from_u64(ret)
}

pub fn nt_read_file(handle: Handle, buffer: &mut [u8], offset: u64, bytes_read: &mut usize) -> NtStatus {
    let buf_ptr = buffer.as_mut_ptr() as u64;
    let buf_len = buffer.len() as u64;
    let bytes_read_ptr = bytes_read as *mut usize as u64;
    let ret = unsafe { syscall(3, handle as u64, buf_ptr, buf_len, offset, bytes_read_ptr) };
    NtStatus::from_u64(ret)
}

pub fn nt_write_file(handle: Handle, buffer: &[u8], offset: u64, bytes_written: &mut usize) -> NtStatus {
    let buf_ptr = buffer.as_ptr() as u64;
    let buf_len = buffer.len() as u64;
    let bytes_written_ptr = bytes_written as *mut usize as u64;
    let ret = unsafe { syscall(4, handle as u64, buf_ptr, buf_len, offset, bytes_written_ptr) };
    NtStatus::from_u64(ret)
}

pub fn nt_device_io_control_file(handle: Handle, io_control_code: u32, arg: usize) -> NtStatus {
    let ret = unsafe { syscall(5, handle as u64, io_control_code as u64, arg as u64, 0, 0) };
    NtStatus::from_u64(ret)
}

pub fn sys_exit() -> ! {
    unsafe { syscall(100, 0, 0, 0, 0, 0) };
    loop {}
}

// ---------------------------------------------------------------------------
// Syscall 10: sys_get_stat
// ---------------------------------------------------------------------------
#[repr(C)]
pub struct SysStatInfo {
    pub ram_mb: u64,
    pub monitor_w: u64,
    pub monitor_h: u64,
    pub uptime_secs: u64,
    pub pci_dev_count: u64,
    /// 0=None, 1=Intel E1000, 2=Realtek RTL8139
    pub nic_type: u32,
    pub padding: u32,
}

pub fn sys_get_stat(info: &mut SysStatInfo) -> NtStatus {
    let ret = unsafe { syscall(10, info as *mut SysStatInfo as u64, 0, 0, 0, 0) };
    NtStatus::from_u64(ret)
}

// ---------------------------------------------------------------------------
// Syscall 11: sys_net_ping
// ---------------------------------------------------------------------------
pub fn sys_net_ping(target_ip: &[u8; 4], out_buf: &mut [u8]) -> NtStatus {
    let ret = unsafe {
        syscall(
            11,
            target_ip.as_ptr() as u64,
            out_buf.as_mut_ptr() as u64,
            out_buf.len() as u64,
            0, 0,
        )
    };
    NtStatus::from_u64(ret)
}

// ---------------------------------------------------------------------------
// Syscall 12: sys_net_ifconfig
// ---------------------------------------------------------------------------
#[repr(C)]
pub struct SysIfconfigInfo {
    pub has_nic: u8,
    pub nic_type: u8,
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

pub fn sys_net_ifconfig(info: &mut SysIfconfigInfo) -> NtStatus {
    let ret = unsafe { syscall(12, info as *mut SysIfconfigInfo as u64, 0, 0, 0, 0) };
    NtStatus::from_u64(ret)
}

// ---------------------------------------------------------------------------
// Syscall 13: sys_net_setdns
// ---------------------------------------------------------------------------
pub fn sys_net_setdns(ip: &[u8; 4]) -> NtStatus {
    let ret = unsafe { syscall(13, ip.as_ptr() as u64, 0, 0, 0, 0) };
    NtStatus::from_u64(ret)
}

// ---------------------------------------------------------------------------
// Syscall 14: sys_net_httpget
// ---------------------------------------------------------------------------
pub fn sys_net_httpget(
    target_ip: &[u8; 4],
    port: u16,
    path: &str,
    out_buf: &mut [u8],
) -> NtStatus {
    let ret = unsafe {
        syscall(
            14,
            target_ip.as_ptr() as u64,
            port as u64,
            path.as_ptr() as u64,
            path.len() as u64,
            // Pack out_buf ptr and len into arg5: ptr in high32 is impossible in 64-bit,
            // so we pass ptr separately via a small trampoline approach.
            // Actually we pass out_buf ptr in arg5 and use a separate call for len:
            // Re-encode: arg5 = out_buf_ptr, kernel uses it directly.
            out_buf.as_mut_ptr() as u64,
        )
    };
    NtStatus::from_u64(ret)
}

// ---------------------------------------------------------------------------
// Syscall 15: sys_beep
// ---------------------------------------------------------------------------
pub fn sys_beep(hz: u32, ms: u64) -> NtStatus {
    let ret = unsafe { syscall(15, hz as u64, ms, 0, 0, 0) };
    NtStatus::from_u64(ret)
}

// ---------------------------------------------------------------------------
// Syscall 16: sys_change_res
// ---------------------------------------------------------------------------
pub fn sys_change_res(width: u16, height: u16) -> NtStatus {
    let ret = unsafe { syscall(16, width as u64, height as u64, 0, 0, 0) };
    NtStatus::from_u64(ret)
}

// ---------------------------------------------------------------------------
// Syscall 17: sys_ramfs_op
// opcodes: 1=ls, 2=mkdir, 3=touch, 4=cat, 5=write
// ---------------------------------------------------------------------------
pub fn sys_ramfs_ls(out_buf: &mut [u8]) -> (NtStatus, u32) {
    let ret = unsafe {
        syscall(17, 1, 0, 0, out_buf.as_mut_ptr() as u64, out_buf.len() as u64)
    };
    let status = NtStatus::from_u64(ret & 0xFFFF_FFFF);
    let count = (ret >> 32) as u32;
    (status, count)
}

pub fn sys_ramfs_mkdir(name: &str) -> NtStatus {
    let ret = unsafe {
        syscall(17, 2, name.as_ptr() as u64, name.len() as u64, 0, 0)
    };
    NtStatus::from_u64(ret)
}

pub fn sys_ramfs_touch(name: &str) -> NtStatus {
    let ret = unsafe {
        syscall(17, 3, name.as_ptr() as u64, name.len() as u64, 0, 0)
    };
    NtStatus::from_u64(ret)
}

pub fn sys_ramfs_cat(name: &str, out_buf: &mut [u8]) -> u64 {
    unsafe {
        syscall(17, 4, name.as_ptr() as u64, name.len() as u64, out_buf.as_mut_ptr() as u64, out_buf.len() as u64)
    }
}

pub fn sys_ramfs_write(name: &str, data: &[u8]) -> NtStatus {
    let ret = unsafe {
        syscall(17, 5, name.as_ptr() as u64, name.len() as u64, data.as_ptr() as u64, data.len() as u64)
    };
    NtStatus::from_u64(ret)
}

// ---------------------------------------------------------------------------
// Syscall 18: sys_fat_touch
// ---------------------------------------------------------------------------
pub fn sys_fat_touch(name: &str) -> NtStatus {
    let ret = unsafe { syscall(18, name.as_ptr() as u64, name.len() as u64, 0, 0, 0) };
    NtStatus::from_u64(ret)
}

// ---------------------------------------------------------------------------
// Syscall 25: sys_fat_mkdir
// ---------------------------------------------------------------------------
pub fn sys_fat_mkdir(name: &str) -> NtStatus {
    let ret = unsafe { syscall(25, name.as_ptr() as u64, name.len() as u64, 0, 0, 0) };
    NtStatus::from_u64(ret)
}

// ---------------------------------------------------------------------------
// Syscall 19: sys_fat_write
// ---------------------------------------------------------------------------
pub fn sys_fat_write(name: &str, data: &[u8]) -> NtStatus {
    let ret = unsafe {
        syscall(19, name.as_ptr() as u64, name.len() as u64, data.as_ptr() as u64, data.len() as u64, 0)
    };
    NtStatus::from_u64(ret)
}

// ---------------------------------------------------------------------------
// Syscall 20/21: get/set current user
// ---------------------------------------------------------------------------
pub fn sys_get_user(out_buf: &mut [u8], out_len: &mut usize) -> NtStatus {
    let ret = unsafe {
        syscall(20, out_buf.as_mut_ptr() as u64, out_len as *mut usize as u64, 0, 0, 0)
    };
    NtStatus::from_u64(ret)
}

pub fn sys_set_user(name: &str) -> NtStatus {
    let ret = unsafe { syscall(21, name.as_ptr() as u64, name.len() as u64, 0, 0, 0) };
    NtStatus::from_u64(ret)
}

// ---------------------------------------------------------------------------
// Syscall 22: sys_fat_ls
// ---------------------------------------------------------------------------
pub fn sys_fat_ls(path: &str, out_buf: &mut [u8]) -> (NtStatus, u32) {
    let ret = unsafe {
        syscall(
            22,
            path.as_ptr() as u64,
            path.len() as u64,
            out_buf.as_mut_ptr() as u64,
            out_buf.len() as u64,
            0,
        )
    };
    let status = NtStatus::from_u64(ret & 0xFFFF_FFFF);
    let count = (ret >> 32) as u32;
    (status, count)
}

// ---------------------------------------------------------------------------
// Syscall 23: sys_fat_cat
// ---------------------------------------------------------------------------
pub fn sys_fat_cat(path: &str, out_buf: &mut [u8]) -> u64 {
    unsafe {
        syscall(
            23,
            path.as_ptr() as u64,
            path.len() as u64,
            out_buf.as_mut_ptr() as u64,
            out_buf.len() as u64,
            0,
        )
    }
}

// ---------------------------------------------------------------------------
// Syscall 24: sys_iumstart
// ---------------------------------------------------------------------------
pub fn sys_iumstart(path: &str) -> NtStatus {
    let ret = unsafe { syscall(24, path.as_ptr() as u64, path.len() as u64, 0, 0, 0) };
    NtStatus::from_u64(ret)
}

// ---------------------------------------------------------------------------
// Stdout wrapper
// ---------------------------------------------------------------------------
pub struct Stdout {
    handle: Handle,
}

impl Stdout {
    pub fn new() -> Self {
        let mut handle = 0;
        nt_open_file(r"\Device\ConOut", &mut handle);
        Self { handle }
    }

    pub fn write_str(&self, s: &str) {
        let mut written = 0;
        nt_write_file(self.handle, s.as_bytes(), 0, &mut written);
    }
}

use core::fmt::{self, Write};

impl Write for Stdout {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        Stdout::write_str(self, s);
        Ok(())
    }
}

pub fn _print(args: fmt::Arguments) {
    let mut stdout = Stdout::new();
    let _ = stdout.write_fmt(args);
    nt_close(stdout.handle);
}

#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => ($crate::_print(format_args!($($arg)*)));
}

#[macro_export]
macro_rules! println {
    () => ($crate::print!("\n"));
    ($($arg:tt)*) => ($crate::print!("{}\n", format_args!($($arg)*)));
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    sys_exit()
}
