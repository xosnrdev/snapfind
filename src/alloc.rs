//! Memory allocation tracking for ensuring compliance with memory constraints

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

/// Tracks memory allocations to ensure we don't allocate after initialization
#[derive(Debug)]
pub struct TrackingAllocator {
    /// The underlying system allocator
    inner:        System,
    /// Whether we're in initialization phase
    initializing: AtomicBool,
    /// Total bytes currently allocated
    allocated:    AtomicUsize,
    /// Peak memory usage
    peak:         AtomicUsize,
}

impl TrackingAllocator {
    /// Create a new tracking allocator
    pub const fn new() -> Self {
        Self {
            inner:        System,
            initializing: AtomicBool::new(true),
            allocated:    AtomicUsize::new(0),
            peak:         AtomicUsize::new(0),
        }
    }

    /// Mark the end of initialization phase
    pub fn end_init(&self) {
        self.initializing.store(false, Ordering::SeqCst);
    }

    /// Get peak allocation size
    pub fn peak(&self) -> usize {
        self.peak.load(Ordering::SeqCst)
    }
}

unsafe impl GlobalAlloc for TrackingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let size = layout.size();

        // Track allocation size
        let old_size = self.allocated.fetch_add(size, Ordering::SeqCst);
        let new_size = old_size + size;

        // Update peak if necessary
        let mut peak = self.peak.load(Ordering::SeqCst);
        while new_size > peak {
            match self.peak.compare_exchange(peak, new_size, Ordering::SeqCst, Ordering::SeqCst) {
                Ok(_) => break,
                Err(current) => peak = current,
            }
        }

        // If we're not initializing, panic with allocation information
        assert!(
            self.initializing.load(Ordering::SeqCst),
            "Post-initialization heap allocation detected: {size} bytes (total: {new_size} \
             bytes). This violates the zero-allocation requirement."
        );

        // SAFETY: We're implementing GlobalAlloc, and this is the required unsafe operation
        unsafe { self.inner.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        let size = layout.size();
        self.allocated.fetch_sub(size, Ordering::SeqCst);
        // SAFETY: We're implementing GlobalAlloc, and this is the required unsafe operation
        unsafe {
            self.inner.dealloc(ptr, layout);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::thread;

    use super::*;

    #[test]
    fn test_allocation_tracking() {
        static ALLOCATOR: TrackingAllocator = TrackingAllocator::new();

        // Allocate during init
        let layout = Layout::new::<[u8; 1024]>();
        // SAFETY: Test-only allocation using our allocator
        let ptr = unsafe { ALLOCATOR.alloc(layout) };
        assert!(!ptr.is_null());

        // Check allocation size
        assert_eq!(ALLOCATOR.peak(), 1024);

        // End init phase
        ALLOCATOR.end_init();

        // Deallocate
        // SAFETY: Deallocating the memory we allocated above
        unsafe { ALLOCATOR.dealloc(ptr, layout) };
        assert_eq!(ALLOCATOR.peak(), 1024);
    }

    #[test]
    #[should_panic(expected = "Post-initialization heap allocation")]
    fn test_post_init_allocation() {
        static ALLOCATOR: TrackingAllocator = TrackingAllocator::new();

        // End init phase immediately
        ALLOCATOR.end_init();

        // This should panic
        let layout = Layout::new::<[u8; 1024]>();
        // SAFETY: Test-only allocation that should panic
        unsafe { ALLOCATOR.alloc(layout) };
    }

    #[test]
    fn test_concurrent_allocation() {
        static ALLOCATOR: TrackingAllocator = TrackingAllocator::new();
        let allocator = Arc::new(&ALLOCATOR);
        let barrier = Arc::new(std::sync::Barrier::new(4));
        let mut handles = vec![];

        // Spawn multiple threads that try to allocate
        for _ in 0..4 {
            let allocator = Arc::clone(&allocator);
            let barrier = Arc::clone(&barrier);
            handles.push(thread::spawn(move || {
                let layout = Layout::new::<[u8; 1024]>();
                // SAFETY: Test-only allocation during init phase
                let ptr = unsafe { allocator.alloc(layout) };
                assert!(!ptr.is_null());
                // Wait for all threads to allocate
                barrier.wait();
                // SAFETY: Deallocating the memory we just allocated
                unsafe { allocator.dealloc(ptr, layout) };
                1024_usize
            }));
        }

        // Wait for all allocations to complete
        let mut total_allocated = 0;
        for handle in handles {
            total_allocated += handle.join().unwrap();
        }

        // Verify peak matches total concurrent allocations
        assert_eq!(ALLOCATOR.peak(), total_allocated);
    }

    #[test]
    fn test_deallocation_tracking() {
        static ALLOCATOR: TrackingAllocator = TrackingAllocator::new();

        // Allocate and immediately deallocate
        let layout = Layout::new::<[u8; 1024]>();
        // SAFETY: Test-only allocation and deallocation
        let ptr = unsafe { ALLOCATOR.alloc(layout) };
        unsafe { ALLOCATOR.dealloc(ptr, layout) };

        // Current allocation should be 0, but peak should remain
        assert_eq!(ALLOCATOR.allocated.load(Ordering::SeqCst), 0);
        assert_eq!(ALLOCATOR.peak(), 1024);
    }
}
