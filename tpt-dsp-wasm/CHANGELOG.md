# Changelog

All notable changes to `tpt-dsp-wasm` are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

For the whole-workspace history see the [root `CHANGELOG.md`](../CHANGELOG.md).

## [Unreleased]

## [0.1.0]

### Added
- Initial release of the WebAssembly / Web Audio guitar-effects pedalboard front
  end, compiling `tpt-dsp-audio` DSP to wasm and driving it from an `AudioWorklet`.
- Signal chain **distortion → delay → reverb → EQ**, wrapping the four effects in a
  single `AudioNode` that drops into an `AudioGraph`.
- `Pedalboard::process_block_128(&mut self, &[f32; 128], &mut [f32; 128])` — the
  allocation-free hot path, with every buffer allocated once in `new` /
  `with_sample_rate`. The claim is enforced by the `process_block_128_does_not_allocate`
  test (counting global allocator) and a companion `counting_allocator_actually_sees_allocations`
  test that proves the probe is wired up.
- Zero-copy JS interop via `input_ptr()` / `output_ptr()` + `process_internal_block()`,
  viewing wasm-linear-memory buffers directly as `Float32Array`s.
- Optional `async` feature adding `open_microphone()`, an `async` `getUserMedia`
  wrapper with echo cancellation, noise suppression and AGC disabled.

[Unreleased]: https://github.com/TPT-Solutions/tpt-dsp/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/TPT-Solutions/tpt-dsp/releases/tag/v0.1.0
