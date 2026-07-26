



use x86_64::VirtAddr;
use x86_64::structures::paging::{OffsetPageTable, PageTable, Translate};
use x86_64::registers::control::Cr3;
use core::ptr::{read_volatile, write_volatile};


const REG_CTRL: usize = 0x0000;
const REG_STATUS: usize = 0x0008;
const REG_EERD: usize = 0x0014; 
const REG_ICR: usize = 0x00C0;  
const REG_IMS: usize = 0x00D0;  
const REG_RCTL: usize = 0x0100;
const REG_TCTL: usize = 0x0400;
const REG_RDBAL: usize = 0x2800;
const REG_RDBAH: usize = 0x2804;
const REG_RDLEN: usize = 0x2808;
const REG_RDH: usize = 0x2810;
const REG_RDT: usize = 0x2818;
const REG_TDBAL: usize = 0x3800;
const REG_TDBAH: usize = 0x3804;
const REG_TDLEN: usize = 0x3808;
const REG_TDH: usize = 0x3810;
const REG_TDT: usize = 0x3818;
const REG_MTA: usize = 0x5200; 
const REG_RAL: usize = 0x5400;
const REG_RAH: usize = 0x5404;


#[repr(C, packed)]
struct RxDesc {
    addr: u64,
    length: u16,
    csum: u16,
    status: u8,
    errors: u8,
    special: u16,
}

#[repr(C, packed)]
struct TxDesc {
    addr: u64,
    length: u16,
    cso: u8,
    cmd: u8,
    status: u8,
    css: u8,
    special: u16,
}

const NUM_RX_DESC: usize = 32;
const NUM_TX_DESC: usize = 8;
const RX_BUF_SIZE: usize = 2048;
const TX_BUF_SIZE: usize = 2048;

#[repr(C, align(16))]
struct RxRing([RxDesc; NUM_RX_DESC]);
#[repr(C, align(16))]
struct TxRing([TxDesc; NUM_TX_DESC]);

#[repr(C, align(16))]
struct RxBuffers([[u8; RX_BUF_SIZE]; NUM_RX_DESC]);
#[repr(C, align(16))]
struct TxBuffers([[u8; TX_BUF_SIZE]; NUM_TX_DESC]);


static mut RX_RING: RxRing = RxRing(unsafe { core::mem::zeroed() });
static mut TX_RING: TxRing = TxRing(unsafe { core::mem::zeroed() });
static mut RX_BUFS: RxBuffers = RxBuffers([[0; RX_BUF_SIZE]; NUM_RX_DESC]);
static mut TX_BUFS: TxBuffers = TxBuffers([[0; TX_BUF_SIZE]; NUM_TX_DESC]);

pub struct E1000 {
    mmio_base: u64,
    pub mac: [u8; 6],
    rx_cur: usize,
    tx_cur: usize,
    eeprom_exists: bool,
}

impl E1000 {
    pub fn new(bar0: u32, phys_mem_offset: u64) -> Self {
        let phys_base = (bar0 & !0xF) as u64; 
        let mmio_base = phys_mem_offset + phys_base;
        Self {
            mmio_base,
            mac: [0; 6],
            rx_cur: 0,
            tx_cur: 0,
            eeprom_exists: false,
        }
    }

    fn write_reg(&self, offset: usize, val: u32) {
        unsafe {
            let addr = (self.mmio_base + offset as u64) as *mut u32;
            write_volatile(addr, val);
        }
    }

    fn read_reg(&self, offset: usize) -> u32 {
        unsafe {
            let addr = (self.mmio_base + offset as u64) as *const u32;
            read_volatile(addr)
        }
    }

    fn detect_eeprom(&mut self) -> bool {
        self.write_reg(REG_EERD, 1);
        for _ in 0..1000 {
            let val = self.read_reg(REG_EERD);
            if (val & (1 << 4)) != 0 {
                self.eeprom_exists = true;
                return true;
            }
        }
        false
    }

    fn read_eeprom(&self, addr: u8) -> u16 {
        let mut temp = 0;
        if self.eeprom_exists {
            self.write_reg(REG_EERD, 1 | ((addr as u32) << 8));
            loop {
                temp = self.read_reg(REG_EERD);
                if (temp & (1 << 4)) != 0 {
                    break;
                }
            }
        } else {
            self.write_reg(REG_EERD, 1 | ((addr as u32) << 2));
            loop {
                temp = self.read_reg(REG_EERD);
                if (temp & (1 << 1)) != 0 {
                    break;
                }
            }
        }
        (temp >> 16) as u16
    }

    fn read_mac(&mut self) {
        let mac_low = self.read_reg(REG_RAL);
        let mac_high = self.read_reg(REG_RAH);
        self.mac[0] = (mac_low & 0xFF) as u8;
        self.mac[1] = ((mac_low >> 8) & 0xFF) as u8;
        self.mac[2] = ((mac_low >> 16) & 0xFF) as u8;
        self.mac[3] = ((mac_low >> 24) & 0xFF) as u8;
        self.mac[4] = (mac_high & 0xFF) as u8;
        self.mac[5] = ((mac_high >> 8) & 0xFF) as u8;
    }

    pub fn init(&mut self, phys_mem_offset: u64) {
        let mapper = make_mapper(phys_mem_offset);

        
        self.read_mac();

        
        for i in 0..128 {
            self.write_reg(REG_MTA + i * 4, 0);
        }

        unsafe {
            
            let rx_virt = VirtAddr::new(&raw const RX_RING.0 as *const _ as u64);
            let rx_phys = mapper.translate_addr(rx_virt).expect("rx ring phys").as_u64();
            self.write_reg(REG_RDBAL, rx_phys as u32);
            self.write_reg(REG_RDBAH, (rx_phys >> 32) as u32);
            self.write_reg(REG_RDLEN, (NUM_RX_DESC * core::mem::size_of::<RxDesc>()) as u32);
            self.write_reg(REG_RDH, 0);
            self.write_reg(REG_RDT, (NUM_RX_DESC - 1) as u32);

            let rx_ring_ptr = &raw mut RX_RING.0;
            for i in 0..NUM_RX_DESC {
                let buf_virt = VirtAddr::new(&raw const RX_BUFS.0[i] as *const _ as u64);
                let buf_phys = mapper.translate_addr(buf_virt).expect("rx buf phys").as_u64();
                (*rx_ring_ptr)[i].addr = buf_phys;
                (*rx_ring_ptr)[i].status = 0;
            }

            
            let rctl = (1 << 1) | 
                       (1 << 3) | 
                       (1 << 4) | 
                       (1 << 15)| 
                       (0 << 16)| 
                       (1 << 26); 
            self.write_reg(REG_RCTL, rctl);

            
            let tx_virt = VirtAddr::new(&raw const TX_RING.0 as *const _ as u64);
            let tx_phys = mapper.translate_addr(tx_virt).expect("tx ring phys").as_u64();
            self.write_reg(REG_TDBAL, tx_phys as u32);
            self.write_reg(REG_TDBAH, (tx_phys >> 32) as u32);
            self.write_reg(REG_TDLEN, (NUM_TX_DESC * core::mem::size_of::<TxDesc>()) as u32);
            self.write_reg(REG_TDH, 0);
            self.write_reg(REG_TDT, 0);

            
            let tctl = (1 << 1) | 
                       (1 << 3) | 
                       (15 << 4)| 
                       (64 << 12); 
            self.write_reg(REG_TCTL, tctl);
        }
        
        
        self.write_reg(REG_IMS, 0);
        
        let _ = self.read_reg(REG_ICR);
    }

    pub fn send(&mut self, frame: &[u8], _phys_mem_offset: u64) {
        let idx = self.tx_cur;
        let len = core::cmp::min(frame.len(), TX_BUF_SIZE);

        unsafe {
            let tx_bufs_ptr = &raw mut TX_BUFS.0;
            core::ptr::copy_nonoverlapping(frame.as_ptr(), (*tx_bufs_ptr)[idx].as_mut_ptr(), len);
            let mapper = make_mapper(_phys_mem_offset);
            let buf_virt = VirtAddr::new((*tx_bufs_ptr)[idx].as_ptr() as u64);
            let buf_phys = mapper.translate_addr(buf_virt).expect("tx buf phys").as_u64();
            
            let tx_ring_ptr = &raw mut TX_RING.0;
            (*tx_ring_ptr)[idx].addr = buf_phys;
            (*tx_ring_ptr)[idx].length = len as u16;
            (*tx_ring_ptr)[idx].cmd = (1 << 3) | (1 << 1) | (1 << 0); 
            (*tx_ring_ptr)[idx].status = 0;

            self.tx_cur = (self.tx_cur + 1) % NUM_TX_DESC;
            self.write_reg(REG_TDT, self.tx_cur as u32);

            
            loop {
                let status = core::ptr::read_volatile(&raw const (*tx_ring_ptr)[idx].status);
                if (status & 1) != 0 { 
                    break;
                }
            }
        }
    }

    pub fn receive(&mut self, buf: &mut [u8]) -> Option<usize> {
        let idx = self.rx_cur;
        unsafe {
            let rx_ring_ptr = &raw mut RX_RING.0;
            let status = core::ptr::read_volatile(&raw const (*rx_ring_ptr)[idx].status);
            if (status & 1) != 0 { 
                let len = (*rx_ring_ptr)[idx].length as usize;
                let copy_len = core::cmp::min(len, buf.len());
                
                let rx_bufs_ptr = &raw const RX_BUFS.0;
                core::ptr::copy_nonoverlapping((*rx_bufs_ptr)[idx].as_ptr(), buf.as_mut_ptr(), copy_len);

                
                (*rx_ring_ptr)[idx].status = 0;
                
                let old_cur = self.rx_cur;
                self.rx_cur = (self.rx_cur + 1) % NUM_RX_DESC;
                self.write_reg(REG_RDT, old_cur as u32);
                
                return Some(copy_len);
            }
        }
        None
    }
}

fn make_mapper(phys_mem_offset: u64) -> OffsetPageTable<'static> {
    unsafe {
        let off = VirtAddr::new(phys_mem_offset);
        let (l4_frame, _) = Cr3::read();
        let l4_virt = off + l4_frame.start_address().as_u64();
        let l4_table: &'static mut PageTable = &mut *l4_virt.as_mut_ptr();
        OffsetPageTable::new(l4_table, off)
    }
}
