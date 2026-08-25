# Changelog

All notable changes to `tpt-dsp-io` are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

For the whole-workspace history see the [root `CHANGELOG.md`](../CHANGELOG.md).

## [Unreleased]

### Added
- `sdr_pipeline` example: end-to-end IQ source → FIR decimation → FM demodulation
  → audio decimation, against the built-in generator or a live `rtl_tcp` server.

### Changed
- Documented that `IqStream::feed` grows its internal buffer without bound if the
  caller never calls `drain`; the bounded `IqReassembler` is the recommended
  streaming path.

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
