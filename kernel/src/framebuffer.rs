//! Basic framebuffer rendering for TuniCore boot banner
//!
//! Uses the Limine framebuffer to draw a colored banner.
//! This is intentionally simple — a real graphics stack comes later.

use limine::framebuffer::Framebuffer;

/// Draw the TuniCore boot banner on the framebuffer
pub fn draw_banner(fb: &limine::framebuffer::Framebuffer) {
    let width = fb.width as usize;
    let height = fb.height as usize;
    let pitch = fb.pitch as usize;
    let bpp = fb.bpp as usize;
    let bytes_per_pixel = bpp / 8;

    // Safety: Limine guarantees valid framebuffer memory at this address
    let fb_ptr = fb.address() as *mut u8;
    let fb_slice = unsafe { core::slice::from_raw_parts_mut(fb_ptr, pitch * height) };

    // Draw a gradient background: deep navy → dark purple
    // Represents the "AI void" from which the agent emerges
    for y in 0..height {
        for x in 0..width {
            let offset = y * pitch + x * bytes_per_pixel;
            if offset + 3 > fb_slice.len() {
                continue;
            }

            // Gradient: navy (0x0a0a2e) → purple (0x1a0a3e)
            let r = (10 + (y * 16 / height)) as u8;
            let g = (10 + (x * 5 / width)) as u8;
            let b = (46 + (y * 20 / height)) as u8;

            // BGRA format (most common for Limine framebuffers)
            fb_slice[offset] = b;
            fb_slice[offset + 1] = g;
            fb_slice[offset + 2] = r;
            if bytes_per_pixel == 4 {
                fb_slice[offset + 3] = 0xFF;
            }
        }
    }

    // Draw a bright accent bar at the top (cyan/teal — capability color)
    let bar_height = 4.min(height);
    for y in 0..bar_height {
        for x in 0..width {
            let offset = y * pitch + x * bytes_per_pixel;
            if offset + 3 > fb_slice.len() {
                continue;
            }
            // Cyan accent: #00e5ff
            fb_slice[offset] = 0xFF; // B
            fb_slice[offset + 1] = 0xE5; // G
            fb_slice[offset + 2] = 0x00; // R
            if bytes_per_pixel == 4 {
                fb_slice[offset + 3] = 0xFF;
            }
        }
    }

    // Draw a centered horizontal line as a divider (around 1/3 from top)
    let divider_y = height / 3;
    let divider_width = width * 2 / 3;
    let divider_start_x = (width - divider_width) / 2;
    for x in divider_start_x..(divider_start_x + divider_width) {
        let offset = divider_y * pitch + x * bytes_per_pixel;
        if offset + 3 > fb_slice.len() {
            continue;
        }
        // Dim cyan line
        fb_slice[offset] = 0x80; // B
        fb_slice[offset + 1] = 0x73; // G
        fb_slice[offset + 2] = 0x00; // R
        if bytes_per_pixel == 4 {
            fb_slice[offset + 3] = 0xFF;
        }
    }

    // Draw a small "shield" icon in the center (represents capability guard)
    // Simple pixel art: 8x10 shield shape
    let shield = [
        0b01111110u8,
        0b11111111,
        0b11111111,
        0b11111111,
        0b11111111,
        0b01111110,
        0b01111110,
        0b00111100,
        0b00111100,
        0b00011000,
    ];

    let scale = 6; // Each pixel = 6x6 screen pixels
    let shield_w = 8 * scale;
    let shield_h = shield.len() * scale;
    let start_x = (width - shield_w) / 2;
    let start_y = height / 2 - shield_h / 2;

    for (row, &bits) in shield.iter().enumerate() {
        for col in 0..8 {
            if bits & (1 << (7 - col)) != 0 {
                // Draw scaled pixel
                for sy in 0..scale {
                    for sx in 0..scale {
                        let px = start_x + col * scale + sx;
                        let py = start_y + row * scale + sy;
                        if px < width && py < height {
                            let offset = py * pitch + px * bytes_per_pixel;
                            if offset + 3 <= fb_slice.len() {
                                // Bright cyan for shield
                                fb_slice[offset] = 0xFF;
                                fb_slice[offset + 1] = 0xE5;
                                fb_slice[offset + 2] = 0x00;
                                if bytes_per_pixel == 4 {
                                    fb_slice[offset + 3] = 0xFF;
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
