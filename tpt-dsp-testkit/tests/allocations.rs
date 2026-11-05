//! Integration tests with the counting allocator installed as the global
//! allocator for this test binary.

use tpt_dsp_testkit::{allocations_in, assert_no_alloc, CountingAllocator};

#[global_allocator]
static ALLOC: CountingAllocator = CountingAllocator;

#[test]
fn detects_a_vector_allocation() {
    let (_v, n) = allocations_in(|| vec![0u8; 1024]);
    assert!(n >= 1, "expected at least one allocation, got {n}");
}

#[test]
fn passes_for_allocation_free_arithmetic() {
    let mut buf = [0.0f32; 256];
    assert_no_alloc(|| {
        for (i, s) in buf.iter_mut().enumerate() {
            *s = (i as f32).sin();
        }
    });
    assert!(buf[1] > 0.0);
}

#[test]
#[should_panic(expected = "heap allocation")]
fn panics_when_code_allocates() {
    assert_no_alloc(|| {
        let _v: Vec<u32> = Vec::with_capacity(16);
    });
}

#[test]
fn counts_are_per_closure() {
    // Allocations before and after the measured closure are not counted.
    let before: Vec<u8> = vec![1; 8];
    let (_, n) = allocations_in(|| 2 + 2);
    assert_eq!(n, 0);
    assert_eq!(before.len(), 8);
}
