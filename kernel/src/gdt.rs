//! Global Descriptor Table (GDT) and Task State Segment (TSS)
//!
//! Sets up the x86_64 segmentation with a minimal GDT containing:
//! - Kernel code segment
//! - Kernel data segment
//! - TSS with Interrupt Stack Table for double-fault handling

use spin::Lazy;
use x86_64::structures::gdt::{Descriptor, GlobalDescriptorTable, SegmentSelector};
use x86_64::structures::tss::TaskStateSegment;
use x86_64::VirtAddr;

/// IST index for the double-fault handler stack
pub const DOUBLE_FAULT_IST_INDEX: u16 = 0;

/// Size of the double-fault handler stack (20 KiB)
const STACK_SIZE: usize = 4096 * 5;

/// Static stack for double-fault handling
/// Must be static to persist across context switches
static mut DOUBLE_FAULT_STACK: [u8; STACK_SIZE] = [0; STACK_SIZE];

/// Task State Segment - provides separate stacks for exception handling
static TSS: Lazy<TaskStateSegment> = Lazy::new(|| {
    let mut tss = TaskStateSegment::new();
    tss.interrupt_stack_table[DOUBLE_FAULT_IST_INDEX as usize] = {
        let stack_start = VirtAddr::from_ptr(unsafe { &raw const DOUBLE_FAULT_STACK });
        stack_start + STACK_SIZE as u64 // Stack grows downward
    };
    tss
});

/// GDT + segment selectors
static GDT: Lazy<(GlobalDescriptorTable, Selectors)> = Lazy::new(|| {
    let mut gdt = GlobalDescriptorTable::new();
    let code_selector = gdt.append(Descriptor::kernel_code_segment());
    let data_selector = gdt.append(Descriptor::kernel_data_segment());
    let tss_selector = gdt.append(Descriptor::tss_segment(&TSS));
    (
        gdt,
        Selectors {
            code_selector,
            data_selector,
            tss_selector,
        },
    )
});

/// Segment selectors for use after GDT is loaded
struct Selectors {
    code_selector: SegmentSelector,
    #[allow(dead_code)]
    data_selector: SegmentSelector,
    tss_selector: SegmentSelector,
}

/// Initialize the GDT and load segment registers
pub fn init() {
    use x86_64::instructions::segmentation::{CS, DS, ES, SS, Segment};
    use x86_64::instructions::tables::load_tss;

    GDT.0.load();
    unsafe {
        CS::set_reg(GDT.1.code_selector);
        DS::set_reg(GDT.1.data_selector);
        ES::set_reg(GDT.1.data_selector);
        SS::set_reg(GDT.1.data_selector);
        load_tss(GDT.1.tss_selector);
    }
}
