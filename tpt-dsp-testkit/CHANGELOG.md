# Changelog

All notable changes to `tpt-dsp-testkit` are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

For the whole-workspace history see the [root `CHANGELOG.md`](https://github.com/TPT-Solutions/tpt-dsp/blob/master/CHANGELOG.md).

## [Unreleased]

### Added
- `CountingAllocator`: a global allocator that delegates to the system allocator and
  counts allocations made on the current thread while counting is on.
- `allocations_in`: runs a closure and returns its result with the number of
  allocations it made.
- `assert_no_alloc`: runs a closure and panics if it allocated.
