//! TuniCore - Confidential Agent Runtime
//!
//! A capability-based kernel where the agent is the interface
//! and the kernel is the guard. No POSIX. No shell. No sudo.
//! Just capabilities, agents, and audit trails.

#![no_std]
#![no_main]
#![feature(abi_x86_interrupt)]

extern crate alloc;

// ─── Core modules (hardware foundation) ─────────────────────────
mod serial;
mod gdt;
mod idt;
mod interrupts;
mod hwdetect;
mod memory;
mod framebuffer;

// ─── Agent architecture (the innovation) ─────────────────────────
mod capability;
mod resource;
mod cap_table;
mod agent;
mod audit;
mod syscall;
mod channel;
mod virtfs;
mod klog;
mod env;
mod alias;
mod llm;
mod intent;
mod guardian;
mod wasm_runtime;
mod keyboard;
mod chat_ui;

use core::panic::PanicInfo;
use limine::request::{FramebufferRequest, HhdmRequest, MemmapRequest};

// ─── Limine boot protocol requests ──────────────────────────────

#[used]
#[unsafe(link_section = ".limine_requests")]
static BASE_REVISION: limine::BaseRevision = limine::BaseRevision::new();

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

// ─── Kernel entry ───────────────────────────────────────────────

#[unsafe(no_mangle)]
extern "C" fn kmain() -> ! {
    // Phase 1: Hardware foundation
    serial::init();
    serial_println!("TuniCore v0.6.0 - Confidential Agent Runtime");
    serial_println!("The agent is the interface. The kernel is the guard.");
    serial_println!();
    klog::boot("TuniCore v0.6.0 starting");

    // CPU feature detection (before anything else)
    serial_print!("[boot] Detecting hardware... ");
    let hw = hwdetect::detect();
    serial_println!("OK");
    hwdetect::log_capabilities(&hw);
    serial_println!();
    klog::boot("Hardware detected: x86_64");

    // GDT + TSS (x86_64 hardware requirement)
    serial_print!("[boot] GDT... ");
    gdt::init();
    serial_println!("OK");
    klog::boot("GDT initialized");

    // IDT (exception + interrupt handlers)
    serial_print!("[boot] IDT... ");
    idt::init();
    serial_println!("OK");
    klog::boot("IDT initialized");

    // APIC (modern interrupt controller)
    serial_print!("[boot] APIC... ");
    if hw.apic {
        let hhdm_offset = HHDM_REQUEST
            .response()
            .map(|r| r.offset)
            .unwrap_or(0);
        interrupts::init(hhdm_offset);
        serial_println!("OK");
        klog::boot("x2APIC enabled (MSR mode)");
    } else {
        serial_println!("WARN: no APIC detected, interrupts limited");
        klog::warn("No APIC detected");
    }

    // Heap allocator
    serial_print!("[boot] Heap... ");
    if let Some(response) = MEMMAP_REQUEST.response() {
        let entries = response.entries();
        let mut usable: u64 = 0;
        for entry in entries {
            if entry.type_ == limine::memmap::MEMMAP_USABLE {
                usable += entry.length;
            }
        }
        memory::init_heap();
        serial_println!("OK ({} MB usable)", usable / (1024 * 1024));
        klog::boot("Heap: 32 MiB static allocator");

        // Page frame allocator - real physical memory management
        serial_print!("[boot] PMM... ");
        memory::init_page_alloc(entries);
        klog::boot("PMM: bitmap page frame allocator");
    } else {
        memory::init_heap();
        serial_println!("OK (no memory map)");
    }

    // Phase 2: Agent architecture
    serial_println!();
    serial_println!("[guard] Initializing agent architecture...");
    klog::boot("Agent architecture initializing");

    // Capability table
    serial_print!("[guard] Capability table... ");
    // Table is statically initialized, just log
    serial_println!("OK (4096 slots)");

    // Audit log - record boot event
    serial_print!("[guard] Audit log... ");
    audit::log_boot(interrupts::ticks());
    serial_println!("OK (hash chain initialized)");

    serial_print!("[guard] Agent table... ");
    serial_println!("OK (256 max agents)");

    // Environment store
    serial_print!("[guard] Environment... ");
    env::init_defaults();
    serial_println!("OK (6 vars)");
    klog::boot("Environment store initialized");

    // Framebuffer
    if let Some(response) = FRAMEBUFFER_REQUEST.response() {
        let fbs = response.framebuffers();
        if !fbs.is_empty() {
            let fb = fbs[0];
            serial_println!(
                "[boot] Display: {}x{} @ {}bpp",
                fb.width, fb.height, fb.bpp
            );
            framebuffer::draw_boot_header(fb);
            // Store for chat UI
            unsafe { FRAMEBUFFER_REF = Some(fb as *const _ as usize); }
        }
    }

    // Phase 3+6: Launch guardian agent (enters interactive intent loop, never returns)
    guardian::run();
}

/// Stored framebuffer pointer (set during boot)
static mut FRAMEBUFFER_REF: Option<usize> = None;

/// Get the framebuffer reference (for chat UI init)
pub fn get_framebuffer() -> Option<&'static limine::framebuffer::Framebuffer> {
    unsafe {
        FRAMEBUFFER_REF.map(|ptr| &*(ptr as *const limine::framebuffer::Framebuffer))
    }
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    serial_println!();
    serial_println!("!!! KERNEL PANIC !!!");
    serial_println!("{}", info);
    loop {
        x86_64::instructions::hlt();
    }
}
