

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Color {
    Black = 0,
    Blue = 1,
    Green = 2,
    Cyan = 3,
    Red = 4,
    Magenta = 5,
    Brown = 6,
    LightGray = 7,
    DarkGray = 8,
    LightBlue = 9,
    LightGreen = 10,
    LightCyan = 11,
    LightRed = 12,
    Pink = 13,
    Yellow = 14,
    White = 15,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(transparent)]
pub struct ColorCode(u8);

impl ColorCode {
    pub const fn new(foreground: Color, background: Color) -> ColorCode {
        ColorCode((background as u8) << 4 | (foreground as u8))
    }

    pub fn fg(&self) -> u32 {
        color_to_u32(self.0 & 0xF)
    }

    pub fn bg(&self) -> u32 {
        color_to_u32(self.0 >> 4)
    }
}

fn color_to_u32(color: u8) -> u32 {
    match color {
        0 => 0x000000,
        1 => 0x0000AA,
        2 => 0x00AA00,
        3 => 0x00AAAA,
        4 => 0xAA0000,
        5 => 0xAA00AA,
        6 => 0xAA5500,
        7 => 0xAAAAAA,
        8 => 0x555555,
        9 => 0x5555FF,
        10 => 0x55FF55,
        11 => 0x55FFFF,
        12 => 0xFF5555,
        13 => 0xFF55FF,
        14 => 0xFFFF55,
        15 => 0xFFFFFF,
        _ => 0xFFFFFF,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
struct ScreenChar {
    unicode_character: char,
    color_code: ColorCode,
}

const MAX_BUFFER_HEIGHT: usize = 144; 
const MAX_BUFFER_WIDTH: usize = 256;  

#[repr(transparent)]
struct Buffer {
    chars: [[ScreenChar; MAX_BUFFER_WIDTH]; MAX_BUFFER_HEIGHT],
}

static mut OFFSCREEN_BUFFER: Buffer = Buffer {
    chars: [[ScreenChar {
        unicode_character: ' ',
        color_code: ColorCode::new(Color::Black, Color::Black),
    }; MAX_BUFFER_WIDTH]; MAX_BUFFER_HEIGHT],
};

pub struct VgaWriter {
    pub row: usize,
    pub col: usize,
    color_code: ColorCode,
    buffer: &'static mut Buffer,
    pub buffer_width: usize,
    pub buffer_height: usize,
    pub scale: usize,
}

impl VgaWriter {
    pub fn new() -> Self {
        let (fb_width, fb_height) = {
            let fb = crate::framebuffer::FRAMEBUFFER.lock();
            (fb.width, fb.height)
        };

        // Automatic scaling for better readability on high resolutions
        let scale = if fb_width >= 1920 {
            2
        } else if fb_width >= 1024 {
            2
        } else {
            1
        };

        let mut buffer_width = fb_width / (8 * scale);
        let mut buffer_height = fb_height / (8 * scale);

        if buffer_width > MAX_BUFFER_WIDTH { buffer_width = MAX_BUFFER_WIDTH; }
        if buffer_height > MAX_BUFFER_HEIGHT { buffer_height = MAX_BUFFER_HEIGHT; }

        Self {
            row: 0,
            col: 0,
            color_code: ColorCode::new(Color::White, Color::Black),
            buffer: unsafe { &mut *core::ptr::addr_of_mut!(OFFSCREEN_BUFFER) },
            buffer_width,
            buffer_height,
            scale,
        }
    }

    pub fn update_scale_and_clear(&mut self) {
        let (fb_width, fb_height) = {
            let fb = crate::framebuffer::FRAMEBUFFER.lock();
            (fb.width, fb.height)
        };

        let scale = if fb_width >= 1920 {
            2
        } else if fb_width >= 1024 {
            2
        } else {
            1
        };

        let mut buffer_width = fb_width / (8 * scale);
        let mut buffer_height = fb_height / (8 * scale);

        if buffer_width > MAX_BUFFER_WIDTH { buffer_width = MAX_BUFFER_WIDTH; }
        if buffer_height > MAX_BUFFER_HEIGHT { buffer_height = MAX_BUFFER_HEIGHT; }

        self.scale = scale;
        self.buffer_width = buffer_width;
        self.buffer_height = buffer_height;
        self.clear_screen();
    }

    pub fn set_color(&mut self, fg: Color, bg: Color) {
        self.color_code = ColorCode::new(fg, bg);
    }

    pub fn write_char(&mut self, c: char) {
        unsafe {
            let mut buf = [0; 4];
            let bytes = c.encode_utf8(&mut buf).as_bytes();
            for &byte in bytes {
                // Wait for serial TX buffer to be empty
                while (x86_64::instructions::port::Port::<u8>::new(0x3F8 + 5).read() & 0x20) == 0 {}
                x86_64::instructions::port::Port::<u8>::new(0x3F8).write(byte);
            }
        }

        match c {
            '\n' => self.new_line(),
            '\r' => self.col = 0,
            '\x08' => {
                // Backspace
                if self.col > 0 {
                    self.col -= 1;
                } else if self.row > 0 {
                    self.row -= 1;
                    self.col = self.buffer_width - 1;
                }
                let row = self.row;
                let col = self.col;
                self.buffer.chars[row][col] = ScreenChar {
                    unicode_character: ' ',
                    color_code: self.color_code,
                };
                crate::framebuffer::write_char(col * 8 * self.scale, row * 8 * self.scale, ' ', self.color_code.fg(), self.color_code.bg(), self.scale);
            }
            c => {
                if self.col >= self.buffer_width {
                    self.new_line();
                }
                let row = self.row;
                let col = self.col;
                let color_code = self.color_code;
                self.buffer.chars[row][col] = ScreenChar {
                    unicode_character: c,
                    color_code,
                };
                crate::framebuffer::write_char(col * 8 * self.scale, row * 8 * self.scale, c, color_code.fg(), color_code.bg(), self.scale);
                self.col += 1;
            }
        }
    }

    pub fn write_byte(&mut self, byte: u8) {
        self.write_char(byte as char);
    }

    pub fn write_string(&mut self, s: &str) {
        for c in s.chars() {
            self.write_char(c);
        }
    }

    fn new_line(&mut self) {
        if self.row < self.buffer_height - 1 {
            self.row += 1;
            self.col = 0;
        } else {
            self.scroll_up();
            self.col = 0;
        }
    }

    fn scroll_up(&mut self) {
        crate::framebuffer::scroll_up(8 * self.scale);
        for row in 1..self.buffer_height {
            for col in 0..self.buffer_width {
                let character = self.buffer.chars[row][col];
                self.buffer.chars[row - 1][col] = character;
            }
        }
        let last_row = self.buffer_height - 1;
        self.clear_row(last_row);
        self.row = last_row;
    }

    fn clear_row(&mut self, row: usize) {
        let blank = ScreenChar {
            unicode_character: ' ',
            color_code: self.color_code,
        };
        for col in 0..self.buffer_width {
            self.buffer.chars[row][col] = blank;
        }
        crate::framebuffer::draw_rect(0, row * 8 * self.scale, self.buffer_width * 8 * self.scale, 8 * self.scale, self.color_code.bg());
    }

    pub fn clear_screen(&mut self) {
        let blank = ScreenChar {
            unicode_character: ' ',
            color_code: self.color_code,
        };
        for row in 0..self.buffer_height {
            for col in 0..self.buffer_width {
                self.buffer.chars[row][col] = blank;
            }
        }
        crate::framebuffer::clear_screen(self.color_code.bg());
        self.row = 0;
        self.col = 0;
    }

    pub fn redraw(&self) {
        for row in 0..self.buffer_height {
            for col in 0..self.buffer_width {
                let character = self.buffer.chars[row][col];
                crate::framebuffer::write_char(col * 8 * self.scale, row * 8 * self.scale, character.unicode_character, character.color_code.fg(), character.color_code.bg(), self.scale);
            }
        }
    }
}

impl crate::pos::PosObject for VgaWriter {
    fn write(&mut self, _offset: u64, buf: &[u8]) -> Result<usize, crate::pos::PosError> {
        for &b in buf {
            self.write_byte(b);
        }
        Ok(buf.len())
    }
}

pub static WRITER: spin::Mutex<Option<VgaWriter>> = spin::Mutex::new(None);

pub fn init() {
    let mut writer_lock = WRITER.lock();
    if writer_lock.is_none() {
        *writer_lock = Some(VgaWriter::new());
    }
    if let Some(writer) = writer_lock.as_mut() {
        writer.clear_screen();
    }
    drop(writer_lock);

    let _ = crate::pos::register_object(
        r"\Device\ConOut",
        alloc::sync::Arc::new(spin::Mutex::new(ConOut))
    );
}

pub fn clear_screen() {
    if let Some(writer) = WRITER.lock().as_mut() {
        writer.clear_screen();
    }
}

pub struct ConOut;

impl crate::pos::PosObject for ConOut {
    fn write(&mut self, _offset: u64, buf: &[u8]) -> Result<usize, crate::pos::PosError> {
        if let Some(writer) = WRITER.lock().as_mut() {
            for &b in buf {
                writer.write_byte(b);
            }
            Ok(buf.len())
        } else {
            Err(crate::pos::PosError::NotFound)
        }
    }
}

pub struct GlobalStdout;

impl core::fmt::Write for GlobalStdout {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        if let Some(writer) = WRITER.lock().as_mut() {
            writer.write_string(s);
        }
        Ok(())
    }
}

#[doc(hidden)]
pub fn _print(args: core::fmt::Arguments) {
    use core::fmt::Write;
    let _ = GlobalStdout.write_fmt(args);
}

#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => ($crate::vga::_print(format_args!($($arg)*)));
}

#[macro_export]
macro_rules! println {
    () => ($crate::print!("\n"));
    ($($arg:tt)*) => ($crate::print!("{}\n", format_args!($($arg)*)));
}
