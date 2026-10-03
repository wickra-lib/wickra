# wickra-simd

Runtime SIMD dispatch for the [Wickra](https://github.com/wickra-lib/wickra)
technical indicators core.

Release binaries target the baseline of their architecture, so on `x86_64`
they cannot assume AVX2 or a fused multiply-add instruction. `dispatch` checks
the CPU once and runs a kernel inside a function compiled with AVX2 and FMA
enabled when both are present, and in the baseline build otherwise. A kernel
that opts in runs with AVX-512F enabled as well where the CPU has it (when built
by Rust 1.89 or later), its element-wise work in one 512-bit register instead of
two 256-bit ones. On `aarch64`, NEON and FMA are baseline and kernels run
directly.

Dispatch changes how a kernel is compiled, never what it computes: Rust neither
reassociates floating-point operations nor contracts `a * b + c` into an FMA,
`f64::mul_add` is correctly rounded on every path, and an element-wise
operation is the same on each lane however wide the register, so the results
are bit-identical. The crate's tests check that at every level the machine has.

This crate holds the one `unsafe` call the dispatch needs; `wickra-core` itself
stays `#![forbid(unsafe_code)]`. It is an implementation detail of Wickra and
carries no stability promise of its own.

Licensed under either of Apache License 2.0 or MIT license at your option.
