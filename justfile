# tpt-dsp — developer task runner.
#
# Run `just` to list recipes. Common entry points:
#   just ci        — the full local gate CI runs (fmt, build, test, clippy, deny, doc)
#   just test      — run the whole workspace test suite (all features)
#   just examples  — build and run the example for every crate

# Default recipe: list everything.
default:
    @just --list

# --- Lint / format --------------------------------------------------------

# Check formatting without modifying files (CI uses this).
fmt:
    cargo fmt --all -- --check

# Auto-format the workspace.
fmt-fix:
    cargo fmt --all

# --- Build / test / verify ------------------------------------------------

# Build every crate with all features.
build:
    cargo build --workspace --all-features

# Run the full workspace test suite (all features).
test:
    cargo test --workspace --all-features

# Clippy across all targets, denying warnings.
clippy:
    cargo clippy --workspace --all-targets --all-features -- -D warnings

# License / advisory / source audit (default + all features).
deny:
    cargo deny check
    cargo deny --all-features check bans licenses advisories sources

# Build the API docs for every crate.
doc:
    cargo doc --workspace --all-features --no-deps

# The complete local gate, mirroring `.github/workflows/ci.yml`.
ci: fmt build test clippy deny doc

# --- Cross / embedded / wasm ----------------------------------------------

# Verify `tpt-dsp-core` stays `no_std` (no features, and +alloc).
no_std:
    cargo build -p tpt-dsp-core --no-default-features
    cargo build -p tpt-dsp-core --no-default-features --features alloc

# Build the wasm pedalboard crate for `wasm32-unknown-unknown`.
wasm:
    cargo build -p tpt-dsp-wasm --all-features --target wasm32-unknown-unknown

# --- Examples -------------------------------------------------------------

# Build every crate's examples.
examples:
    cargo build --workspace --all-features --examples

# Run one crate's example (e.g. `just example tpt-dsp-io sdr_pipeline`).
example crate name:
    cargo run -p {{crate}} --example {{name}}
