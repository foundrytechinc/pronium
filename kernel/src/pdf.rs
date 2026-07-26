use crate::pos::{PosError, PosObjectRef};
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

use crate::drivers::pci::PciDevice;

/// Record of a .pd module that has been successfully loaded.
pub struct LoadedPdModule {
    /// Null-terminated 8.3-compatible name (e.g. b"MYDRV   PD ").
    pub name: [u8; 11],
    /// Virtual address of the module's first page.
    pub base: usize,
}

pub trait Driver: Send + Sync {
    
    fn name(&self) -> &str;
    
    
    fn probe(&mut self, _device: &PciDevice) -> bool {
        false
    }

    
    fn probe_isa(&mut self) -> bool {
        false
    }
    
    
    fn init(&mut self) -> Result<(), PosError>;
}

pub struct DriverFramework {
    drivers:        Vec<alloc::boxed::Box<dyn Driver>>,
    /// Tracks FAT 8.3 names of .pd files already loaded to prevent duplicates.
    pub already_loaded: Vec<[u8; 11]>,
    /// Metadata for every successfully loaded module.
    pub loaded_modules: Vec<LoadedPdModule>,
}

impl DriverFramework {
    pub const fn new() -> Self {
        Self {
            drivers:        Vec::new(),
            already_loaded: Vec::new(),
            loaded_modules: Vec::new(),
        }
    }

    pub fn register_driver(&mut self, driver: alloc::boxed::Box<dyn Driver>) {
        self.drivers.push(driver);
    }

    pub fn register_device(&self, path: &str, device: PosObjectRef) -> Result<(), PosError> {
        crate::pos::register_object(path, device)
    }

    pub fn init_all(&mut self) {
        for driver in self.drivers.iter_mut() {
            
            if driver.probe_isa() {
                let _ = driver.init();
            }
        }
    }

    pub fn probe_pci(&mut self) {
        for bus in 0..=255 {
            for dev in 0..32 {
                if let Some(device) = crate::drivers::pci::check_device(bus as u8, dev) {
                    for driver in self.drivers.iter_mut() {
                        if driver.probe(&device) {
                            crate::println!("    [PCI] Probing driver: {}", driver.name());
                            let _ = driver.init();
                            crate::println!("    [PCI] Init complete for: {}", driver.name());
                            break;
                        }
                    }
                }
            }
        }
    }

    /// Load and execute a .pd driver module from raw ELF64 bytes.
    ///
    /// * `data`     — the raw bytes of the .pd (ELF64 relocatable) file.
    /// * `fat_name` — the FAT 8.3 directory-entry name (11 bytes, for duplicate detection).
    ///
    /// On success the module's `pd_init(&KERNEL_API)` is called.  The module
    /// is added to `loaded_modules` so it can be enumerated later.
    pub fn load_pd_module(&mut self, data: &[u8], fat_name: [u8; 11]) -> Result<(), &'static str> {
        use crate::vga::Color;

        // Duplicate check
        for n in &self.already_loaded {
            if n == &fat_name { return Ok(()); } // already loaded
        }

        if let Some(w) = crate::vga::WRITER.lock().as_mut() {
            w.set_color(Color::LightCyan, Color::Black);
            w.write_string("[PDF] Loading driver module...\n");
            w.set_color(Color::White, Color::Black);
        }

        // Parse & relocate the ELF
        let module = crate::elf_loader::load_elf(data).map_err(|_| {
            if let Some(w2) = crate::vga::WRITER.lock().as_mut() {
                w2.set_color(Color::Yellow, Color::Black);
                w2.write_string("[PDF] ELF load failed\n");
                w2.set_color(Color::White, Color::Black);
            }
            "ELF load error"
        })?;

        // Call pd_init(api: *const KernelApi)
        let init_fn: extern "C" fn(*const crate::kernel_api::KernelApi) -> i32 =
            unsafe { core::mem::transmute(module.entry) };

        let result = init_fn(&crate::kernel_api::KERNEL_API);

        if result != 0 {
            if let Some(w) = crate::vga::WRITER.lock().as_mut() {
                w.set_color(Color::Yellow, Color::Black);
                w.write_string("[PDF] pd_init() returned error\n");
                w.set_color(Color::White, Color::Black);
            }
            return Err("pd_init failed");
        }

        // Record
        self.already_loaded.push(fat_name);
        self.loaded_modules.push(LoadedPdModule { name: fat_name, base: module.base });

        if let Some(w) = crate::vga::WRITER.lock().as_mut() {
            w.set_color(Color::LightGreen, Color::Black);
            w.write_string("[PDF] Driver module loaded OK\n");
            w.set_color(Color::White, Color::Black);
        }
        Ok(())
    }
}

pub static PDF: Mutex<DriverFramework> = Mutex::new(DriverFramework::new());
