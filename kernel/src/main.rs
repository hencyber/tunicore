//! TuniCore — Confidential Agent Runtime
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
mod guardian;

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
    serial_println!("TuniCore v0.3.0 — Confidential Agent Runtime");
    serial_println!("The agent is the interface. The kernel is the guard.");
    serial_println!();

    // CPU feature detection (before anything else)
    serial_print!("[boot] Detecting hardware... ");
    let hw = hwdetect::detect();
    serial_println!("OK");
    hwdetect::log_capabilities(&hw);
    serial_println!();

    // GDT + TSS (x86_64 hardware requirement)
    serial_print!("[boot] GDT... ");
    gdt::init();
    serial_println!("OK");

    // IDT (exception + interrupt handlers)
    serial_print!("[boot] IDT... ");
    idt::init();
    serial_println!("OK");

    // APIC (modern interrupt controller)
    serial_print!("[boot] APIC... ");
    if hw.apic {
        let hhdm_offset = HHDM_REQUEST
            .response()
            .map(|r| r.offset)
            .unwrap_or(0);
        interrupts::init(hhdm_offset);
        serial_println!("OK");
    } else {
        serial_println!("WARN: no APIC detected, interrupts limited");
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
    } else {
        memory::init_heap();
        serial_println!("OK (no memory map)");
    }

    // Phase 2: Agent architecture
    serial_println!();
    serial_println!("[guard] Initializing agent architecture...");

    // Capability table
    serial_print!("[guard] Capability table... ");
    // Table is statically initialized, just log
    serial_println!("OK (4096 slots)");

    // Audit log — record boot event
    serial_print!("[guard] Audit log... ");
    audit::log_boot(interrupts::ticks());
    serial_println!("OK (hash chain initialized)");

    // Agent table
    serial_print!("[guard] Agent table... ");
    serial_println!("OK (256 max agents)");

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
        }
    }

    // Phase 3: Launch guardian agent
    guardian::run();

    // Final status
    serial_println!();
    serial_println!("═══════════════════════════════════════");
    serial_println!("  TuniCore v0.3.0 — fully operational.");
    serial_println!("  Caps: {} | Agents: {} | Audit: {} events",
        cap_table::CAP_TABLE.lock().active_count(),
        agent::AGENT_TABLE.lock().active_count(),
        audit::AUDIT_LOG.lock().total_events(),
    );
    serial_println!("  The kernel is the guard.");
    serial_println!("═══════════════════════════════════════");

    // Idle loop — APIC timer keeps ticking for timeout enforcement
    loop {
        x86_64::instructions::hlt();
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
