









#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct IumHeader {
    pub magic: [u8; 4],
    pub version: u32,
    pub entry_point: u64,
    pub bss_size: u64,
}

impl IumHeader {
    
    pub fn is_valid(&self) -> bool {
        &self.magic == b"IUM!"
    }
}
