<!--
Thanks for contributing to tpt-dsp! Please confirm the checklist below before
requesting review. Maintainers run the full gate in CI, but it is much faster
to catch issues locally first. See AGENTS.md for the rules that break CI.
-->

## Summary

<!-- What does this PR change, and why? -->

## Affected crate(s)

<!-- e.g. tpt-dsp-core, tpt-dsp-viz -->

## Type of change

- [ ] Bug fix (non-breaking)
- [ ] New feature / primitive
- [ ] Breaking change (API change requiring a version bump)
- [ ] Documentation / tooling only

## Real-time contract

<!-- If this touches a hot path, confirm it does not allocate / lock / syscall.
The `alloc_probe` counting-allocator test in tpt-dsp-wasm documents the
accepted proof pattern. -->

## Local verification

Run before requesting review:

```sh
just ci        # fmt, build, test, clippy, deny, doc
just no_std    # core stays no_std
```

- [ ] `cargo fmt --all -- --check` is clean
- [ ] `cargo clippy --workspace --all-targets --all-features -- -D warnings` is clean
- [ ] `cargo test --workspace --all-features` passes
- [ ] New public items have doc comments (workspace uses `#![warn(missing_docs)]`)
- [ ] `cargo deny --all-features check` passes (any new dependency is MIT/Apache-2.0 compatible)

## Notes for reviewers

<!-- Anything non-obvious, tradeoffs, or follow-up work. -->
