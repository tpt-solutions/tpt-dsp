# Design decisions

Decisions that shape the framework's public API and dependencies, with the
reasoning, so they are not re-argued each time. Each one can be revisited, but
the entry should be changed in the same commit that changes the behaviour.

## Errors: one error enum per crate, no shared type

Each crate keeps its own error enum: `FilterDesignError` in `tpt-dsp-core`,
`PidError` and `ProfileError`/`ShaperError` in `tpt-dsp-control`, `WavError` and
`IqError` in `tpt-dsp-io`.

The crates depend only on `tpt-dsp-core` (AGENTS.md dependency rule), and
`tpt-dsp-io`, `-analysis` and `-control` do not depend on each other. A single
shared error type would have to live in `core` and name errors from `io` and
`control`, which inverts the dependency direction. Each enum implements
`Display`, so callers can wrap them in their own error type.

*Revisit* if an application needs one `Result` type across several crates. Then
add a thin wrapper in the facade crate `tpt-dsp`, not in the leaf crates.

## Precision: f32 on the processing path, f64 for design and measurement

Processors (`process`, `tick`, `render`) use `f32`. That matches the real-time
types, the SIMD paths and the WASM and Python bindings. Design and measurement
use `f64` internally: filter design (`iir_design.rs`, `remez.rs`), the LUFS
meter, and the analysis helpers that accumulate over long windows.

Public `f64` processors are not added unless a golden test shows that `f32`
error exceeds its tolerance for a real use. The coefficients are cast to `f32`
after design, and `checked_design` rejects any design that is not finite and
stable after that cast (see `FilterDesignError::NumericallyDegenerate`).

*Revisit* per processor, with a golden test in hand.

## Python arrays: zero-copy input is opt-in; outputs stay lists

Input: a `zero-copy` Cargo feature reads a 1-D float32 NumPy array in place,
through pyo3's buffer protocol. It needs a non-abi3 wheel, because the buffer
protocol is not in the limited API. The default abi3 wheel converts arrays with
`tolist()`. See `tpt-dsp-py/README.md` for the two build commands.

Output: results are still Python lists. Returning arrays would need the
`numpy` crate. That crate is BSD-2-Clause, and this project allows only
MIT/Apache-2.0 dependencies, so it is not used.

*Revisit* if an output-heavy workload needs arrays, and a permissive way to
build them is available (pyo3's buffer API, or a licence decision on `numpy`).

## Semver checks: deferred until the first published release

`cargo-semver-checks` compares the crate against a published baseline. There is
no crates.io release yet, so a job would have nothing to compare against. Add
the job in the same change as the first release.

## Pipelines: JSON and TOML in the CLI only

The declarative pipeline format (`tpt-dsp-cli` `pipeline` subcommand) is the
one place a text format is parsed. It takes JSON (`{"effects": [...]}`) or
TOML (`[[effects]]` tables), chosen by the file extension. TOML is converted to
the same `serde_json::Value` tree and goes through the same field handling, so
the two formats cannot disagree. A test checks that they give the same effects.

The WASM and Python bindings expose typed constructors (`Biquad`, `Pid`,
`FilterSpec`) and do not duplicate the parser. Parsing a second format in each
binding would give three places to keep in sync.

A `serde` derive over typed effect structs is not added. Each field's default
lives in one place, `effect_spec`, which both formats use. A derive would copy
those defaults into struct attributes, and the two copies could drift.

## Benchmarks: recorded weekly, not gated

The `bench` CI job runs `cargo bench -p tpt-dsp-core` on the weekly schedule
and keeps the criterion output as an artifact. It does not fail on a slower
result. Shared runners vary too much for a timing gate to be reliable. Compare
runs by downloading two artifacts. A gate would need dedicated hardware.

## Chebyshev I gain convention

Designs are normalised to unity gain at DC, as the unit tests and the golden
comparison (relative to DC) require. For an even order, that puts DC at the
ripple peak (0 dB), while scipy puts DC at the ripple trough (−ripple dB). The
shape is identical, so the golden tests agree. The designer page states the
difference.
