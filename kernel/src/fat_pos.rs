

use crate::port::Port;
use crate::pos::{PosObject, PosError, PosObjectRef};
use alloc::sync::Arc;
use spin::{Lazy, Mutex};

pub static FAT_SERVER_PORT: Lazy<Arc<Port>> = Lazy::new(|| Port::new(256));

#[repr(C)]
pub struct FatIoRequest {
    pub reply_port: *const Port,
    pub operation: u32, 
    pub first_cluster: u32,
    pub file_size: usize,
    pub offset: usize,
    pub buffer_ptr: *mut u8,
    pub buffer_len: usize,
}

#[derive(Clone, Copy)]
#[repr(C, packed)]
pub struct FatFindReply {
    pub found: bool,
    pub is_dir: bool,
    pub first_cluster: u32,
    pub file_size: usize,
}

/// One entry returned by the op-4 directory listing.
#[derive(Clone, Copy)]
#[repr(C, packed)]
pub struct FatListEntry {
    pub name:        [u8; 11],
    pub attr:        u8,
    pub fst_clus_hi: u16,
    pub fst_clus_lo: u16,
    pub file_size:   u32,
}

impl FatListEntry {
    pub const SIZE: usize = 20;

    pub fn first_cluster(&self) -> u32 {
        ((self.fst_clus_hi as u32) << 16) | (self.fst_clus_lo as u32)
    }

    pub fn is_dir(&self) -> bool { self.attr & 0x10 != 0 }
}

pub extern "C" fn fat32_server_thread() {
    loop {
        if let Some(msg) = FAT_SERVER_PORT.read_port() {
            if msg.code == 0x1001 && msg.data.len() == core::mem::size_of::<FatIoRequest>() {
                let req = unsafe { &*(msg.data.as_ptr() as *const FatIoRequest) };
                let reply_port = unsafe { &*req.reply_port };
                let buf = unsafe { core::slice::from_raw_parts_mut(req.buffer_ptr, req.buffer_len) };
                
                if req.operation == 1 { 
                    let mut bytes_read = 0;
                    let mut fat_lock = unsafe { crate::FAT_FS.lock() };
                    if let Some(fat) = &mut *fat_lock {
                        if let Ok(br) = fat.read_file_offset(req.first_cluster, req.file_size, req.offset, buf) {
                            bytes_read = br;
                        }
                    }
                    drop(fat_lock);
                    let reply_data = bytes_read.to_ne_bytes();
                    let _ = reply_port.write_port(0x2001, &reply_data);
                } else if req.operation == 3 { 
                    let mut reply = FatFindReply { found: false, is_dir: false, first_cluster: 0, file_size: 0 };
                    if let Ok(path_str) = core::str::from_utf8(buf) {
                        let mut fat_lock = unsafe { crate::FAT_FS.lock() };
                        if let Some(fat) = &mut *fat_lock {
                            if let Ok(Some(entry)) = fat.find_entry(fat.root_cluster, path_str) {
                                reply.found = true;
                                reply.is_dir = entry.is_dir();
                                reply.first_cluster = entry.first_cluster();
                                reply.file_size = entry.file_size as usize;
                            }
                        }
                    }
                    let reply_bytes = unsafe { core::slice::from_raw_parts(&reply as *const _ as *const u8, core::mem::size_of::<FatFindReply>()) };
                    let _ = reply_port.write_port(0x2001, reply_bytes);
                } else if req.operation == 4 {
                    // List directory: buf holds the dir path string (empty = root).
                    // Reply code 0x2004: packed FatListEntry structs (20 bytes each).
                    if let Ok(path_str) = core::str::from_utf8(buf) {
                        let mut fat_lock = unsafe { crate::FAT_FS.lock() };
                        if let Some(fat) = &mut *fat_lock {
                            let dir_cluster = if path_str.is_empty() {
                                Some(fat.root_cluster)
                            } else {
                                fat.find_entry(fat.root_cluster, path_str)
                                    .ok()
                                    .flatten()
                                    .filter(|e| e.is_dir())
                                    .map(|e| e.first_cluster())
                            };

                            if let Some(cluster) = dir_cluster {
                                if let Ok(entries) = fat.read_dir(cluster) {
                                    let mut reply_buf: alloc::vec::Vec<u8> = alloc::vec::Vec::new();
                                    for e in &entries {
                                        reply_buf.extend_from_slice(&e.name);
                                        reply_buf.push(e.attr);
                                        reply_buf.extend_from_slice(&e.fst_clus_hi.to_le_bytes());
                                        reply_buf.extend_from_slice(&e.fst_clus_lo.to_le_bytes());
                                        reply_buf.extend_from_slice(&e.file_size.to_le_bytes());
                                    }
                                    drop(fat_lock);
                                    let _ = reply_port.write_port(0x2004, &reply_buf);
                                    continue;
                                }
                            }
                            drop(fat_lock);
                        }
                    }
                    // Empty reply on failure
                    let _ = reply_port.write_port(0x2004, &[]);
                }
            }
        } else {
            crate::task::yield_now();
        }
    }
}

pub struct FatPosObject {
    pub first_cluster: u32,
    pub file_size: usize,
    pub offset: usize,
}

impl PosObject for FatPosObject {
    fn read(&mut self, _offset: u64, buf: &mut [u8]) -> Result<usize, PosError> {
        let reply_port = Port::new(1);
        let req = FatIoRequest {
            reply_port: Arc::as_ptr(&reply_port),
            operation: 1,
            first_cluster: self.first_cluster,
            file_size: self.file_size,
            offset: self.offset,
            buffer_ptr: buf.as_mut_ptr(),
            buffer_len: buf.len(),
        };
        
        let req_bytes = unsafe { core::slice::from_raw_parts(&req as *const _ as *const u8, core::mem::size_of::<FatIoRequest>()) };
        FAT_SERVER_PORT.write_port(0x1001, req_bytes).map_err(|_| PosError::NotSupported)?;
        
        loop {
            if let Some(msg) = reply_port.read_port() {
                if msg.code == 0x2001 {
                    let bytes_read = usize::from_ne_bytes(msg.data.try_into().unwrap_or([0; 8]));
                    self.offset += bytes_read;
                    return Ok(bytes_read);
                }
            }
            crate::task::yield_now();
        }
    }
    
    fn write(&mut self, _offset: u64, _buf: &[u8]) -> Result<usize, PosError> { Err(PosError::NotSupported) }
}
