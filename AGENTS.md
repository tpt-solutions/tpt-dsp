# AGENTS.md

`tpt-dsp` — pure-Rust, real-time-safe DSP framework (audio, RF/SDR, control).
One Cargo workspace, 10 member crates (+ 2 excluded standalone crates), dual MIT/Apache-2.0.

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
- **`tpt-dsp-audio` / `tpt-dsp-analysis` / `tpt-dsp-io`** — depend on `core` only, never on
  each other. **`tpt-dsp-control`** has no workspace dependencies at all.
- **`tpt-dsp-testkit`** is a dev-dependency only (of `tpt-dsp-wasm`). **`tpt-dsp`** is the
  facade crate that re-exports core, audio, analysis, control and io behind features.
- **`tpt-dsp-wasm`** — core + audio; the Web Audio pedalboard
  (`Waveshaper → Delay → ConvolutionReverb → Eq`) driven from `www/` via an `AudioWorklet`.
- **`tpt-dsp-viz` is implemented**: `app.rs` (`VizApp`, an `eframe::App`) renders a
  scrolling waterfall (`Spectrogram`, colour-mapped via `colormap.rs`) plus a live dB
  spectrum line, peak-frequency/dB readout, and a pause toggle. `pipeline.rs` streams
  `SpectrumFrame`s from a producer thread over a `crossbeam-channel`: `run_synthetic`
  (deterministic multi-tone + noise, no hardware needed) or, under the `audio` feature,
  `run_audio_input` (default input device via the built-in backend, any format converted to f32, mono downmix). It still
  drags `egui`/`eframe` (0.37) into every `--workspace` build. Those now require Rust 1.95,
  so `viz` sets its own `rust-version = "1.95"` and the workspace MSRV (1.85) check runs with
  `--exclude tpt-dsp-viz`.
  A live (on-screen) render check still needs a machine with a display — not verified in
  this headless environment, though it builds, tests, and clips clean.
- **`tpt-dsp-cli`** — a command-line WAV/IQ DSP pipeline (`src/main.rs` + `src/lib.rs`)
  built on the framework; depends on `core`/`audio`/`analysis`/`io` and is a normal workspace member.
- **`tpt-dsp-nihplug`** and **`tpt-dsp-py`** are **excluded** from the default workspace
  build (see root `Cargo.toml` `exclude`): `nihplug` is a standalone CLAP/VST3 wrapper
  around `tpt-dsp-audio` (its transitive licenses aren't in the main `deny.toml`
  allow-list), and `py` is a standalone Python-extension binding. Both are verified
  separately with `cargo build` inside their own directories.
- `tpt-dsp-io`'s `rtl-sdr` feature is a **stubbed backend** — it only changes the error
  `RtlSdrSource` reports; there is no USB driver. Use `SyntheticIqSource`/`TcpIqSource`.
- **Audio I/O is fully in-tree; `cpal` is gone.** `tpt-dsp-io/src/audio/` is split into
  `backend_windows.rs` (shared-mode WASAPI, hand-declared COM vtables), `backend_linux.rs`
  (raw ALSA UAPI ioctls on `/dev/snd`, no libasound), `backend_mac.rs` (CoreAudio
  AudioUnits via hand-declared `extern "C"` bindings) and `backend_stub.rs`. The
  `audio` feature has **zero external dependencies** — do not re-add an audio-crate
  dependency (that was removed for MIT-only redistribution reasons). COM vtable calls
  must go through `vt_call!`/`call_vt` (non-variadic, per-arity signatures); a variadic
  transmuted call type miscompiles under release optimization (fixed 2026-08-26).
  Device enumeration/selection API: `run_output(_on_device)` / `run_input(_on_device)`,
  `list_*_devices`. WAV read/write lives in `tpt-dsp-io/src/wav.rs` (`hound` was also
  replaced by this in-tree module). Linux/macOS backends are compile-checked only —
  runtime validation needs real hardware on those platforms.

## Rules that break things if ignored

- **Real-time contract.** Anything named `process`/`tick`/`render`/`convolve`/`fft` must
  not allocate, lock, or syscall. Use one of the two established idioms: a free function
  over caller-owned slices, or a struct that allocates all scratch in `new()` and reuses
  it. Accepted existing exceptions are catalogued in `ARCHITECTURE.md` §3.2 — read it
  before claiming a path is or isn't allocation-free.
- To prove a new hot path is allocation-free, use `tpt-dsp-testkit`: install its
  `CountingAllocator` as the global allocator in a test binary and wrap the path in
  `assert_no_alloc` (see `docs/RT_SAFETY.md`). The wasm pedalboard test uses it, and
  the `counting_allocator_actually_sees_allocations` companion test proves it fires.
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
- Every library crate sets `#![warn(missing_docs)]`, and clippy runs with
  `-D warnings`, so a new public item without a doc comment fails CI. Doctests run as
  part of `cargo test`.
- `cargo fmt` is the source of truth. Commit subjects: imperative, under 72 chars.
  Topic branches off `master`.

## Testing quirks

- `tpt-dsp-io/tests/streaming.rs` asserts **wall-clock** throughput: 1 s of 2.4 MS/s IQ
  must process within 6× real time (debug) / 1× (release). A failure on a loaded machine
  is not necessarily a regression — re-run on an idle machine before "fixing" it.
- Feature defaults: `tpt-dsp-analysis` has **no default features** (pure math/spectrum);
  async is opt-in via `async`/`async-tokio`/`async-std`. `async-std` is the alternate
  adapter and its RUSTSEC-2025-0052 advisory is deliberately ignored in `deny.toml`.
  All `tpt-dsp-io` features (`audio`, `serial`, `tcp`, `rtl-sdr`) are off by
  default, so plain `cargo test -p tpt-dsp-io` exercises only the IQ layer.

## Which docs to trust

Executable sources (`Cargo.toml`, `.github/workflows/`) > `ARCHITECTURE.md` > the READMEs.
Known stale spots, verified against the code (2026-11-07):

- `ARCHITECTURE.md` (revised 2026-11-07) is the deep reference: dependency direction,
  zero-alloc caveats, application data flows and known limitations. §10–11 are dated
  2026-09-07 and are the only sections not rechecked in that revision.
- `BENCHMARKS.md` figures are indicative. `WindowType` now includes `Kaiser`, `Tukey`,
  `Gaussian`, `FlatTop` and `Rectangular`, but the `window/kaiser_f32` row has not been
  measured yet.
- `todo.md` is the live status log (see its "Last synced" date). `docs/archive/history/todo 1260804.md`
  and `docs/archive/spec.txt` are the original snapshots — historical, not current state.
