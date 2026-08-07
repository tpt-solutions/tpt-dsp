// SPDX-License-Identifier: MIT OR Apache-2.0
//
// Detects whether the active toolchain can enable `core::simd`. The
// `portable_simd` language feature is nightly-only, so it must not be turned on
// for a stable build: enabling the `simd` cargo feature on stable would
// otherwise fail to compile. We advertise a `tpt_portable_simd` cfg that the
// crate root uses to pick the vectorised module and gate the `feature(..)`
// attribute, letting `--all-features` build cleanly on every channel.

use std::process::Command;

fn main() {
    // Declare the cfg so `unexpected_cfgs` stays quiet regardless of channel.
    println!("cargo:rustc-check-cfg=cfg(tpt_portable_simd)");

    let rustc = std::env::var("RUSTC").unwrap_or_else(|_| "rustc".into());
    let is_nightly = Command::new(rustc)
        .arg("--version")
        .output()
        .map(|o| {
            let version = String::from_utf8_lossy(&o.stdout);
            version.contains("-nightly") || version.contains("-dev")
        })
        .unwrap_or(false);

    if is_nightly {
        println!("cargo:rustc-cfg=tpt_portable_simd");
    }

    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=RUSTC_BOOTSTRAP");
}
