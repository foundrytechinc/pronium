




use crate::drivers::rtl8139::Rtl8139;
use crate::drivers::e1000::E1000;
use crate::vga::{VgaWriter, Color};

pub enum NetworkCard {
    Rtl8139(Rtl8139),
    E1000(E1000),
}

impl NetworkCard {
    pub fn mac(&self) -> [u8; 6] {
        match self {
            NetworkCard::Rtl8139(nic) => nic.mac,
            NetworkCard::E1000(nic) => nic.mac,
        }
    }
    
    pub fn send(&mut self, frame: &[u8], phys_mem_offset: u64) {
        match self {
            NetworkCard::Rtl8139(nic) => nic.send(frame, phys_mem_offset),
            NetworkCard::E1000(nic) => nic.send(frame, phys_mem_offset),
        }
    }
    
    pub fn receive(&mut self, buf: &mut [u8]) -> Option<usize> {
        match self {
            NetworkCard::Rtl8139(nic) => nic.receive(buf),
            NetworkCard::E1000(nic) => nic.receive(buf),
        }
    }
}


pub const OUR_IP: [u8; 4] = [10, 0, 2, 15];
pub const GATEWAY_IP: [u8; 4] = [10, 0, 2, 2];
pub const SUBNET_MASK: [u8; 4] = [255, 255, 255, 0];
const BROADCAST_MAC: [u8; 6] = [0xFF; 6];

pub static mut DNS_SERVER: [u8; 4] = [8, 8, 8, 8];

pub fn set_dns(ip: [u8; 4]) {
    unsafe {
        let ptr = &raw mut DNS_SERVER;
        *ptr = ip;
    }
}

pub fn get_dns() -> [u8; 4] {
    unsafe {
        let ptr = &raw const DNS_SERVER;
        *ptr
    }
}

const ETH_ARP: u16 = 0x0806;
const ETH_IPV4: u16 = 0x0800;
const ETH_HDR: usize = 14;


fn build_eth(buf: &mut [u8], dst: [u8; 6], src: [u8; 6], etype: u16, payload: &[u8]) -> usize {
    buf[0..6].copy_from_slice(&dst);
    buf[6..12].copy_from_slice(&src);
    buf[12..14].copy_from_slice(&etype.to_be_bytes());
    let plen = payload.len();
    buf[14..14 + plen].copy_from_slice(payload);
    14 + plen
}


fn build_arp_request(buf: &mut [u8], src_mac: [u8; 6], src_ip: [u8; 4], target_ip: [u8; 4]) -> usize {
    let mut arp = [0u8; 28];
    arp[0..2].copy_from_slice(&[0x00, 0x01]); 
    arp[2..4].copy_from_slice(&[0x08, 0x00]); 
    arp[4] = 6; 
    arp[5] = 4; 
    arp[6..8].copy_from_slice(&[0x00, 0x01]); 
    arp[8..14].copy_from_slice(&src_mac);
    arp[14..18].copy_from_slice(&src_ip);
    
    arp[24..28].copy_from_slice(&target_ip);
    build_eth(buf, BROADCAST_MAC, src_mac, ETH_ARP, &arp)
}

fn parse_arp_reply(pkt: &[u8]) -> Option<([u8; 4], [u8; 6])> {
    if pkt.len() < 42 {
        return None;
    }
    let etype = u16::from_be_bytes([pkt[12], pkt[13]]);
    if etype != ETH_ARP {
        return None;
    }
    let op = u16::from_be_bytes([pkt[20], pkt[21]]);
    if op != 2 {
        return None;
    } 
    let mut mac = [0u8; 6];
    let mut ip = [0u8; 4];
    mac.copy_from_slice(&pkt[22..28]);
    ip.copy_from_slice(&pkt[28..32]);
    Some((ip, mac))
}


fn inet_cksum(data: &[u8]) -> u16 {
    let mut sum: u32 = 0;
    let mut i = 0;
    while i + 1 < data.len() {
        sum += u16::from_be_bytes([data[i], data[i + 1]]) as u32;
        i += 2;
    }
    if i < data.len() {
        sum += (data[i] as u32) << 8;
    }
    while (sum >> 16) != 0 {
        sum = (sum & 0xFFFF) + (sum >> 16);
    }
    !(sum as u16)
}

fn build_ipv4(buf: &mut [u8], src: [u8; 4], dst: [u8; 4], proto: u8, payload: &[u8]) -> usize {
    let total: u16 = 20 + payload.len() as u16;
    buf[0] = 0x45; 
    buf[1] = 0x00;
    buf[2..4].copy_from_slice(&total.to_be_bytes());
    buf[4..6].copy_from_slice(&[0x00, 0x01]); 
    buf[6..8].copy_from_slice(&[0x00, 0x00]); 
    buf[8] = 64; 
    buf[9] = proto;
    buf[10..12].copy_from_slice(&[0x00, 0x00]); 
    buf[12..16].copy_from_slice(&src);
    buf[16..20].copy_from_slice(&dst);
    let cksum = inet_cksum(&buf[0..20]);
    buf[10..12].copy_from_slice(&cksum.to_be_bytes());
    buf[20..20 + payload.len()].copy_from_slice(payload);
    20 + payload.len()
}


fn build_icmp_echo(buf: &mut [u8], id: u16, seq: u16) -> usize {
    buf[0] = 8; 
    buf[1] = 0; 
    buf[2..4].copy_from_slice(&[0x00, 0x00]); 
    buf[4..6].copy_from_slice(&id.to_be_bytes());
    buf[6..8].copy_from_slice(&seq.to_be_bytes());
    
    for i in 0..8usize {
        buf[8 + i] = i as u8;
    }
    let cksum = inet_cksum(&buf[0..16]);
    buf[2..4].copy_from_slice(&cksum.to_be_bytes());
    16
}


pub fn print_ip(w: &mut VgaWriter, ip: [u8; 4]) {
    for (i, &o) in ip.iter().enumerate() {
        if i > 0 {
            w.write_byte(b'.');
        }
        print_u8(w, o);
    }
}

pub fn print_mac(w: &mut VgaWriter, mac: [u8; 6]) {
    for (i, &b) in mac.iter().enumerate() {
        if i > 0 {
            w.write_byte(b':');
        }
        let hi = b >> 4;
        let lo = b & 0x0F;
        w.write_byte(hex_nibble(hi));
        w.write_byte(hex_nibble(lo));
    }
}

fn hex_nibble(n: u8) -> u8 {
    if n < 10 { b'0' + n } else { b'a' + n - 10 }
}

fn print_u8(w: &mut VgaWriter, mut n: u8) {
    if n == 0 {
        w.write_byte(b'0');
        return;
    }
    let mut tmp = [0u8; 3];
    let mut i = 0;
    while n > 0 {
        tmp[i] = b'0' + n % 10;
        n /= 10;
        i += 1;
    }
    while i > 0 {
        i -= 1;
        w.write_byte(tmp[i]);
    }
}

pub fn parse_ip(s: &str) -> Option<[u8; 4]> {
    let mut ip = [0u8; 4];
    let mut idx = 0usize;
    for part in s.split('.') {
        if idx >= 4 {
            return None;
        }
        let mut n: u16 = 0;
        if part.is_empty() {
            return None;
        }
        for c in part.bytes() {
            if !(b'0'..=b'9').contains(&c) {
                return None;
            }
            n = n * 10 + (c - b'0') as u16;
            if n > 255 {
                return None;
            }
        }
        ip[idx] = n as u8;
        idx += 1;
    }
    if idx == 4 { Some(ip) } else { None }
}


pub fn ping(
    nic: &mut NetworkCard,
    target_ip: [u8; 4],
    phys_mem_offset: u64,
    writer: &mut VgaWriter,
) {
    let src_mac = nic.mac();

    writer.write_string("PING ");
    print_ip(writer, target_ip);
    writer.write_string(" from ");
    print_ip(writer, OUR_IP);
    writer.write_string("\n");

    
    let arp_target = if same_subnet(target_ip, OUR_IP, SUBNET_MASK) {
        target_ip
    } else {
        GATEWAY_IP
    };

    
    let mut frame = [0u8; 1536];
    let len = build_arp_request(&mut frame, src_mac, OUR_IP, arp_target);
    nic.send(&frame[..len], phys_mem_offset);

    let mut target_mac = [0u8; 6];
    let mut recv = [0u8; 1536];
    let mut resolved = false;

    let start = unsafe { *(&raw const crate::interrupts::TICKS) };
    while unsafe { *(&raw const crate::interrupts::TICKS) } - start < 1000 {
        if let Some(n) = nic.receive(&mut recv) {
            if let Some((ip, mac)) = parse_arp_reply(&recv[..n]) {
                if ip == arp_target {
                    target_mac = mac;
                    resolved = true;
                    break;
                }
            }
        } else {
            crate::task::yield_now();
        }
    }
    if !resolved {
        writer.set_color(Color::LightRed, Color::Black);
        writer.write_string("ARP timeout - could not resolve ");
        print_ip(writer, arp_target);
        writer.write_string("\n");
        writer.set_color(Color::White, Color::Black);
        return;
    }

    writer.write_string("ARP: ");
    print_ip(writer, arp_target);
    writer.write_string(" is at ");
    print_mac(writer, target_mac);
    writer.write_string("\n");

    
    for seq in 1u16..=3 {
        let mut icmp = [0u8; 64];
        let icmp_len = build_icmp_echo(&mut icmp, 0xABCD, seq);

        let mut ip_pkt = [0u8; 128];
        let ip_len = build_ipv4(&mut ip_pkt, OUR_IP, target_ip, 1, &icmp[..icmp_len]);

        let eth_len = build_eth(&mut frame, target_mac, src_mac, ETH_IPV4, &ip_pkt[..ip_len]);
        nic.send(&frame[..eth_len], phys_mem_offset);

        
        let mut got_reply = false;
        let start = unsafe { *(&raw const crate::interrupts::TICKS) };
        while unsafe { *(&raw const crate::interrupts::TICKS) } - start < 2000 {
            if let Some(n) = nic.receive(&mut recv) {
                if n > 34 {
                    let etype = u16::from_be_bytes([recv[12], recv[13]]);
                    if etype == ETH_IPV4 && recv[23] == 1 {
                        
                        let ip_hdr_len = ((recv[ETH_HDR] & 0x0F) as usize) * 4;
                        let icmp_type = recv[ETH_HDR + ip_hdr_len];
                        if icmp_type == 0 {
                            
                            writer.set_color(Color::LightGreen, Color::Black);
                            writer.write_string("Reply from ");
                            print_ip(writer, target_ip);
                            writer.write_string(": icmp_seq=");
                            print_u8(writer, seq as u8);
                            writer.write_string(" ttl=");
                            print_u8(writer, recv[ETH_HDR + 8]);
                            writer.write_string("\n");
                            writer.set_color(Color::White, Color::Black);
                            got_reply = true;
                            break;
                        }
                    }
                }
            } else {
                crate::task::yield_now();
            }
        }
        if !got_reply {
            writer.write_string("Request timed out (seq=");
            print_u8(writer, seq as u8);
            writer.write_string(")\n");
        }
    }

    writer.write_string("--- ping statistics ---\n");
    writer.write_string("3 packets transmitted\n");
}

fn same_subnet(a: [u8; 4], b: [u8; 4], mask: [u8; 4]) -> bool {
    for i in 0..4 {
        if (a[i] & mask[i]) != (b[i] & mask[i]) {
            return false;
        }
    }
    true
}



struct TcpPacketInfo {
    src_port: u16,
    dst_port: u16,
    seq: u32,
    #[allow(dead_code)]
    ack_num: u32,
    flags: u8,
    payload_start: usize,
    payload_len: usize,
}

fn parse_tcp_packet(frame: &[u8]) -> Option<TcpPacketInfo> {
    if frame.len() < ETH_HDR + 40 {
        return None;
    }
    let etype = u16::from_be_bytes([frame[12], frame[13]]);
    if etype != ETH_IPV4 {
        return None;
    }
    if frame[ETH_HDR + 9] != 6 {
        return None; 
    }
    let ip_hdr_len = ((frame[ETH_HDR] & 0x0F) as usize) * 4;
    let ip_total_len = u16::from_be_bytes([frame[ETH_HDR + 2], frame[ETH_HDR + 3]]) as usize;
    let tcp_off = ETH_HDR + ip_hdr_len;
    if frame.len() < tcp_off + 20 {
        return None;
    }

    let src_port = u16::from_be_bytes([frame[tcp_off], frame[tcp_off + 1]]);
    let dst_port = u16::from_be_bytes([frame[tcp_off + 2], frame[tcp_off + 3]]);
    let seq = u32::from_be_bytes(frame[tcp_off + 4..tcp_off + 8].try_into().ok()?);
    let ack_num = u32::from_be_bytes(frame[tcp_off + 8..tcp_off + 12].try_into().ok()?);
    let data_offset = ((frame[tcp_off + 12] >> 4) as usize) * 4;
    let flags = frame[tcp_off + 13];

    let payload_start = tcp_off + data_offset;
    let tcp_payload_len = ip_total_len.saturating_sub(ip_hdr_len + data_offset);
    let payload_len = core::cmp::min(tcp_payload_len, frame.len().saturating_sub(payload_start));

    Some(TcpPacketInfo {
        src_port,
        dst_port,
        seq,
        ack_num,
        flags,
        payload_start,
        payload_len,
    })
}


pub fn resolve_arp(
    nic: &mut NetworkCard,
    target_ip: [u8; 4],
    phys_mem_offset: u64,
) -> Option<[u8; 6]> {
    let src_mac = nic.mac();
    let arp_target = if same_subnet(target_ip, OUR_IP, SUBNET_MASK) {
        target_ip
    } else {
        GATEWAY_IP
    };

    let mut frame = [0u8; 1536];
    let len = build_arp_request(&mut frame, src_mac, OUR_IP, arp_target);
    nic.send(&frame[..len], phys_mem_offset);

    let mut recv = [0u8; 1536];
    let start = unsafe { *(&raw const crate::interrupts::TICKS) };
    while unsafe { *(&raw const crate::interrupts::TICKS) } - start < 1000 {
        if let Some(n) = nic.receive(&mut recv) {
            if let Some((ip, mac)) = parse_arp_reply(&recv[..n]) {
                if ip == arp_target {
                    return Some(mac);
                }
            }
        } else {
            crate::task::yield_now();
        }
    }
    None
}


fn send_ip_packet(
    nic: &mut NetworkCard,
    dst_mac: [u8; 6],
    dst_ip: [u8; 4],
    protocol: u8,
    payload: &[u8],
    phys_mem_offset: u64,
) {
    let src_mac = nic.mac();
    let mut ip_buf = [0u8; 1500];
    let ip_len = build_ipv4(&mut ip_buf, OUR_IP, dst_ip, protocol, payload);
    let mut frame = [0u8; 1536];
    let eth_len = build_eth(&mut frame, dst_mac, src_mac, ETH_IPV4, &ip_buf[..ip_len]);
    nic.send(&frame[..eth_len], phys_mem_offset);
}

fn print_usize_tcp(w: &mut VgaWriter, mut n: usize) {
    if n == 0 {
        w.write_byte(b'0');
        return;
    }
    let mut buf = [0u8; 20];
    let mut i = 0;
    while n > 0 {
        buf[i] = (n % 10) as u8 + b'0';
        n /= 10;
        i += 1;
    }
    while i > 0 {
        i -= 1;
        w.write_byte(buf[i]);
    }
}



pub fn tcp_exchange(
    nic: &mut NetworkCard,
    target_ip: [u8; 4],
    target_port: u16,
    request: &[u8],
    phys_mem_offset: u64,
    writer: &mut VgaWriter,
) -> bool {
    use crate::tcp;

    
    let dst_mac = match resolve_arp(nic, target_ip, phys_mem_offset) {
        Some(m) => m,
        None => {
            writer.set_color(Color::LightRed, Color::Black);
            writer.write_string("ARP timeout\n");
            writer.set_color(Color::White, Color::Black);
            return false;
        }
    };

    
    let ticks = unsafe { *(&raw const crate::interrupts::TICKS) };
    let local_port = 49152u16.wrapping_add((ticks as u16) & 0x3FFF);

    let conn_idx = unsafe {
        let pool = &mut *(&raw mut tcp::TCP_POOL);
        match pool.alloc(target_ip, target_port, local_port) {
            Some(i) => i,
            None => {
                writer.set_color(Color::LightRed, Color::Black);
                writer.write_string("TCP pool full\n");
                writer.set_color(Color::White, Color::Black);
                return false;
            }
        }
    };

    
    let mut seg = [0u8; 1500];
    let initial_seq: u32 = 1000;
    let len = tcp::build_tcp_segment(
        local_port, target_port, initial_seq, 0,
        tcp::TCP_FLAG_SYN, 8192, OUR_IP, target_ip, &[], &mut seg,
    );
    send_ip_packet(nic, dst_mac, target_ip, 6, &seg[..len], phys_mem_offset);

    
    let mut recv = [0u8; 1536];
    let mut connected = false;
    let mut cur_seq = initial_seq;
    let mut cur_ack: u32 = 0;

    let start = unsafe { *(&raw const crate::interrupts::TICKS) };
    while unsafe { *(&raw const crate::interrupts::TICKS) } - start < 3000 {
        if let Some(n) = nic.receive(&mut recv) {
            if let Some(info) = parse_tcp_packet(&recv[..n]) {
                if info.src_port == target_port && info.dst_port == local_port {
                    if (info.flags & tcp::TCP_FLAG_RST) != 0 {
                        writer.set_color(Color::LightRed, Color::Black);
                        writer.write_string("Connection refused (RST)\n");
                        writer.set_color(Color::White, Color::Black);
                        unsafe { (&mut *(&raw mut tcp::TCP_POOL)).free(conn_idx); }
                        return false;
                    }
                    if (info.flags & (tcp::TCP_FLAG_SYN | tcp::TCP_FLAG_ACK))
                        == (tcp::TCP_FLAG_SYN | tcp::TCP_FLAG_ACK)
                    {
                        cur_seq = initial_seq + 1; 
                        cur_ack = info.seq + 1;    
                        
                        let ack_len = tcp::build_tcp_segment(
                            local_port, target_port, cur_seq, cur_ack,
                            tcp::TCP_FLAG_ACK, 8192, OUR_IP, target_ip,
                            &[], &mut seg,
                        );
                        send_ip_packet(nic, dst_mac, target_ip, 6, &seg[..ack_len], phys_mem_offset);
                        connected = true;
                        break;
                    }
                }
            }
        } else {
            crate::task::yield_now();
        }
    }

    if !connected {
        writer.set_color(Color::LightRed, Color::Black);
        writer.write_string("TCP handshake timeout\n");
        writer.set_color(Color::White, Color::Black);
        unsafe { (&mut *(&raw mut tcp::TCP_POOL)).free(conn_idx); }
        return false;
    }

    writer.set_color(Color::LightGreen, Color::Black);
    writer.write_string("Connected! ");
    writer.set_color(Color::White, Color::Black);

    
    if !request.is_empty() {
        let send_len = tcp::build_tcp_segment(
            local_port, target_port, cur_seq, cur_ack,
            tcp::TCP_FLAG_PSH | tcp::TCP_FLAG_ACK, 8192, OUR_IP, target_ip,
            request, &mut seg,
        );
        send_ip_packet(nic, dst_mac, target_ip, 6, &seg[..send_len], phys_mem_offset);
        cur_seq = cur_seq.wrapping_add(request.len() as u32);
    }

    
    writer.write_string("Receiving...\n");
    let mut total_rx: usize = 0;

    let start = unsafe { *(&raw const crate::interrupts::TICKS) };
    while unsafe { *(&raw const crate::interrupts::TICKS) } - start < 5000 {
        if let Some(n) = nic.receive(&mut recv) {
            if let Some(info) = parse_tcp_packet(&recv[..n]) {
                if info.src_port == target_port && info.dst_port == local_port {
                    if (info.flags & tcp::TCP_FLAG_RST) != 0 {
                        writer.write_string("\nConnection reset\n");
                        break;
                    }

                    
                    if info.payload_len > 0 {
                        let end = info.payload_start + info.payload_len;
                        for &b in &recv[info.payload_start..end] {
                            if b >= 0x20 || b == b'\n' || b == b'\r' || b == b'\t' {
                                writer.write_byte(b);
                            }
                        }
                        total_rx += info.payload_len;
                        cur_ack = info.seq.wrapping_add(info.payload_len as u32);

                        
                        let ack_len = tcp::build_tcp_segment(
                            local_port, target_port, cur_seq, cur_ack,
                            tcp::TCP_FLAG_ACK, 8192, OUR_IP, target_ip,
                            &[], &mut seg,
                        );
                        send_ip_packet(nic, dst_mac, target_ip, 6, &seg[..ack_len], phys_mem_offset);
                    }

                    
                    if (info.flags & tcp::TCP_FLAG_FIN) != 0 {
                        cur_ack = cur_ack.wrapping_add(1);
                        let fin_len = tcp::build_tcp_segment(
                            local_port, target_port, cur_seq, cur_ack,
                            tcp::TCP_FLAG_FIN | tcp::TCP_FLAG_ACK, 8192,
                            OUR_IP, target_ip, &[], &mut seg,
                        );
                        send_ip_packet(nic, dst_mac, target_ip, 6, &seg[..fin_len], phys_mem_offset);
                        break;
                    }
                }
            }
        } else {
            crate::task::yield_now();
        }
    }

    writer.write_string("\n--- ");
    print_usize_tcp(writer, total_rx);
    writer.write_string(" bytes received ---\n");

    unsafe { (&mut *(&raw mut tcp::TCP_POOL)).free(conn_idx); }
    true
}

/// Ping that returns true/false without a VgaWriter (for syscall use).
pub fn ping_raw(
    nic: &mut NetworkCard,
    target_ip: [u8; 4],
    phys_mem_offset: u64,
) -> bool {
    let src_mac = nic.mac();
    let arp_target = if same_subnet(target_ip, OUR_IP, SUBNET_MASK) {
        target_ip
    } else {
        GATEWAY_IP
    };

    let mut frame = [0u8; 1536];
    let len = build_arp_request(&mut frame, src_mac, OUR_IP, arp_target);
    nic.send(&frame[..len], phys_mem_offset);

    let mut target_mac = [0u8; 6];
    let mut recv = [0u8; 1536];
    let mut resolved = false;

    let start = unsafe { *(&raw const crate::interrupts::TICKS) };
    while unsafe { *(&raw const crate::interrupts::TICKS) } - start < 1000 {
        if let Some(n) = nic.receive(&mut recv) {
            if let Some((ip, mac)) = parse_arp_reply(&recv[..n]) {
                if ip == arp_target {
                    target_mac = mac;
                    resolved = true;
                    break;
                }
            }
        } else {
            crate::task::yield_now();
        }
    }
    if !resolved { return false; }

    // Send one ICMP echo
    let mut icmp = [0u8; 64];
    let icmp_len = build_icmp_echo(&mut icmp, 0xABCD, 1);
    let mut ip_pkt = [0u8; 128];
    let ip_len = build_ipv4(&mut ip_pkt, OUR_IP, target_ip, 1, &icmp[..icmp_len]);
    let eth_len = build_eth(&mut frame, target_mac, src_mac, ETH_IPV4, &ip_pkt[..ip_len]);
    nic.send(&frame[..eth_len], phys_mem_offset);

    let start = unsafe { *(&raw const crate::interrupts::TICKS) };
    while unsafe { *(&raw const crate::interrupts::TICKS) } - start < 2000 {
        if let Some(n) = nic.receive(&mut recv) {
            if n > 34 {
                let etype = u16::from_be_bytes([recv[12], recv[13]]);
                if etype == ETH_IPV4 && recv[23] == 1 {
                    let ip_hdr_len = ((recv[ETH_HDR] & 0x0F) as usize) * 4;
                    if recv[ETH_HDR + ip_hdr_len] == 0 {
                        return true;
                    }
                }
            }
        } else {
            crate::task::yield_now();
        }
    }
    false
}

/// TCP exchange that writes the HTTP response into a raw byte buffer (no VgaWriter).
/// Returns true on success. `out_buf` may be null (response is discarded).
pub fn tcp_exchange_raw(
    nic: &mut NetworkCard,
    target_ip: [u8; 4],
    target_port: u16,
    request: &[u8],
    phys_mem_offset: u64,
    out_buf: *mut u8,
    _out_buf_len: usize,
) -> bool {
    use crate::tcp;

    let dst_mac = match resolve_arp(nic, target_ip, phys_mem_offset) {
        Some(m) => m,
        None => return false,
    };

    let ticks = unsafe { *(&raw const crate::interrupts::TICKS) };
    let local_port = 49152u16.wrapping_add((ticks as u16) & 0x3FFF);

    let conn_idx = unsafe {
        let pool = &mut *(&raw mut tcp::TCP_POOL);
        match pool.alloc(target_ip, target_port, local_port) {
            Some(i) => i,
            None => return false,
        }
    };

    let mut seg = [0u8; 1500];
    let initial_seq: u32 = 1000;
    let len = tcp::build_tcp_segment(
        local_port, target_port, initial_seq, 0,
        tcp::TCP_FLAG_SYN, 8192, OUR_IP, target_ip, &[], &mut seg,
    );
    send_ip_packet(nic, dst_mac, target_ip, 6, &seg[..len], phys_mem_offset);

    let mut recv = [0u8; 1536];
    let mut connected = false;
    let mut cur_seq = initial_seq;
    let mut cur_ack: u32 = 0;

    let start = unsafe { *(&raw const crate::interrupts::TICKS) };
    while unsafe { *(&raw const crate::interrupts::TICKS) } - start < 3000 {
        if let Some(n) = nic.receive(&mut recv) {
            if let Some(info) = parse_tcp_packet(&recv[..n]) {
                if info.src_port == target_port && info.dst_port == local_port {
                    if (info.flags & tcp::TCP_FLAG_RST) != 0 {
                        unsafe { (&mut *(&raw mut tcp::TCP_POOL)).free(conn_idx); }
                        return false;
                    }
                    if (info.flags & (tcp::TCP_FLAG_SYN | tcp::TCP_FLAG_ACK))
                        == (tcp::TCP_FLAG_SYN | tcp::TCP_FLAG_ACK)
                    {
                        cur_seq = initial_seq + 1;
                        cur_ack = info.seq + 1;
                        let ack_len = tcp::build_tcp_segment(
                            local_port, target_port, cur_seq, cur_ack,
                            tcp::TCP_FLAG_ACK, 8192, OUR_IP, target_ip, &[], &mut seg,
                        );
                        send_ip_packet(nic, dst_mac, target_ip, 6, &seg[..ack_len], phys_mem_offset);
                        connected = true;
                        break;
                    }
                }
            }
        } else {
            crate::task::yield_now();
        }
    }
    if !connected {
        unsafe { (&mut *(&raw mut tcp::TCP_POOL)).free(conn_idx); }
        return false;
    }

    if !request.is_empty() {
        let send_len = tcp::build_tcp_segment(
            local_port, target_port, cur_seq, cur_ack,
            tcp::TCP_FLAG_PSH | tcp::TCP_FLAG_ACK, 8192, OUR_IP, target_ip,
            request, &mut seg,
        );
        send_ip_packet(nic, dst_mac, target_ip, 6, &seg[..send_len], phys_mem_offset);
        cur_seq = cur_seq.wrapping_add(request.len() as u32);
    }

    let mut write_pos: usize = 0;

    let start = unsafe { *(&raw const crate::interrupts::TICKS) };
    while unsafe { *(&raw const crate::interrupts::TICKS) } - start < 5000 {
        if let Some(n) = nic.receive(&mut recv) {
            if let Some(info) = parse_tcp_packet(&recv[..n]) {
                if info.src_port == target_port && info.dst_port == local_port {
                    if (info.flags & tcp::TCP_FLAG_RST) != 0 { break; }
                    if info.payload_len > 0 {
                        // Write to out_buf if provided
                        if !out_buf.is_null() && _out_buf_len > write_pos {
                            let avail = _out_buf_len - write_pos - 1;
                            let to_copy = info.payload_len.min(avail);
                            if to_copy > 0 {
                                unsafe {
                                    core::ptr::copy_nonoverlapping(
                                        recv[info.payload_start..].as_ptr(),
                                        out_buf.add(write_pos),
                                        to_copy,
                                    );
                                    write_pos += to_copy;
                                    *out_buf.add(write_pos) = 0;
                                }
                            }
                        }
                        cur_ack = info.seq.wrapping_add(info.payload_len as u32);
                        let ack_len = tcp::build_tcp_segment(
                            local_port, target_port, cur_seq, cur_ack,
                            tcp::TCP_FLAG_ACK, 8192, OUR_IP, target_ip, &[], &mut seg,
                        );
                        send_ip_packet(nic, dst_mac, target_ip, 6, &seg[..ack_len], phys_mem_offset);
                    }
                    if (info.flags & tcp::TCP_FLAG_FIN) != 0 {
                        cur_ack = cur_ack.wrapping_add(1);
                        let fin_len = tcp::build_tcp_segment(
                            local_port, target_port, cur_seq, cur_ack,
                            tcp::TCP_FLAG_FIN | tcp::TCP_FLAG_ACK, 8192,
                            OUR_IP, target_ip, &[], &mut seg,
                        );
                        send_ip_packet(nic, dst_mac, target_ip, 6, &seg[..fin_len], phys_mem_offset);
                        break;
                    }
                }
            }
        } else {
            crate::task::yield_now();
        }
    }

    unsafe { (&mut *(&raw mut tcp::TCP_POOL)).free(conn_idx); }
    true
}

