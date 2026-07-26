

use core::arch::asm;


#[inline]
pub unsafe fn inb(port: u16) -> u8 {
    let data: u8;
    asm!(
        "in al, dx",
        in("dx") port,
        out("al") data,
        options(nomem, nostack, preserves_flags)
    );
    data
}


pub fn poll_scancode() -> Option<u8> {
    unsafe {
        let status = inb(0x64);
        if (status & 0x01) != 0 {
            Some(inb(0x60))
        } else {
            None
        }
    }
}

pub enum KeyEvent {
    Char(char),
    Up,
    Down,
    Left,
    Right,
}

pub static KEYBOARD: spin::Mutex<Keyboard> = spin::Mutex::new(Keyboard::new());

pub struct ConIn;

impl crate::pos::PosObject for ConIn {
    fn read(&mut self, _offset: u64, buf: &mut [u8]) -> Result<usize, crate::pos::PosError> {
        if buf.is_empty() { return Ok(0); }
        
        loop {
            if let Some(scancode) = poll_scancode() {
                let mut kb = KEYBOARD.lock();
                if let Some(event) = kb.process_scancode(scancode) {
                    if let KeyEvent::Char(c) = event {
                        let mut buf_idx = 0;
                        for &b in c.encode_utf8(&mut [0; 4]).as_bytes() {
                            if buf_idx < buf.len() {
                                buf[buf_idx] = b;
                                buf_idx += 1;
                            }
                        }
                        return Ok(buf_idx);
                    }
                }
            }
            crate::task::yield_now();
        }
    }
}

pub fn init() {
    let _ = crate::pos::register_object(
        r"\Device\ConIn",
        alloc::sync::Arc::new(spin::Mutex::new(ConIn))
    );
}

pub struct Keyboard {
    shift_down: bool,
    extended: bool,
}

impl Keyboard {
    pub const fn new() -> Self {
        Self {
            shift_down: false,
            extended: false,
        }
    }

    pub fn process_scancode(&mut self, scancode: u8) -> Option<KeyEvent> {
        if scancode == 0xE0 {
            self.extended = true;
            return None;
        }

        let is_release = (scancode & 0x80) != 0;
        let base_code = scancode & 0x7F;

        if base_code == 0x2A || base_code == 0x36 {
            self.shift_down = !is_release;
            self.extended = false;
            return None;
        }

        if is_release {
            self.extended = false;
            return None;
        }

        let ext = self.extended;
        self.extended = false;

        if ext {
            match base_code {
                0x48 => return Some(KeyEvent::Up),
                0x50 => return Some(KeyEvent::Down),
                0x4B => return Some(KeyEvent::Left),
                0x4D => return Some(KeyEvent::Right),
                _ => return None,
            }
        }

        let c = match base_code {
            0x01 => Some('\x1B'),  
            0x02 => if self.shift_down { Some('!') } else { Some('1') },
            0x03 => if self.shift_down { Some('@') } else { Some('2') },
            0x04 => if self.shift_down { Some('#') } else { Some('3') },
            0x05 => if self.shift_down { Some('$') } else { Some('4') },
            0x06 => if self.shift_down { Some('%') } else { Some('5') },
            0x07 => if self.shift_down { Some('^') } else { Some('6') },
            0x08 => if self.shift_down { Some('&') } else { Some('7') },
            0x09 => if self.shift_down { Some('*') } else { Some('8') },
            0x0A => if self.shift_down { Some('(') } else { Some('9') },
            0x0B => if self.shift_down { Some(')') } else { Some('0') },
            0x0C => if self.shift_down { Some('_') } else { Some('-') },
            0x0D => if self.shift_down { Some('+') } else { Some('=') },
            0x0E => Some('\x08'),  
            0x0F => Some('\t'),    
            0x10 => if self.shift_down { Some('Q') } else { Some('q') },
            0x11 => if self.shift_down { Some('W') } else { Some('w') },
            0x12 => if self.shift_down { Some('E') } else { Some('e') },
            0x13 => if self.shift_down { Some('R') } else { Some('r') },
            0x14 => if self.shift_down { Some('T') } else { Some('t') },
            0x15 => if self.shift_down { Some('Y') } else { Some('y') },
            0x16 => if self.shift_down { Some('U') } else { Some('u') },
            0x17 => if self.shift_down { Some('I') } else { Some('i') },
            0x18 => if self.shift_down { Some('O') } else { Some('o') },
            0x19 => if self.shift_down { Some('P') } else { Some('p') },
            0x1A => if self.shift_down { Some('{') } else { Some('[') },
            0x1B => if self.shift_down { Some('}') } else { Some(']') },
            0x1C => Some('\n'),    
            0x1E => if self.shift_down { Some('A') } else { Some('a') },
            0x1F => if self.shift_down { Some('S') } else { Some('s') },
            0x20 => if self.shift_down { Some('D') } else { Some('d') },
            0x21 => if self.shift_down { Some('F') } else { Some('f') },
            0x22 => if self.shift_down { Some('G') } else { Some('g') },
            0x23 => if self.shift_down { Some('H') } else { Some('h') },
            0x24 => if self.shift_down { Some('J') } else { Some('j') },
            0x25 => if self.shift_down { Some('K') } else { Some('k') },
            0x26 => if self.shift_down { Some('L') } else { Some('l') },
            0x27 => if self.shift_down { Some(':') } else { Some(';') },
            0x28 => if self.shift_down { Some('"') } else { Some('\'') },
            0x29 => if self.shift_down { Some('~') } else { Some('`') },
            0x2B => if self.shift_down { Some('|') } else { Some('\\') },
            0x2C => if self.shift_down { Some('Z') } else { Some('z') },
            0x2D => if self.shift_down { Some('X') } else { Some('x') },
            0x2E => if self.shift_down { Some('C') } else { Some('c') },
            0x2F => if self.shift_down { Some('V') } else { Some('v') },
            0x30 => if self.shift_down { Some('B') } else { Some('b') },
            0x31 => if self.shift_down { Some('N') } else { Some('n') },
            0x32 => if self.shift_down { Some('M') } else { Some('m') },
            0x33 => if self.shift_down { Some('<') } else { Some(',') },
            0x34 => if self.shift_down { Some('>') } else { Some('.') },
            0x35 => if self.shift_down { Some('?') } else { Some('/') },
            0x39 => Some(' '),
            _ => None,
        };

        c.map(KeyEvent::Char)
    }
}
