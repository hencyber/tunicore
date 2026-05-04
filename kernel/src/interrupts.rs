//! Hardware interrupt handling — PIC 8259 initialization and IRQ handlers
//!
//! Sets up the legacy PIC (Programmable Interrupt Controller) for timer
//! and keyboard interrupts. Will be replaced by APIC in a future phase.

use crate::{serial_print, serial_println};
use x86_64::instructions::port::Port;
use x86_64::structures::idt::InterruptStackFrame;

/// PIC port addresses
const PIC1_COMMAND: u16 = 0x20;
const PIC1_DATA: u16 = 0x21;
const PIC2_COMMAND: u16 = 0xA0;
const PIC2_DATA: u16 = 0xA1;

/// PIC interrupt vector offset (must not conflict with CPU exceptions 0-31)
pub const PIC_OFFSET: u8 = 32;

/// End of Interrupt command
const EOI: u8 = 0x20;

/// Hardware interrupt indices (offset from PIC_OFFSET)
#[derive(Debug, Clone, Copy)]
#[repr(u8)]
pub enum InterruptIndex {
    Timer = PIC_OFFSET,
    Keyboard = PIC_OFFSET + 1,
}

impl InterruptIndex {
    pub fn as_u8(self) -> u8 {
        self as u8
    }

    pub fn as_usize(self) -> usize {
        self as usize
    }
}

/// Initialize the 8259 PIC with standard ICW sequence
pub fn init() {
    unsafe {
        let mut pic1_cmd = Port::<u8>::new(PIC1_COMMAND);
        let mut pic1_data = Port::<u8>::new(PIC1_DATA);
        let mut pic2_cmd = Port::<u8>::new(PIC2_COMMAND);
        let mut pic2_data = Port::<u8>::new(PIC2_DATA);

        // Save masks
        let mask1 = pic1_data.read();
        let mask2 = pic2_data.read();

        // ICW1: start initialization sequence (cascade mode, ICW4 needed)
        pic1_cmd.write(0x11);
        io_wait();
        pic2_cmd.write(0x11);
        io_wait();

        // ICW2: vector offset
        pic1_data.write(PIC_OFFSET);
        io_wait();
        pic2_data.write(PIC_OFFSET + 8);
        io_wait();

        // ICW3: tell Master PIC there is a slave at IRQ2
        pic1_data.write(4); // bit 2 = IRQ2 has slave
        io_wait();
        pic2_data.write(2); // slave cascade identity
        io_wait();

        // ICW4: 8086 mode
        pic1_data.write(0x01);
        io_wait();
        pic2_data.write(0x01);
        io_wait();

        // Restore masks (but unmask timer and keyboard)
        let _ = mask1;
        let _ = mask2;
        pic1_data.write(0b11111100); // unmask IRQ0 (timer) and IRQ1 (keyboard)
        pic2_data.write(0xFF); // mask all slave IRQs for now
    }

    // Enable interrupts
    x86_64::instructions::interrupts::enable();
}

/// Send End of Interrupt to PIC
fn send_eoi(irq: u8) {
    unsafe {
        if irq >= PIC_OFFSET + 8 {
            // Slave PIC
            Port::<u8>::new(PIC2_COMMAND).write(EOI);
        }
        // Always send to master
        Port::<u8>::new(PIC1_COMMAND).write(EOI);
    }
}

/// I/O wait — short delay for PIC initialization
fn io_wait() {
    unsafe {
        Port::<u8>::new(0x80).write(0);
    }
}

// ─── IRQ Handlers ───────────────────────────────────────────────

/// Timer tick counter
static TIMER_TICKS: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);

/// Timer interrupt handler (IRQ0) — fires ~18.2 times per second
pub extern "x86-interrupt" fn timer_handler(_stack_frame: InterruptStackFrame) {
    let ticks = TIMER_TICKS.fetch_add(1, core::sync::atomic::Ordering::Relaxed);

    // Print a dot every ~1 second (every 18 ticks)
    if ticks % 18 == 0 && ticks > 0 && ticks <= 180 {
        crate::serial_print!(".");
    }

    send_eoi(InterruptIndex::Timer.as_u8());
}

/// Keyboard interrupt handler (IRQ1) — reads scancode
pub extern "x86-interrupt" fn keyboard_handler(_stack_frame: InterruptStackFrame) {
    let scancode: u8 = unsafe { Port::<u8>::new(0x60).read() };
    serial_println!("[keyboard] scancode: {:#04x}", scancode);
    send_eoi(InterruptIndex::Keyboard.as_u8());
}
