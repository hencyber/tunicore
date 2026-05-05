//! Page Frame Allocator - Physical memory management
//!
//! Bitmap-based allocator that uses the Limine memory map to track
//! all usable 4 KiB physical frames in the system.
//!
//! This is the foundation for:
//! - Per-agent address spaces (paging)
//! - Dynamic memory allocation beyond the static heap
//! - DMA buffers for device drivers

use spin::Mutex;
use crate::serial_println;

/// Page frame size: 4 KiB
pub const PAGE_SIZE: usize = 4096;

/// Max supported physical memory: 4 GiB (1M frames)
/// Bitmap: 1M bits = 128 KiB
const MAX_FRAMES: usize = 1024 * 1024;
const BITMAP_SIZE: usize = MAX_FRAMES / 8;

/// Physical frame address
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PhysFrame(pub u64);

impl PhysFrame {
    pub fn containing(addr: u64) -> Self {
        PhysFrame(addr & !(PAGE_SIZE as u64 - 1))
    }

    pub fn index(&self) -> usize {
        (self.0 / PAGE_SIZE as u64) as usize
    }

    pub fn from_index(idx: usize) -> Self {
        PhysFrame(idx as u64 * PAGE_SIZE as u64)
    }
}

/// Bitmap-based physical frame allocator
struct FrameAllocator {
    /// 1 = used, 0 = free
    bitmap: [u8; BITMAP_SIZE],
    /// Total usable frames detected from memmap
    total_frames: usize,
    /// Currently allocated frames
    used_frames: usize,
    /// Lowest usable frame index
    min_frame: usize,
    /// Highest usable frame index
    max_frame: usize,
    /// Initialized flag
    initialized: bool,
}

impl FrameAllocator {
    const fn new() -> Self {
        Self {
            bitmap: [0xFF; BITMAP_SIZE], // All marked as used initially
            total_frames: 0,
            used_frames: 0,
            min_frame: usize::MAX,
            max_frame: 0,
            initialized: false,
        }
    }

    /// Mark a range of frames as free (usable)
    fn mark_free(&mut self, start_frame: usize, count: usize) {
        for i in 0..count {
            let frame = start_frame + i;
            if frame < MAX_FRAMES {
                let byte = frame / 8;
                let bit = frame % 8;
                self.bitmap[byte] &= !(1 << bit); // Clear bit = free
                self.total_frames += 1;

                if frame < self.min_frame { self.min_frame = frame; }
                if frame > self.max_frame { self.max_frame = frame; }
            }
        }
    }

    /// Mark a single frame as used
    fn mark_used(&mut self, frame: usize) {
        if frame < MAX_FRAMES {
            let byte = frame / 8;
            let bit = frame % 8;
            self.bitmap[byte] |= 1 << bit;
        }
    }

    /// Check if a frame is free
    fn is_free(&self, frame: usize) -> bool {
        if frame >= MAX_FRAMES { return false; }
        let byte = frame / 8;
        let bit = frame % 8;
        self.bitmap[byte] & (1 << bit) == 0
    }

    /// Allocate a single physical frame
    fn alloc(&mut self) -> Option<PhysFrame> {
        if !self.initialized { return None; }

        // Scan bitmap for first free frame
        for byte_idx in (self.min_frame / 8)..=(self.max_frame / 8) {
            if byte_idx >= BITMAP_SIZE { break; }
            if self.bitmap[byte_idx] == 0xFF { continue; } // All used

            for bit in 0..8 {
                let frame = byte_idx * 8 + bit;
                if frame > self.max_frame { return None; }
                if self.is_free(frame) {
                    self.mark_used(frame);
                    self.used_frames += 1;
                    return Some(PhysFrame::from_index(frame));
                }
            }
        }
        None
    }

    /// Free a physical frame
    fn free(&mut self, frame: PhysFrame) {
        let idx = frame.index();
        if idx < MAX_FRAMES && !self.is_free(idx) {
            let byte = idx / 8;
            let bit = idx % 8;
            self.bitmap[byte] &= !(1 << bit);
            self.used_frames -= 1;
        }
    }

    /// Get stats
    fn stats(&self) -> FrameStats {
        FrameStats {
            total: self.total_frames,
            used: self.used_frames,
            free: self.total_frames.saturating_sub(self.used_frames),
        }
    }
}

/// Memory statistics
#[derive(Debug, Clone, Copy)]
pub struct FrameStats {
    pub total: usize,
    pub used: usize,
    pub free: usize,
}

impl FrameStats {
    pub fn total_mb(&self) -> usize { self.total * PAGE_SIZE / (1024 * 1024) }
    pub fn used_mb(&self) -> usize { self.used * PAGE_SIZE / (1024 * 1024) }
    pub fn free_mb(&self) -> usize { self.free * PAGE_SIZE / (1024 * 1024) }
}

/// Global frame allocator
static FRAME_ALLOC: Mutex<FrameAllocator> = Mutex::new(FrameAllocator::new());

/// Initialize from Limine memory map
pub fn init_from_memmap(entries: &[&limine::memmap::Entry]) {
    let mut alloc = FRAME_ALLOC.lock();

    let mut regions = 0u32;
    let mut total_bytes: u64 = 0;

    for entry in entries {
        if entry.type_ == limine::memmap::MEMMAP_USABLE {
            let start_frame = (entry.base as usize + PAGE_SIZE - 1) / PAGE_SIZE;
            let end_frame = (entry.base as usize + entry.length as usize) / PAGE_SIZE;

            if end_frame > start_frame {
                let count = end_frame - start_frame;
                alloc.mark_free(start_frame, count);
                total_bytes += entry.length;
                regions += 1;
            }
        }
    }

    // Reserve first 1 MiB (BIOS/legacy)
    for frame in 0..256 {
        alloc.mark_used(frame);
        if alloc.total_frames > 0 {
            alloc.used_frames += 1;
        }
    }

    alloc.initialized = true;

    let stats = alloc.stats();
    serial_println!("[pmm] {} regions, {} frames ({} MiB usable), {} reserved",
        regions, stats.total, stats.total_mb(), stats.used);
}

/// Allocate a physical frame
pub fn alloc_frame() -> Option<PhysFrame> {
    FRAME_ALLOC.lock().alloc()
}

/// Free a physical frame
pub fn free_frame(frame: PhysFrame) {
    FRAME_ALLOC.lock().free(frame);
}

/// Get memory statistics
pub fn stats() -> FrameStats {
    FRAME_ALLOC.lock().stats()
}
