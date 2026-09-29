//! Runtime SIMD dispatch for the Wickra technical indicators core.
//!
//! Release binaries are built for the baseline of their target — on `x86_64`
//! that is SSE2, with no AVX and no fused multiply-add instruction — so they
//! run on every CPU of the architecture. A batch kernel that would profit from
//! AVX2 lanes or a hardware FMA therefore cannot simply be compiled for them.
//! [`dispatch`] closes that gap: it checks the CPU once (the standard library
//! caches the answer) and runs a [`Kernel`] inside a function compiled with
//! AVX2 and FMA enabled when both are present, and in the baseline build
//! otherwise.
//!
//! # Same result on every path
//!
//! Dispatch changes how a kernel is compiled, never what it computes. Rust
//! evaluates floating-point code exactly as written — it never reassociates,
//! and never contracts `a * b + c` into a fused multiply-add — so the AVX2
//! build and the baseline build of one kernel perform the same IEEE-754
//! operations in the same order. `f64::mul_add` is correctly rounded whether it
//! becomes the FMA instruction or the C library's `fma`. The two paths
//! therefore return bit-identical values; the crate's tests check exactly that.
//!
//! On `aarch64`, NEON and FMA are part of the baseline, so kernels already get
//! them and [`dispatch`] runs them directly.
//!
//! # Writing a kernel
//!
//! Implement [`Kernel`] and mark [`Kernel::run`] `#[inline(always)]`, as well
//! as any helper it calls: code is only compiled with the enabled features if
//! it is inlined into the dispatching function.

use std::sync::atomic::{AtomicU8, Ordering};

/// A unit of work [`dispatch`] can run with the best instruction set available.
///
/// Implementors mark [`run`](Kernel::run) `#[inline(always)]` so the body is
/// compiled into the feature-enabled function rather than called out of it.
pub trait Kernel {
    /// What the kernel returns.
    type Output;

    /// Execute the kernel.
    fn run(self) -> Self::Output;
}

/// The instruction set [`dispatch`] selects on this machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    /// Baseline instructions of the build target.
    Baseline,
    /// `x86_64` with AVX2 and FMA.
    Avx2Fma,
    /// `aarch64`, whose baseline includes NEON and FMA.
    Neon,
}

impl Level {
    /// Stable lowercase name, for diagnostics and benchmark labels.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Baseline => "baseline",
            Self::Avx2Fma => "avx2+fma",
            Self::Neon => "neon",
        }
    }
}

/// Cached detection result: 0 = not yet detected, 1 = baseline, 2 = AVX2+FMA.
static DETECTED: AtomicU8 = AtomicU8::new(0);

/// The instruction set [`dispatch`] runs kernels with on this machine.
pub fn level() -> Level {
    if cfg!(target_arch = "aarch64") {
        return Level::Neon;
    }
    match DETECTED.load(Ordering::Relaxed) {
        1 => Level::Baseline,
        2 => Level::Avx2Fma,
        _ => {
            let level = detect();
            DETECTED.store(
                if level == Level::Avx2Fma { 2 } else { 1 },
                Ordering::Relaxed,
            );
            level
        }
    }
}

#[cfg(target_arch = "x86_64")]
fn detect() -> Level {
    if std::arch::is_x86_feature_detected!("avx2") && std::arch::is_x86_feature_detected!("fma") {
        Level::Avx2Fma
    } else {
        Level::Baseline
    }
}

#[cfg(not(target_arch = "x86_64"))]
fn detect() -> Level {
    Level::Baseline
}

/// Run `kernel` with the best instruction set this CPU offers.
///
/// Returns bit-for-bit what [`run_baseline`] returns for the same kernel.
#[inline]
pub fn dispatch<K: Kernel>(kernel: K) -> K::Output {
    #[cfg(target_arch = "x86_64")]
    if level() == Level::Avx2Fma {
        // SAFETY: `run_avx2_fma` is only compiled-for, not guaranteed-by, the
        // build target; `level()` has just confirmed at runtime that this CPU
        // implements both AVX2 and FMA, which is the one precondition.
        #[allow(unsafe_code)]
        return unsafe { run_avx2_fma(kernel) };
    }
    kernel.run()
}

/// Run `kernel` in the baseline build, never dispatching. Exists so tests (in
/// this crate and in its users) can prove a kernel returns the same bits on
/// both paths.
#[inline(never)]
pub fn run_baseline<K: Kernel>(kernel: K) -> K::Output {
    kernel.run()
}

/// The AVX2 + FMA build of a kernel.
///
/// # Safety
///
/// The CPU must support AVX2 and FMA.
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2,fma")]
#[allow(unsafe_code)]
unsafe fn run_avx2_fma<K: Kernel>(kernel: K) -> K::Output {
    kernel.run()
}

#[cfg(test)]
// Kernels must inline into the dispatching function to be compiled with its
// features; `#[inline(always)]` on `run` is the documented contract.
#[allow(clippy::inline_always)]
mod tests {
    use super::*;

    /// Three interleaved EMA-style recurrences with `mul_add` plus a plain
    /// multiply-add the compiler must not fuse: the shape the core's kernels
    /// take.
    struct Recurrences<'a> {
        xs: &'a [f64],
    }

    impl Kernel for Recurrences<'_> {
        type Output = Vec<f64>;

        #[inline(always)]
        fn run(self) -> Vec<f64> {
            let (mut a, mut b, mut c) = (1.0_f64, 2.0_f64, 3.0_f64);
            let mut out = Vec::with_capacity(self.xs.len() * 3);
            for &x in self.xs {
                a = 0.1_f64.mul_add(x, 0.9 * a);
                b = 0.3_f64.mul_add(x, 0.7 * b);
                c = c * 0.95 + x * 0.05;
                out.extend_from_slice(&[a, b, c - a]);
            }
            out
        }
    }

    /// Four independent lanes of a prefix sum, the pattern AVX2 vectorises.
    struct Lanes<'a> {
        xs: &'a [f64],
    }

    impl Kernel for Lanes<'_> {
        type Output = [f64; 4];

        #[inline(always)]
        fn run(self) -> [f64; 4] {
            let mut acc = [0.0_f64; 4];
            for chunk in self.xs.chunks_exact(4) {
                for (lane, &x) in acc.iter_mut().zip(chunk) {
                    *lane = x.mul_add(1.000_001, *lane);
                }
            }
            acc
        }
    }

    fn series() -> Vec<f64> {
        (0..4096)
            .map(|i| (f64::from(i) * 0.731).sin() * 1e3 + f64::from(i % 17))
            .collect()
    }

    #[test]
    fn dispatch_matches_the_baseline_bit_for_bit() {
        let xs = series();
        let fast = dispatch(Recurrences { xs: &xs });
        let base = run_baseline(Recurrences { xs: &xs });
        assert!(fast
            .iter()
            .zip(&base)
            .all(|(a, b)| a.to_bits() == b.to_bits()));
        let fast = dispatch(Lanes { xs: &xs });
        let base = run_baseline(Lanes { xs: &xs });
        assert!(fast
            .iter()
            .zip(&base)
            .all(|(a, b)| a.to_bits() == b.to_bits()));
    }

    #[test]
    fn level_is_stable_and_named() {
        let first = level();
        assert_eq!(level(), first);
        assert!(["baseline", "avx2+fma", "neon"].contains(&first.name()));
        assert_eq!(Level::Baseline.name(), "baseline");
        assert_eq!(Level::Avx2Fma.name(), "avx2+fma");
        assert_eq!(Level::Neon.name(), "neon");
    }

    #[cfg(target_arch = "x86_64")]
    #[test]
    fn detection_agrees_with_the_standard_library() {
        let both = std::arch::is_x86_feature_detected!("avx2")
            && std::arch::is_x86_feature_detected!("fma");
        assert_eq!(level() == Level::Avx2Fma, both);
    }
}
