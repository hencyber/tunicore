//! Chat UI - conversational interface on the framebuffer
//!
//! Makes TuniCore feel like a messaging app, not a terminal.
//! System messages appear on the left, user messages on the right.
//! Simple, clean, designed for non-technical users.

use alloc::string::{String, ToString};
use alloc::vec::Vec;
use spin::Mutex;

/// Chat message types
#[derive(Clone, Copy)]
pub enum Sender {
    System,
    User,
}

/// A single chat message
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
    fb_ptr: usize,
    width: u32,
    height: u32,
    pitch: u32,
    messages: Vec<Message>,
    input_buf: [u8; 200],
    input_len: usize,
}

unsafe impl Send for ChatUI {}

// Modern color palette
const BG: u32         = 0x0F0F1A;  // Very dark blue-black
const HEADER_BG: u32  = 0x161625;  // Header background
const HEADER_LINE: u32= 0x2A2A45;  // Subtle separator line
const SYS_BG: u32     = 0x1E1E35;  // System bubble - subtle dark
const USER_BG: u32    = 0x3B5BDB;  // User bubble - rich blue
const INPUT_BG: u32   = 0x161625;  // Input area background
const INPUT_FIELD: u32= 0x1C1C32;  // Input field background
const INPUT_BORDER: u32= 0x2A2A50; // Input field border
const TEXT_PRIMARY: u32= 0xE4E4F0; // Primary text - bright
const TEXT_SECONDARY: u32 = 0x9090B0; // Secondary/dim text
const TEXT_ACCENT: u32 = 0x7B93FF; // Accent text (TuniCore title)
const CURSOR_CLR: u32 = 0x7B93FF;  // Cursor color

/// Font scale: 2x (each pixel drawn as 2x2)
const FONT_SCALE: u32 = 2;
/// Character width at 2x
const CHAR_W: u32 = 8 * FONT_SCALE;   // 16px
/// Character height at 2x  
const CHAR_H: u32 = 8 * FONT_SCALE;   // 16px

/// Font data
static FONT_8X8: &[u8] = include_bytes!("font8x8.bin");

/// Global chat UI instance
pub static CHAT: Mutex<Option<ChatUI>> = Mutex::new(None);

impl ChatUI {
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

    pub fn system_msg(&mut self, text: &str) {
        self.messages.push(Message::new(Sender::System, text));
        self.render();
    }

    pub fn user_msg(&mut self, text: &str) {
        self.messages.push(Message::new(Sender::User, text));
        self.render();
    }

    pub fn key_input(&mut self, key: u8) -> Option<String> {
        match key {
            b'\n' | 13 => {
                if self.input_len == 0 { return None; }
                let cmd = core::str::from_utf8(&self.input_buf[..self.input_len])
                    .unwrap_or("").to_string();
                self.user_msg(&cmd);
                self.input_len = 0;
                self.render();
                Some(cmd)
            }
            8 | 0x7F => {
                if self.input_len > 0 { self.input_len -= 1; }
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

    fn render(&mut self) {
        let w = self.width;
        let h = self.height;
        let p = self.pitch;
        let fb = self.fb_ptr as *mut u8;
        let total = (p * h) as usize;

        // === Background ===
        fill_rect(fb, p, total, w, h, 0, 0, w, h, BG);

        // === Header (56px tall) ===
        let header_h = 56u32;
        fill_rect(fb, p, total, w, h, 0, 0, w, header_h, HEADER_BG);
        // Separator line
        fill_rect(fb, p, total, w, h, 0, header_h - 1, w, 1, HEADER_LINE);
        // Title "TuniCore" - left
        draw_text_2x(fb, p, total, w, h, "TuniCore", 24, 18, TEXT_ACCENT);
        // Subtitle - center
        let sub = "Just type what you need";
        let sub_x = (w / 2).saturating_sub((sub.len() as u32 * CHAR_W) / 2);
        draw_text_2x(fb, p, total, w, h, sub, sub_x, 18, TEXT_SECONDARY);

        // === Input bar (64px tall) at bottom ===
        let input_bar_h = 64u32;
        let input_y = h.saturating_sub(input_bar_h);
        fill_rect(fb, p, total, w, h, 0, input_y, w, input_bar_h, INPUT_BG);
        // Separator line at top of input
        fill_rect(fb, p, total, w, h, 0, input_y, w, 1, HEADER_LINE);
        // Input field with border
        let field_x = 24u32;
        let field_y = input_y + 14;
        let field_w = w.saturating_sub(48);
        let field_h = 36u32;
        // Border (1px)
        fill_rect(fb, p, total, w, h, field_x - 1, field_y - 1, field_w + 2, field_h + 2, INPUT_BORDER);
        // Field background
        fill_rect(fb, p, total, w, h, field_x, field_y, field_w, field_h, INPUT_FIELD);

        if self.input_len == 0 {
            draw_text_2x(fb, p, total, w, h, "Type a message...", field_x + 12, field_y + 10, TEXT_SECONDARY);
        } else {
            let txt = core::str::from_utf8(&self.input_buf[..self.input_len]).unwrap_or("");
            draw_text_2x(fb, p, total, w, h, txt, field_x + 12, field_y + 10, TEXT_PRIMARY);
            // Blinking cursor
            let cx = field_x + 12 + (self.input_len as u32 * CHAR_W);
            fill_rect(fb, p, total, w, h, cx, field_y + 8, 2, 20, CURSOR_CLR);
        }

        // === Messages area ===
        let msg_top = header_h + 16;
        let msg_bottom = input_y.saturating_sub(16);
        let bubble_h = 40u32;      // Height of each bubble
        let bubble_gap = 12u32;     // Gap between bubbles
        let bubble_pad_x = 16u32;   // Horizontal padding inside bubble
        let bubble_pad_y = 12u32;   // Vertical padding inside bubble
        let max_msgs = ((msg_bottom - msg_top) / (bubble_h + bubble_gap)) as usize;

        let start = if self.messages.len() > max_msgs {
            self.messages.len() - max_msgs
        } else {
            0
        };

        let mut y = msg_top;
        for i in start..self.messages.len() {
            let msg = &self.messages[i];
            let text = msg.as_str();
            let text_w = text.len() as u32 * CHAR_W;
            let bw = text_w + bubble_pad_x * 2;
            let bw = bw.min(w - 48); // Max width

            match msg.sender {
                Sender::System => {
                    // Left-aligned bubble
                    let bx = 24u32;
                    // Bubble with rounded feel (3-rect approach)
                    draw_bubble(fb, p, total, w, h, bx, y, bw, bubble_h, SYS_BG);
                    draw_text_2x(fb, p, total, w, h, text, bx + bubble_pad_x, y + bubble_pad_y, TEXT_PRIMARY);
                }
                Sender::User => {
                    // Right-aligned bubble
                    let bx = w.saturating_sub(bw + 24);
                    draw_bubble(fb, p, total, w, h, bx, y, bw, bubble_h, USER_BG);
                    draw_text_2x(fb, p, total, w, h, text, bx + bubble_pad_x, y + bubble_pad_y, TEXT_PRIMARY);
                }
            }
            y += bubble_h + bubble_gap;
        }
    }
}

// === Drawing primitives ===

fn fill_rect(fb: *mut u8, pitch: u32, total: usize, max_w: u32, max_h: u32,
             x: u32, y: u32, w: u32, h: u32, color: u32) {
    let r = ((color >> 16) & 0xFF) as u8;
    let g = ((color >> 8) & 0xFF) as u8;
    let b = (color & 0xFF) as u8;
    let sl = unsafe { core::slice::from_raw_parts_mut(fb, total) };

    for py in y..y.saturating_add(h).min(max_h) {
        for px in x..x.saturating_add(w).min(max_w) {
            let off = (py * pitch + px * 4) as usize;
            if off + 3 < total {
                sl[off] = b; sl[off+1] = g; sl[off+2] = r; sl[off+3] = 0xFF;
            }
        }
    }
}

/// Draw a bubble shape (simulated rounded corners)
fn draw_bubble(fb: *mut u8, pitch: u32, total: usize, max_w: u32, max_h: u32,
               x: u32, y: u32, w: u32, h: u32, color: u32) {
    let r = 4u32; // corner radius
    // Main body
    fill_rect(fb, pitch, total, max_w, max_h, x + r, y, w.saturating_sub(r * 2), h, color);
    // Left/right strips
    fill_rect(fb, pitch, total, max_w, max_h, x, y + r, r, h.saturating_sub(r * 2), color);
    fill_rect(fb, pitch, total, max_w, max_h, x + w.saturating_sub(r), y + r, r, h.saturating_sub(r * 2), color);
    // Corner fills (small rects to approximate)
    fill_rect(fb, pitch, total, max_w, max_h, x + 1, y + 1, r, r, color);
    fill_rect(fb, pitch, total, max_w, max_h, x + w.saturating_sub(r + 1), y + 1, r, r, color);
    fill_rect(fb, pitch, total, max_w, max_h, x + 1, y + h.saturating_sub(r + 1), r, r, color);
    fill_rect(fb, pitch, total, max_w, max_h, x + w.saturating_sub(r + 1), y + h.saturating_sub(r + 1), r, r, color);
}

/// Draw text at 2x scale (each font pixel becomes 2x2 screen pixels)
fn draw_text_2x(fb: *mut u8, pitch: u32, total: usize, max_w: u32, max_h: u32,
                s: &str, x: u32, y: u32, color: u32) {
    let r = ((color >> 16) & 0xFF) as u8;
    let g = ((color >> 8) & 0xFF) as u8;
    let b = (color & 0xFF) as u8;
    let sl = unsafe { core::slice::from_raw_parts_mut(fb, total) };

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
                    // Draw 2x2 block for each pixel
                    for sy in 0..FONT_SCALE {
                        for sx in 0..FONT_SCALE {
                            let px = cx + col * FONT_SCALE + sx;
                            let py = y + row * FONT_SCALE + sy;
                            if px < max_w && py < max_h {
                                let off = (py * pitch + px * 4) as usize;
                                if off + 3 < total {
                                    sl[off] = b; sl[off+1] = g; sl[off+2] = r; sl[off+3] = 0xFF;
                                }
                            }
                        }
                    }
                }
            }
        }
        cx += CHAR_W;
        if cx + CHAR_W > max_w { break; }
    }
}

/// Public API for adding system messages
pub fn system_msg(text: &str) {
    if let Some(ref mut ui) = *CHAT.lock() {
        ui.system_msg(text);
    }
}
