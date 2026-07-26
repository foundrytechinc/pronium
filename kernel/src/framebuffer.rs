use spin::Mutex;
use core::ptr;
use font8x8::{UnicodeFonts, BASIC_FONTS};

pub struct Framebuffer {
    pub base_addr: *mut u8,
    pub width: usize,
    pub height: usize,
    pub pitch: usize,
    pub bytes_per_pixel: usize,
}

unsafe impl Send for Framebuffer {}
unsafe impl Sync for Framebuffer {}

pub static FRAMEBUFFER: Mutex<Framebuffer> = Mutex::new(Framebuffer {
    base_addr: ptr::null_mut(),
    width: 0,
    height: 0,
    pitch: 0,
    bytes_per_pixel: 1,
});

pub fn init_framebuffer(base_addr: usize, width: usize, height: usize, pitch: usize, bytes_per_pixel: usize) {
    let mut fb = FRAMEBUFFER.lock();
    fb.base_addr = base_addr as *mut u8;
    fb.width = width;
    fb.height = height;
    fb.pitch = pitch;
    fb.bytes_per_pixel = bytes_per_pixel;
}

pub fn write_pixel(x: usize, y: usize, color: u32) {
    let fb = FRAMEBUFFER.lock();
    
    if fb.base_addr.is_null() {
        return;
    }

    if x >= fb.width || y >= fb.height {
        return;
    }

    let offset = (y * fb.pitch) + (x * fb.bytes_per_pixel);
    
    unsafe {
        if fb.bytes_per_pixel == 1 {
            ptr::write_volatile(fb.base_addr.add(offset), (color & 0xFF) as u8);
        } else {
            ptr::write_volatile(fb.base_addr.add(offset) as *mut u32, color);
        }
    }
}

pub fn clear_screen(color: u32) {
    let fb = FRAMEBUFFER.lock();
    if fb.base_addr.is_null() { return; }

    unsafe {
        let size = fb.pitch * fb.height;
        if color == 0 {
            core::ptr::write_bytes(fb.base_addr, 0, size);
        } else if fb.bytes_per_pixel == 4 {
            
            let color64 = (color as u64) | ((color as u64) << 32);
            let ptr64 = fb.base_addr as *mut u64;
            let count64 = size / 8;
            for i in 0..count64 {
                *ptr64.add(i) = color64;
            }
            
            let remainder = size % 8;
            if remainder != 0 {
                let start = size - remainder;
                for i in 0..remainder {
                    *fb.base_addr.add(start + i) = (color & 0xFF) as u8;
                }
            }
        } else {
            
            for y in 0..fb.height {
                for x in 0..fb.width {
                    let offset = (y * fb.pitch) + (x * fb.bytes_per_pixel);
                    if fb.bytes_per_pixel == 1 {
                        core::ptr::write_volatile(fb.base_addr.add(offset), (color & 0xFF) as u8);
                    } else {
                        core::ptr::write_volatile(fb.base_addr.add(offset) as *mut u32, color);
                    }
                }
            }
        }
    }
}

pub fn draw_rect(x: usize, y: usize, width: usize, height: usize, color: u32) {
    let fb = FRAMEBUFFER.lock();
    if fb.base_addr.is_null() { return; }

    for h in 0..height {
        for w in 0..width {
            let cx = x + w;
            let cy = y + h;
            if cx < fb.width && cy < fb.height {
                let offset = (cy * fb.pitch) + (cx * fb.bytes_per_pixel);
                unsafe {
                    if fb.bytes_per_pixel == 1 {
                        ptr::write_volatile(fb.base_addr.add(offset), (color & 0xFF) as u8);
                    } else {
                        ptr::write_volatile(fb.base_addr.add(offset) as *mut u32, color);
                    }
                }
            }
        }
    }
}

pub fn scroll_up(pixels: usize) {
    let fb = FRAMEBUFFER.lock();
    if fb.base_addr.is_null() { return; }

    let bytes_to_scroll = pixels * fb.pitch;
    let total_bytes = fb.height * fb.pitch;

    if bytes_to_scroll >= total_bytes {
        return;
    }

    unsafe {
        core::ptr::copy(
            fb.base_addr.add(bytes_to_scroll),
            fb.base_addr,
            total_bytes - bytes_to_scroll
        );
    }
}

fn get_glyph(c: char) -> Option<[u8; 8]> {
    BASIC_FONTS.get(c)
}

pub fn write_char(x: usize, y: usize, c: char, color: u32, bg_color: u32, scale: usize) {
    let fb = FRAMEBUFFER.lock();
    if fb.base_addr.is_null() { return; }

    if let Some(glyph) = get_glyph(c) {
        for (row_idx, row) in glyph.iter().enumerate() {
            for col_idx in 0..8 {
                let bit = (row >> col_idx) & 1;
                let final_color = if bit != 0 { color } else { bg_color };
                for sy in 0..scale {
                    for sx in 0..scale {
                        let cx = x + col_idx * scale + sx;
                        let cy = y + row_idx * scale + sy;
                        if cx < fb.width && cy < fb.height {
                            let offset = (cy * fb.pitch) + (cx * fb.bytes_per_pixel);
                            unsafe {
                                if fb.bytes_per_pixel == 1 {
                                    ptr::write_volatile(fb.base_addr.add(offset), (final_color & 0xFF) as u8);
                                } else {
                                    ptr::write_volatile(fb.base_addr.add(offset) as *mut u32, final_color);
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

pub fn write_string(x: usize, y: usize, s: &str, color: u32, bg_color: u32, scale: usize) {
    let mut current_x = x;
    let mut current_y = y;

    for c in s.chars() {
        if c == '\n' {
            current_x = x;
            current_y += 8 * scale;
            continue;
        }

        write_char(current_x, current_y, c, color, bg_color, scale);
        current_x += 8 * scale;
    }
}
