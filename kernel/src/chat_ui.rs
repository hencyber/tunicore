//! Chat UI v3 - Professional conversational interface
//!
//! Design principles:
//! - Centered content column (max 640px) like iMessage/WhatsApp
//! - 8x16 font at 2x = 16x32 effective pixels - large, crisp, readable
//! - High contrast bubbles against dark background
//! - Generous whitespace and padding
//! - Clear visual hierarchy: header > messages > input

use alloc::string::{String, ToString};
use alloc::vec::Vec;
use spin::Mutex;

#[derive(Clone, Copy)]
pub enum Sender { System, User }

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

// === Color Palette (high contrast, modern) ===
const BG_MAIN: u32    = 0x0C0C18;  // Deep space black
const BG_SIDEBAR: u32 = 0x101020;  // Slightly lighter sides
const HEADER_BG: u32  = 0x14142A;  // Header
const HEADER_ACCENT: u32 = 0x5B7FFF; // Title color
const DIVIDER: u32    = 0x252545;  // Lines
const SYS_BUBBLE: u32 = 0x1C1C38;  // System - visible against BG
const SYS_BORDER: u32 = 0x2E2E55;  // System bubble border
const USR_BUBBLE: u32 = 0x2952CC;  // User - strong blue
const USR_BORDER: u32 = 0x3D66E6;  // User bubble highlight edge
const INPUT_BG: u32   = 0x14142A;  // Input bar
const FIELD_BG: u32   = 0x0C0C18;  // Input field
const FIELD_BORDER: u32 = 0x3A3A60; // Input border
const TXT_BRIGHT: u32 = 0xEAEAF6;  // Primary text
const TXT_DIM: u32    = 0x7878A0;  // Placeholder/secondary
const CURSOR: u32     = 0x5B7FFF;  // Cursor

// Font: 8x16 at 2x scale = 16x32 per character
static FONT: &[u8] = include_bytes!("font8x16.bin");
const SCALE: u32 = 2;
const CW: u32 = 8 * SCALE;  // 16px char width
const CH: u32 = 16 * SCALE; // 32px char height

pub static CHAT: Mutex<Option<ChatUI>> = Mutex::new(None);

impl ChatUI {
    pub fn init(fb: &limine::framebuffer::Framebuffer) {
        *CHAT.lock() = Some(ChatUI {
            fb_ptr: fb.address() as usize,
            width: fb.width as u32,
            height: fb.height as u32,
            pitch: fb.pitch as u32,
            messages: Vec::new(),
            input_buf: [0u8; 200],
            input_len: 0,
        });
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
                Some(cmd)
            }
            8 | 0x7F => {
                if self.input_len > 0 { self.input_len -= 1; }
                self.render(); None
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
        let t = (p * h) as usize;

        // Content column: centered, max 700px
        let col_w = 700u32.min(w - 40);
        let col_x = (w - col_w) / 2;

        // --- Clear entire screen ---
        rect(fb, p, t, w, h, 0, 0, w, h, BG_MAIN);

        // --- Header: 70px ---
        let hdr_h = 70u32;
        rect(fb, p, t, w, h, 0, 0, w, hdr_h, HEADER_BG);
        rect(fb, p, t, w, h, 0, hdr_h - 1, w, 1, DIVIDER);
        // Title
        text16(fb, p, t, w, h, "TuniCore", col_x, 20, HEADER_ACCENT);
        // Version badge
        let ver = "v0.6.0";
        let vx = col_x + 9 * CW;
        text16(fb, p, t, w, h, ver, vx, 20, TXT_DIM);

        // --- Input bar: 80px at bottom ---
        let inp_h = 80u32;
        let inp_y = h.saturating_sub(inp_h);
        rect(fb, p, t, w, h, 0, inp_y, w, inp_h, INPUT_BG);
        rect(fb, p, t, w, h, 0, inp_y, w, 1, DIVIDER);

        // Input field
        let fy = inp_y + 20;
        let fh = 40u32;
        // Border
        rect(fb, p, t, w, h, col_x - 1, fy - 1, col_w + 2, fh + 2, FIELD_BORDER);
        // Background
        rect(fb, p, t, w, h, col_x, fy, col_w, fh, FIELD_BG);

        if self.input_len == 0 {
            text16(fb, p, t, w, h, "Type a message...", col_x + 16, fy + 5, TXT_DIM);
        } else {
            let s = core::str::from_utf8(&self.input_buf[..self.input_len]).unwrap_or("");
            text16(fb, p, t, w, h, s, col_x + 16, fy + 5, TXT_BRIGHT);
            let cx = col_x + 16 + self.input_len as u32 * CW;
            rect(fb, p, t, w, h, cx, fy + 4, 2, CH, CURSOR);
        }

        // --- Messages ---
        let msg_top = hdr_h + 20;
        let msg_bot = inp_y.saturating_sub(12);
        let bub_h = CH + 20; // bubble height = text + padding
        let gap = 16u32;
        let max_msgs = ((msg_bot - msg_top) / (bub_h + gap)) as usize;

        let start = self.messages.len().saturating_sub(max_msgs);
        let mut y = msg_top;

        for i in start..self.messages.len() {
            if y + bub_h > msg_bot { break; }
            let msg = &self.messages[i];
            let s = msg.as_str();
            let tw = s.len() as u32 * CW;
            let bw = (tw + 32).min(col_w);

            match msg.sender {
                Sender::System => {
                    let bx = col_x;
                    // Border (1px)
                    bubble(fb, p, t, w, h, bx, y, bw + 2, bub_h + 2, SYS_BORDER);
                    // Fill
                    bubble(fb, p, t, w, h, bx + 1, y + 1, bw, bub_h, SYS_BUBBLE);
                    text16(fb, p, t, w, h, s, bx + 16, y + 10, TXT_BRIGHT);
                }
                Sender::User => {
                    let bx = col_x + col_w - bw;
                    // Border
                    bubble(fb, p, t, w, h, bx - 2, y, bw + 2, bub_h + 2, USR_BORDER);
                    // Fill
                    bubble(fb, p, t, w, h, bx - 1, y + 1, bw, bub_h, USR_BUBBLE);
                    text16(fb, p, t, w, h, s, bx + 14, y + 10, TXT_BRIGHT);
                }
            }
            y += bub_h + gap;
        }
    }
}

// === Drawing primitives ===

fn rect(fb: *mut u8, p: u32, t: usize, mw: u32, mh: u32,
        x: u32, y: u32, w: u32, h: u32, c: u32) {
    let (cr, cg, cb) = ((c >> 16) as u8, ((c >> 8) & 0xFF) as u8, (c & 0xFF) as u8);
    let sl = unsafe { core::slice::from_raw_parts_mut(fb, t) };
    for py in y..y.saturating_add(h).min(mh) {
        for px in x..x.saturating_add(w).min(mw) {
            let o = (py * p + px * 4) as usize;
            if o + 3 < t { sl[o] = cb; sl[o+1] = cg; sl[o+2] = cr; sl[o+3] = 0xFF; }
        }
    }
}

fn bubble(fb: *mut u8, p: u32, t: usize, mw: u32, mh: u32,
          x: u32, y: u32, w: u32, h: u32, c: u32) {
    let r = 6u32;
    rect(fb, p, t, mw, mh, x + r, y, w.saturating_sub(r*2), h, c);
    rect(fb, p, t, mw, mh, x, y + r, w, h.saturating_sub(r*2), c);
    // Corners (graduated for smoother look)
    rect(fb, p, t, mw, mh, x+2, y+1, r-1, r-1, c);
    rect(fb, p, t, mw, mh, x+1, y+2, r-1, r-1, c);
    rect(fb, p, t, mw, mh, x+w-r-1, y+1, r-1, r-1, c);
    rect(fb, p, t, mw, mh, x+w-r, y+2, r-1, r-1, c);
    rect(fb, p, t, mw, mh, x+2, y+h-r, r-1, r-1, c);
    rect(fb, p, t, mw, mh, x+1, y+h-r-1, r-1, r-1, c);
    rect(fb, p, t, mw, mh, x+w-r-1, y+h-r, r-1, r-1, c);
    rect(fb, p, t, mw, mh, x+w-r, y+h-r-1, r-1, r-1, c);
}

fn text16(fb: *mut u8, p: u32, t: usize, mw: u32, mh: u32,
          s: &str, x: u32, y: u32, c: u32) {
    let (cr, cg, cb) = ((c >> 16) as u8, ((c >> 8) & 0xFF) as u8, (c & 0xFF) as u8);
    let sl = unsafe { core::slice::from_raw_parts_mut(fb, t) };
    let mut cx = x;
    for byte in s.bytes() {
        if byte < 32 || byte > 126 { continue; }
        let idx = (byte - 32) as usize;
        let off = idx * 16;
        if off + 16 > FONT.len() { continue; }
        for row in 0..16u32 {
            let bits = FONT[off + row as usize];
            for col in 0..8u32 {
                if bits & (1 << (7 - col)) != 0 {
                    for sy in 0..SCALE {
                        for sx in 0..SCALE {
                            let px = cx + col * SCALE + sx;
                            let py = y + row * SCALE + sy;
                            if px < mw && py < mh {
                                let o = (py * p + px * 4) as usize;
                                if o+3 < t { sl[o]=cb; sl[o+1]=cg; sl[o+2]=cr; sl[o+3]=0xFF; }
                            }
                        }
                    }
                }
            }
        }
        cx += CW;
        if cx + CW > mw { break; }
    }
}

pub fn system_msg(text: &str) {
    if let Some(ref mut ui) = *CHAT.lock() { ui.system_msg(text); }
}
