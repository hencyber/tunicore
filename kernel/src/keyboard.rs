//! PS/2 Keyboard Driver
//!
//! Handles keyboard interrupts (IRQ1 -> vector 33) and translates
//! scancodes to ASCII characters. Uses a ring buffer for key events.

use spin::Mutex;
use x86_64::instructions::port::Port;

/// Keyboard data port
const KB_DATA_PORT: u16 = 0x60;
/// Keyboard status port
const KB_STATUS_PORT: u16 = 0x64;

/// Key event buffer (ring buffer)
const KEY_BUF_SIZE: usize = 64;
static KEY_BUFFER: Mutex<KeyBuffer> = Mutex::new(KeyBuffer::new());

struct KeyBuffer {
    buf: [u8; KEY_BUF_SIZE],
    head: usize,
    tail: usize,
}

impl KeyBuffer {
    const fn new() -> Self {
        Self { buf: [0; KEY_BUF_SIZE], head: 0, tail: 0 }
    }

    fn push(&mut self, key: u8) {
        let next = (self.head + 1) % KEY_BUF_SIZE;
        if next != self.tail {
            self.buf[self.head] = key;
            self.head = next;
        }
    }

    fn pop(&mut self) -> Option<u8> {
        if self.head == self.tail {
            None
        } else {
            let key = self.buf[self.tail];
            self.tail = (self.tail + 1) % KEY_BUF_SIZE;
            Some(key)
        }
    }
}

/// US QWERTY scancode set 1 -> ASCII
/// Index = scancode, value = ASCII (0 = no mapping)
static SCANCODE_MAP: [u8; 128] = [
    0, 27, // 0x00: none, 0x01: ESC
    b'1', b'2', b'3', b'4', b'5', b'6', b'7', b'8', b'9', b'0', // 0x02-0x0B
    b'-', b'=', 8,    // 0x0C: -, 0x0D: =, 0x0E: backspace
    b'\t',             // 0x0F: tab
    b'q', b'w', b'e', b'r', b't', b'y', b'u', b'i', b'o', b'p', // 0x10-0x19
    b'[', b']', b'\n', // 0x1A: [, 0x1B: ], 0x1C: enter
    0,                  // 0x1D: left ctrl
    b'a', b's', b'd', b'f', b'g', b'h', b'j', b'k', b'l', // 0x1E-0x26
    b';', b'\'', b'`', // 0x27-0x29
    0,                  // 0x2A: left shift
    b'\\',              // 0x2B
    b'z', b'x', b'c', b'v', b'b', b'n', b'm', // 0x2C-0x32
    b',', b'.', b'/',  // 0x33-0x35
    0, b'*', 0,        // 0x36: right shift, 0x37: kp *, 0x38: left alt
    b' ',               // 0x39: space
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, // 0x3A-0x43: caps, F1-F9
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, // 0x44-0x4D: F10, num, scroll, kp7-9, kp-, kp4-6
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, // 0x4E-0x57
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, // 0x58-0x61
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, // 0x62-0x6B
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, // 0x6C-0x75
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, // 0x76-0x7F
];

/// Shift key scancode map
static SCANCODE_MAP_SHIFT: [u8; 128] = [
    0, 27,
    b'!', b'@', b'#', b'$', b'%', b'^', b'&', b'*', b'(', b')',
    b'_', b'+', 8,
    b'\t',
    b'Q', b'W', b'E', b'R', b'T', b'Y', b'U', b'I', b'O', b'P',
    b'{', b'}', b'\n',
    0,
    b'A', b'S', b'D', b'F', b'G', b'H', b'J', b'K', b'L',
    b':', b'"', b'~',
    0, b'|',
    b'Z', b'X', b'C', b'V', b'B', b'N', b'M',
    b'<', b'>', b'?',
    0, b'*', 0,
    b' ',
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
];

/// Track modifier keys
static SHIFT_HELD: core::sync::atomic::AtomicBool =
    core::sync::atomic::AtomicBool::new(false);

/// Keyboard interrupt handler (IRQ1 -> vector 33)
pub extern "x86-interrupt" fn keyboard_handler(
    _stack_frame: x86_64::structures::idt::InterruptStackFrame,
) {
    let scancode: u8 = unsafe { Port::<u8>::new(KB_DATA_PORT).read() };

    // Key release (bit 7 set)
    if scancode & 0x80 != 0 {
        let released = scancode & 0x7F;
        if released == 0x2A || released == 0x36 {
            SHIFT_HELD.store(false, core::sync::atomic::Ordering::Relaxed);
        }
        unsafe { Port::<u8>::new(0x20).write(0x20); } // PIC EOI
        return;
    }

    // Shift press
    if scancode == 0x2A || scancode == 0x36 {
        SHIFT_HELD.store(true, core::sync::atomic::Ordering::Relaxed);
        unsafe { Port::<u8>::new(0x20).write(0x20); } // PIC EOI
        return;
    }

    // Translate scancode to ASCII
    let shift = SHIFT_HELD.load(core::sync::atomic::Ordering::Relaxed);
    let ascii = if shift {
        SCANCODE_MAP_SHIFT.get(scancode as usize).copied().unwrap_or(0)
    } else {
        SCANCODE_MAP.get(scancode as usize).copied().unwrap_or(0)
    };

    if ascii != 0 {
        KEY_BUFFER.lock().push(ascii);
    }

    unsafe { Port::<u8>::new(0x20).write(0x20); } // PIC EOI
}

/// Read a key from the buffer (non-blocking)
pub fn read_key() -> Option<u8> {
    KEY_BUFFER.lock().pop()
}

/// Initialize PS/2 keyboard
pub fn init() {
    // Flush any pending data
    unsafe {
        while Port::<u8>::new(KB_STATUS_PORT).read() & 1 != 0 {
            Port::<u8>::new(KB_DATA_PORT).read();
        }
    }
}
