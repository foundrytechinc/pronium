

use x86_64::instructions::port::Port;

const PCI_CONFIG_ADDR: u16 = 0x0CF8;
const PCI_CONFIG_DATA: u16 = 0x0CFC;

#[derive(Debug, Clone, Copy)]
pub struct PciDevice {
    pub bus: u8,
    pub device: u8,
    pub function: u8,
    pub vendor_id: u16,
    pub device_id: u16,
    pub irq: u8,
    pub bar0: u32,
}

pub fn pci_read(bus: u8, dev: u8, func: u8, offset: u8) -> u32 {
    let addr: u32 = 0x8000_0000
        | ((bus as u32) << 16)
        | ((dev as u32) << 11)
        | ((func as u32) << 8)
        | ((offset as u32) & 0xFC);
    unsafe {
        Port::new(PCI_CONFIG_ADDR).write(addr);
        Port::<u32>::new(PCI_CONFIG_DATA).read()
    }
}

fn pci_write(bus: u8, dev: u8, func: u8, offset: u8, value: u32) {
    let addr: u32 = 0x8000_0000
        | ((bus as u32) << 16)
        | ((dev as u32) << 11)
        | ((func as u32) << 8)
        | ((offset as u32) & 0xFC);
    unsafe {
        Port::new(PCI_CONFIG_ADDR).write(addr);
        Port::new(PCI_CONFIG_DATA).write(value);
    }
}


pub fn check_device(bus: u8, dev: u8) -> Option<PciDevice> {
    let id = pci_read(bus, dev, 0, 0x00);
    if id == 0xFFFF_FFFF {
        return None;
    }
    let vid = (id & 0xFFFF) as u16;
    let did = ((id >> 16) & 0xFFFF) as u16;

    let bar0 = pci_read(bus, dev, 0, 0x10);
    let irq_reg = pci_read(bus, dev, 0, 0x3C);
    let irq = (irq_reg & 0xFF) as u8;

    Some(PciDevice {
        bus,
        device: dev,
        function: 0,
        vendor_id: vid,
        device_id: did,
        irq,
        bar0,
    })
}


pub fn find_device(vendor: u16, device_id: u16) -> Option<PciDevice> {
    for bus in 0u16..=255 {
        for dev in 0u8..32 {
            if let Some(d) = check_device(bus as u8, dev) {
                if d.vendor_id == vendor && d.device_id == device_id {
                    return Some(d);
                }
            }
        }
    }
    None
}


pub fn enable_bus_mastering(d: &PciDevice) {
    let cmd = pci_read(d.bus, d.device, d.function, 0x04);
    
    pci_write(d.bus, d.device, d.function, 0x04, cmd | 0x0000_0007);
}
