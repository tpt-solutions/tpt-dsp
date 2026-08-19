# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- `justfile` with `ci`, `test`, `examples` and cross-platform recipes
  (`no_std`, `wasm`) to de-duplicate the command list repeated in
  README / CONTRIBUTING.md / AGENTS.md.
- GitHub issue and pull-request templates.
- Runnable examples for `tpt-dsp-core`, `tpt-dsp-audio`,
  `tpt-dsp-analysis` and `tpt-dsp-control` (in addition to the existing
  `tpt-dsp-io` SDR pipeline example).

### Changed
- `tpt-dsp-analysis`: `peak_bin` now uses a total order over `f32`, so a
  NaN/Inf value in the magnitude spectrum (malformed IQ-derived data) can no
  longer panic the real-time analysis path.
- `tpt-dsp-viz`: the `cpal` audio-input callbacks recover from a poisoned
  mutex (`lock().unwrap_or_else(|e| e.into_inner())`) instead of unwrapping,
  so a panic on another thread no longer crashes every subsequent callback.
- `tpt-dsp-io`: documented that `IqStream::feed` grows the internal buffer
  without bound if the caller never calls `drain` — the bounded
  `IqReassembler` is the recommended streaming path.
- CI: added a top-level `permissions: contents: read` block to
  `.github/workflows/ci.yml` for least-privilege defense-in-depth.

### Fixed
- Regression test covering `peak_bin` behaviour with NaN-containing input.

[Unreleased]: https://github.com/TPT-Solutions/tpt-dsp/compare/v0.1.0...HEAD
