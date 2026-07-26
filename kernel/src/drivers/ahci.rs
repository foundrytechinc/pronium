



#![allow(dead_code)]

use crate::fs::BlockDevice;
use core::ptr::{read_volatile, write_volatile};
use x86_64::structures::paging::{OffsetPageTable, PageTable, Translate};
use x86_64::registers::control::Cr3;
use x86_64::VirtAddr;

fn make_mapper(phys_mem_offset: u64) -> OffsetPageTable<'static> {
    unsafe {
        let off = VirtAddr::new(phys_mem_offset);
        let (l4_frame, _) = Cr3::read();
        let l4_virt = off + l4_frame.start_address().as_u64();
        let l4_table: &'static mut PageTable = &mut *l4_virt.as_mut_ptr();
        OffsetPageTable::new(l4_table, off)
    }
}



#[repr(C)]
struct HbaPort {
    clb: u32,       
    clbu: u32,      
    fb: u32,        
    fbu: u32,       
    is: u32,        
    ie: u32,        
    cmd: u32,       
    _rsv0: u32,
    tfd: u32,       
    sig: u32,       
    ssts: u32,      
    sctl: u32,      
    serr: u32,      
    sact: u32,      
    ci: u32,        
    sntf: u32,      
    fbs: u32,       
    _res: [u32; 11],
    vendor: [u32; 4],
}

#[repr(C)]
struct HbaMem {
    cap: u32,       
    ghc: u32,       
    is: u32,        
    pi: u32,        
    vs: u32,        
    ccc_ctl: u32,   
    ccc_pts: u32,   
    em_loc: u32,    
    em_ctl: u32,    
    cap2: u32,      
    bohc: u32,      
    _res: [u8; 0x74],
    vendor: [u8; 0x60],
    ports: [HbaPort; 32],
}

#[derive(Clone, Copy)]
#[repr(C, packed)]
struct HbaCmdHeader {
    
    cfl: u8,        
    flags: u8,      
    prdtl: u16,     
    
    prdbc: u32,     
    
    ctba: u32,      
    ctbau: u32,     
    
    res1: [u32; 4],
}

#[derive(Clone, Copy)]
#[repr(C, packed)]
struct HbaPrdtEntry {
    dba: u32,       
    dbau: u32,      
    res0: u32,      
    dbc: u32,       
}

#[derive(Clone, Copy)]
#[repr(C, packed)]
struct HbaCmdTable {
    cfis: [u8; 64], 
    acmd: [u8; 16], 
    res: [u8; 48],  
    prdt_entry: [HbaPrdtEntry; 1], 
}



#[repr(C, align(1024))]
struct AhciMemory {
    cmd_list: [HbaCmdHeader; 32],
    rx_fis: [u8; 256],
    cmd_table: HbaCmdTable,
    dma_buf: [u8; 512],
}

static AHCI_MEM: spin::Mutex<AhciMemory> = spin::Mutex::new(AhciMemory {
    cmd_list: [HbaCmdHeader {
        cfl: 0, flags: 0, prdtl: 0, prdbc: 0, ctba: 0, ctbau: 0, res1: [0; 4]
    }; 32],
    rx_fis: [0; 256],
    cmd_table: HbaCmdTable {
        cfis: [0; 64], acmd: [0; 16], res: [0; 48],
        prdt_entry: [HbaPrdtEntry { dba: 0, dbau: 0, res0: 0, dbc: 0 }],
    },
    dma_buf: [0; 512],
});

pub struct AhciDriver {
    hba_base: *mut HbaMem,
    phys_mem_offset: u64,
    port_index: Option<usize>,
}

unsafe impl Send for AhciDriver {}
unsafe impl Sync for AhciDriver {}

const HBA_PX_CMD_ST: u32 = 0x0001;
const HBA_PX_CMD_FRE: u32 = 0x0010;
const HBA_PX_CMD_FR: u32 = 0x4000;
const HBA_PX_CMD_CR: u32 = 0x8000;

impl AhciDriver {
    pub fn new(bar5: usize, phys_mem_offset: u64) -> Self {
        AhciDriver {
            hba_base: (bar5 + phys_mem_offset as usize) as *mut HbaMem,
            phys_mem_offset,
            port_index: None,
        }
    }

    pub fn init(&mut self) -> bool {
        let hba = unsafe { &mut *self.hba_base };
        
        
        let pi = unsafe { read_volatile(&hba.pi) };
        for i in 0..32 {
            if (pi & (1 << i)) != 0 {
                let sig = unsafe { read_volatile(&hba.ports[i].sig) };
                if sig == 0x00000101 { 
                    self.port_index = Some(i);
                    break;
                }
            }
        }

        if let Some(pi) = self.port_index {
            self.rebase_port(pi);
            return true;
        }
        false
    }

    fn rebase_port(&mut self, port_no: usize) {
        let port = unsafe { &mut (*self.hba_base).ports[port_no] };
        
        self.stop_cmd(port);

        unsafe {
            let mapper = make_mapper(self.phys_mem_offset);
            let mut mem_guard = AHCI_MEM.lock();
            let mem = &mut *mem_guard;

            let clb_virt = VirtAddr::new(mem as *const _ as u64 + core::mem::offset_of!(AhciMemory, cmd_list) as u64);
            let clb_phys = mapper.translate_addr(clb_virt).expect("clb phys").as_u64();
            write_volatile(&mut port.clb, clb_phys as u32);
            write_volatile(&mut port.clbu, (clb_phys >> 32) as u32);

            let fb_virt = VirtAddr::new(mem as *const _ as u64 + core::mem::offset_of!(AhciMemory, rx_fis) as u64);
            let fb_phys = mapper.translate_addr(fb_virt).expect("fb phys").as_u64();
            write_volatile(&mut port.fb, fb_phys as u32);
            write_volatile(&mut port.fbu, (fb_phys >> 32) as u32);

            let ctba_virt = VirtAddr::new(mem as *const _ as u64 + core::mem::offset_of!(AhciMemory, cmd_table) as u64);
            let ctba_phys = mapper.translate_addr(ctba_virt).expect("ctba phys").as_u64();
            (*mem).cmd_list[0].ctba = ctba_phys as u32;
            (*mem).cmd_list[0].ctbau = (ctba_phys >> 32) as u32;
            (*mem).cmd_list[0].prdtl = 1;
        }

        self.start_cmd(port);
    }

    fn start_cmd(&mut self, port: &mut HbaPort) {
        unsafe {
            let mut spin = 1_000_000;
            while (read_volatile(&port.cmd) & HBA_PX_CMD_CR) != 0 && spin > 0 {
                spin -= 1;
            }
            let cmd = read_volatile(&port.cmd);
            write_volatile(&mut port.cmd, cmd | HBA_PX_CMD_FRE | HBA_PX_CMD_ST);
        }
    }

    fn stop_cmd(&mut self, port: &mut HbaPort) {
        unsafe {
            let cmd = read_volatile(&port.cmd);
            write_volatile(&mut port.cmd, cmd & !HBA_PX_CMD_ST);
            let mut spin = 1_000_000;
            while (read_volatile(&port.cmd) & HBA_PX_CMD_CR) != 0 && spin > 0 {
                spin -= 1;
            }
            let cmd2 = read_volatile(&port.cmd);
            write_volatile(&mut port.cmd, cmd2 & !HBA_PX_CMD_FRE);
            spin = 1_000_000;
            while (read_volatile(&port.cmd) & HBA_PX_CMD_FR) != 0 && spin > 0 {
                spin -= 1;
            }
        }
    }

    fn read(&mut self, lba: u32, buf_out: &mut [u8; 512]) -> Result<(), &'static str> {
        let pi = self.port_index.ok_or("No port")?;
        let port = unsafe { &mut (*self.hba_base).ports[pi] };

        unsafe {
            let mapper = make_mapper(self.phys_mem_offset);
            let mut mem_guard = loop {
                if let Some(guard) = AHCI_MEM.try_lock() {
                    break guard;
                }
                crate::task::yield_now();
            };
            let mem = &mut *mem_guard;
            
            // Clear interrupt status
            write_volatile(&mut port.is, 0xFFFF_FFFF);

            // Setup PRDT
            let buf_virt = VirtAddr::new(mem.dma_buf.as_ptr() as u64);
            let buf_phys = mapper.translate_addr(buf_virt).expect("dma buf phys").as_u64();
            mem.cmd_table.prdt_entry[0].dba = buf_phys as u32;
            mem.cmd_table.prdt_entry[0].dbau = (buf_phys >> 32) as u32;
            mem.cmd_table.prdt_entry[0].dbc = 511; // 512 bytes - 1

            // Setup Command Header
            mem.cmd_list[0].cfl = 5; // FIS length in DWORDS
            mem.cmd_list[0].flags = 0; // Read
            mem.cmd_list[0].prdbc = 0;

            // Setup Command FIS (Register H2D)
            mem.cmd_table.cfis[0] = 0x27; // FIS_TYPE_REG_H2D
            mem.cmd_table.cfis[1] = 0x80; // Command bit (Bit 7)
            mem.cmd_table.cfis[2] = 0x20; // ATA_CMD_READ_PIO
            
            mem.cmd_table.cfis[4] = (lba & 0xFF) as u8;
            mem.cmd_table.cfis[5] = ((lba >> 8) & 0xFF) as u8;
            mem.cmd_table.cfis[6] = ((lba >> 16) & 0xFF) as u8;
            mem.cmd_table.cfis[7] = 0x40; // LBA mode
            
            mem.cmd_table.cfis[8] = ((lba >> 24) & 0xFF) as u8;
            mem.cmd_table.cfis[12] = 1; // Count = 1 sector

            // Wait until port is ready (BSY and DRQ are clear)
            let mut spin = 1_000_000;
            while spin > 0 {
                if (read_volatile(&port.tfd) & (0x80 | 0x08)) == 0 {
                    break;
                }
                crate::task::yield_now();
                spin -= 1;
            }
            if spin == 0 {
                return Err("AHCI port not ready");
            }

            // Issue command
            write_volatile(&mut port.ci, 1);

            // Wait for completion (polling)
            let mut timeout = 1000000;
            while timeout > 0 {
                if (read_volatile(&port.ci) & 1) == 0 {
                    break;
                }
                crate::task::yield_now();
                timeout -= 1;
            }

            if timeout == 0 {
                return Err("AHCI read timeout");
            }

            buf_out.copy_from_slice(&mem.dma_buf);
        }
        Ok(())
    }

    fn write(&mut self, lba: u32, buf_in: &[u8; 512]) -> Result<(), &'static str> {
        let pi = self.port_index.ok_or("No port")?;
        let port = unsafe { &mut (*self.hba_base).ports[pi] };

        unsafe {
            let mapper = make_mapper(self.phys_mem_offset);
            let mut mem_guard = loop {
                if let Some(guard) = AHCI_MEM.try_lock() {
                    break guard;
                }
                crate::task::yield_now();
            };
            let mem = &mut *mem_guard;
            
            
            mem.dma_buf.copy_from_slice(buf_in);

            
            write_volatile(&mut port.is, 0xFFFF_FFFF);

            
            let buf_virt = VirtAddr::new(mem.dma_buf.as_ptr() as u64);
            let buf_phys = mapper.translate_addr(buf_virt).expect("dma buf phys").as_u64();
            mem.cmd_table.prdt_entry[0].dba = buf_phys as u32;
            mem.cmd_table.prdt_entry[0].dbau = (buf_phys >> 32) as u32;
            mem.cmd_table.prdt_entry[0].dbc = 511; 

            
            mem.cmd_list[0].cfl = 5 | 0x40; 
            mem.cmd_list[0].flags = 0;
            mem.cmd_list[0].prdbc = 0;

            
            mem.cmd_table.cfis[0] = 0x27; 
            mem.cmd_table.cfis[1] = 0x80; 
            mem.cmd_table.cfis[2] = 0x30; 
            
            mem.cmd_table.cfis[4] = (lba & 0xFF) as u8;
            mem.cmd_table.cfis[5] = ((lba >> 8) & 0xFF) as u8;
            mem.cmd_table.cfis[6] = ((lba >> 16) & 0xFF) as u8;
            mem.cmd_table.cfis[7] = 0x40; 
            
            mem.cmd_table.cfis[8] = ((lba >> 24) & 0xFF) as u8;
            mem.cmd_table.cfis[12] = 1; 

            
            let mut spin = 1_000_000;
            while spin > 0 {
                if (read_volatile(&port.tfd) & (0x80 | 0x08)) == 0 {
                    break;
                }
                crate::task::yield_now();
                spin -= 1;
            }
            if spin == 0 {
                return Err("AHCI port not ready");
            }

            
            write_volatile(&mut port.ci, 1);

            
            let mut timeout = 1000000;
            while timeout > 0 {
                if (read_volatile(&port.ci) & 1) == 0 {
                    break;
                }
                crate::task::yield_now();
                timeout -= 1;
            }

            if timeout == 0 {
                return Err("AHCI write timeout");
            }
        }
        Ok(())
    }
}

impl BlockDevice for AhciDriver {
    fn read_sector(&mut self, lba: u32, buffer: &mut [u8; 512]) -> Result<(), &'static str> {
        self.read(lba, buffer)
    }
    fn write_sector(&mut self, lba: u32, buffer: &[u8; 512]) -> Result<(), &'static str> {
        self.write(lba, buffer)
    }
}
