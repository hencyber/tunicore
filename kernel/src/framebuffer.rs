//! Minimal framebuffer text console
//!
//! Provides basic text rendering for the boot conversation interface.
//! No decorations, no gradients — just clean text output.
//! This will evolve into the agent conversation display.

/// Basic 8x8 bitmap font for ASCII printable characters (32-126)
/// Each character is 8 bytes, one byte per row, MSB = leftmost pixel
static FONT_8X8: &[u8] = include_bytes!("font8x8.bin");

/// Console colors
const BG_COLOR: u32 = 0x0A0A1A; // Deep dark blue-black
const FG_COLOR: u32 = 0x00E5FF; // Cyan (capability accent)
const PROMPT_COLOR: u32 = 0x7B68EE; // Medium slate blue

/// Framebuffer console state
pub struct Console {
    /// Raw framebuffer pointer
    fb_ptr: *mut u8,
    /// Framebuffer width in pixels
    width: u32,
    /// Framebuffer height in pixels
    height: u32,
    /// Bytes per scanline
    pitch: u32,
    /// Current cursor column (in characters)
    col: u32,
    /// Current cursor row (in characters)
    row: u32,
    /// Characters per row
    cols: u32,
    /// Characters per column
    rows: u32,
}

impl Console {
    /// Create a new console from a Limine framebuffer
    pub fn new(fb: &limine::framebuffer::Framebuffer) -> Self {
        let fb_ptr = fb.address() as *mut u8;
        let width = fb.width as u32;
        let height = fb.height as u32;
        let pitch = fb.pitch as u32;

        let cols = width / 8;
        let rows = height / 10; // 8px char + 2px line spacing

        let mut console = Console {
            fb_ptr,
            width,
            height,
            pitch,
            col: 0,
            row: 0,
            cols,
            rows,
        };

        // Clear screen
        console.clear();
        console
    }

    /// Clear the entire screen
    pub fn clear(&mut self) {
        let total_bytes = (self.pitch * self.height) as usize;
        let fb = unsafe { core::slice::from_raw_parts_mut(self.fb_ptr, total_bytes) };

        // Fill with background color (BGRA)
        let bg_b = (BG_COLOR & 0xFF) as u8;
        let bg_g = ((BG_COLOR >> 8) & 0xFF) as u8;
        let bg_r = ((BG_COLOR >> 16) & 0xFF) as u8;

        for y in 0..self.height as usize {
            for x in 0..self.width as usize {
                let offset = y * self.pitch as usize + x * 4;
                if offset + 3 < total_bytes {
                    fb[offset] = bg_b;
                    fb[offset + 1] = bg_g;
                    fb[offset + 2] = bg_r;
                    fb[offset + 3] = 0xFF;
                }
            }
        }
    }

    /// Draw a single character at pixel position
    fn draw_char(&mut self, ch: u8, px: u32, py: u32, color: u32) {
        if ch < 32 || ch > 126 {
            return;
        }
        let idx = (ch - 32) as usize;
        let glyph_offset = idx * 8;
        if glyph_offset + 8 > FONT_8X8.len() {
            return;
        }

        let total_bytes = (self.pitch * self.height) as usize;
        let fb = unsafe { core::slice::from_raw_parts_mut(self.fb_ptr, total_bytes) };

        let r = ((color >> 16) & 0xFF) as u8;
        let g = ((color >> 8) & 0xFF) as u8;
        let b = (color & 0xFF) as u8;

        for row in 0..8u32 {
            let bits = FONT_8X8[glyph_offset + row as usize];
            for col in 0..8u32 {
                if bits & (1 << (7 - col)) != 0 {
                    let x = px + col;
                    let y = py + row;
                    if x < self.width && y < self.height {
                        let offset = (y * self.pitch + x * 4) as usize;
                        if offset + 3 < total_bytes {
                            fb[offset] = b;
                            fb[offset + 1] = g;
                            fb[offset + 2] = r;
                            fb[offset + 3] = 0xFF;
                        }
                    }
                }
            }
        }
    }

    /// Write a string to the console
    pub fn write_str(&mut self, s: &str, color: u32) {
        for byte in s.bytes() {
            match byte {
                b'\n' => {
                    self.col = 0;
                    self.row += 1;
                    if self.row >= self.rows {
                        self.scroll();
                    }
                }
                b'\r' => {
                    self.col = 0;
                }
                byte => {
                    let px = self.col * 8;
                    let py = self.row * 10;
                    self.draw_char(byte, px, py, color);
                    self.col += 1;
                    if self.col >= self.cols {
                        self.col = 0;
                        self.row += 1;
                        if self.row >= self.rows {
                            self.scroll();
                        }
                    }
                }
            }
        }
    }

    /// Scroll the console up by one line
    fn scroll(&mut self) {
        let line_height = 10u32;
        let total_bytes = (self.pitch * self.height) as usize;
        let fb = unsafe { core::slice::from_raw_parts_mut(self.fb_ptr, total_bytes) };
        let line_bytes = (self.pitch * line_height) as usize;

        // Move everything up
        let content_bytes = total_bytes.saturating_sub(line_bytes);
        fb.copy_within(line_bytes..total_bytes, 0);

        // Clear last line with background
        let bg_b = (BG_COLOR & 0xFF) as u8;
        let bg_g = ((BG_COLOR >> 8) & 0xFF) as u8;
        let bg_r = ((BG_COLOR >> 16) & 0xFF) as u8;
        for i in (content_bytes..total_bytes).step_by(4) {
            if i + 3 < total_bytes {
                fb[i] = bg_b;
                fb[i + 1] = bg_g;
                fb[i + 2] = bg_r;
                fb[i + 3] = 0xFF;
            }
        }

        self.row = self.rows - 1;
    }
}

/// Draw the TuniCore boot header
pub fn draw_boot_header(fb: &limine::framebuffer::Framebuffer) {
    let mut console = Console::new(fb);

    // Minimal, clean boot header
    console.write_str("TuniCore v0.1.0", FG_COLOR);
    console.write_str("  Confidential Agent Runtime\n", PROMPT_COLOR);
    console.write_str("  The agent is the interface. The kernel is the guard.\n", 0x666666);
    console.write_str("\n", FG_COLOR);
}
