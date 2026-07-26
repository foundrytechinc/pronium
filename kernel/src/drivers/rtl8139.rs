



use x86_64::instructions::port::Port;
use x86_64::structures::paging::{OffsetPageTable, PageTable, Translate};
use x86_64::registers::control::Cr3;
use x86_64::VirtAddr;


const REG_MAC: u16 = 0x00;
const REG_TSD0: u16 = 0x10;   
const REG_TSAD0: u16 = 0x20;  
const REG_RBSTART: u16 = 0x30;
const REG_CMD: u16 = 0x37;
const REG_CAPR: u16 = 0x38;   
const REG_IMR: u16 = 0x3C;
const REG_ISR: u16 = 0x3E;
const REG_RCR: u16 = 0x44;
const REG_CONFIG1: u16 = 0x52;


const CMD_BUFE: u8 = 0x01;    
const CMD_TE: u8 = 0x04;      
const CMD_RE: u8 = 0x08;      
const CMD_RST: u8 = 0x10;     

const TSD_OWN: u32 = 1 << 13;
const TSD_TOK: u32 = 1 << 15;


const RCR_AAP: u32 = 1;       
const RCR_APM: u32 = 1 << 1;  
const RCR_AM: u32 = 1 << 2;   
const RCR_AB: u32 = 1 << 3;   
const RCR_WRAP: u32 = 1 << 7; 
const RCR_MXDMA: u32 = 7 << 8;  
const RCR_RBLEN_8K: u32 = 0 << 11;


const RX_BUF_LEN: usize = 8192 + 16 + 1500;
const TX_BUF_LEN: usize = 1536;

#[repr(C, align(16))]
struct RxBuf([u8; RX_BUF_LEN]);
#[repr(C, align(16))]
struct TxBuf([u8; TX_BUF_LEN]);

static mut RX_BUF: RxBuf = RxBuf([0u8; RX_BUF_LEN]);
static mut TX_BUFS: [TxBuf; 4] = [
    TxBuf([0u8; TX_BUF_LEN]),
    TxBuf([0u8; TX_BUF_LEN]),
    TxBuf([0u8; TX_BUF_LEN]),
    TxBuf([0u8; TX_BUF_LEN]),
];


pub struct Rtl8139 {
    io: u16,
    pub mac: [u8; 6],
    tx_cur: usize,
    rx_off: usize,
}

impl Rtl8139 {
    
    pub fn new(io_base: u16) -> Self {
        Self {
            io: io_base,
            mac: [0; 6],
            tx_cur: 0,
            rx_off: 0,
        }
    }

    
    pub fn init(&mut self, phys_mem_offset: u64) {
        let mapper = make_mapper(phys_mem_offset);
        unsafe {
            
            Port::<u8>::new(self.io + REG_CONFIG1).write(0x00);

            
            Port::<u8>::new(self.io + REG_CMD).write(CMD_RST);
            loop {
                let cmd: u8 = Port::<u8>::new(self.io + REG_CMD).read();
                if cmd & CMD_RST == 0 {
                    break;
                }
            }

            
            for i in 0..6u16 {
                self.mac[i as usize] = Port::<u8>::new(self.io + REG_MAC + i).read();
            }

            
            let rx_virt = VirtAddr::new(&raw const RX_BUF.0 as *const _ as u64);
            let rx_phys = mapper.translate_addr(rx_virt).expect("rx buf phys");
            Port::<u32>::new(self.io + REG_RBSTART).write(rx_phys.as_u64() as u32);

            
            Port::<u16>::new(self.io + REG_IMR).write(0x0005);

            
            let rcr = RCR_AAP | RCR_APM | RCR_AM | RCR_AB | RCR_WRAP | RCR_MXDMA | RCR_RBLEN_8K;
            Port::<u32>::new(self.io + REG_RCR).write(rcr);

            
            Port::<u8>::new(self.io + REG_CMD).write(CMD_RE | CMD_TE);
        }
    }

    
    pub fn send(&mut self, frame: &[u8], phys_mem_offset: u64) {
        let mapper = make_mapper(phys_mem_offset);
        let idx = self.tx_cur;
        let len = core::cmp::min(frame.len(), TX_BUF_LEN);

        unsafe {
            let tx_bufs_ptr = &raw mut TX_BUFS;
            core::ptr::copy_nonoverlapping(frame.as_ptr(), (*tx_bufs_ptr)[idx].0.as_mut_ptr(), len);

            let buf_virt = VirtAddr::new((*tx_bufs_ptr)[idx].0.as_ptr() as u64);
            let buf_phys = mapper.translate_addr(buf_virt).expect("tx buf phys");

            Port::<u32>::new(self.io + REG_TSAD0 + (idx as u16) * 4)
                .write(buf_phys.as_u64() as u32);

            
            Port::<u32>::new(self.io + REG_TSD0 + (idx as u16) * 4)
                .write(len as u32);

            
            loop {
                let s: u32 =
                    Port::<u32>::new(self.io + REG_TSD0 + (idx as u16) * 4).read();
                if s & (TSD_OWN | TSD_TOK) != 0 {
                    break;
                }
            }
        }
        self.tx_cur = (self.tx_cur + 1) % 4;
    }

    
    
    pub fn receive(&mut self, buf: &mut [u8]) -> Option<usize> {
        unsafe {
            let cmd: u8 = Port::<u8>::new(self.io + REG_CMD).read();
            if cmd & CMD_BUFE != 0 {
                return None; 
            }

            let off = self.rx_off;
            let rx_buf_ptr = &raw const RX_BUF.0 as *const u8;

            
            let status = u16::from_le_bytes([
                *rx_buf_ptr.add(off % RX_BUF_LEN),
                *rx_buf_ptr.add((off + 1) % RX_BUF_LEN),
            ]);
            let pkt_len = u16::from_le_bytes([
                *rx_buf_ptr.add((off + 2) % RX_BUF_LEN),
                *rx_buf_ptr.add((off + 3) % RX_BUF_LEN),
            ]) as usize;

            if status & 0x01 == 0 {
                
                let new_off = (off + 4 + pkt_len + 3) & !3;
                self.rx_off = new_off % RX_BUF_LEN;
                Port::<u16>::new(self.io + REG_CAPR)
                    .write((self.rx_off as u16).wrapping_sub(0x10));
                Port::<u16>::new(self.io + REG_ISR).write(0x0001);
                return None;
            }

            
            let data_len = if pkt_len >= 4 { pkt_len - 4 } else { 0 };
            let copy_len = core::cmp::min(data_len, buf.len());
            let data_start = off + 4;

            
            for i in 0..copy_len {
                buf[i] = *rx_buf_ptr.add((data_start + i) % RX_BUF_LEN);
            }

            
            let new_off = (off + 4 + pkt_len + 3) & !3;
            self.rx_off = new_off % RX_BUF_LEN;

            Port::<u16>::new(self.io + REG_CAPR)
                .write((self.rx_off as u16).wrapping_sub(0x10));
            
            Port::<u16>::new(self.io + REG_ISR).write(0x0001);

            Some(copy_len)
        }
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
