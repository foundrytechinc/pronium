



use crate::pos::{PosObject, PosError};
use spin::Mutex;
use x86_64::instructions::port::Port;

const PIT_COMMAND_PORT: u16 = 0x43;
const PIT_DATA_PORT: u16 = 0x42;
const SPEAKER_PORT: u16 = 0x61;


pub struct SpeakerDriver {
    
    state: Mutex<SpeakerState>,
}

struct SpeakerState {
    is_playing: bool,
    current_frequency: u32,
}

impl SpeakerDriver {
    pub fn new() -> Self {
        Self {
            state: Mutex::new(SpeakerState {
                is_playing: false,
                current_frequency: 0,
            }),
        }
    }

    
    fn play_tone(&self, hz: u32) {
        if hz == 0 {
            self.stop_tone();
            return;
        }

        
        let division = 1193180 / hz;

        unsafe {
            let mut cmd_port = Port::<u8>::new(PIT_COMMAND_PORT);
            let mut data_port = Port::<u8>::new(PIT_DATA_PORT);
            let mut speaker_port = Port::<u8>::new(SPEAKER_PORT);

            
            cmd_port.write(0xB6);
            
            data_port.write((division & 0xFF) as u8);
            data_port.write(((division >> 8) & 0xFF) as u8);

            
            let port_val = speaker_port.read();
            
            
            if (port_val & 3) != 3 {
                speaker_port.write(port_val | 3);
            }
        }
    }

    
    fn stop_tone(&self) {
        unsafe {
            let mut speaker_port = Port::<u8>::new(SPEAKER_PORT);
            let port_val = speaker_port.read();
            
            speaker_port.write(port_val & 0xFC);
        }
    }
}


impl PosObject for SpeakerDriver {
    
    
    
    fn write(&mut self, _offset: u64, buffer: &[u8]) -> Result<usize, PosError> {
        if buffer.len() < 4 {
            return Err(PosError::InvalidParameter);
        }

        
        let hz = u32::from_le_bytes([buffer[0], buffer[1], buffer[2], buffer[3]]);
        
        let mut state = self.state.lock();
        self.play_tone(hz);
        state.is_playing = hz > 0;
        state.current_frequency = hz;

        Ok(4) 
    }

    
    fn read(&mut self, _offset: u64, buffer: &mut [u8]) -> Result<usize, PosError> {
        if buffer.is_empty() {
            return Err(PosError::InvalidParameter);
        }

        let state = self.state.lock();
        
        buffer[0] = if state.is_playing { 1 } else { 0 };
        Ok(1)
    }

    
    fn ioctl(&mut self, command: u32, arg: usize) -> Result<(), PosError> {
        match command {
            0x4001 => { 
                self.play_tone(arg as u32);
                Ok(())
            }
            0x4002 => { 
                self.stop_tone();
                Ok(())
            }
            _ => Err(PosError::InvalidCommand),
        }
    }
}

impl crate::pdf::Driver for SpeakerDriver {
    fn name(&self) -> &str {
        "PC-SPEAKER Driver"
    }

    fn probe_isa(&mut self) -> bool {
        
        true
    }

    fn init(&mut self) -> Result<(), PosError> {
        crate::pos::register_object(
            r"\Device\Speaker",
            alloc::sync::Arc::new(spin::Mutex::new(SpeakerDriver::new()))
        )?;
        Ok(())
    }
}

use alloc::sync::Arc;

pub static SPEAKER_SERVER_PORT: spin::Once<Arc<crate::port::Port>> = spin::Once::new();

pub extern "C" fn speaker_server() {
    let port = SPEAKER_SERVER_PORT.call_once(|| crate::port::Port::new(32));
    
    let mut handle = 0;
    if crate::syscall::NtOpenFile("\\Device\\Speaker", &mut handle) != crate::syscall::NtStatus::Success {
        loop {
            crate::task::yield_now();
        }
    }
    
    loop {
        if let Some(msg) = port.read_port() {
            match msg.code {
                0x4001 => {
                    if msg.data.len() >= 4 {
                        let hz = u32::from_le_bytes([msg.data[0], msg.data[1], msg.data[2], msg.data[3]]);
                        let _ = crate::syscall::NtDeviceIoControlFile(handle, 0x4001, hz as usize);
                    }
                }
                0x4002 => {
                    let _ = crate::syscall::NtDeviceIoControlFile(handle, 0x4002, 0);
                }
                _ => {}
            }
        }
        crate::task::yield_now();
    }
}

