use crate::drivers::pci;
use x86_64::instructions::port::Port;
use spin::Mutex;

const VBE_DISPI_IOPORT_INDEX: u16 = 0x01CE;
const VBE_DISPI_IOPORT_DATA: u16 = 0x01CF;

const VBE_DISPI_INDEX_ID: u16 = 0;
const VBE_DISPI_INDEX_XRES: u16 = 1;
const VBE_DISPI_INDEX_YRES: u16 = 2;
const VBE_DISPI_INDEX_BPP: u16 = 3;
const VBE_DISPI_INDEX_ENABLE: u16 = 4;

const VBE_DISPI_ENABLE: u16 = 0x01;
const VBE_DISPI_LFB_ENABLED: u16 = 0x40;
const VBE_DISPI_NOCLEARMEM: u16 = 0x80;

pub fn init_bochs_vga(phys_mem_offset: u64, width: u16, height: u16) -> Option<()> {
    
    let device = pci::find_device(0x1234, 0x1111)?;
    
    pci::enable_bus_mastering(&device);
    
    
    let bar0 = pci::pci_read(device.bus, device.device, device.function, 0x10);
    let fb_phys = (bar0 & !0xF) as u64;
    let fb_virt = phys_mem_offset + fb_phys;

    let mut index_port = Port::<u16>::new(VBE_DISPI_IOPORT_INDEX);
    let mut data_port = Port::<u16>::new(VBE_DISPI_IOPORT_DATA);

    unsafe {
        
        index_port.write(VBE_DISPI_INDEX_ENABLE);
        data_port.write(0);

        
        index_port.write(VBE_DISPI_INDEX_XRES);
        data_port.write(width);

        
        index_port.write(VBE_DISPI_INDEX_YRES);
        data_port.write(height);

        
        index_port.write(VBE_DISPI_INDEX_BPP);
        data_port.write(32);

        
        index_port.write(VBE_DISPI_INDEX_ENABLE);
        data_port.write(VBE_DISPI_ENABLE | VBE_DISPI_LFB_ENABLED);
    }

    
    crate::framebuffer::init_framebuffer(
        fb_virt as usize,
        width as usize,
        height as usize,
        (width * 4) as usize, 
        4
    );

    Some(())
}
