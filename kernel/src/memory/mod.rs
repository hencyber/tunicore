//! Memory management module
//!
//! Phase 1: Simple heap allocator using a static byte array.
//! Phase 2 will add proper page table management using the bootloader memory map.

pub mod allocator;

/// Initialize the heap allocator
pub fn init_heap() {
    allocator::init();
}
