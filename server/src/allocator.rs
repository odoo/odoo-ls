use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};

pub struct TrackingAllocator;

pub static ALLOCATED: AtomicUsize = AtomicUsize::new(0);

unsafe impl GlobalAlloc for TrackingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let size = layout.size();
        ALLOCATED.fetch_add(size, Ordering::SeqCst);
        System.alloc(layout)
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        let size = layout.size();
        ALLOCATED.fetch_sub(size, Ordering::SeqCst);
        System.dealloc(ptr, layout)
    }

    unsafe fn realloc(
        &self,
        ptr: *mut u8,
        old_layout: Layout,
        new_size: usize,
    ) -> *mut u8 {
        let old_size = old_layout.size();
        ALLOCATED.fetch_sub(old_size, Ordering::SeqCst);
        ALLOCATED.fetch_add(new_size, Ordering::SeqCst);
        System.realloc(ptr, old_layout, new_size)
    }
}