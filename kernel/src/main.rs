

#![no_std]
#![no_main]
#![feature(abi_x86_interrupt)]
#![feature(naked_functions)]

extern crate alloc;

mod vga;
mod ramfs;
mod port;
pub mod framebuffer;
pub mod drivers;

use alloc::sync::Arc;
use crate::pos::{PosObject, PosError};
mod gdt;
mod interrupts;
mod net;
mod ium;
pub mod fat_pos;
pub mod fs;
mod tcp;
mod memory;
mod allocator;
mod pos;
mod pdf;
mod syscall;
mod task;
mod kernel_api;
mod kernel_syms;
mod elf_loader;
mod pd_loader;
mod shell;
mod commands;
mod psd_tools;

use core::panic::PanicInfo;
use crate::vga::{VgaWriter, Color};
use crate::net::NetworkCard;


pub static mut NIC: Option<NetworkCard> = None;
pub static mut PHYS_MEM_OFFSET: u64 = 0;
pub static mut TOTAL_RAM_MB: usize = 0;
pub static mut MONITOR_INFO: (usize, usize) = (80, 25); 


pub static FAT_FS: spin::Mutex<Option<fs::Fat32Volume<drivers::ahci::AhciDriver>>> = spin::Mutex::new(None);


pub static RAM_FS: spin::Mutex<ramfs::RamFs> = spin::Mutex::new(ramfs::RamFs::new());


#[used]
#[link_section = ".limine_reqs"]
static ENTRY_POINT_REQUEST: limine::request::EntryPointRequest = limine::request::EntryPointRequest::new(kernel_main);

#[used]
#[link_section = ".limine_reqs"]
static HHDM_REQUEST: limine::request::HhdmRequest = limine::request::HhdmRequest::new();

#[used]
#[link_section = ".limine_reqs"]
static FRAMEBUFFER_REQUEST: limine::request::FramebufferRequest = limine::request::FramebufferRequest::new();

#[used]
#[link_section = ".limine_reqs"]
static MEMORY_MAP_REQUEST: limine::request::MemmapRequest = limine::request::MemmapRequest::new();

#[no_mangle]
extern "C" fn kernel_main() -> ! {
    let hhdm = HHDM_REQUEST.response().unwrap();
    let phys_mem_offset = hhdm.offset;
    unsafe {
        let ptr = &raw mut PHYS_MEM_OFFSET;
        *ptr = phys_mem_offset;
    }
    
    let mut total_bytes: u64 = 0;
    if let Some(mmap) = MEMORY_MAP_REQUEST.response() {
        for entry in mmap.entries() {
            if entry.type_ == limine::memmap::MEMMAP_USABLE {
                total_bytes += entry.length;
            }
        }
    }
    unsafe { 
        TOTAL_RAM_MB = (total_bytes / (1024 * 1024)) as usize; 
    }

    if let Some(fb_response) = FRAMEBUFFER_REQUEST.response() {
        if let Some(fb) = fb_response.framebuffers().first() {
            let fb_addr = fb.address() as usize;
            let width = fb.width as usize;
            let height = fb.height as usize;
            let pitch = fb.pitch as usize;
            
            unsafe {
                MONITOR_INFO = (width, height);
            }
            crate::framebuffer::init_framebuffer(fb_addr, width, height, pitch, 4);
        } else {
            unsafe { MONITOR_INFO = (320, 200); }
            crate::framebuffer::init_framebuffer(0xa0000, 320, 200, 320, 1);
        }
    } else {
        unsafe { MONITOR_INFO = (320, 200); }
        crate::framebuffer::init_framebuffer(0xa0000, 320, 200, 320, 1);
    }


    
    allocator::init_heap();

    
    pos::init();

    
    crate::gdt::init();
    interrupts::init();
    crate::syscall::init_syscall_stack();
    crate::syscall::init_syscalls();
    x86_64::instructions::interrupts::enable();

    
    crate::vga::init();
    
    // Set colors for the boot message
    if let Some(w) = crate::vga::WRITER.lock().as_mut() {
        w.set_color(Color::LightCyan, Color::Black);
        w.write_string("ProniumOS Build 260726\n");
        w.set_color(Color::White, Color::Black);
    }

    struct KeyboardWrapper;
    impl pos::PosObject for KeyboardWrapper {
        fn read(&mut self, _offset: u64, buf: &mut [u8]) -> Result<usize, pos::PosError> {
            loop {
                if let Some(scancode) = crate::drivers::keyboard::poll_scancode() {
                    buf[0] = scancode;
                    return Ok(1);
                }
                crate::task::yield_now();
            }
        }
    }
    let keyboard_obj: alloc::sync::Arc<spin::Mutex<dyn pos::PosObject>> = alloc::sync::Arc::new(spin::Mutex::new(KeyboardWrapper));
    let _ = pos::register_object("\\Device\\Keyboard", keyboard_obj);

    
    struct RamFsWrapper;
    impl PosObject for RamFsWrapper {
        fn read(&mut self, _offset: u64, buf: &mut [u8]) -> Result<usize, PosError> {
            
            Ok(0)
        }
        fn write(&mut self, _offset: u64, _buf: &[u8]) -> Result<usize, PosError> {
            Err(PosError::NotSupported)
        }
        fn ioctl(&mut self, _command: u32, _arg: usize) -> Result<(), PosError> {
            Err(PosError::NotSupported)
        }
    }
    let ramfs_obj: Arc<spin::Mutex<dyn PosObject>> = Arc::new(spin::Mutex::new(RamFsWrapper));
    let _ = pos::register_object("\\Device\\RamFs", ramfs_obj);

    struct Fat32Wrapper;
    impl PosObject for Fat32Wrapper {
        fn read(&mut self, _offset: u64, _buf: &mut [u8]) -> Result<usize, PosError> {
            Ok(0)
        }
        fn write(&mut self, _offset: u64, _buf: &[u8]) -> Result<usize, PosError> {
            Err(PosError::NotSupported)
        }
        fn ioctl(&mut self, _command: u32, _arg: usize) -> Result<(), PosError> {
            Err(PosError::NotSupported)
        }
        fn open_node(&mut self, path: &str) -> Result<pos::PosObjectRef, PosError> {
            use alloc::sync::Arc;
            use crate::port::Port;
            use crate::fat_pos::{FatIoRequest, FatFindReply, FatPosObject, FAT_SERVER_PORT};

            let reply_port = Port::new(1);
            let mut path_buf = alloc::vec::Vec::from(path.as_bytes());
            let req = FatIoRequest {
                reply_port: Arc::as_ptr(&reply_port),
                operation: 3, 
                first_cluster: 0,
                file_size: 0,
                offset: 0,
                buffer_ptr: path_buf.as_mut_ptr(),
                buffer_len: path_buf.len(),
            };
            
            let req_bytes = unsafe { core::slice::from_raw_parts(&req as *const _ as *const u8, core::mem::size_of::<FatIoRequest>()) };
            FAT_SERVER_PORT.write_port(0x1001, req_bytes).map_err(|_| PosError::NotSupported)?;
            
            loop {
                if let Some(msg) = reply_port.read_port() {
                    if msg.code == 0x2001 && msg.data.len() == core::mem::size_of::<FatFindReply>() {
                        let reply = unsafe { &*(msg.data.as_ptr() as *const FatFindReply) };
                        if reply.found && !reply.is_dir {
                            return Ok(Arc::new(spin::Mutex::new(FatPosObject {
                                first_cluster: reply.first_cluster,
                                file_size: reply.file_size,
                                offset: 0,
                            })));
                        } else {
                            return Err(PosError::NotFound);
                        }
                    }
                }
                crate::task::yield_now();
            }
        }
    }
    let fat32_obj: Arc<spin::Mutex<dyn PosObject>> = Arc::new(spin::Mutex::new(Fat32Wrapper));
    let _ = pos::register_object("\\Device\\Harddisk0\\Partition1", fat32_obj);

    
    crate::println!("Initializing Pronium Driver Framework...");
    {
        crate::println!("  Acquiring PDF lock...");
        let mut pdf = pdf::PDF.lock();
        crate::println!("  PDF lock acquired.");

        
        struct E1000PdfDriver { phys_mem_offset: u64 }
        impl pdf::Driver for E1000PdfDriver {
            fn name(&self) -> &str { "Intel E1000 Network" }
            fn probe(&mut self, device: &crate::drivers::pci::PciDevice) -> bool {
                if device.vendor_id == 0x8086 && (device.device_id == 0x100E || device.device_id == 0x153A) {
                    crate::drivers::pci::enable_bus_mastering(device);
                    let mut nic = crate::drivers::e1000::E1000::new(device.bar0, self.phys_mem_offset);
                    nic.init(self.phys_mem_offset);
                    unsafe { NIC = Some(NetworkCard::E1000(nic)); }
                    return true;
                }
                false
            }
            fn init(&mut self) -> Result<(), pos::PosError> { Ok(()) }
        }
        
        
        struct Rtl8139PdfDriver { phys_mem_offset: u64 }
        impl pdf::Driver for Rtl8139PdfDriver {
            fn name(&self) -> &str { "Realtek RTL8139 Network" }
            fn probe(&mut self, device: &crate::drivers::pci::PciDevice) -> bool {
                if device.vendor_id == 0x10EC && device.device_id == 0x8139 {
                    crate::drivers::pci::enable_bus_mastering(device);
                    let io_base = (device.bar0 & !0x03) as u16;
                    let mut nic = crate::drivers::rtl8139::Rtl8139::new(io_base);
                    nic.init(self.phys_mem_offset);
                    unsafe { NIC = Some(NetworkCard::Rtl8139(nic)); }
                    return true;
                }
                false
            }
            fn init(&mut self) -> Result<(), pos::PosError> { Ok(()) }
        }
        
        
        struct AhciPdfDriver { phys_mem_offset: u64 }
        impl pdf::Driver for AhciPdfDriver {
            fn name(&self) -> &str { "SATA AHCI Controller" }
            fn probe(&mut self, device: &crate::drivers::pci::PciDevice) -> bool {
                let class = crate::drivers::pci::pci_read(device.bus, device.device, device.function, 0x08) >> 16;
                let class_id = (class >> 8) as u8;
                let subclass = class as u8;
                if class_id == 0x01 && subclass == 0x06 {
                    crate::drivers::pci::enable_bus_mastering(device);
                    let bar5 = crate::drivers::pci::pci_read(device.bus, device.device, device.function, 0x24) & !0xF;
                    let mut driver = crate::drivers::ahci::AhciDriver::new(bar5 as usize, self.phys_mem_offset);
                    if driver.init() {
                        if let Ok(fs) = fs::Fat32Volume::new(driver) {
                            unsafe { *FAT_FS.lock() = Some(fs); }
                        }
                    }
                    return true;
                }
                false
            }
            fn init(&mut self) -> Result<(), pos::PosError> { Ok(()) }
        }

        pdf.register_driver(alloc::boxed::Box::new(E1000PdfDriver { phys_mem_offset }));
        pdf.register_driver(alloc::boxed::Box::new(Rtl8139PdfDriver { phys_mem_offset }));
        pdf.register_driver(alloc::boxed::Box::new(AhciPdfDriver { phys_mem_offset }));
        pdf.register_driver(alloc::boxed::Box::new(crate::drivers::ps2_mouse::Ps2MouseDriver));
        
        crate::println!("  Calling pdf.init_all()...");
        pdf.init_all();
        crate::println!("  Calling pdf.probe_pci()...");
        pdf.probe_pci();
        crate::println!("  Driver Framework initialized.");
    }

    
    x86_64::instructions::interrupts::without_interrupts(|| {
        let mut sched = task::SCHEDULER.lock();
        sched.set_current_thread_as_main();
        sched.spawn(crate::drivers::speaker::speaker_server);
        unsafe { sched.spawn(crate::fat_pos::fat32_server_thread); }
        sched.spawn(crate::pd_loader::pd_scan_thread);
    });

    crate::shell::run();
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    let mut writer = VgaWriter::new();
    
    writer.set_color(Color::White, Color::Red);
    for _ in 0..15 { writer.write_string("\n"); }
    writer.write_string("\nKERNEL PANIC: ");
    if let Some(location) = info.location() {
        writer.write_string(location.file());
        writer.write_string(":");
        print_usize(&mut writer, location.line() as usize);
    }
    writer.write_string("\n");
    loop {
        x86_64::instructions::hlt();
    }
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

fn print_hex16(w: &mut VgaWriter, v: u16) {
    for shift in (0..16).rev().step_by(4) {
        let nibble = ((v >> shift) & 0xF) as u8;
        w.write_byte(if nibble < 10 { b'0' + nibble } else { b'a' + nibble - 10 });
    }
}

/// Public wrapper for shell.rs debug output.
pub fn print_usize_pub(w: &mut VgaWriter, n: usize) {
    print_usize(w, n);
}

/// Print 64-bit value as hex — used by shell.rs debug code.
pub fn print_hex64_pub(w: &mut VgaWriter, v: u64) {
    for shift in (0..64u32).rev().step_by(4) {
        let nibble = ((v >> shift) & 0xF) as u8;
        w.write_byte(if nibble < 10 { b'0' + nibble } else { b'a' + nibble - 10 });
    }
}
