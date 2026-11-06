# tpt-dsp-testkit

> Allocation checks for real-time code in the
> [tpt-dsp](https://github.com/TPT-Solutions/tpt-dsp) framework.

The framework's real-time rule is that `process`-style code must not allocate, lock
or make syscalls. This crate lets your own tests check the allocation part of that
rule. It is a test-only tool: it is a dev-dependency, not something to ship.

## What's inside

- **`CountingAllocator`**: a global allocator that delegates to the system allocator
  and counts the allocations made on the current thread while counting is on.
- **`allocations_in(f)`**: runs `f` and returns its result with the number of
  allocations it made.
- **`assert_no_alloc(f)`**: runs `f` and panics if it allocated.

Counting is per thread and opt-in. Only allocations inside the closure are counted,
so tests that run in parallel do not interfere with one another.

## Example

Install the allocator in the test binary, then wrap the code under test:

```rust
use tpt_dsp_testkit::{assert_no_alloc, CountingAllocator};

#[global_allocator]
static ALLOC: CountingAllocator = CountingAllocator;

#[test]
fn my_chain_is_allocation_free() {
    let mut chain = build_chain();          // allocate everything up front
    let mut block = [0.0f32; 128];
    assert_no_alloc(|| chain.process_block(&mut block));
}
```

Warm the path up once before asserting, if it lazily creates state on its first call.
A test that checks the probe itself is wired up is a good companion: a silently broken
allocator hook would otherwise let the real test pass by accident.

Add it as a dev-dependency. It is not on crates.io yet, so depend on it by path or git:

```toml
[dev-dependencies]
tpt-dsp-testkit = { git = "https://github.com/TPT-Solutions/tpt-dsp" }
```

The framework uses it in `tpt-dsp-wasm` (`pedalboard.rs`), and its own tests are in
`tpt-dsp-testkit/tests/allocations.rs`. See `docs/RT_SAFETY.md` for the full
real-time guidance and `ARCHITECTURE.md` §3.2 for the accepted exceptions.

## License

Dual licensed under **MIT** and **Apache-2.0**. © TPT Solutions.
