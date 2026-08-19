# tpt-dsp-nihplug

A CLAP/VST3 audio plugin wrapping `tpt-dsp-audio`, built with
[nice-plug](https://docs.rs/nice-plug) (the actively-maintained, API-compatible
successor to `nih-plug`, which is no longer published on crates.io).

The plugin is a pedalboard chain built entirely from the framework's own
real-time-safe effects — the same chain used by the WASM pedalboard and the
`tpt-dsp-cli` `filter` command:

```text
Waveshaper (Tanh) → Delay → ConvolutionReverb → 3-band EQ
```

Every stage is driven by a host-automated parameter, and all processing runs in
place on pre-allocated buffers (no allocation on the audio thread).

## Build

This crate is **intentionally excluded** from the top-level `tpt-dsp` workspace
(see the root `Cargo.toml` `exclude` list). The VST3/CLAP SDK bindings it pulls
in carry licences (e.g. MIT-0) that are not in the main framework's
`deny.toml` allow-list, so it is built and verified as its own standalone
workspace:

```sh
cd tpt-dsp-nihplug
cargo build --release        # produces target/release/*.clap and *.vst3 bundles
```

With the optional `nice-plug-xtask` tooling you can produce native plugin
bundles; see the nice-plug documentation for details.

Dual licensed under MIT / Apache-2.0. Copyright TPT Solutions.
