




use alloc::sync::Arc;
use alloc::string::String;
use alloc::collections::BTreeMap;
use spin::Mutex;

#[derive(Debug, Clone, Copy)]
pub enum PosError {
    NotFound,
    AccessDenied,
    NotSupported,
    InvalidHandle,
    InvalidParameter,
    InvalidCommand,
    IoError(&'static str),
}

/// The unified interface that all POS entities must implement.
pub trait PosObject: Send + Sync {
    fn read(&mut self, _offset: u64, _buf: &mut [u8]) -> Result<usize, PosError> {
        Err(PosError::NotSupported)
    }
    
    fn write(&mut self, _offset: u64, _buf: &[u8]) -> Result<usize, PosError> {
        Err(PosError::NotSupported)
    }
    
    fn ioctl(&mut self, _command: u32, _arg: usize) -> Result<(), PosError> {
        Err(PosError::NotSupported)
    }
    
    fn open_node(&mut self, _path: &str) -> Result<PosObjectRef, PosError> {
        Err(PosError::NotSupported)
    }
}

pub type PosObjectRef = Arc<Mutex<dyn PosObject>>;

/// The global object namespace directory.
pub struct ObjectDirectory {
    pub entries: BTreeMap<String, PosObjectRef>,
}

pub static POS_ROOT: Mutex<Option<ObjectDirectory>> = Mutex::new(None);

pub fn init() {
    *POS_ROOT.lock() = Some(ObjectDirectory {
        entries: BTreeMap::new(),
    });
}

/// Registers an object in the POS namespace.
pub fn register_object(path: &str, obj: PosObjectRef) -> Result<(), PosError> {
    let mut root_lock = POS_ROOT.lock();
    if let Some(root) = root_lock.as_mut() {
        root.entries.insert(String::from(path), obj);
        Ok(())
    } else {
        Err(PosError::NotFound)
    }
}

/// Opens an object by its path and returns a reference.
pub fn open_object(path: &str) -> Result<PosObjectRef, PosError> {
    let root_lock = POS_ROOT.lock();
    if let Some(root) = root_lock.as_ref() {
        let mut best_match: Option<(PosObjectRef, usize)> = None;
        for (key, obj) in &root.entries {
            if path.starts_with(key.as_str()) {
                let len = key.len();
                if best_match.is_none() || len > best_match.as_ref().unwrap().1 {
                    best_match = Some((Arc::clone(obj), len));
                }
            }
        }
        
        if let Some((obj, len)) = best_match {
            if len == path.len() {
                return Ok(obj);
            } else {
                let remainder = &path[len..];
                // drop root lock before locking object to prevent deadlocks
                drop(root_lock);
                return obj.lock().open_node(remainder);
            }
        }
    }
    Err(PosError::NotFound)
}

/// A per-process (or currently global) table of handles.
pub struct HandleTable {
    handles: BTreeMap<usize, PosObjectRef>,
    next_handle: usize,
}

impl HandleTable {
    pub const fn new() -> Self {
        Self {
            handles: BTreeMap::new(),
            next_handle: 1, // Handle 0 is usually invalid
        }
    }

    pub fn open(&mut self, path: &str) -> Result<usize, PosError> {
        let obj = open_object(path)?;
        let handle = self.next_handle;
        self.next_handle += 1;
        self.handles.insert(handle, obj);
        Ok(handle)
    }

    pub fn close(&mut self, handle: usize) -> Result<(), PosError> {
        if self.handles.remove(&handle).is_some() {
            Ok(())
        } else {
            Err(PosError::InvalidHandle)
        }
    }

    pub fn get_object(&self, handle: usize) -> Result<PosObjectRef, PosError> {
        self.handles.get(&handle).cloned().ok_or(PosError::InvalidHandle)
    }
}
