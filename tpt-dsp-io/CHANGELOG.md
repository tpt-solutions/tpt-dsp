# Changelog

All notable changes to `tpt-dsp-io` are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

For the whole-workspace history see the [root `CHANGELOG.md`](../CHANGELOG.md).

## [Unreleased]

### Added
- macOS audio backend (`backend_mac.rs`): CoreAudio AudioUnits — default-output
  playback and HALOutput capture through hand-declared `extern "C"` bindings to
  the system `AudioToolbox`/`CoreAudio` frameworks (no `coreaudio-sys`, no
  wrapper crates). Compile-checked for `x86_64-apple-darwin`; runtime validation
  on real hardware still pending.
- Linux audio backend (`backend_linux.rs`): raw ALSA UAPI ioctls directly on
  `/dev/snd/pcmC*D*p|c` — no libasound linkage, blocking `RW_INTERLEAVED`
  transfers, FLOAT_LE/S32_LE/S16_LE format negotiation, XRUN recovery, and
  device enumeration via a `/dev/snd` scan with `/proc/asound` card names.
- Cross-platform device selection: `run_output_on_device` / `run_input_on_device`
  plus `list_output_devices` / `list_input_devices`, matching devices by exact
  name or case-insensitive substring on all three platforms.
- `wav` module (`src/wav.rs`): built-in RIFF/WAVE reader/writer replacing the
  `hound` dependency — PCM 8/16/24/32-bit and IEEE float 32/64 input,
  WAVE_FORMAT_EXTENSIBLE support, 32-bit float output, normalised to `f32`
  (`read_wav_f32_path` / `write_wav_f32_path` and reader/writer variants).
- `sdr_pipeline` example: end-to-end IQ source → FIR decimation → FM demodulation
  → audio decimation, against the built-in generator or a live `rtl_tcp` server.

### Changed
- **`cpal` fully removed from the tree**; the `audio` feature now has zero
  external dependencies (shared-mode WASAPI on Windows, raw ALSA UAPI on Linux
  and CoreAudio AudioUnits on macOS, all implemented in-tree). This resolves
  the Apache-2.0-only licensing constraint for MIT-only redistribution.
- Documented that `IqStream::feed` grows its internal buffer without bound if the
  caller never calls `drain`; the bounded `IqReassembler` is the recommended
  streaming path.

### Fixed
- Windows audio backend buffer handling and device enumeration.
- Release-profile WASAPI capture crash (`0xC0000005` in `ntdll.dll`): COM
  vtable methods were invoked through a *variadic* transmuted function-pointer
  type, which is not ABI-safe under optimization. `vt_call!` now coerces each
  argument to a machine word and dispatches through exactly-typed non-variadic
  per-arity signatures (`call_vt`). Verified with repeated release runs of live
  capture on Windows hardware.

## [0.1.0]

### Added
- Initial release of the pure-Rust hardware I/O crate built on `tpt-dsp-core`.
  Default features are the `iq` + `source` + `tcp` (client) core only — no audio or
  serial dependencies.
- `iq`: allocation-free parsing of raw interleaved I/Q byte streams into `Complex32`
  samples (`parse_iq` + `IqFormat`), plus `IqStream` and the bounded, resumable
  `IqReassembler`.
- `source`: the `IqSource` trait and `SyntheticIqSource`, an in-memory generator for
  tests/examples.
- `tcp`: `TcpIqSource` (blocking source over any reader) and the async `serve_iq`
  server (feature `tcp`).
- `rtlsdr`: `RtlSdrSource` + `RtlSdrConfig`. A documented stub unless a driver is
  wired in behind the `rtl-sdr` feature.
- `audio` (feature `audio`): Built-in dependency-free real-time output via `run_output` and
  `list_output_devices` (WASAPI on Windows; other platforms stubbed).
- `serial` (feature `serial`): `SerialReader`, a serial-port byte reader.

[Unreleased]: https://github.com/TPT-Solutions/tpt-dsp/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/TPT-Solutions/tpt-dsp/releases/tag/v0.1.0
