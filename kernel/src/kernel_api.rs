
use crate::vga::VgaWriter;

/// The stable kernel ABI passed to every .pd driver module via pd_init().
/// Add fields only at the END to maintain backward compatibility.
#[repr(C)]
pub struct KernelApi {
    /// ABI version — always check this first in your driver (current: 1).
    pub version: u32,

    /// Register a device in the POS (\Device\...) namespace.
    /// `path` must be a valid UTF-8 byte slice.  Returns 0 on success.
    pub register_device: extern "C" fn(path: *const u8, path_len: usize) -> i32,

    /// Read / write a single byte from an x86 I/O port.
    pub port_in8:  extern "C" fn(port: u16) -> u8,
    pub port_out8: extern "C" fn(port: u16, val: u8),

    /// Write a UTF-8 message to the kernel VGA console (prefixed with "[PD] ").
    pub klog: extern "C" fn(msg: *const u8, len: usize),

    /// Allocate `count` contiguous 4096-byte pages.  Returns null on failure.
    pub alloc_pages: extern "C" fn(count: usize) -> *mut u8,

    /// Raw PCI configuration-space read (32-bit).
    pub pci_read: extern "C" fn(bus: u8, dev: u8, func: u8, offset: u8) -> u32,

    /// Locate a PCI device by Vendor ID + Device ID.
    /// Returns 1 and fills *out_bus / *out_dev on success, 0 if not found.
    pub pci_find: extern "C" fn(vendor: u16, device: u16,
                                out_bus: *mut u8, out_dev: *mut u8) -> i32,
}

// ---------------------------------------------------------------------------
// ABI implementation functions (called via the vtable above)
// ---------------------------------------------------------------------------

extern "C" fn api_register_device(path: *const u8, path_len: usize) -> i32 {
    use alloc::sync::Arc;
    use crate::pos::PosObject;

    struct NullDevice;
    impl PosObject for NullDevice {}

    let bytes = unsafe { core::slice::from_raw_parts(path, path_len) };
    let Ok(path_str) = core::str::from_utf8(bytes) else { return -1 };

    let obj: Arc<spin::Mutex<dyn PosObject>> = Arc::new(spin::Mutex::new(NullDevice));
    match crate::pos::register_object(path_str, obj) {
        Ok(()) => 0,
        Err(_) => -1,
    }
}

extern "C" fn api_port_in8(port: u16) -> u8 {
    unsafe { x86_64::instructions::port::Port::<u8>::new(port).read() }
}

extern "C" fn api_port_out8(port: u16, val: u8) {
    unsafe { x86_64::instructions::port::Port::<u8>::new(port).write(val) }
}

extern "C" fn api_klog(msg: *const u8, len: usize) {
    let bytes = unsafe { core::slice::from_raw_parts(msg, len) };
    if let Ok(s) = core::str::from_utf8(bytes) {
        crate::println!("[PD] {}", s);
    }
}

extern "C" fn api_alloc_pages(count: usize) -> *mut u8 {
    crate::memory::sys_allocate_pages(count)
}

extern "C" fn api_pci_read(bus: u8, dev: u8, func: u8, offset: u8) -> u32 {
    crate::drivers::pci::pci_read(bus, dev, func, offset)
}

extern "C" fn api_pci_find(vendor: u16, device: u16,
                            out_bus: *mut u8, out_dev: *mut u8) -> i32 {
    match crate::drivers::pci::find_device(vendor, device) {
        Some(d) => {
            unsafe { *out_bus = d.bus; *out_dev = d.device; }
            1
        }
        None => 0,
    }
}

// ---------------------------------------------------------------------------
// The global vtable instance — passed by reference to every pd_init()
// ---------------------------------------------------------------------------
pub static KERNEL_API: KernelApi = KernelApi {
    version:         1,
    register_device: api_register_device,
    port_in8:        api_port_in8,
    port_out8:       api_port_out8,
    klog:            api_klog,
    alloc_pages:     api_alloc_pages,
    pci_read:        api_pci_read,
    pci_find:        api_pci_find,
};
