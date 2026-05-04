//! TuniCore — AI Agent Kernel
//!
//! A capability-based, Rust-native microkernel designed to run
//! AI agents securely. The agent is the interface. The kernel is the guard.

#![no_std]
#![no_main]
#![feature(abi_x86_interrupt)]

extern crate alloc;

mod capability;
mod framebuffer;
mod gdt;
mod idt;
mod interrupts;
mod memory;
mod serial;

use core::panic::PanicInfo;
use limine::request::{FramebufferRequest, HhdmRequest, MemmapRequest};

/// Base revision — tells Limine what protocol version we support
#[used]
#[unsafe(link_section = ".limine_requests")]
static BASE_REVISION: limine::BaseRevision = limine::BaseRevision::new();

/// Limine requests — placed in special linker sections
#[used]
#[unsafe(link_section = ".limine_requests_start")]
static _START: limine::RequestsStartMarker = limine::RequestsStartMarker::new();

#[used]
#[unsafe(link_section = ".limine_requests")]
static FRAMEBUFFER_REQUEST: FramebufferRequest = FramebufferRequest::new();

#[used]
#[unsafe(link_section = ".limine_requests")]
static MEMMAP_REQUEST: MemmapRequest = MemmapRequest::new();

#[used]
#[unsafe(link_section = ".limine_requests")]
static HHDM_REQUEST: HhdmRequest = HhdmRequest::new();

#[used]
#[unsafe(link_section = ".limine_requests_end")]
static _END: limine::RequestsEndMarker = limine::RequestsEndMarker::new();

/// Kernel entry point — called by Limine after boot
#[unsafe(no_mangle)]
extern "C" fn kmain() -> ! {
    // 1. Serial console first — our primary debug channel
    serial::init();
    serial_println!("========================================");
    serial_println!("  TuniCore v0.1.0 — AI Agent Kernel");
    serial_println!("  Capability-based. Rust-native.");
    serial_println!("========================================");
    serial_println!();

    // Check base revision (non-fatal for now)
    if BASE_REVISION.is_supported() {
        serial_println!("[boot] Limine base revision: supported");
    } else {
        serial_println!("[boot] WARN: Limine base revision not confirmed (continuing anyway)");
    }

    // 2. GDT + TSS
    serial_print!("[boot] Loading GDT... ");
    gdt::init();
    serial_println!("OK");

    // 3. IDT + interrupt handlers
    serial_print!("[boot] Loading IDT... ");
    idt::init();
    serial_println!("OK");

    // 4. Initialize PIC and enable hardware interrupts
    serial_print!("[boot] Initializing interrupts... ");
    interrupts::init();
    serial_println!("OK");

    // 5. Memory map + heap allocator
    serial_print!("[boot] Initializing heap... ");
    if let Some(response) = MEMMAP_REQUEST.response() {
        let entries = response.entries();
        let mut usable_bytes: u64 = 0;
        for entry in entries {
            if entry.type_ == limine::memmap::MEMMAP_USABLE {
                usable_bytes += entry.length;
            }
        }
        memory::init_heap();
        serial_println!("OK ({} MB usable RAM)", usable_bytes / (1024 * 1024));
    } else {
        serial_println!("WARN: no memory map from bootloader");
        memory::init_heap();
    }

    // 6. Capability system skeleton
    serial_print!("[boot] Capability system... ");
    capability::init();
    serial_println!("OK (type skeleton loaded)");

    // 7. Framebuffer banner
    if let Some(response) = FRAMEBUFFER_REQUEST.response() {
        let fbs = response.framebuffers();
        if !fbs.is_empty() {
            let fb = fbs[0];
            serial_println!(
                "[boot] Framebuffer: {}x{} @ {} bpp",
                fb.width,
                fb.height,
                fb.bpp
            );
            framebuffer::draw_banner(fb);
        }
    } else {
        serial_println!("[boot] No framebuffer available");
    }

    // 8. Test interrupt (breakpoint)
    serial_println!();
    serial_println!("[test] Triggering breakpoint exception...");
    x86_64::instructions::interrupts::int3();
    serial_println!("[test] Returned from breakpoint — interrupts work!");

    // 9. Boot complete
    serial_println!();
    serial_println!("========================================");
    serial_println!("  TuniCore boot complete.");
    serial_println!("  Entering idle loop (HLT).");
    serial_println!("  The kernel is the guard.");
    serial_println!("========================================");

    // Idle loop — halt CPU until next interrupt
    loop {
        x86_64::instructions::hlt();
    }
}

/// Panic handler — prints to serial and halts
#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    serial_println!();
    serial_println!("!!! KERNEL PANIC !!!");
    serial_println!("{}", info);
    loop {
        x86_64::instructions::hlt();
    }
}
