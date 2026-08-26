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
| `tpt-dsp-core` | 0.1.0 | ☐ | No dependencies; publish first. |
| `tpt-dsp-audio` | 0.1.0 | ☐ | Depends on `tpt-dsp-core`. |
| `tpt-dsp-analysis` | 0.1.0 | ☐ | Depends on `tpt-dsp-core`. |
| `tpt-dsp-control` | 0.1.0 | ☐ | No path dependencies. |
| `tpt-dsp-io` | 0.1.0 | ☐ | Depends on `tpt-dsp-core`. |
| `tpt-dsp-viz` | 0.1.0 | ☐ | Depends on `tpt-dsp-analysis`, `tpt-dsp-io`. |
| `tpt-dsp-wasm` | 0.1.0 | ☐ | Depends on `tpt-dsp-audio`. |
| `tpt-dsp-cli` | 0.1.0 | ☐ | Depends on `tpt-dsp-core`, `tpt-dsp-audio`, `tpt-dsp-analysis`, `tpt-dsp-io`. |

### Intentionally not published

| Crate | Reason |
|---|---|
| `tpt-dsp-nihplug` | Excluded from the workspace: pulls VST3/CLAP SDK bindings under licenses (e.g. MIT-0) not in the main `deny.toml` allow-list. Depends on `nice-plug`, a maintained fork/successor of nih-plug — publishing a CLAP/VST3 wrapper to crates.io ahead of a stable host ecosystem convention isn't a fit for the normal release cadence; distribute via git/binary release instead. |
| `tpt-dsp-py` | Excluded from the workspace: builds a Python `extension-module` (links libpython at load time) and cannot be built/tested by the normal `cargo build/test --workspace` gate. Python bindings belong on PyPI (via `maturin`/`pyo3`), not crates.io — no action needed here. |
| `templates/dsp-effect-crate` | A `cargo-generate` template, not a real crate (its `Cargo.toml` contains `{{crate_name}}` placeholders). Never published. |

## Release procedure

1. Bump `workspace.package.version` in the root [`Cargo.toml`](Cargo.toml) (all
   workspace members inherit it via `version.workspace = true`).
2. Move `[Unreleased]` in the root [`CHANGELOG.md`](CHANGELOG.md) and in each
   changed crate's own `CHANGELOG.md` to a new dated version section.
3. Tag the release: `git tag vX.Y.Z && git push origin vX.Y.Z`.
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
| _pending_ | 0.1.0 | all eight workspace crates listed above | First public release. |
