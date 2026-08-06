# Contributing to tpt-dsp

Thanks for your interest in improving `tpt-dsp`! This document covers the
conventions and checks every change must satisfy before it is merged.

## Ground rules

- **Real-time safety is non-negotiable.** Processing hot paths (anything
  named `process`, `tick`, `render`, `convolve`, `fft`, etc.) must never
  allocate, lock or perform syscalls. Pass buffers in from the caller, or
  allocate them once in a struct's constructor.
- **`no_std` for `tpt-dsp-core`.** Any change to `tpt-dsp-core` must keep
  `--no-default-features` building, and must keep the `thumbv7em-none-eabihf`
  check green. Gated (`#[cfg(feature = "alloc")]` / `std`) code is fine.
- **License hygiene.** The project is strictly MIT / Apache-2.0. `cargo-deny`
  blocks GPL/LGPL/AGPL and unlicensed dependencies. Do not add copyleft
  crates.
- **Zero-copy, zero-panic in the loop.** Use `assert!` only for genuine
  programming errors (e.g. buffer-too-small at construction). Avoid panic
  paths on the hot path.

## Workflow

1. Fork and create a topic branch off `master`.
2. Make your change with tests. New public items need doc comments
   (`//!` / `///`) — `#![warn(missing_docs)]` is enforced.
3. Run the local checks below. CI must be green.
4. Open a pull request describing the *why* and the *what*.

## Local checks

Run these before pushing:

```sh
cargo fmt   --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test  --workspace --all-features
cargo deny  check
```

Cross-compilation sanity (no default features for `core`):

```sh
cargo build -p tpt-dsp-core --no-default-features
cargo check -p tpt-dsp-core --target thumbv7em-none-eabihf --no-default-features
cargo check -p tpt-dsp-core -p tpt-dsp-audio --target wasm32-unknown-unknown
```

## Commit messages

- Use present tense, imperative mood: `fix spsc disconnect after producer drop`.
- Keep the subject under 72 characters.
- Reference issues where relevant.

## Adding a dependency

- Prefer `workspace.dependencies` in the root `Cargo.toml`.
- Confirm it is license-compatible (`cargo deny check`).
- Prefer crates that are `no_std`-friendly or feature-gate them.

## Code style

- `cargo fmt` is the source of truth — do not hand-format.
- Prefer iterators over index loops where it stays readable.
- Public APIs should be documented with at least one example where the
  behaviour is non-obvious.

## License

By contributing you agree that your contributions are licensed under the
MIT / Apache-2.0 dual license, matching the rest of the project.
