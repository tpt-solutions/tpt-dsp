# Publishing

Tracks what has actually been pushed to [crates.io](https://crates.io), since
that is a separate, one-way step from tagging a release in git — a version
can be yanked but never deleted or overwritten, so this file is the source of
truth for "is version X of crate Y really out there."

## Workspace crates

Published in dependency order (a crate can't publish until everything it
path-depends on is live at the version it requests).

| Crate | Latest published version | Published | Notes |
|---|---|---|---|
| `tpt-dsp-core` | 0.2.0 | ☑ | No dependencies; published first. |
| `tpt-dsp-audio` | 0.2.0 | ☑ | Depends on `tpt-dsp-core`. |
| `tpt-dsp-analysis` | 0.2.0 | ☑ | Depends on `tpt-dsp-core`. |
| `tpt-dsp-control` | 0.2.0 | ☑ | No path dependencies. |
| `tpt-dsp-io` | 0.2.0 | ☑ | Depends on `tpt-dsp-core`. |
| `tpt-dsp-viz` | 0.2.0 | ☑ | Depends on `tpt-dsp-analysis`, `tpt-dsp-io`. |
| `tpt-dsp-wasm` | 0.2.0 | ☑ | Depends on `tpt-dsp-audio`. |
| `tpt-dsp-cli` | 0.2.0 | ☑ | Depends on `tpt-dsp-core`, `tpt-dsp-audio`, `tpt-dsp-analysis`, `tpt-dsp-io`. |

### Intentionally not published

| Crate | Reason |
|---|---|
| `tpt-dsp-nihplug` | Excluded from the workspace: pulls VST3/CLAP SDK bindings under licenses (e.g. MIT-0) not in the main `deny.toml` allow-list. Depends on `nice-plug`, a maintained fork/successor of nih-plug — publishing a CLAP/VST3 wrapper to crates.io ahead of a stable host ecosystem convention isn't a fit for the normal release cadence; distribute via git/binary release instead. |
| `tpt-dsp-py` | Excluded from the workspace: builds a Python `extension-module` (links libpython at load time) and cannot be built/tested by the normal `cargo build/test --workspace` gate. Python bindings belong on PyPI (via `maturin`/`pyo3`), not crates.io — no action needed here. |
| `templates/dsp-effect-crate` | A `cargo-generate` template, not a real crate (its `Cargo.toml` contains `{{crate_name}}` placeholders). Never published. |

## Release 0.2.0 (published 2026-11-01)

All eight publishable crates were uploaded at 0.2.0 on 2026-11-01 from the tag
`v0.2.0` (commit `d9aca4c`). The plan below is kept as a record of what was done.

### Why 0.2.0

Pre-1.0 semver: a minor bump is the signal for additions and behaviour changes. This
release adds a large set of DSP modules to `tpt-dsp-core` and changes some numerical
results, so it is not a patch release.

- New public API in `tpt-dsp-core`: fast DCT/MDCT plans, LPC/LSP, pitch, Bark/ERB,
  psychoacoustic model, noise shaping, vector quantisation.
- Behaviour fixes: `FftConvolver` is now exact for impulse responses of any length
  (this also fixes `tpt-dsp-audio`'s `ConvolutionReverb`), and the DCT/MDCT direct
  transforms now reduce the angle before `cos()`, so results change slightly at large N.

### Crates in this release

All eight publishable crates go out together at 0.2.0, because the version is
inherited from the workspace and the crates.io pages are the main reason for this
release. Several crates changed only documentation, but they still need a new version
so their crates.io pages show the corrected READMEs.

| Crate | Code changes since 0.1.0 | Docs-only? | Notes |
|---|---|---|---|
| `tpt-dsp-core` | Yes: new modules, `FftConvolver` fix, DCT/MDCT angle reduction | No | Publish first; everything else depends on it. |
| `tpt-dsp-audio` | Behaviour: `ConvolutionReverb` fix (via core) | No | Depends on `tpt-dsp-core`. |
| `tpt-dsp-analysis` | None (intra-doc link fixes) | Yes | README feature table and examples corrected. |
| `tpt-dsp-control` | None | Yes | README rewritten; changelog variant name fixed. |
| `tpt-dsp-io` | `audio_check` example gated behind `audio` | No | README rewritten; new `wav` section. |
| `tpt-dsp-viz` | Dependency: direct `egui` dependency removed, `eframe::egui` used | No | Depends on `tpt-dsp-analysis`, `tpt-dsp-io`. |
| `tpt-dsp-wasm` | None in the crate (its duplicate `www/` was removed) | Yes | Depends on `tpt-dsp-audio`. README build and reverb notes corrected. |
| `tpt-dsp-cli` | None (doc comments and README) | Yes | Depends on `tpt-dsp-core`, `tpt-dsp-audio`, `tpt-dsp-analysis`, `tpt-dsp-io`. |

Not part of this release: `tpt-dsp-nihplug` and `tpt-dsp-py` (see above). Their
CHANGELOG/README edits are unpublished work, and their path dependencies also say
`0.1.0`, which is harmless because they are excluded from the workspace.

### Publish order

Dependency order, with the same wait-for-index rule as step 4 below:

1. `tpt-dsp-core`
2. `tpt-dsp-audio`, `tpt-dsp-analysis`, `tpt-dsp-control`, `tpt-dsp-io` (any order;
   each needs only `core`)
3. `tpt-dsp-viz` (needs `analysis`, `io`)
4. `tpt-dsp-wasm` (needs `audio`)
5. `tpt-dsp-cli` (needs `core`, `audio`, `analysis`, `io`)

### Checklist

Before tagging:

- [x] Commit the pending work: the rewritten root `CHANGELOG.md` `[Unreleased]`
      section and the removal of `tpt-dsp-wasm/www/`.
- [x] Confirm the version is 0.2.0.
- [x] Set `workspace.package.version = "0.2.0"` in the root `Cargo.toml`.
- [x] Change the 12 internal path-dependency requirements from `version = "0.1.0"` to
      `version = "0.2.0"`. They are in `tpt-dsp-analysis`, `tpt-dsp-audio`,
      `tpt-dsp-cli` (5), `tpt-dsp-io` (2), `tpt-dsp-viz` (2) and `tpt-dsp-wasm`. Cargo
      will not resolve a `0.1.0` requirement against a `0.2.0` upload, so the dependent
      crates would fail to publish.
- [x] Make the README links work on crates.io. The READMEs link to sibling crates
      with relative paths (for example `../tpt-dsp-core`), which crates.io cannot
      resolve. Use absolute GitHub URLs for those links.
- [x] Move `[Unreleased]` to `## [0.2.0] - YYYY-MM-DD` in the root `CHANGELOG.md` and
      in all eight crate changelogs. Add a `[0.2.0]` compare link and point
      `[Unreleased]` at `v0.2.0...HEAD`.
- [x] Run the full gate from `AGENTS.md`: `cargo fmt --all -- --check`, build, test,
      `cargo clippy --workspace --all-targets --all-features -- -D warnings`,
      `cargo deny check`, and `cargo doc --workspace --all-features --no-deps`.
- [x] Check each package's contents with `cargo package --list -p <crate>`. Confirm the
      README and CHANGELOG are included and nothing stray is shipped. `tpt-dsp-wasm`
      should no longer contain `www/`.
- [x] Dry run: `cargo publish --dry-run -p tpt-dsp-core`. For the dependent crates,
      a dry run can't resolve their `0.2.0` dependencies until those are live on
      crates.io, so expect that error until the dependencies are published.
- [x] Working tree is clean and on `master`.

Publishing:

- [x] Tag and push: `git tag v0.2.0 && git push origin v0.2.0`.
- [x] Publish in the order above, waiting for each dependency to appear on crates.io
      before publishing its dependents.
- [x] After each upload, check that the crates.io page renders the README and that the
      links work.
- [x] Update the "Latest published version" and "Published" columns in the table above,
      and add a row to History.
- [ ] Create the GitHub Release from `v0.2.0`, pasting in the matching `CHANGELOG.md`
      section. **Still open:** the `gh` CLI is not installed on this machine, so this
      step has not been done.

## Release procedure

1. Bump `workspace.package.version` in the root [`Cargo.toml`](Cargo.toml) (all
   workspace members inherit it via `version.workspace = true`). Also update the
   internal `version = "…"` requirements on the path dependencies, which are not
   inherited.
2. Move `[Unreleased]` in the root [`CHANGELOG.md`](CHANGELOG.md) and in each
   changed crate's own `CHANGELOG.md` to a new dated version section.
3. Tag the release: `git tag vX.Y.Z`, then push the branch and the tag separately,
   checking that each push succeeds before going on. Don't chain the push into the
   publish step with `| tail`, which hides a failed push. For 0.2.0 the `master` push
   was rejected after the tag went up, and core had already been published; the
   fix was to merge `origin/master` and push again.
4. Publish in dependency order, waiting for each crate to be visible on
   crates.io before publishing the next one that depends on it (the index
   needs a few seconds to update):
   ```sh
   cargo publish -p tpt-dsp-core
   cargo publish -p tpt-dsp-audio
   cargo publish -p tpt-dsp-analysis
   cargo publish -p tpt-dsp-control
   cargo publish -p tpt-dsp-io
   cargo publish -p tpt-dsp-viz
   cargo publish -p tpt-dsp-wasm
   cargo publish -p tpt-dsp-cli
   ```
5. Check off the crate in the table above and record the version once the
   upload succeeds.
6. Cut a GitHub Release from the tag, pasting in the relevant `CHANGELOG.md`
   section.

## History

| Date | Version | Crates | Notes |
|---|---|---|---|
| 2026-11-01 | 0.2.0 | all eight workspace crates listed above | Tag `v0.2.0` at `d9aca4c`; all eight published from that tag (the `tpt-dsp-viz` upload uses eframe 0.36, while `master` has since moved to eframe 0.37 by dependabot). New DSP modules in core, `FftConvolver` and DCT/MDCT fixes, and README and changelog corrections. The GitHub Release is still to be created. |
| 2026-08-26 | 0.1.0 | all eight workspace crates listed above | First public release. Published in dependency order (core → control/audio/analysis/io → viz/wasm/cli); crates.io's new-crate rate limit forced a few minutes' wait between the `viz`, `wasm` and `cli` uploads. |
