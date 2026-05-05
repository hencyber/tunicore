//! Chat UI - conversational interface on the framebuffer
//!
//! Makes TuniCore feel like a messaging app, not a terminal.
//! System messages appear on the left, user messages on the right.
//! Simple, clean, no technical jargon visible.

use alloc::string::{String, ToString};
use alloc::vec::Vec;
use spin::Mutex;

/// Chat message types
#[derive(Clone, Copy)]
pub enum Sender {
    System,
    User,
}

/// A single chat message (fixed size for simplicity)
#[derive(Clone)]
pub struct Message {
    pub sender: Sender,
    pub data: [u8; 200],
    pub len: usize,
}

impl Message {
    fn new(sender: Sender, text: &str) -> Self {
        let mut data = [0u8; 200];
        let len = text.len().min(200);
        data[..len].copy_from_slice(&text.as_bytes()[..len]);
        Self { sender, data, len }
    }

    fn as_str(&self) -> &str {
        core::str::from_utf8(&self.data[..self.len]).unwrap_or("")
    }
}

/// Chat UI state
pub struct ChatUI {
    /// Raw framebuffer pointer
    fb_ptr: usize, // Store as usize to be Send-safe
    width: u32,
    height: u32,
    pitch: u32,
    /// All messages in the conversation
    messages: Vec<Message>,
    /// Current user input
    input_buf: [u8; 200],
    input_len: usize,
}

// Safety: framebuffer pointer is valid for the kernel's lifetime
unsafe impl Send for ChatUI {}

// Colors
const BG: u32 = 0x1A1B2E;          // Dark navy
const INPUT_BG: u32 = 0x252640;     // Slightly lighter
const SYS_BUBBLE: u32 = 0x2D2F50;  // System message background
const USER_BUBBLE: u32 = 0x4A6CF7; // Blue user message background
const TEXT_WHITE: u32 = 0xE8E8F0;  // Main text
const TEXT_DIM: u32 = 0x8888AA;    // Dim text
const ACCENT: u32 = 0x6C8CFF;     // Accent color
const CURSOR_COLOR: u32 = 0x6C8CFF;

/// Font from framebuffer module
static FONT_8X8: &[u8] = include_bytes!("font8x8.bin");

/// Global chat UI instance
pub static CHAT: Mutex<Option<ChatUI>> = Mutex::new(None);

impl ChatUI {
    /// Initialize the chat UI from a Limine framebuffer
    pub fn init(fb: &limine::framebuffer::Framebuffer) {
        let ui = ChatUI {
            fb_ptr: fb.address() as usize,
            width: fb.width as u32,
            height: fb.height as u32,
            pitch: fb.pitch as u32,
            messages: Vec::new(),
            input_buf: [0u8; 200],
            input_len: 0,
        };
        *CHAT.lock() = Some(ui);
    }

    fn fb(&mut self) -> *mut u8 {
        self.fb_ptr as *mut u8
    }

    /// Add a system message and redraw
    pub fn system_msg(&mut self, text: &str) {
        self.messages.push(Message::new(Sender::System, text));
        self.render();
    }

    /// Add a user message and redraw
    pub fn user_msg(&mut self, text: &str) {
        self.messages.push(Message::new(Sender::User, text));
        self.render();
    }

    /// Handle a keypress
    pub fn key_input(&mut self, key: u8) -> Option<String> {
        match key {
            b'\n' | 13 => {
                if self.input_len == 0 {
                    return None;
                }
                let cmd = core::str::from_utf8(&self.input_buf[..self.input_len])
                    .unwrap_or("")
                    .to_string();
                self.user_msg(&cmd);
                self.input_len = 0;
                self.render();
                Some(cmd)
            }
            8 | 0x7F => {
                if self.input_len > 0 {
                    self.input_len -= 1;
                }
                self.render();
                None
            }
            0x20..=0x7E => {
                if self.input_len < 199 {
                    self.input_buf[self.input_len] = key;
                    self.input_len += 1;
                    self.render();
                }
                None
            }
            _ => None,
        }
    }

    /// Render the entire chat UI
    fn render(&mut self) {
        let width = self.width;
        let height = self.height;
        let pitch = self.pitch;
        let fb_ptr = self.fb_ptr as *mut u8;
        let total = (pitch * height) as usize;

        // Clear background
        fill_rect_raw(fb_ptr, pitch, total, width, height, 0, 0, width, height, BG);

        // Header bar
        fill_rect_raw(fb_ptr, pitch, total, width, height, 0, 0, width, 40, 0x16172A);
        draw_text_raw(fb_ptr, pitch, total, width, height, "TuniCore", 16, 12, ACCENT);
        let hint_x = width / 2 - 80;
        draw_text_raw(fb_ptr, pitch, total, width, height, "Just type what you need", hint_x, 12, TEXT_DIM);

        // Messages area
        let msg_area_top = 50u32;
        let msg_area_bottom = height.saturating_sub(60);
        let line_height = 36u32;
        let max_msgs = ((msg_area_bottom - msg_area_top) / line_height) as usize;

        let start = if self.messages.len() > max_msgs {
            self.messages.len() - max_msgs
        } else {
            0
        };

        let mut y = msg_area_top;
        for i in start..self.messages.len() {
            let msg = &self.messages[i];
            let text = msg.as_str();
            let text_w = (text.len() as u32 * 8).min(width - 80);
            let bubble_w = text_w + 24;

            match msg.sender {
                Sender::System => {
                    fill_rounded_rect_raw(fb_ptr, pitch, total, width, height, 16, y, bubble_w, 28, SYS_BUBBLE);
                    draw_text_raw(fb_ptr, pitch, total, width, height, text, 28, y + 8, TEXT_WHITE);
                }
                Sender::User => {
                    let x = width.saturating_sub(bubble_w + 16);
                    fill_rounded_rect_raw(fb_ptr, pitch, total, width, height, x, y, bubble_w, 28, USER_BUBBLE);
                    draw_text_raw(fb_ptr, pitch, total, width, height, text, x + 12, y + 8, TEXT_WHITE);
                }
            }
            y += line_height;
        }

        // Input bar at bottom
        let input_y = height - 50;
        fill_rect_raw(fb_ptr, pitch, total, width, height, 0, input_y, width, 50, INPUT_BG);
        fill_rounded_rect_raw(fb_ptr, pitch, total, width, height, 16, input_y + 8, width - 32, 34, 0x1E1F38);

        if self.input_len == 0 {
            draw_text_raw(fb_ptr, pitch, total, width, height, "Type a message...", 28, input_y + 18, TEXT_DIM);
        } else {
            let input_text = core::str::from_utf8(&self.input_buf[..self.input_len]).unwrap_or("");
            draw_text_raw(fb_ptr, pitch, total, width, height, input_text, 28, input_y + 18, TEXT_WHITE);
            let cursor_x = 28 + (self.input_len as u32 * 8);
            fill_rect_raw(fb_ptr, pitch, total, width, height, cursor_x, input_y + 16, 2, 14, CURSOR_COLOR);
        }
    }
}

// -- Free functions for drawing (no &mut self needed) --

fn fill_rect_raw(fb_ptr: *mut u8, pitch: u32, total: usize, max_w: u32, max_h: u32,
                 x: u32, y: u32, w: u32, h: u32, color: u32) {
    let r = ((color >> 16) & 0xFF) as u8;
    let g = ((color >> 8) & 0xFF) as u8;
    let b = (color & 0xFF) as u8;
    let fb = unsafe { core::slice::from_raw_parts_mut(fb_ptr, total) };

    for py in y..y.saturating_add(h).min(max_h) {
        for px in x..x.saturating_add(w).min(max_w) {
            let off = (py * pitch + px * 4) as usize;
            if off + 3 < total {
                fb[off] = b; fb[off+1] = g; fb[off+2] = r; fb[off+3] = 0xFF;
            }
        }
    }
}

fn fill_rounded_rect_raw(fb_ptr: *mut u8, pitch: u32, total: usize, max_w: u32, max_h: u32,
                          x: u32, y: u32, w: u32, h: u32, color: u32) {
    fill_rect_raw(fb_ptr, pitch, total, max_w, max_h, x + 2, y, w.saturating_sub(4), h, color);
    fill_rect_raw(fb_ptr, pitch, total, max_w, max_h, x, y + 2, w, h.saturating_sub(4), color);
    fill_rect_raw(fb_ptr, pitch, total, max_w, max_h, x + 1, y + 1, w.saturating_sub(2), h.saturating_sub(2), color);
}

fn draw_text_raw(fb_ptr: *mut u8, pitch: u32, total: usize, max_w: u32, max_h: u32,
                 s: &str, x: u32, y: u32, color: u32) {
    let r = ((color >> 16) & 0xFF) as u8;
    let g = ((color >> 8) & 0xFF) as u8;
    let b = (color & 0xFF) as u8;
    let fb = unsafe { core::slice::from_raw_parts_mut(fb_ptr, total) };

    let mut cx = x;
    for byte in s.bytes() {
        if byte < 32 || byte > 126 { continue; }
        let idx = (byte - 32) as usize;
        let glyph_off = idx * 8;
        if glyph_off + 8 > FONT_8X8.len() { continue; }

        for row in 0..8u32 {
            let bits = FONT_8X8[glyph_off + row as usize];
            for col in 0..8u32 {
                if bits & (1 << (7 - col)) != 0 {
                    let px = cx + col;
                    let py = y + row;
                    if px < max_w && py < max_h {
                        let off = (py * pitch + px * 4) as usize;
                        if off + 3 < total {
                            fb[off] = b; fb[off+1] = g; fb[off+2] = r; fb[off+3] = 0xFF;
                        }
                    }
                }
            }
        }
        cx += 8;
        if cx + 8 > max_w { break; }
    }
}

/// Public API for adding system messages
pub fn system_msg(text: &str) {
    if let Some(ref mut ui) = *CHAT.lock() {
        ui.system_msg(text);
    }
}
