




use alloc::collections::VecDeque;
use alloc::vec::Vec;
use spin::Mutex;
use alloc::sync::Arc;

#[derive(Debug, Clone)]
pub struct Message {
    pub code: u32,
    pub data: Vec<u8>,
}

pub struct Port {
    queue: Mutex<VecDeque<Message>>,
    capacity: usize,
}

impl Port {
    pub fn new(capacity: usize) -> Arc<Self> {
        Arc::new(Self {
            queue: Mutex::new(VecDeque::with_capacity(capacity)),
            capacity,
        })
    }

    pub fn write_port(&self, code: u32, data: &[u8]) -> Result<(), &'static str> {
        let mut q = self.queue.lock();
        if q.len() >= self.capacity {
            return Err("Port is full");
        }
        q.push_back(Message {
            code,
            data: data.to_vec(),
        });
        Ok(())
    }

    pub fn read_port(&self) -> Option<Message> {
        let mut q = self.queue.lock();
        q.pop_front()
    }
}
