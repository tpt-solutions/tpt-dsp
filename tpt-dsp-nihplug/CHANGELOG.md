# Changelog

All notable changes to `tpt-dsp-nihplug` are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

For the whole-workspace history see the [root `CHANGELOG.md`](../CHANGELOG.md).
Note that this crate is excluded from the main workspace build and versioned
independently of it (see [`README.md`](README.md#standalone-build)).

## [Unreleased]

### Added
- Initial pedalboard plugin: `Waveshaper (Tanh) → Delay → ConvolutionReverb →
  3-band EQ`, built on `tpt-dsp-audio`'s real-time-safe effects.
- Host-automatable parameters for waveshaper drive/mix, delay time/feedback,
  reverb wet, and low-shelf/peak/high-shelf EQ gains.
- CLAP and VST3 export via the `nice-plug` (nih-plug-compatible) framework.
