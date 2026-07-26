



#![allow(dead_code)]

pub trait BlockDevice {
    
    fn read_sector(&mut self, lba: u32, buffer: &mut [u8; 512]) -> Result<(), &'static str>;
    /// Writes exactly one sector (usually 512 bytes)
    fn write_sector(&mut self, lba: u32, buffer: &[u8; 512]) -> Result<(), &'static str>;
}

pub struct Fat32File {
    pub first_cluster: u32,
    pub size: u32,
    pub offset: u32,
}

impl crate::pos::PosObject for Fat32File {
    fn read(&mut self, _offset: u64, _buf: &mut [u8]) -> Result<usize, crate::pos::PosError> {
        
        Err(crate::pos::PosError::NotSupported)
    }

    fn write(&mut self, _offset: u64, _buf: &[u8]) -> Result<usize, crate::pos::PosError> {
        Err(crate::pos::PosError::NotSupported)
    }
}

#[derive(Debug, Clone, Copy)]
#[repr(C, packed)]
struct BootRecord {
    jump_boot: [u8; 3],
    oem_name: [u8; 8],
    bytes_per_sector: u16,
    sectors_per_cluster: u8,
    reserved_sector_count: u16,
    num_fats: u8,
    root_entry_count: u16,
    total_sectors_16: u16,
    media: u8,
    fat_size_16: u16,
    sectors_per_track: u16,
    num_heads: u16,
    hidden_sectors: u32,
    total_sectors_32: u32,
    
    fat_size_32: u32,
    ext_flags: u16,
    fs_version: u16,
    root_cluster: u32,
}

pub struct Fat32Volume<D: BlockDevice> {
    pub device: D,
    first_data_sector: u32,
    fat_start_sector: u32,
    fat_size: u32,
    num_fats: u8,
    sectors_per_cluster: u8,
    pub root_cluster: u32,
}

impl<D: BlockDevice> Fat32Volume<D> {
    pub fn new(mut device: D) -> Result<Self, &'static str> {
        let mut boot_sector = [0u8; 512];
        device.read_sector(0, &mut boot_sector)?;

        let br: &BootRecord = unsafe { &*(boot_sector.as_ptr() as *const BootRecord) };
        
        if br.bytes_per_sector != 512 {
            return Err("Only 512-byte sectors are supported");
        }

        let fat_size = if br.fat_size_16 == 0 { br.fat_size_32 } else { br.fat_size_16 as u32 };
        let root_dir_sectors = ((br.root_entry_count * 32) + 511) / 512;
        let first_data_sector = br.reserved_sector_count as u32 + (br.num_fats as u32 * fat_size) + root_dir_sectors as u32;

        Ok(Fat32Volume {
            device,
            first_data_sector,
            fat_start_sector: br.reserved_sector_count as u32,
            fat_size,
            num_fats: br.num_fats,
            sectors_per_cluster: br.sectors_per_cluster,
            root_cluster: br.root_cluster,
        })
    }

    /// Converts a cluster number to Logical Block Address (LBA)
    pub fn cluster_to_lba(&self, cluster: u32) -> u32 {
        self.first_data_sector + ((cluster - 2) * self.sectors_per_cluster as u32)
    }

    /// Reads the next cluster in the chain from the FAT
    pub fn next_cluster(&mut self, current_cluster: u32) -> Result<Option<u32>, &'static str> {
        let fat_offset = current_cluster * 4;
        let fat_sector = self.fat_start_sector + (fat_offset / 512);
        let entry_offset = (fat_offset % 512) as usize;

        let mut sector_buf = [0u8; 512];
        self.device.read_sector(fat_sector, &mut sector_buf)?;

        let next_cluster = u32::from_le_bytes(
            sector_buf[entry_offset..entry_offset + 4].try_into().unwrap()
        ) & 0x0FFFFFFF;

        if next_cluster >= 0x0FFFFFF8 {
            Ok(None)
        } else {
            Ok(Some(next_cluster))
        }
    }
    
    
    pub fn read_cluster(&mut self, cluster: u32, buffer: &mut [u8]) -> Result<(), &'static str> {
        if buffer.len() < (self.sectors_per_cluster as usize * 512) {
            return Err("Buffer too small for cluster");
        }
        
        let start_lba = self.cluster_to_lba(cluster);
        for i in 0..self.sectors_per_cluster {
            let mut sec_buf = [0u8; 512];
            self.device.read_sector(start_lba + i as u32, &mut sec_buf)?;
            let offset = i as usize * 512;
            buffer[offset..offset + 512].copy_from_slice(&sec_buf);
        }
        Ok(())
    }

    /// Writes data from a buffer into a cluster
    pub fn write_cluster(&mut self, cluster: u32, buffer: &[u8]) -> Result<(), &'static str> {
        if buffer.len() < (self.sectors_per_cluster as usize * 512) {
            return Err("Buffer too small for cluster");
        }
        
        let start_lba = self.cluster_to_lba(cluster);
        for i in 0..self.sectors_per_cluster {
            let mut sec_buf = [0u8; 512];
            let offset = i as usize * 512;
            sec_buf.copy_from_slice(&buffer[offset..offset + 512]);
            self.device.write_sector(start_lba + i as u32, &sec_buf)?;
        }
        Ok(())
    }

    
    pub fn update_fat_entry(&mut self, cluster: u32, value: u32) -> Result<(), &'static str> {
        let fat_offset = cluster * 4;
        let sector_offset = fat_offset / 512;
        let entry_offset = (fat_offset % 512) as usize;

        for fat_idx in 0..self.num_fats {
            let fat_sector = self.fat_start_sector + (fat_idx as u32 * self.fat_size) + sector_offset;
            let mut sector_buf = [0u8; 512];
            self.device.read_sector(fat_sector, &mut sector_buf)?;

            let val_bytes = value.to_le_bytes();
            sector_buf[entry_offset..entry_offset + 4].copy_from_slice(&val_bytes);

            self.device.write_sector(fat_sector, &sector_buf)?;
        }
        Ok(())
    }

    /// Allocates a free cluster, optionally linking it to a previous cluster
    pub fn allocate_cluster(&mut self, prev_cluster: Option<u32>) -> Result<u32, &'static str> {
        
        let total_clusters = (self.fat_size * 512) / 4;
        for cluster in 2..total_clusters {
            let next = self.next_cluster(cluster)?;
            
            if next == Some(0) {
                
                self.update_fat_entry(cluster, 0x0FFFFFFF)?;
                
                
                if let Some(prev) = prev_cluster {
                    self.update_fat_entry(prev, cluster)?;
                }
                
                
                let cluster_size = self.sectors_per_cluster as usize * 512;
                let zero_buf = alloc::vec![0u8; cluster_size];
                self.write_cluster(cluster, &zero_buf)?;

                return Ok(cluster);
            }
        }
        Err("Disk full")
    }

    /// Creates a subdirectory named `name` inside `parent_cluster`.
    ///
    /// Allocates a fresh cluster for the new directory, writes the mandatory
    /// `.` and `..` entries, then adds a directory-type entry to the parent.
    /// Returns the first cluster of the new directory on success.
    pub fn create_dir(&mut self, parent_cluster: u32, name: &str) -> Result<u32, &'static str> {
        // Bail out early if the directory already exists
        if let Ok(Some(e)) = self.find_entry(parent_cluster, name) {
            if e.is_dir() {
                return Ok(e.first_cluster());   // already there — return its cluster
            }
            return Err("Name exists but is not a directory");
        }

        // Allocate a fresh cluster for the new directory
        let dir_cluster = self.allocate_cluster(None)?;

        let cluster_size = self.sectors_per_cluster as usize * 512;
        let mut buf = alloc::vec![0u8; cluster_size];

        // Helper: build a minimal 32-byte FAT directory entry in `buf[slot*32..]`
        let write_entry = |buf: &mut [u8], slot: usize, name: [u8; 11],
                           attr: u8, first_cluster: u32, size: u32| {
            let base = slot * 32;
            buf[base..base + 11].copy_from_slice(&name);
            buf[base + 11] = attr;
            // fst_clus_hi @ offset 20, fst_clus_lo @ offset 26
            let hi = ((first_cluster >> 16) & 0xFFFF) as u16;
            let lo = (first_cluster & 0xFFFF) as u16;
            buf[base + 20] = (hi & 0xFF) as u8;
            buf[base + 21] = (hi >> 8) as u8;
            buf[base + 26] = (lo & 0xFF) as u8;
            buf[base + 27] = (lo >> 8) as u8;
            // file_size @ offset 28
            buf[base + 28] = (size & 0xFF) as u8;
            buf[base + 29] = ((size >> 8) & 0xFF) as u8;
            buf[base + 30] = ((size >> 16) & 0xFF) as u8;
            buf[base + 31] = ((size >> 24) & 0xFF) as u8;
        };

        const ATTR_DIR: u8 = 0x10;

        // `.` entry — points to self
        let dot_name  = *b".          ";
        write_entry(&mut buf, 0, dot_name,  ATTR_DIR, dir_cluster,    0);

        // `..` entry — points to parent
        let dotdot_name = *b"..         ";
        write_entry(&mut buf, 1, dotdot_name, ATTR_DIR, parent_cluster, 0);

        self.write_cluster(dir_cluster, &buf)?;

        // Build the parsed 8.3 name for the directory entry in the parent
        let parsed_name = parse_83_name(name);

        // Find a free slot in the parent directory and write the new entry
        let mut current_dir = parent_cluster;
        loop {
            let mut cluster_buf = alloc::vec![0u8; cluster_size];
            self.read_cluster(current_dir, &mut cluster_buf)?;

            let mut wrote = false;
            for i in (0..cluster_size).step_by(32) {
                if cluster_buf[i] == 0x00 || cluster_buf[i] == 0xE5 {
                    write_entry(&mut cluster_buf, i / 32, parsed_name, ATTR_DIR, dir_cluster, 0);
                    self.write_cluster(current_dir, &cluster_buf)?;
                    wrote = true;
                    break;
                }
            }

            if wrote { break; }

            match self.next_cluster(current_dir)? {
                Some(next) => current_dir = next,
                None => {
                    // Parent directory is full — allocate another cluster for it
                    let new_parent_cluster = self.allocate_cluster(Some(current_dir))?;
                    let mut new_buf = alloc::vec![0u8; cluster_size];
                    write_entry(&mut new_buf, 0, parsed_name, ATTR_DIR, dir_cluster, 0);
                    self.write_cluster(new_parent_cluster, &new_buf)?;
                    break;
                }
            }
        }

        Ok(dir_cluster)
    }


    
    pub fn read_chain(&mut self, start_cluster: u32, size: usize) -> Result<alloc::vec::Vec<u8>, &'static str> {
        let mut data = alloc::vec::Vec::new();
        let mut current = start_cluster;
        let cluster_size = self.sectors_per_cluster as usize * 512;
        let mut cluster_buf = alloc::vec![0u8; cluster_size];

        loop {
            self.read_cluster(current, &mut cluster_buf)?;
            data.extend_from_slice(&cluster_buf);
            
            match self.next_cluster(current)? {
                Some(next) => current = next,
                None => break,
            }
        }
        
        data.truncate(size);
        Ok(data)
    }

    /// Stream a portion of a file directly into a buffer without heap allocation
    pub fn read_file_offset(
        &mut self,
        start_cluster: u32,
        file_size: usize,
        offset: usize,
        buf: &mut [u8],
    ) -> Result<usize, &'static str> {
        if offset >= file_size || buf.is_empty() {
            return Ok(0); 
        }

        let cluster_size = self.sectors_per_cluster as usize * 512;
        let mut current_cluster = start_cluster;

        let clusters_to_skip = offset / cluster_size;
        for _ in 0..clusters_to_skip {
            match self.next_cluster(current_cluster)? {
                Some(next) => current_cluster = next,
                None => return Err("Unexpected EOF while traversing FAT chain"),
            }
        }

        let mut cluster_offset = offset % cluster_size;
        let bytes_to_read = core::cmp::min(buf.len(), file_size - offset);
        let mut bytes_read = 0;

        
        
        while bytes_read < bytes_to_read {
            let start_lba = self.cluster_to_lba(current_cluster);
            let sector_in_cluster = cluster_offset / 512;
            let mut sector_offset = cluster_offset % 512;

            for sec in sector_in_cluster..self.sectors_per_cluster as usize {
                if bytes_read >= bytes_to_read {
                    break;
                }

                let mut sec_buf = [0u8; 512];
                self.device.read_sector(start_lba + sec as u32, &mut sec_buf)?;

                let chunk_size = core::cmp::min(
                    bytes_to_read - bytes_read,
                    512 - sector_offset
                );

                buf[bytes_read..bytes_read + chunk_size].copy_from_slice(
                    &sec_buf[sector_offset..sector_offset + chunk_size]
                );

                bytes_read += chunk_size;
                sector_offset = 0;
            }

            cluster_offset = 0;

            if bytes_read < bytes_to_read {
                match self.next_cluster(current_cluster)? {
                    Some(next) => current_cluster = next,
                    None => break,
                }
            }
        }

        Ok(bytes_read)
    }

    
    pub fn read_dir(&mut self, dir_cluster: u32) -> Result<alloc::vec::Vec<FatDirEntry>, &'static str> {
        let data = self.read_chain(dir_cluster, usize::MAX)?; // size doesn't matter for directories (we parse till 0x00)
        let mut entries = alloc::vec::Vec::new();

        for chunk in data.chunks_exact(32) {
            if chunk[0] == 0x00 {
                break; 
            }
            if chunk[0] == 0xE5 {
                continue; 
            }
            if chunk[11] & 0x0F == 0x0F {
                continue; 
            }

            let mut entry: FatDirEntry = unsafe { core::mem::zeroed() };
            unsafe {
                core::ptr::copy_nonoverlapping(
                    chunk.as_ptr(),
                    &mut entry as *mut FatDirEntry as *mut u8,
                    32
                );
            }
            entries.push(entry);
        }

        Ok(entries)
    }

    
    pub fn find_entry(&mut self, dir_cluster: u32, name: &str) -> Result<Option<FatDirEntry>, &'static str> {
        let entries = self.read_dir(dir_cluster)?;
        let parsed_name = parse_83_name(name);

        for entry in entries {
            if entry.name == parsed_name {
                return Ok(Some(entry));
            }
        }
        Ok(None)
    }

    /// Creates a new file in the given directory
    pub fn create_file(&mut self, dir_cluster: u32, name: &str) -> Result<u32, &'static str> {
        
        if let Some(_) = self.find_entry(dir_cluster, name)? {
            return Err("File already exists");
        }

        
        let file_cluster = self.allocate_cluster(None)?;

        
        let parsed_name = parse_83_name(name);
        let mut new_entry: FatDirEntry = unsafe { core::mem::zeroed() };
        new_entry.name = parsed_name;
        new_entry.attr = 0x20; 
        new_entry.fst_clus_hi = (file_cluster >> 16) as u16;
        new_entry.fst_clus_lo = (file_cluster & 0xFFFF) as u16;
        new_entry.file_size = 0; 

        let mut current_dir_cluster = dir_cluster;
        let cluster_size = self.sectors_per_cluster as usize * 512;
        let mut cluster_buf = alloc::vec![0u8; cluster_size];

        loop {
            self.read_cluster(current_dir_cluster, &mut cluster_buf)?;
            let mut found_slot = false;

            for i in (0..cluster_size).step_by(32) {
                if cluster_buf[i] == 0x00 || cluster_buf[i] == 0xE5 {
                    
                    unsafe {
                        core::ptr::copy_nonoverlapping(
                            &new_entry as *const _ as *const u8,
                            cluster_buf[i..].as_mut_ptr(),
                            32,
                        );
                    }
                    self.write_cluster(current_dir_cluster, &cluster_buf)?;
                    found_slot = true;
                    break;
                }
            }

            if found_slot {
                return Ok(file_cluster);
            }

            
            match self.next_cluster(current_dir_cluster)? {
                Some(next) => current_dir_cluster = next,
                None => {
                    let new_dir_cluster = self.allocate_cluster(Some(current_dir_cluster))?;
                    
                    self.read_cluster(new_dir_cluster, &mut cluster_buf)?;
                    unsafe {
                        core::ptr::copy_nonoverlapping(
                            &new_entry as *const _ as *const u8,
                            cluster_buf.as_mut_ptr(),
                            32,
                        );
                    }
                    self.write_cluster(new_dir_cluster, &cluster_buf)?;
                    return Ok(file_cluster);
                }
            }
        }
    }

    
    pub fn write_file(&mut self, dir_cluster: u32, name: &str, data: &[u8]) -> Result<(), &'static str> {
        let file_cluster = match self.find_entry(dir_cluster, name)? {
            Some(entry) => entry.first_cluster(),
            None => self.create_file(dir_cluster, name)?,
        };

        let cluster_size = self.sectors_per_cluster as usize * 512;
        let mut current_cluster = file_cluster;
        let mut bytes_written = 0;

        while bytes_written < data.len() {
            let chunk_size = core::cmp::min(cluster_size, data.len() - bytes_written);
            let mut cluster_buf = alloc::vec![0u8; cluster_size];
            cluster_buf[..chunk_size].copy_from_slice(&data[bytes_written..bytes_written + chunk_size]);
            
            self.write_cluster(current_cluster, &cluster_buf)?;
            bytes_written += chunk_size;

            if bytes_written < data.len() {
                // Need another cluster
                match self.next_cluster(current_cluster)? {
                    Some(next) => current_cluster = next,
                    None => {
                        current_cluster = self.allocate_cluster(Some(current_cluster))?;
                    }
                }
            }
        }

        // Update file size in directory entry
        let mut current_dir_cluster = dir_cluster;
        let mut cluster_buf = alloc::vec![0u8; cluster_size];
        let parsed_name = parse_83_name(name);

        loop {
            self.read_cluster(current_dir_cluster, &mut cluster_buf)?;
            let mut found = false;

            for i in (0..cluster_size).step_by(32) {
                if cluster_buf[i] == 0x00 { break; } // End of directory
                if cluster_buf[i] == 0xE5 { continue; }

                if cluster_buf[i..i+11] == parsed_name {
                    let size_bytes = (data.len() as u32).to_le_bytes();
                    cluster_buf[i+28..i+32].copy_from_slice(&size_bytes);
                    self.write_cluster(current_dir_cluster, &cluster_buf)?;
                    found = true;
                    break;
                }
            }

            if found { break; }

            match self.next_cluster(current_dir_cluster)? {
                Some(next) => current_dir_cluster = next,
                None => return Err("Could not find directory entry to update size"),
            }
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Copy)]
#[repr(C, packed)]
pub struct FatDirEntry {
    pub name: [u8; 11],
    pub attr: u8,
    pub nt_res: u8,
    pub crt_time_tenth: u8,
    pub crt_time: u16,
    pub crt_date: u16,
    pub lst_acc_date: u16,
    pub fst_clus_hi: u16,
    pub wrt_time: u16,
    pub wrt_date: u16,
    pub fst_clus_lo: u16,
    pub file_size: u32,
}

impl FatDirEntry {
    pub fn is_dir(&self) -> bool {
        self.attr & 0x10 != 0
    }

    pub fn first_cluster(&self) -> u32 {
        ((self.fst_clus_hi as u32) << 16) | (self.fst_clus_lo as u32)
    }

    pub fn filename(&self) -> alloc::string::String {
        let mut name = alloc::string::String::new();
        for &b in &self.name[0..8] {
            if b != b' ' { name.push(b as char); }
        }
        let ext = &self.name[8..11];
        if ext[0] != b' ' {
            name.push('.');
            for &b in ext {
                if b != b' ' { name.push(b as char); }
            }
        }
        name
    }
}

pub fn parse_83_name(name: &str) -> [u8; 11] {
    let mut res = [b' '; 11];
    let mut parts = name.split('.');
    
    if let Some(base) = parts.next() {
        for (i, b) in base.bytes().take(8).enumerate() {
            res[i] = b.to_ascii_uppercase();
        }
    }
    
    if let Some(ext) = parts.next() {
        for (i, b) in ext.bytes().take(3).enumerate() {
            res[8 + i] = b.to_ascii_uppercase();
        }
    }
    
    res
}
