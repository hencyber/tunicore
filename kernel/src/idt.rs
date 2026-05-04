//! Interrupt Descriptor Table (IDT)
//!
//! Defines handlers for CPU exceptions and hardware interrupts.
//! Uses the x86_64 crate's type-safe IDT builder.

use crate::serial_println;
use spin::Lazy;
use x86_64::structures::idt::{InterruptDescriptorTable, InterruptStackFrame, PageFaultErrorCode};

use crate::gdt;
use crate::interrupts::InterruptIndex;

/// Static IDT — initialized once, lives for the kernel's lifetime
static IDT: Lazy<InterruptDescriptorTable> = Lazy::new(|| {
    let mut idt = InterruptDescriptorTable::new();

    // CPU Exceptions
    idt.breakpoint.set_handler_fn(breakpoint_handler);

    unsafe {
        idt.double_fault
            .set_handler_fn(double_fault_handler)
            .set_stack_index(gdt::DOUBLE_FAULT_IST_INDEX);
    }

    idt.page_fault.set_handler_fn(page_fault_handler);
    idt.general_protection_fault
        .set_handler_fn(general_protection_handler);

    // Hardware interrupts (PIC)
    idt[InterruptIndex::Timer.as_u8()].set_handler_fn(crate::interrupts::timer_handler);
    idt[InterruptIndex::Keyboard.as_u8()].set_handler_fn(crate::interrupts::keyboard_handler);

    idt
});

/// Load the IDT
pub fn init() {
    IDT.load();
}

// ─── Exception Handlers ─────────────────────────────────────────

/// Breakpoint exception (#BP) — used for debugging
extern "x86-interrupt" fn breakpoint_handler(stack_frame: InterruptStackFrame) {
    serial_println!("[exception] BREAKPOINT at {:#?}", stack_frame);
}

/// Double fault (#DF) — unrecoverable, halt the CPU
extern "x86-interrupt" fn double_fault_handler(
    stack_frame: InterruptStackFrame,
    error_code: u64,
) -> ! {
    serial_println!("!!! DOUBLE FAULT (error_code={}) !!!", error_code);
    serial_println!("{:#?}", stack_frame);
    loop {
        x86_64::instructions::hlt();
    }
}

/// Page fault (#PF) — log and halt for now
extern "x86-interrupt" fn page_fault_handler(
    stack_frame: InterruptStackFrame,
    error_code: PageFaultErrorCode,
) {
    use x86_64::registers::control::Cr2;
    serial_println!("!!! PAGE FAULT !!!");
    serial_println!("  Accessed address: {:?}", Cr2::read());
    serial_println!("  Error code: {:?}", error_code);
    serial_println!("{:#?}", stack_frame);
    loop {
        x86_64::instructions::hlt();
    }
}

/// General protection fault (#GP) — log and halt
extern "x86-interrupt" fn general_protection_handler(
    stack_frame: InterruptStackFrame,
    error_code: u64,
) {
    serial_println!("!!! GENERAL PROTECTION FAULT (error_code={}) !!!", error_code);
    serial_println!("{:#?}", stack_frame);
    loop {
        x86_64::instructions::hlt();
    }
}
