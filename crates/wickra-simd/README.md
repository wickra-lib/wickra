# wickra-simd

Runtime SIMD dispatch for the [Wickra](https://github.com/wickra-lib/wickra)
technical indicators core.

Release binaries target the baseline of their architecture, so on `x86_64`
they cannot assume AVX2 or a fused multiply-add instruction. `dispatch` checks
the CPU once and runs a kernel inside a function compiled with AVX2 and FMA
enabled when both are present, and in the baseline build otherwise. On
`aarch64`, NEON and FMA are baseline and kernels run directly.

Dispatch changes how a kernel is compiled, never what it computes: Rust neither
reassociates floating-point operations nor contracts `a * b + c` into an FMA,
and `f64::mul_add` is correctly rounded on both paths, so the results are
bit-identical. The crate's tests check that.

This crate holds the one `unsafe` call the dispatch needs; `wickra-core` itself
stays `#![forbid(unsafe_code)]`. It is an implementation detail of Wickra and
carries no stability promise of its own.

Licensed under either of Apache License 2.0 or MIT license at your option.
