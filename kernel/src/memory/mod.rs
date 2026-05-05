//! Memory management module
//!
//! Phase 1: Simple heap allocator using a static byte array.
//! Phase 10: Bitmap page frame allocator from Limine memmap.

pub mod allocator;
pub mod page_alloc;

/// Initialize the heap allocator
pub fn init_heap() {
    allocator::init();
}

/// Initialize page frame allocator from Limine memmap
pub fn init_page_alloc(entries: &[&limine::memmap::Entry]) {
    page_alloc::init_from_memmap(entries);
}
