# Security policy

## Supported versions

Security fixes are made for the latest published `0.x` release line.

## Reporting a vulnerability

Please do not open a public issue for a suspected vulnerability. Use GitHub's
private vulnerability reporting on this repository (Security tab, "Report a
vulnerability"). Include the affected crate and version, a minimal input that
reproduces the problem, and the observed impact.

Expect an acknowledgement within a few days. Fixes are released with a
RustSec advisory where the issue affects published versions.

## Scope

Input-driven crashes (for example malformed WAV or IQ data, or non-finite
parameters) are treated as bugs and are in scope. Denial of service through
unbounded allocation from untrusted files is also in scope.
