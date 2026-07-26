



#![allow(dead_code)]


pub const TCP_FLAG_FIN: u8 = 0x01;
pub const TCP_FLAG_SYN: u8 = 0x02;
pub const TCP_FLAG_RST: u8 = 0x04;
pub const TCP_FLAG_PSH: u8 = 0x08;
pub const TCP_FLAG_ACK: u8 = 0x10;


#[derive(Clone, Copy, PartialEq)]
pub enum TcpState {
    Closed,
    SynSent,
    Established,
    FinWait1,
    FinWait2,
    CloseWait,
    TimeWait,
}


const MAX_TCP_CONNECTIONS: usize = 4;

pub struct TcpConnection {
    pub state: TcpState,
    pub local_port: u16,
    pub remote_port: u16,
    pub remote_ip: [u8; 4],
    pub seq_num: u32,
    pub ack_num: u32,
}

impl TcpConnection {
    pub const fn empty() -> Self {
        TcpConnection {
            state: TcpState::Closed,
            local_port: 0,
            remote_port: 0,
            remote_ip: [0; 4],
            seq_num: 0,
            ack_num: 0,
        }
    }
}


pub struct TcpPool {
    pub connections: [TcpConnection; MAX_TCP_CONNECTIONS],
}

impl TcpPool {
    pub const fn new() -> Self {
        TcpPool {
            connections: [const { TcpConnection::empty() }; MAX_TCP_CONNECTIONS],
        }
    }

    
    pub fn alloc(
        &mut self,
        remote_ip: [u8; 4],
        remote_port: u16,
        local_port: u16,
    ) -> Option<usize> {
        for (i, conn) in self.connections.iter_mut().enumerate() {
            if conn.state == TcpState::Closed {
                conn.state = TcpState::SynSent;
                conn.remote_ip = remote_ip;
                conn.remote_port = remote_port;
                conn.local_port = local_port;
                conn.seq_num = 1000;
                conn.ack_num = 0;
                return Some(i);
            }
        }
        None
    }

    
    pub fn free(&mut self, idx: usize) {
        if idx < MAX_TCP_CONNECTIONS {
            self.connections[idx] = TcpConnection::empty();
        }
    }
}

pub static mut TCP_POOL: TcpPool = TcpPool::new();


pub fn tcp_checksum(src_ip: [u8; 4], dst_ip: [u8; 4], segment: &[u8]) -> u16 {
    let mut sum: u32 = 0;

    
    sum += u16::from_be_bytes([src_ip[0], src_ip[1]]) as u32;
    sum += u16::from_be_bytes([src_ip[2], src_ip[3]]) as u32;
    sum += u16::from_be_bytes([dst_ip[0], dst_ip[1]]) as u32;
    sum += u16::from_be_bytes([dst_ip[2], dst_ip[3]]) as u32;
    sum += 6u32; 
    sum += segment.len() as u32;

    
    let mut i = 0;
    while i + 1 < segment.len() {
        sum += u16::from_be_bytes([segment[i], segment[i + 1]]) as u32;
        i += 2;
    }
    if i < segment.len() {
        sum += (segment[i] as u32) << 8;
    }

    while (sum >> 16) != 0 {
        sum = (sum & 0xFFFF) + (sum >> 16);
    }

    !(sum as u16)
}




pub fn build_tcp_segment(
    src_port: u16,
    dst_port: u16,
    seq: u32,
    ack: u32,
    flags: u8,
    window: u16,
    src_ip: [u8; 4],
    dst_ip: [u8; 4],
    payload: &[u8],
    buf: &mut [u8],
) -> usize {
    let hdr_len: usize = 20;
    let total = hdr_len + payload.len();
    if buf.len() < total {
        return 0;
    }

    buf[0..2].copy_from_slice(&src_port.to_be_bytes());
    buf[2..4].copy_from_slice(&dst_port.to_be_bytes());
    buf[4..8].copy_from_slice(&seq.to_be_bytes());
    buf[8..12].copy_from_slice(&ack.to_be_bytes());
    buf[12] = 5 << 4; 
    buf[13] = flags;
    buf[14..16].copy_from_slice(&window.to_be_bytes());
    buf[16..18].copy_from_slice(&[0, 0]); 
    buf[18..20].copy_from_slice(&[0, 0]); 

    if !payload.is_empty() {
        buf[hdr_len..total].copy_from_slice(payload);
    }

    let cksum = tcp_checksum(src_ip, dst_ip, &buf[..total]);
    buf[16..18].copy_from_slice(&cksum.to_be_bytes());

    total
}
