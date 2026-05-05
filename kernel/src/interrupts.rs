//! Interrupt handling - Local APIC (MSR-based)
//!
//! Uses MSR-based APIC register access to avoid MMIO page faults.
//! The legacy PIC is fully disabled.

use crate::serial_println;
use x86_64::instructions::port::Port;

/// APIC MSR base (xAPIC MSR mode: base + (register_offset >> 4))
const APIC_MSR_BASE: u32 = 0x800;

/// Convenience APIC register IDs (offset >> 4)
const APIC_REG_SPURIOUS: u32 = 0x0F;
const APIC_REG_TPR: u32 = 0x08;
const APIC_REG_EOI: u32 = 0x0B;
const APIC_REG_TIMER_LVT: u32 = 0x32;
const APIC_REG_TIMER_INIT: u32 = 0x38;
const APIC_REG_TIMER_DIVIDE: u32 = 0x3E;
const APIC_REG_ID: u32 = 0x02;
const APIC_REG_VERSION: u32 = 0x03;

/// Interrupt vectors
const TIMER_VECTOR: u8 = 32;
const SPURIOUS_VECTOR: u8 = 0xFF;

/// Monotonic tick counter (for agent timeouts & scheduling)
pub static TICK_COUNT: core::sync::atomic::AtomicU64 =
    core::sync::atomic::AtomicU64::new(0);

/// Read an APIC register via MSR
fn apic_read_msr(reg: u32) -> u64 {
    let msr = APIC_MSR_BASE + reg;
    let (lo, hi): (u32, u32);
    unsafe {
        core::arch::asm!(
            "rdmsr",
            in("ecx") msr,
            out("eax") lo,
            out("edx") hi,
        );
    }
    ((hi as u64) << 32) | lo as u64
}

/// Write an APIC register via MSR
fn apic_write_msr(reg: u32, value: u64) {
    let msr = APIC_MSR_BASE + reg;
    let lo = value as u32;
    let hi = (value >> 32) as u32;
    unsafe {
        core::arch::asm!(
            "wrmsr",
            in("ecx") msr,
            in("eax") lo,
            in("edx") hi,
        );
    }
}

/// Read APIC register via MMIO (fallback for xAPIC mode)
unsafe fn apic_read_mmio(base: u64, offset: u32) -> u32 {
    let addr = base + offset as u64;
    unsafe { core::ptr::read_volatile(addr as *const u32) }
}

/// Write APIC register via MMIO (fallback for xAPIC mode)
unsafe fn apic_write_mmio(base: u64, offset: u32, value: u32) {
    let addr = base + offset as u64;
    unsafe { core::ptr::write_volatile(addr as *mut u32, value) }
}

/// Initialize PIC with keyboard IRQ1 enabled
/// Remaps PIC to vectors 32-47 so IRQ1 -> vector 33
fn init_pic_for_keyboard() {
    unsafe {
        // ICW1: begin init
        Port::<u8>::new(0x20).write(0x11);
        Port::<u8>::new(0xA0).write(0x11);
        // ICW2: remap to 32/40
        Port::<u8>::new(0x21).write(32);
        Port::<u8>::new(0xA1).write(40);
        // ICW3: cascade
        Port::<u8>::new(0x21).write(4);
        Port::<u8>::new(0xA1).write(2);
        // ICW4: 8086 mode
        Port::<u8>::new(0x21).write(0x01);
        Port::<u8>::new(0xA1).write(0x01);
        // Mask all except IRQ1 (keyboard) on master
        // Bit 0 = IRQ0 (timer - handled by APIC), Bit 1 = IRQ1 (keyboard)
        Port::<u8>::new(0x21).write(0xFD); // 11111101 - only IRQ1 enabled
        Port::<u8>::new(0xA1).write(0xFF); // all slave masked
    }
}

/// Enable x2APIC mode via IA32_APIC_BASE MSR
fn enable_x2apic() -> bool {
    let lo: u32;
    let hi: u32;
    unsafe {
        core::arch::asm!("rdmsr", in("ecx") 0x1Bu32, out("eax") lo, out("edx") hi);
    }

    // Set bit 10 (xAPIC enable) and bit 11 (x2APIC enable)
    let new_lo = lo | (1 << 10) | (1 << 11);
    unsafe {
        core::arch::asm!("wrmsr", in("ecx") 0x1Bu32, in("eax") new_lo, in("edx") hi);
    }

    // Verify
    let verify_lo: u32;
    unsafe {
        core::arch::asm!("rdmsr", in("ecx") 0x1Bu32, out("eax") verify_lo, out("edx") _);
    }
    verify_lo & (1 << 11) != 0
}

/// Mode of APIC operation
#[derive(Debug, Clone, Copy)]
enum ApicMode {
    X2Apic,
    XApic { base: u64 },
}

static mut APIC_MODE: ApicMode = ApicMode::X2Apic;

/// Initialize the Local APIC
pub fn init(hhdm_offset: u64) {
    init_pic_for_keyboard();

    // Try x2APIC first (MSR-based, no MMIO needed)
    let x2apic_ok = enable_x2apic();

    if x2apic_ok {
        unsafe { APIC_MODE = ApicMode::X2Apic; }
        serial_println!("[apic] x2APIC mode enabled (MSR-based)");

        // Set spurious vector + enable
        apic_write_msr(APIC_REG_SPURIOUS, 0x100 | SPURIOUS_VECTOR as u64);
        // Accept all interrupts
        apic_write_msr(APIC_REG_TPR, 0);
        // Timer: divide by 16
        apic_write_msr(APIC_REG_TIMER_DIVIDE, 0x03);
        // Timer: periodic mode, vector 32
        apic_write_msr(APIC_REG_TIMER_LVT, (1 << 17) | TIMER_VECTOR as u64);
        // Timer initial count
        apic_write_msr(APIC_REG_TIMER_INIT, 0x0010_0000);

        let id = apic_read_msr(APIC_REG_ID);
        let ver = apic_read_msr(APIC_REG_VERSION);
        serial_println!("[apic] ID: {}, Version: {:#x}", id, ver & 0xFF);
    } else {
        // Fallback: xAPIC MMIO mode
        let lo: u32;
        let hi: u32;
        unsafe {
            core::arch::asm!("rdmsr", in("ecx") 0x1Bu32, out("eax") lo, out("edx") hi);
        }
        let phys = ((hi as u64) << 32 | lo as u64) & 0xFFFFF000;
        let virt = hhdm_offset + phys;
        unsafe { APIC_MODE = ApicMode::XApic { base: virt }; }
        serial_println!("[apic] xAPIC MMIO mode at {:#x}", virt);

        unsafe {
            apic_write_mmio(virt, 0x0F0, 0x100 | SPURIOUS_VECTOR as u32);
            apic_write_mmio(virt, 0x080, 0);
            apic_write_mmio(virt, 0x3E0, 0x03);
            apic_write_mmio(virt, 0x320, (1 << 17) | TIMER_VECTOR as u32);
            apic_write_mmio(virt, 0x380, 0x0010_0000);
        }
    }

    x86_64::instructions::interrupts::enable();
}

/// Send End of Interrupt
pub fn send_eoi() {
    match unsafe { APIC_MODE } {
        ApicMode::X2Apic => apic_write_msr(APIC_REG_EOI, 0),
        ApicMode::XApic { base } => unsafe { apic_write_mmio(base, 0x0B0, 0) },
    }
}

/// Get current monotonic tick count
pub fn ticks() -> u64 {
    TICK_COUNT.load(core::sync::atomic::Ordering::Relaxed)
}

/// APIC timer handler
pub extern "x86-interrupt" fn timer_handler(
    _stack_frame: x86_64::structures::idt::InterruptStackFrame,
) {
    TICK_COUNT.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
    send_eoi();
}

/// Spurious interrupt handler
pub extern "x86-interrupt" fn spurious_handler(
    _stack_frame: x86_64::structures::idt::InterruptStackFrame,
) {
    // No EOI for spurious
}
