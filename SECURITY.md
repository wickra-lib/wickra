# Security Policy

## Supported versions

Security fixes are applied to the latest released version, `1.0.6`, only; please
upgrade to the newest release before reporting an issue.

| Version | Supported |
| --- | --- |
| 1.0.6 (latest) | :white_check_mark: |
| < 1.0.6 | :x: |

## Reporting a vulnerability

**Do not open a public issue for a security vulnerability.**

Report it privately through one of:

- GitHub's [private vulnerability reporting](https://github.com/wickra-lib/wickra/security/advisories/new)
  ("Report a vulnerability" under the repository's *Security* tab), or
- email to **support@wickra.org** with a subject line starting with
  `[wickra security]`.

Please include:

- the affected version(s) and platform / language binding,
- a description of the issue and its impact,
- steps to reproduce, ideally a minimal proof of concept.

## What to expect

- An acknowledgement within **5 working days**.
- An assessment and, if confirmed, a planned fix with a target release.
- Coordinated disclosure: we will agree on a disclosure date with you and
  credit you in the release notes unless you prefer to stay anonymous.

## Scope

In scope: the published crates (`wickra-core`, `wickra-data`, `wickra`), the
PyPI/npm packages, and the build/release workflows in `.github/workflows/`.

Out of scope: vulnerabilities in third-party dependencies (report those
upstream; we track them via Dependabot and `cargo-deny`).

## Security assurance case

This is a short, evidence-backed argument for why Wickra can be used safely.

**Security requirements.** Wickra is a computational library: it ingests
numeric market data and produces indicator values. It stores no user
credentials, authenticates no external users, and implements no cryptography of
its own. The requirements are therefore: (1) memory safety and freedom from
undefined behaviour, (2) robust handling of untrusted/degenerate numeric input
without panics or unbounded resource use, (3) integrity of the published
artifacts, and (4) a healthy dependency supply chain.

**How the requirements are met.**

- *Memory safety* — the core and all bindings are written in Rust. The crates
  forbid or minimise `unsafe`, so the compiler guarantees memory and thread
  safety for the indicator logic. The one exception is the C ABI
  ([`bindings/c`](bindings/c)), whose thin FFI shim is necessarily `unsafe`
  because it dereferences caller-supplied pointers; it adds no indicator logic,
  validates every handle for NULL, and never lets a panic cross the boundary, so
  the safe core's guarantees still cover all computation.
- *Input robustness* — every indicator validates its parameters and rejects
  non-finite inputs at construction; behaviour on edge cases (flat markets,
  warmup, reset) is pinned by unit tests, and the public update paths are
  exercised by coverage-guided fuzzing (`cargo-fuzz` / libFuzzer) in CI.
- *Static and dynamic analysis* — every push and pull request runs Clippy
  (`clippy::pedantic`, warnings-as-errors), CodeQL, fuzzing, and the full test
  suite, with 100% line coverage on the core crate tracked by Codecov.
- *Artifact integrity* — releases are built in CI, commits and tags are signed,
  the `main` branch requires signed commits, and release artifacts carry build
  provenance attestations.
- *Supply chain* — dependencies are pinned and monitored with Dependabot and
  audited with `cargo-deny` (license + advisory checks) on every change.

**Residual risk.** The optional `live-binance` feature opens a TLS WebSocket to
an exchange using the platform TLS library; transport security therefore
depends on that library, not on Wickra. Wickra is not a trading system and is
provided "as is" — see the disclaimers in `README.md` and the licenses.

## Secrets management

The project stores **no** secrets or credentials in the version control system.
Secrets required by automation (publishing tokens, the about-sync PAT) are kept
exclusively as **GitHub Actions encrypted secrets** and referenced via the
`secrets.*` context; they are never written to the repository, logs, or build
artifacts. GitHub **secret scanning with push protection** is enabled to block
accidental commits of credentials. Secrets follow least privilege (the narrowest
scope that works) and are rotated when a holder changes or on suspected
exposure.

## Verifying releases

Released artifacts can be verified for integrity and authenticity:

- **Build provenance.** Release assets carry GitHub build provenance
  attestations. Verify a downloaded asset with the GitHub CLI:
  `gh attestation verify <file> --repo wickra-lib/wickra`.
- **Signed tags.** Each release corresponds to a signed git tag (`vX.Y.Z`);
  the tag signature identifies the maintainer who authorised the release.
- **Registry integrity.** Packages are distributed over HTTPS from crates.io,
  PyPI and npm, which serve package checksums that package managers verify on
  install.

The release is published only by the maintainer through the tag-triggered
release workflow, so a verified tag signature establishes the expected
publisher identity.

## Support timeline and end of support

Only the **latest released version** receives security fixes. When a newer
release is published, the previous version **immediately reaches end of
support** and will not receive further fixes; users should upgrade to the
latest release. The supported-versions table above is authoritative. A defined
support window covering older releases may be introduced later; until it is,
only the latest release is supported.

## Remediation policy (dependencies and code scanning)

- **Severity threshold.** Vulnerabilities of **medium severity or higher** in
  the project's own code or its dependencies are remediated promptly and before
  the next release; lower-severity findings are addressed on a best-effort
  basis.
- **Automated enforcement (SCA).** Every change is evaluated by `cargo-deny`
  (RUSTSEC advisories + license policy) and Dependabot; a known-vulnerable
  dependency fails CI and **blocks the change** until resolved or explicitly
  waived with justification.
- **Automated enforcement (SAST).** Every change is evaluated by CodeQL and
  Clippy (`-D warnings`); findings **block the change** in CI until fixed.
- **Pre-release gate.** A release is not cut while an unresolved medium-or-higher
  SCA/SAST finding is outstanding.

## Vulnerability exploitability (VEX)

Advisories reported by `cargo-deny`/Dependabot for third-party dependencies that
do **not** affect Wickra (e.g. the vulnerable code path is not reachable, or the
affected feature is not enabled) are triaged and recorded — with the
not-affected justification — in the `cargo-deny` configuration (`deny.toml`) and
the relevant pull request, rather than forcing an unnecessary dependency bump.
This serves as the project's exploitability (VEX) record.
