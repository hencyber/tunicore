//! UART 16550 serial driver for COM1
//!
//! Provides the primary debug output channel for TuniCore.
//! All kernel messages go through serial before framebuffer is available.

use core::fmt;
use spin::Mutex;
use x86_64::instructions::port::Port;

/// COM1 base I/O port address
const COM1: u16 = 0x3F8;

/// Global serial port instance, protected by a spinlock
pub static SERIAL: Mutex<SerialPort> = Mutex::new(SerialPort::new(COM1));

/// A UART 16550 serial port
pub struct SerialPort {
    data: Port<u8>,
    int_enable: Port<u8>,
    fifo_ctrl: Port<u8>,
    line_ctrl: Port<u8>,
    modem_ctrl: Port<u8>,
    line_status: Port<u8>,
}

impl SerialPort {
    /// Create a new serial port at the given base address
    const fn new(base: u16) -> Self {
        Self {
            data: Port::new(base),
            int_enable: Port::new(base + 1),
            fifo_ctrl: Port::new(base + 2),
            line_ctrl: Port::new(base + 3),
            modem_ctrl: Port::new(base + 4),
            line_status: Port::new(base + 5),
        }
    }

    /// Initialize the serial port with standard settings:
    /// 115200 baud, 8N1, FIFO enabled
    pub fn init(&mut self) {
        unsafe {
            // Disable interrupts
            self.int_enable.write(0x00);

            // Enable DLAB (set baud rate divisor)
            self.line_ctrl.write(0x80);

            // Set divisor to 1 (115200 baud)
            self.data.write(0x01); // divisor low byte
            self.int_enable.write(0x00); // divisor high byte

            // 8 bits, no parity, one stop bit (8N1), disable DLAB
            self.line_ctrl.write(0x03);

            // Enable FIFO, clear them, 14-byte threshold
            self.fifo_ctrl.write(0xC7);

            // IRQs enabled, RTS/DSR set
            self.modem_ctrl.write(0x0B);

            // Set in loopback mode to test the serial chip
            self.modem_ctrl.write(0x1E);

            // Test: send byte 0xAE and check if serial returns same byte
            self.data.write(0xAE);
            if self.data.read() != 0xAE {
                return; // Serial port is faulty
            }

            // Set normal operation mode (not loopback, IRQs on, OUT#1 and OUT#2 on)
            self.modem_ctrl.write(0x0F);
        }
    }

    /// Check if the transmit buffer is empty
    fn is_transmit_empty(&mut self) -> bool {
        unsafe { self.line_status.read() & 0x20 != 0 }
    }

    /// Write a single byte to the serial port
    pub fn write_byte(&mut self, byte: u8) {
        // Wait for transmit buffer to be empty
        while !self.is_transmit_empty() {
            core::hint::spin_loop();
        }
        unsafe {
            self.data.write(byte);
        }
    }
}

impl fmt::Write for SerialPort {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for byte in s.bytes() {
            // Convert \n to \r\n for proper serial terminal display
            if byte == b'\n' {
                self.write_byte(b'\r');
            }
            self.write_byte(byte);
        }
        Ok(())
    }
}

/// Initialize the serial port
pub fn init() {
    SERIAL.lock().init();
}

/// Print to the serial console
#[macro_export]
macro_rules! serial_print {
    ($($arg:tt)*) => {{
        use core::fmt::Write;
        $crate::serial::SERIAL.lock().write_fmt(format_args!($($arg)*)).unwrap();
    }};
}

/// Print to the serial console with a newline
#[macro_export]
macro_rules! serial_println {
    () => ($crate::serial_print!("\n"));
    ($($arg:tt)*) => ($crate::serial_print!("{}\n", format_args!($($arg)*)));
}
