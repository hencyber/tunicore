//! Heap allocator for TuniCore
//!
//! Uses a simple linked-list allocator backed by a static byte array.
//! This is sufficient for Phase 1 (boot + basic alloc::Vec/String usage).
//!
//! In Phase 2, this will be replaced by a proper page-based allocator
//! using the bootloader's memory map for physical frame allocation.

use linked_list_allocator::LockedHeap;

/// Heap size: 32 MiB - needed for wasmi WASM runtime + multi-agent
const HEAP_SIZE: usize = 32 * 1024 * 1024;

/// Static heap backing store
/// Aligned to page boundary for future compatibility
#[repr(align(4096))]
struct HeapStorage([u8; HEAP_SIZE]);

static mut HEAP_STORAGE: HeapStorage = HeapStorage([0; HEAP_SIZE]);

/// Global allocator instance
#[global_allocator]
static ALLOCATOR: LockedHeap = LockedHeap::empty();

/// Initialize the heap allocator with the static backing store
pub fn init() {
    unsafe {
        let heap_start = core::ptr::addr_of_mut!(HEAP_STORAGE.0) as *mut u8;
        ALLOCATOR.lock().init(heap_start, HEAP_SIZE);
    }
}
