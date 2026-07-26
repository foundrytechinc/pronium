

pub const MAX_NODES: usize = 64;
pub const MAX_NAME_LEN: usize = 16;
pub const MAX_FILE_SIZE: usize = 512;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum NodeType {
    File,
    Directory,
}

#[derive(Clone, Copy)]
pub struct Node {
    pub name: [u8; MAX_NAME_LEN],
    pub name_len: usize,
    pub node_type: NodeType,
    pub in_use: bool,
    pub parent_id: Option<usize>,
    pub data: [u8; MAX_FILE_SIZE],
    pub data_len: usize,
}

pub struct RamFs {
    pub nodes: [Node; MAX_NODES],
    pub occupied_nodes: usize,
}

impl RamFs {
    pub const fn new() -> Self {
        let empty_node = Node {
            name: [0; MAX_NAME_LEN],
            name_len: 0,
            node_type: NodeType::File,
            in_use: false,
            parent_id: None,
            data: [0; MAX_FILE_SIZE],
            data_len: 0,
        };
        
        let mut fs = Self {
            nodes: [empty_node; MAX_NODES],
            occupied_nodes: 0,
        };
        
        
        fs.nodes[0].in_use = true;
        fs.nodes[0].node_type = NodeType::Directory;
        fs.nodes[0].name[0] = b'/';
        fs.nodes[0].name_len = 1;
        fs.occupied_nodes = 1;
        
        fs
    }

    pub fn add_node(&mut self, name: &str, node_type: NodeType) -> Result<usize, &'static str> {
        if self.occupied_nodes >= MAX_NODES {
            return Err("RamFS is full");
        }

        if name.is_empty() || name.len() > MAX_NAME_LEN {
            return Err("Invalid name length");
        }

        // Check if node already exists in root
        for i in 0..MAX_NODES {
            let node = &self.nodes[i];
            if node.in_use && node.parent_id == Some(0) {
                if self.name_equals(node, name) {
                    return Err("File or Directory already exists");
                }
            }
        }

        let mut free_index = None;
        for i in 1..MAX_NODES {
            if !self.nodes[i].in_use {
                free_index = Some(i);
                break;
            }
        }

        if let Some(i) = free_index {
            let name_bytes = name.as_bytes();
            let len = core::cmp::min(name_bytes.len(), MAX_NAME_LEN);
            
            let mut name_arr = [0; MAX_NAME_LEN];
            name_arr[..len].copy_from_slice(&name_bytes[..len]);

            self.nodes[i] = Node {
                name: name_arr,
                name_len: len,
                node_type,
                in_use: true,
                parent_id: Some(0),
                data: [0; MAX_FILE_SIZE],
                data_len: 0,
            };
            self.occupied_nodes += 1;
            Ok(i)
        } else {
            Err("RamFS is full")
        }
    }

    pub fn find_node(&self, name: &str) -> Option<usize> {
        for i in 0..MAX_NODES {
            let node = &self.nodes[i];
            if node.in_use && node.parent_id == Some(0) && self.name_equals(node, name) {
                return Some(i);
            }
        }
        None
    }

    pub fn remove_node(&mut self, name: &str) -> Result<(), &'static str> {
        if let Some(idx) = self.find_node(name) {
            self.nodes[idx].in_use = false;
            self.occupied_nodes -= 1;
            Ok(())
        } else {
            Err("Node not found")
        }
    }

    fn name_equals(&self, node: &Node, name: &str) -> bool {
        let name_bytes = name.as_bytes();
        if node.name_len != name_bytes.len() {
            return false;
        }
        &node.name[..node.name_len] == name_bytes
    }
}

pub struct RamFsFile {
    pub node_index: usize,
    pub offset: u64,
}

impl crate::pos::PosObject for RamFsFile {
    fn read(&mut self, _offset: u64, buf: &mut [u8]) -> Result<usize, crate::pos::PosError> {
        // Here we'd access global RAMFS
        Err(crate::pos::PosError::NotSupported)
    }

    fn write(&mut self, _offset: u64, buf: &[u8]) -> Result<usize, crate::pos::PosError> {
        Err(crate::pos::PosError::NotSupported)
    }
}
