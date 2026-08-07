# AGENTS.md

`tpt-dsp` — pure-Rust, real-time-safe DSP framework (audio, RF/SDR, control).
One Cargo workspace, 7 crates, dual MIT/Apache-2.0.

## Commands

Full local gate (mirrors `.github/workflows/ci.yml`; all of these currently pass
on Windows/stable):

```sh
cargo fmt --all -- --check
cargo build  --workspace --all-features
cargo test   --workspace --all-features
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo deny check                                                   # default features
cargo deny --all-features check bans licenses advisories sources   # --all-features must precede `check`
cargo doc --workspace --all-features --no-deps
```

Focused work:

```sh
cargo test -p tpt-dsp-core                                        # one crate
cargo test -p tpt-dsp-wasm process_block_128_does_not_allocate    # one test by substring
cargo test -p tpt-dsp-io --test streaming                         # one integration test target
cargo run  -p tpt-dsp-io --example sdr_pipeline                   # end-to-end IQ → decimate → FM demod (no features needed)
cargo bench -p tpt-dsp-core --no-run                              # compile-only bench check
cargo bench -p tpt-dsp-core --bench resampling_bench -- --warm-up-time 2 --measurement-time 5
```

Cross targets (`thumbv7em-none-eabihf` and `wasm32-unknown-unknown` are already installed):

```sh
cargo build -p tpt-dsp-core --no-default-features
cargo build -p tpt-dsp-core --no-default-features --features alloc
cargo build -p tpt-dsp-core --target thumbv7em-none-eabihf --no-default-features  # build, not check — linking is what proves no_std
cargo build -p tpt-dsp-wasm --all-features --target wasm32-unknown-unknown
```

Web pedalboard (`wasm-pack` is **not** installed here — `cargo install wasm-pack` first):

```sh
wasm-pack build tpt-dsp-wasm --target web --out-dir ../www/pkg
python -m http.server 8080 --directory www     # then http://localhost:8080
```

`www/pkg/` is generated and gitignored. `Cargo.lock` is also gitignored — do not commit it.

## Layout, and what is actually implemented

- **`tpt-dsp-core`** — the only leaf crate, the only `no_std` crate, and
  `#![forbid(unsafe_code)]`. Every other crate depends on it.
- **`tpt-dsp-audio` / `tpt-dsp-analysis` / `tpt-dsp-io` / `tpt-dsp-control`** — depend on
  `core` only, never on each other. `control` *declares* the `core` dependency but its
  PID/shaping/kinematics code does not call into it.
- **`tpt-dsp-wasm`** — core + audio; the Web Audio pedalboard
  (`Waveshaper → Delay → ConvolutionReverb → Eq`) driven from `www/` via an `AudioWorklet`.
- **`tpt-dsp-viz` is an empty stub**: `src/lib.rs` is a license comment and `src/main.rs`
  is `fn main() {}`, despite its README and `todo.md` describing an egui waterfall UI. It
  still drags `egui`/`eframe` into every `--workspace` build and pins `rust-version = 1.85`
  while the workspace MSRV is 1.74.
- `tpt-dsp-io`'s `rtl-sdr` feature is a **stubbed backend** — it only changes the error
  `RtlSdrSource` reports; there is no USB driver. Use `SyntheticIqSource`/`TcpIqSource`.

## Rules that break things if ignored

- **Real-time contract.** Anything named `process`/`tick`/`render`/`convolve`/`fft` must
  not allocate, lock, or syscall. Use one of the two established idioms: a free function
  over caller-owned slices, or a struct that allocates all scratch in `new()` and reuses
  it. Accepted existing exceptions are catalogued in `ARCHITECTURE.md` §3.2 — read it
  before claiming a path is or isn't allocation-free.
- To prove a new hot path is allocation-free, copy the counting-global-allocator probe in
  `tpt-dsp-wasm/src/pedalboard.rs` (`mod alloc_probe`: per-thread and opt-in, so parallel
  tests don't interfere), including its companion test that proves the probe itself fires.
- **`simd` is nightly-only.** `tpt-dsp-core/build.rs` sets a `tpt_portable_simd` cfg only
  on nightly; the crate root gates `#![feature(portable_simd)]` on it and otherwise
  compiles `simd_scalar.rs` under the same `simd` module path. That fallback is the only
  reason `--all-features` builds on stable — keep it, and keep the `feature(..)` attribute
  in `lib.rs` (crate-root only).
- **`tpt-dsp-core` must keep building with `--no-default-features` and with
  `--features alloc`.** Gate anything needing `alloc`/`std` behind those flags. `std` =
  `alloc` + `rustfft` + `crossbeam-channel`.
- **Workspace dependency defaults.** `num-complex`/`num-traits`/`rustfft` carry
  `default-features = false` in `[workspace.dependencies]` on purpose: cargo silently
  ignores a member-level `default-features = false` when the workspace entry lacks it.
  Add new deps there, and run `cargo deny` — licensing is strictly MIT/Apache-2.0.
- Every crate except the `viz` stub sets `#![warn(missing_docs)]`, and clippy runs with
  `-D warnings`, so a new public item without a doc comment fails CI. Doctests run as
  part of `cargo test`.
- `cargo fmt` is the source of truth. Commit subjects: imperative, under 72 chars.
  Topic branches off `master`.

## Testing quirks

- `tpt-dsp-io/tests/streaming.rs` asserts **wall-clock** throughput: 1 s of 2.4 MS/s IQ
  must process within 6× real time (debug) / 1× (release). A failure on a loaded machine
  is not necessarily a regression — re-run on an idle machine before "fixing" it.
- Feature defaults: `tpt-dsp-analysis` defaults to `async` (tokio); `async-std` is the
  alternate adapter and its RUSTSEC-2025-0052 advisory is deliberately ignored in
  `deny.toml`. All `tpt-dsp-io` features (`audio`, `serial`, `tcp`, `rtl-sdr`) are off by
  default, so plain `cargo test -p tpt-dsp-io` exercises only the IQ layer.

## Which docs to trust

Executable sources (`Cargo.toml`, `.github/workflows/`) > `ARCHITECTURE.md` > the READMEs.
Known stale spots, verified against the code:

- `ARCHITECTURE.md` is the best deep reference (dependency direction, zero-alloc caveats,
  MVP data flows) but predates `viz`/`wasm` — it still says "five crates".
- `BENCHMARKS.md` lists a `window/kaiser_f32` benchmark, but there is no Kaiser window
  (`WindowType` is Hann/Hamming/Blackman only). Treat its figures as indicative, not as
  a baseline to compare against.
- `tpt-dsp-wasm/README.md` is stale on the web layout: it says to build with
  `wasm-pack build --target web` from the crate dir, that `www/main.js` imports `../pkg/`
  so you must serve the crate root, and that a `worklet-polyfill.js` exists. None of that
  holds. `main.js` resolves `./pkg/tpt_dsp_wasm_bg.wasm` and `pedal-processor.js` imports
  `./pkg/tpt_dsp_wasm.js`, both relative to `www/` — which is what `www/README.md` and
  `.github/workflows/pages.yml` (`--out-dir ../www/pkg`, artifact = `www`) assume.
  Only the worklet touches the wasm-bindgen JS glue; `main.js` compiles the `.wasm`
  itself and hands the `WebAssembly.Module` to the worklet via `processorOptions`.
- `todo.md` is the live status log (see its "Last synced" date). `todo 1260804.md` and
  `spec.txt` are the original snapshots — historical, not current state.
