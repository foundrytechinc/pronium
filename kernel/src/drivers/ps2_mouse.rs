use crate::pdf::Driver;
use crate::pos::{PosObject, PosError, PosObjectRef};
use alloc::boxed::Box;
use spin::Mutex;
use x86_64::instructions::port::Port;
use alloc::sync::Arc;
use alloc::collections::VecDeque;

pub struct Ps2MouseDriver;

impl Driver for Ps2MouseDriver {
    fn name(&self) -> &str {
        "PS/2 Mouse"
    }

    fn probe_isa(&mut self) -> bool {
        // Assume PS/2 controller has a mouse.
        true
    }

    fn init(&mut self) -> Result<(), PosError> {
        crate::println!("    [PS/2] Starting init...");
        unsafe {
            let mut cmd = Port::<u8>::new(0x64);
            let mut data = Port::<u8>::new(0x60);

            let mut wait_write = |cmd: &mut Port<u8>| {
                for _ in 0..100000 {
                    if (cmd.read() & 2) == 0 { break; }
                }
            };
            let mut wait_read = |cmd: &mut Port<u8>| {
                for _ in 0..100000 {
                    if (cmd.read() & 1) != 0 { break; }
                }
            };

            crate::println!("    [PS/2] Enabling auxiliary device...");
            wait_write(&mut cmd);
            cmd.write(0xA8);

            crate::println!("    [PS/2] Reading Compaq Status...");
            wait_write(&mut cmd);
            cmd.write(0x20);
            wait_read(&mut cmd);
            let mut status = data.read();
            status |= 2; // Enable IRQ 12
            status &= !0x20; // Disable mouse clock line

            crate::println!("    [PS/2] Writing Compaq Status...");
            wait_write(&mut cmd);
            cmd.write(0x60);
            wait_write(&mut cmd);
            data.write(status);

            crate::println!("    [PS/2] Telling mouse to use default settings...");
            wait_write(&mut cmd);
            cmd.write(0xD4);
            wait_write(&mut cmd);
            data.write(0xF6);
            crate::println!("    [PS/2] Waiting for ACK 1...");
            wait_read(&mut cmd);
            let _ack = data.read();

            crate::println!("    [PS/2] Enabling packet streaming...");
            wait_write(&mut cmd);
            cmd.write(0xD4);
            wait_write(&mut cmd);
            data.write(0xF4);
            crate::println!("    [PS/2] Waiting for ACK 2...");
            wait_read(&mut cmd);
            let _ack2 = data.read();
            crate::println!("    [PS/2] Initialization complete.");
        }

        let mouse_obj = Arc::new(Mutex::new(MouseDevice::new()));
        // Store global reference so interrupt handler can push data
        x86_64::instructions::interrupts::without_interrupts(|| {
            *MOUSE_DEV.lock() = Some(mouse_obj.clone());
        });

        crate::pos::register_object("\\Device\\Mouse0", mouse_obj)
    }
}

pub static MOUSE_DEV: Mutex<Option<Arc<Mutex<MouseDevice>>>> = Mutex::new(None);

pub struct MouseDevice {
    buffer: VecDeque<u8>,
}

impl MouseDevice {
    pub fn new() -> Self {
        Self {
            buffer: VecDeque::with_capacity(256),
        }
    }

    pub fn push_byte(&mut self, b: u8) {
        if self.buffer.len() < 256 {
            self.buffer.push_back(b);
        }
    }
}

impl PosObject for MouseDevice {
    fn read(&mut self, _offset: u64, buf: &mut [u8]) -> Result<usize, PosError> {
        let mut count = 0;
        x86_64::instructions::interrupts::without_interrupts(|| {
            while count < buf.len() && !self.buffer.is_empty() {
                buf[count] = self.buffer.pop_front().unwrap();
                count += 1;
            }
        });
        Ok(count)
    }

    fn write(&mut self, _offset: u64, _buf: &[u8]) -> Result<usize, PosError> {
        Err(PosError::AccessDenied)
    }
    
    fn ioctl(&mut self, _command: u32, _arg: usize) -> Result<(), PosError> {
        Ok(())
    }
    
    fn open_node(&mut self, _path: &str) -> Result<PosObjectRef, PosError> {
        Err(PosError::NotFound)
    }
}
