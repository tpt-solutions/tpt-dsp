//! Test helpers for checking that a DSP path is allocation-free.
//!
//! The real-time rule in this framework is that `process`-style code must not
//! allocate, lock or make syscalls. This crate lets your own tests check the
//! allocation half of that rule.
//!
//! Install [`CountingAllocator`] as the global allocator in your test binary,
//! then wrap the code under test in [`assert_no_alloc`]:
//!
//! ```ignore
//! use tpt_dsp_testkit::{assert_no_alloc, CountingAllocator};
//!
//! #[global_allocator]
//! static ALLOC: CountingAllocator = CountingAllocator;
//!
//! #[test]
//! fn my_chain_is_allocation_free() {
//!     let mut chain = build_chain();
//!     let mut block = [0.0f32; 128];
//!     assert_no_alloc(|| chain.process_block(&mut block));
//! }
//! ```
//!
//! Counting is per thread and opt-in. Only allocations made inside the closure
//! passed to [`allocations_in`] or [`assert_no_alloc`] are counted, so tests
//! running in parallel do not interfere with one another.
//!
//! The allocator delegates to [`System`], so it is safe to
//! use in any test. It is a test-only tool and should not be used in release
//! builds.
#![warn(missing_docs)]

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

thread_local! {
    static COUNTING: Cell<bool> = const { Cell::new(false) };
    static COUNT: Cell<usize> = const { Cell::new(0) };
}

/// A `GlobalAlloc` that forwards to the system allocator and counts calls made
/// on a thread while that thread is inside [`allocations_in`].
#[derive(Debug, Default, Clone, Copy)]
pub struct CountingAllocator;

fn bump() {
    // `try_with` avoids a panic if the thread-local is being torn down.
    let _ = COUNTING.try_with(|on| {
        if on.get() {
            let _ = COUNT.try_with(|c| c.set(c.get() + 1));
        }
    });
}

// SAFETY: every method forwards to `System` with the caller's arguments, and
// the only extra work is a thread-local counter that performs no allocation.
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        bump();
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        bump();
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        bump();
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

/// Run `f` and return how many heap allocations it made on this thread.
///
/// Requires [`CountingAllocator`] to be the global allocator. Without it the
/// count is always zero, so the check passes trivially.
pub fn allocations_in<R>(f: impl FnOnce() -> R) -> (R, usize) {
    COUNT.with(|c| c.set(0));
    COUNTING.with(|on| on.set(true));
    let out = f();
    COUNTING.with(|on| on.set(false));
    let n = COUNT.with(|c| c.get());
    (out, n)
}

/// Run `f` and panic if it allocates. Returns `f`'s result otherwise.
///
/// # Panics
///
/// Panics with the allocation count if `f` allocates on this thread.
pub fn assert_no_alloc<R>(f: impl FnOnce() -> R) -> R {
    let (out, n) = allocations_in(f);
    assert_eq!(n, 0, "code under test made {n} heap allocation(s)");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counting_is_off_by_default() {
        // Without the allocator installed the count stays at zero.
        let (v, n) = allocations_in(|| vec![1u8, 2, 3]);
        assert_eq!(v.len(), 3);
        assert_eq!(n, 0);
    }
}
