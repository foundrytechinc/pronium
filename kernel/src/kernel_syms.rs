
/// Resolves a kernel symbol name to its virtual address.
///
/// Called by the ELF loader when a .pd module has an undefined (SHN_UNDEF)
/// reference to a kernel-exported symbol.  The driver module's object file
/// must use the exact names listed here.
pub fn resolve_symbol(name: &str) -> Option<usize> {
    match name {
        // Kernel ABI vtable — the primary symbol every .pd driver needs.
        "KERNEL_API" | "__kernel_api" =>
            Some(&crate::kernel_api::KERNEL_API as *const _ as usize),

        // Convenience: let drivers call pos::register_object directly if needed.
        "pos_register_object" =>
            Some(crate::pos::register_object as *const () as usize),

        // PCI helpers
        "pci_read" =>
            Some(crate::drivers::pci::pci_read as *const () as usize),
        "pci_find_device" =>
            Some(crate::drivers::pci::find_device as *const () as usize),

        // Memory page allocator
        "sys_allocate_pages" =>
            Some(crate::memory::sys_allocate_pages as *const () as usize),

        _ => None,
    }
}
