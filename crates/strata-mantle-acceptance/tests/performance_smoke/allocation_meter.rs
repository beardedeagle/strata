use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicU64, Ordering};

use super::AllocationSnapshot;

#[global_allocator]
static GLOBAL_ALLOCATOR: CountingAllocator = CountingAllocator;

static ALLOCATION_COUNTERS: AllocationCounters = AllocationCounters::new();

struct CountingAllocator;

pub(super) fn capture() -> AllocationSnapshot {
    ALLOCATION_COUNTERS.capture()
}

pub(super) fn capture_interval_start() -> AllocationSnapshot {
    ALLOCATION_COUNTERS.capture_interval_start()
}

struct AllocationCounters {
    allocations: AtomicU64,
    deallocations: AtomicU64,
    allocated_bytes: AtomicU64,
    deallocated_bytes: AtomicU64,
    live_bytes: AtomicU64,
    peak_live_bytes: AtomicU64,
}

impl AllocationCounters {
    const fn new() -> Self {
        Self {
            allocations: AtomicU64::new(0),
            deallocations: AtomicU64::new(0),
            allocated_bytes: AtomicU64::new(0),
            deallocated_bytes: AtomicU64::new(0),
            live_bytes: AtomicU64::new(0),
            peak_live_bytes: AtomicU64::new(0),
        }
    }

    fn record_allocation(&self, size: usize) {
        let size = allocation_size(size);
        self.allocations.fetch_add(1, Ordering::Relaxed);
        self.allocated_bytes.fetch_add(size, Ordering::Relaxed);
        let live = self
            .live_bytes
            .fetch_add(size, Ordering::Relaxed)
            .saturating_add(size);
        self.record_peak_live(live);
    }

    fn record_deallocation(&self, size: usize) {
        let size = allocation_size(size);
        self.deallocations.fetch_add(1, Ordering::Relaxed);
        self.deallocated_bytes.fetch_add(size, Ordering::Relaxed);
        self.live_bytes.fetch_sub(size, Ordering::Relaxed);
    }

    fn record_peak_live(&self, live: u64) {
        let mut peak = self.peak_live_bytes.load(Ordering::Relaxed);
        while live > peak {
            match self.peak_live_bytes.compare_exchange_weak(
                peak,
                live,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(next_peak) => peak = next_peak,
            }
        }
    }

    fn capture(&self) -> AllocationSnapshot {
        AllocationSnapshot {
            allocations: self.allocations.load(Ordering::Relaxed),
            deallocations: self.deallocations.load(Ordering::Relaxed),
            allocated_bytes: self.allocated_bytes.load(Ordering::Relaxed),
            deallocated_bytes: self.deallocated_bytes.load(Ordering::Relaxed),
            live_bytes: self.live_bytes.load(Ordering::Relaxed),
            peak_live_bytes: self.peak_live_bytes.load(Ordering::Relaxed),
        }
    }

    fn capture_interval_start(&self) -> AllocationSnapshot {
        let live = self.live_bytes.load(Ordering::Relaxed);
        self.peak_live_bytes.store(live, Ordering::Relaxed);
        self.capture()
    }
}

// SAFETY: This test-only allocator delegates all allocation behavior to
// `System` and only records atomic counters after successful allocation calls.
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: Forwarding the exact layout to the platform allocator.
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() {
            ALLOCATION_COUNTERS.record_allocation(layout.size());
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: Forwarding the original pointer and layout to the allocator.
        unsafe { System.dealloc(ptr, layout) };
        ALLOCATION_COUNTERS.record_deallocation(layout.size());
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        // SAFETY: Forwarding the exact layout to the platform allocator.
        let ptr = unsafe { System.alloc_zeroed(layout) };
        if !ptr.is_null() {
            ALLOCATION_COUNTERS.record_allocation(layout.size());
        }
        ptr
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: Forwarding the original pointer/layout and requested size.
        let new_ptr = unsafe { System.realloc(ptr, layout, new_size) };
        if !new_ptr.is_null() {
            ALLOCATION_COUNTERS.record_deallocation(layout.size());
            ALLOCATION_COUNTERS.record_allocation(new_size);
        }
        new_ptr
    }
}

fn allocation_size(size: usize) -> u64 {
    u64::try_from(size).unwrap_or(u64::MAX)
}
