
use crate::pdf::Driver;
use crate::pos::PosError;
use crate::drivers::pci::PciDevice;
use crate::drivers::ahci::AhciDriver;
use crate::fs::Fat32Volume;
use crate::FAT_FS;
use alloc::boxed::Box;

pub struct AhciPdfDriver {
    phys_mem_offset: u64,
}

impl AhciPdfDriver {
    pub fn new(phys_mem_offset: u64) -> Self {
        Self { phys_mem_offset }
    }
}

impl Driver for AhciPdfDriver {
    fn name(&self) -> &str {
        "AHCI SATA Driver"
    }

    fn probe(&mut self, device: &PciDevice) -> bool {
        let class = crate::drivers::pci::pci_read(device.bus, device.device, device.function, 0x08) >> 16;
        let class_id = (class >> 8) as u8;
        let subclass = class as u8;
        if class_id == 0x01 && subclass == 0x06 {
            crate::drivers::pci::enable_bus_mastering(device);
            let bar5 = crate::drivers::pci::pci_read(device.bus, device.device, device.function, 0x24) & !0xF;
            let mut driver = AhciDriver::new(bar5 as usize, self.phys_mem_offset);
            if driver.init() {
                if let Ok(fs) = Fat32Volume::new(driver) {
                    unsafe { *FAT_FS.lock() = Some(fs); }
                }
            }
            return true;
        }
        false
    }

    fn init(&mut self) -> Result<(), PosError> {
        Ok(())
    }
}
