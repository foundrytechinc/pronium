// Copyright (C) 2026 Pronin. All rights reserved.

use crate::vga::{VgaWriter, Color};
use crate::commands::execute_command;
use crate::ramfs::{RamFs, MAX_FILE_SIZE};
use crate::drivers::keyboard::KeyEvent;

const MAX_INPUT: usize = 128;
const MAX_HISTORY: usize = 8;

#[derive(PartialEq, Eq, Clone, Copy)]
pub enum ShellState {
    Command,
    Editor { node_index: usize },
    FatEditor { path: [u8; 32], path_len: usize, data: [u8; 4096], data_len: usize },
}

pub struct Shell {
    input_buffer: [u8; MAX_INPUT],
    input_len: usize,
    history: [[u8; MAX_INPUT]; MAX_HISTORY],
    history_len: [usize; MAX_HISTORY],
    history_count: usize,
    history_index: usize,
    pub current_user: &'static str,
    pub state: ShellState,
}

impl Shell {
    pub const fn new() -> Self {
        Self {
            input_buffer: [0; MAX_INPUT],
            input_len: 0,
            history: [[0; MAX_INPUT]; MAX_HISTORY],
            history_len: [0; MAX_HISTORY],
            history_count: 0,
            history_index: 0,
            current_user: "pronin",
            state: ShellState::Command,
        }
    }

    pub fn print_prompt(&self, writer: &mut VgaWriter) {
        if self.state == ShellState::Command {
            writer.set_color(Color::LightGreen, Color::Black);
            writer.write_string(self.current_user);
            writer.write_string("@pronium:~# ");
            writer.set_color(Color::White, Color::Black);
        }
    }

    pub fn handle_event(&mut self, event: KeyEvent, writer: &mut VgaWriter) {
        match self.state {
            ShellState::Command => self.handle_event_command(event, writer),
            ShellState::Editor { node_index } => self.handle_event_editor(event, writer, node_index),
            ShellState::FatEditor { .. } => self.handle_event_fat_editor(event, writer),
        }
    }

    fn save_history(&mut self) {
        if self.input_len == 0 { return; }
        
        // Shift history
        for i in (1..MAX_HISTORY).rev() {
            self.history[i] = self.history[i-1];
            self.history_len[i] = self.history_len[i-1];
        }
        
        self.history[0] = self.input_buffer;
        self.history_len[0] = self.input_len;
        
        if self.history_count < MAX_HISTORY {
            self.history_count += 1;
        }
        self.history_index = 0;
    }

    fn load_history(&mut self, idx: usize, writer: &mut VgaWriter) {
        if idx >= self.history_count { return; }
        
        // Clear current line
        for _ in 0..self.input_len {
            writer.write_byte(0x08);
            writer.write_byte(b' ');
            writer.write_byte(0x08);
        }
        
        self.input_len = self.history_len[idx];
        self.input_buffer[..self.input_len].copy_from_slice(&self.history[idx][..self.input_len]);
        
        for i in 0..self.input_len {
            writer.write_byte(self.input_buffer[i]);
        }
    }

    fn handle_event_command(&mut self, event: KeyEvent, writer: &mut VgaWriter) {
        match event {
            KeyEvent::Char(c) => {
                if c == '\n' {
                    writer.write_byte(b'\n');
                    self.save_history();
                    self.process_command(writer);
                    self.input_len = 0;
                    self.print_prompt(writer);
                } else if c == '\x08' {
                    if self.input_len > 0 {
                        self.input_len -= 1;
                        writer.write_byte(0x08);
                        writer.write_byte(b' ');
                        writer.write_byte(0x08);
                    }
                } else if c.is_ascii() && c != '\r' && c != '\x1B' {
                    if self.input_len < MAX_INPUT {
                        self.input_buffer[self.input_len] = c as u8;
                        self.input_len += 1;
                        writer.write_byte(c as u8);
                    }
                }
            },
            KeyEvent::Up => {
                if self.history_count > 0 {
                    let next_idx = if self.input_len == 0 && self.history_index == 0 { 0 } else { (self.history_index + 1).min(self.history_count - 1) };
                    self.load_history(next_idx, writer);
                    self.history_index = next_idx;
                }
            },
            KeyEvent::Down => {
                if self.history_count > 0 {
                    if self.history_index > 0 {
                        let prev_idx = self.history_index - 1;
                        self.load_history(prev_idx, writer);
                        self.history_index = prev_idx;
                    } else {
                        // Clear line
                        for _ in 0..self.input_len {
                            writer.write_byte(0x08);
                            writer.write_byte(b' ');
                            writer.write_byte(0x08);
                        }
                        self.input_len = 0;
                    }
                }
            },
            _ => {}
        }
    }

    fn handle_event_editor(&mut self, event: KeyEvent, writer: &mut VgaWriter, node_index: usize) {
        if let KeyEvent::Char(c) = event {
            if c == '\x1B' { // ESC
                self.state = ShellState::Command;
                writer.clear_screen();
                writer.set_color(Color::LightCyan, Color::Black);
                writer.write_string("File saved.\n");
                self.print_prompt(writer);
                return;
            }

            let node = &mut crate::RAM_FS.lock().nodes[node_index];
            if c == '\x08' {
                if node.data_len > 0 {
                    node.data_len -= 1;
                    writer.write_byte(0x08);
                    writer.write_byte(b' ');
                    writer.write_byte(0x08);
                }
            } else if c == '\n' {
                if node.data_len < MAX_FILE_SIZE {
                    node.data[node.data_len] = b'\n';
                    node.data_len += 1;
                    writer.write_byte(b'\n');
                }
            } else if c.is_ascii() && c != '\r' {
                if node.data_len < MAX_FILE_SIZE {
                    node.data[node.data_len] = c as u8;
                    node.data_len += 1;
                    writer.write_byte(c as u8);
                }
            }
        }
    }

    fn handle_event_fat_editor(&mut self, event: KeyEvent, writer: &mut VgaWriter) {
        let mut do_save = false;
        
        if let KeyEvent::Char(c) = event {
            if c == '\x1B' { // ESC
                do_save = true;
            } else {
                if let ShellState::FatEditor { ref mut data, ref mut data_len, .. } = self.state {
                    if c == '\x08' {
                        if *data_len > 0 {
                            *data_len -= 1;
                            writer.write_byte(0x08);
                            writer.write_byte(b' ');
                            writer.write_byte(0x08);
                        }
                    } else if c == '\n' {
                        if *data_len < data.len() {
                            data[*data_len] = b'\n';
                            *data_len += 1;
                            writer.write_byte(b'\n');
                        }
                    } else if c.is_ascii() && c != '\r' {
                        if *data_len < data.len() {
                            data[*data_len] = c as u8;
                            *data_len += 1;
                            writer.write_byte(c as u8);
                        }
                    }
                }
            }
        }

        if do_save {
            if let ShellState::FatEditor { path, path_len, data, data_len } = self.state {
                if let Ok(path_str) = core::str::from_utf8(&path[..path_len]) {
                    let mut fat_lock = unsafe { crate::FAT_FS.lock() };
                    if let Some(fat) = &mut *fat_lock {
                        let _ = fat.write_file(fat.root_cluster, path_str, &data[..data_len]);
                    }
                }
            }
            self.state = ShellState::Command;
            writer.clear_screen();
            writer.set_color(Color::LightCyan, Color::Black);
            writer.write_string("File saved to FAT32.\n");
            self.print_prompt(writer);
        }
    }

    fn process_command(&mut self, writer: &mut VgaWriter) {
        if self.input_len == 0 {
            return;
        }

        let mut temp_buf = [0u8; MAX_INPUT];
        let len = self.input_len;
        temp_buf[..len].copy_from_slice(&self.input_buffer[..len]);

        if let Ok(cmd_str) = core::str::from_utf8(&temp_buf[..len]) {
            let cmd_str = cmd_str.trim();
            if !cmd_str.is_empty() {
                execute_command(cmd_str, writer, self);
            }
        }
    }
}

pub fn run() -> ! {
    let mut shell = Shell::new();
    if let Some(writer) = crate::vga::WRITER.lock().as_mut() {
        shell.print_prompt(writer);
    }
    let mut kb = crate::drivers::keyboard::Keyboard::new();

    loop {
        if let Some(scancode) = crate::drivers::keyboard::poll_scancode() {
            if let Some(event) = kb.process_scancode(scancode) {
                if let Some(writer) = crate::vga::WRITER.lock().as_mut() {
                    shell.handle_event(event, writer);
                }
            }
        }
        crate::task::yield_now();
    }
}
