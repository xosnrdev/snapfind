//! Memory allocation tracking implementation

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

impl Default for TrackingAllocator {
    fn default() -> Self {
        Self::new()
    }
}

impl TrackingAllocator {
    /// Create a new tracking allocator
    #[must_use]
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

        let old_size = self.allocated.fetch_add(size, Ordering::SeqCst);
        let new_size = old_size + size;

        let mut peak = self.peak.load(Ordering::SeqCst);
        while new_size > peak {
            match self.peak.compare_exchange(peak, new_size, Ordering::SeqCst, Ordering::SeqCst) {
                Ok(_) => break,
                Err(current) => peak = current,
            }
        }

        assert!(
            self.initializing.load(Ordering::SeqCst),
            "Post-initialization heap allocation detected: {size} bytes (total: {new_size} \
             bytes). This violates the zero-allocation requirement."
        );

        unsafe { self.inner.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        let size = layout.size();
        self.allocated.fetch_sub(size, Ordering::SeqCst);

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

        let layout = Layout::new::<[u8; 1024]>();
        let ptr = unsafe { ALLOCATOR.alloc(layout) };
        assert!(!ptr.is_null());

        assert_eq!(ALLOCATOR.peak(), 1024);

        ALLOCATOR.end_init();

        unsafe { ALLOCATOR.dealloc(ptr, layout) };
        assert_eq!(ALLOCATOR.peak(), 1024);
    }

    #[test]
    #[should_panic(expected = "Post-initialization heap allocation")]
    fn test_post_init_allocation() {
        static ALLOCATOR: TrackingAllocator = TrackingAllocator::new();

        ALLOCATOR.end_init();

        let layout = Layout::new::<[u8; 1024]>();
        unsafe { ALLOCATOR.alloc(layout) };
    }

    #[test]
    fn test_concurrent_allocation() {
        static ALLOCATOR: TrackingAllocator = TrackingAllocator::new();
        let allocator = Arc::new(&ALLOCATOR);
        let barrier = Arc::new(std::sync::Barrier::new(4));
        let mut handles = vec![];

        for _ in 0..4 {
            let allocator = Arc::clone(&allocator);
            let barrier = Arc::clone(&barrier);
            handles.push(thread::spawn(move || {
                let layout = Layout::new::<[u8; 1024]>();
                let ptr = unsafe { allocator.alloc(layout) };
                assert!(!ptr.is_null());
                barrier.wait();
                unsafe { allocator.dealloc(ptr, layout) };
                1024_usize
            }));
        }

        let mut total_allocated = 0;
        for handle in handles {
            total_allocated += handle.join().unwrap();
        }

        assert_eq!(ALLOCATOR.peak(), total_allocated);
    }

    #[test]
    fn test_deallocation_tracking() {
        static ALLOCATOR: TrackingAllocator = TrackingAllocator::new();

        let layout = Layout::new::<[u8; 1024]>();
        let ptr = unsafe { ALLOCATOR.alloc(layout) };
        unsafe { ALLOCATOR.dealloc(ptr, layout) };

        assert_eq!(ALLOCATOR.allocated.load(Ordering::SeqCst), 0);
        assert_eq!(ALLOCATOR.peak(), 1024);
    }
}
