# Changelog

All notable changes to the `tpt-dsp` facade crate are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

For the whole-workspace history see the [root `CHANGELOG.md`](https://github.com/TPT-Solutions/tpt-dsp/blob/master/CHANGELOG.md).

## [Unreleased]

### Added
- Facade crate that re-exports the workspace crates behind feature flags: `core` always,
  and `audio`, `analysis`, `control` and `io` as features. `audio`, `analysis` and
  `control` are enabled by default.
- `prelude` module with the types most programs use, from every enabled crate.
- `walkthrough` module: a compiled example that reads a WAV file, filters it, analyses
  it and writes the result.
