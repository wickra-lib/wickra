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

mod avx2;
mod portable;

#[cfg(target_arch = "x86_64")]
pub use avx2::Avx2;
pub use portable::Portable;

/// Four `f64` lanes and the operations the Wickra kernels are built from.
///
/// An implementation is a *token*: a zero-sized value whose existence proves
/// the instruction set it stands for is available, so its methods are safe to
/// call. [`Portable`] is available everywhere; [`Avx2`] is only ever created by
/// [`dispatch`] after the CPU was checked.
///
/// Every operation is exact per lane in the IEEE-754 sense — add, subtract,
/// multiply, divide, square root and fused multiply-add are correctly rounded,
/// `max`/`min` follow the x86 `MAXPD`/`MINPD` rule (`a > b ? a : b`, so the
/// second operand wins on a tie or a `NaN`) and `abs` clears the sign bit — so
/// one kernel written against this trait returns the same bits under every
/// implementation.
pub trait Simd: Copy {
    /// Four `f64` lanes.
    type V: Copy;

    /// All lanes `x`.
    fn splat(self, x: f64) -> Self::V;
    /// Lanes from an array.
    fn load(self, a: &[f64; 4]) -> Self::V;
    /// Lanes into an array.
    fn store(self, v: Self::V, a: &mut [f64; 4]);
    /// Lanes as an array.
    fn to_array(self, v: Self::V) -> [f64; 4];
    /// Lane-wise `a + b`.
    fn add(self, a: Self::V, b: Self::V) -> Self::V;
    /// Lane-wise `a - b`.
    fn sub(self, a: Self::V, b: Self::V) -> Self::V;
    /// Lane-wise `a * b`.
    fn mul(self, a: Self::V, b: Self::V) -> Self::V;
    /// Lane-wise `a / b`.
    fn div(self, a: Self::V, b: Self::V) -> Self::V;
    /// Lane-wise square root.
    fn sqrt(self, a: Self::V) -> Self::V;
    /// Lane-wise `a * b + c` with a single rounding.
    fn mul_add(self, a: Self::V, b: Self::V, c: Self::V) -> Self::V;
    /// Lane-wise `if a > b { a } else { b }`.
    fn max(self, a: Self::V, b: Self::V) -> Self::V;
    /// Lane-wise `if a < b { a } else { b }`.
    fn min(self, a: Self::V, b: Self::V) -> Self::V;
    /// Lane-wise absolute value (sign bit cleared).
    fn abs(self, a: Self::V) -> Self::V;
    /// `[0, v0, v1, v2]`: lanes moved up by one, `+0.0` shifted in.
    fn shift1(self, v: Self::V) -> Self::V;
    /// `[0, 0, v0, v1]`: lanes moved up by two, `+0.0` shifted in.
    fn shift2(self, v: Self::V) -> Self::V;
    /// All lanes the last lane `v3`.
    fn broadcast_last(self, v: Self::V) -> Self::V;
    /// The last lane `v3` as a scalar.
    fn last_lane(self, v: Self::V) -> f64;
    /// The 4×4 transpose of the rows `a`, `b`, `c`, `d`: result `j` is
    /// `[a_j, b_j, c_j, d_j]`. Pure data movement, so no value changes.
    fn transpose4(self, a: Self::V, b: Self::V, c: Self::V, d: Self::V) -> [Self::V; 4];
}

/// A unit of work [`dispatch`] can run with the best instruction set available.
///
/// `run` receives a [`Simd`] token: vector kernels compute through it, scalar
/// kernels ignore it and profit from the enabled instruction set through their
/// plain arithmetic (a `mul_add` becomes the FMA instruction). Implementors mark
/// [`run`](Kernel::run) `#[inline(always)]` so the body is compiled into the
/// feature-enabled function rather than called out of it.
pub trait Kernel {
    /// What the kernel returns.
    type Output;

    /// Execute the kernel with the lanes `simd` provides.
    fn run<S: Simd>(self, simd: S) -> Self::Output;
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
        return avx2::run(kernel);
    }
    kernel.run(Portable)
}

/// Run `kernel` in the baseline build with the [`Portable`] lanes, never
/// dispatching. Exists so tests (in this crate and in its users) can prove a
/// kernel returns the same bits on every path.
#[inline(never)]
pub fn run_baseline<K: Kernel>(kernel: K) -> K::Output {
    kernel.run(Portable)
}

#[cfg(test)]
// Kernels must inline into the dispatching function to be compiled with its
// features; `#[inline(always)]` on `run` is the documented contract.
#[allow(clippy::inline_always)]
mod tests {
    use super::*;

    /// Three interleaved EMA-style recurrences with `mul_add` plus a plain
    /// multiply-add the compiler must not fuse: the shape of the scalar kernels.
    struct Recurrences<'a> {
        xs: &'a [f64],
    }

    impl Kernel for Recurrences<'_> {
        type Output = Vec<f64>;

        #[inline(always)]
        fn run<S: Simd>(self, _simd: S) -> Vec<f64> {
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

    /// Every lane operation over one batch of inputs, the results concatenated.
    struct AllOps<'a> {
        a: &'a [f64],
        b: &'a [f64],
    }

    impl Kernel for AllOps<'_> {
        type Output = Vec<f64>;

        #[inline(always)]
        fn run<S: Simd>(self, s: S) -> Vec<f64> {
            let mut out = Vec::new();
            for (ca, cb) in self.a.chunks_exact(4).zip(self.b.chunks_exact(4)) {
                let x = s.load(ca.try_into().unwrap());
                let y = s.load(cb.try_into().unwrap());
                let z = s.splat(ca[0]);
                let mut stored = [0.0; 4];
                s.store(s.add(x, y), &mut stored);
                out.extend_from_slice(&stored);
                for v in [
                    s.sub(x, y),
                    s.mul(x, y),
                    s.div(x, y),
                    s.sqrt(s.abs(x)),
                    s.mul_add(x, y, z),
                    s.max(x, y),
                    s.min(x, y),
                    s.abs(x),
                    s.shift1(x),
                    s.shift2(x),
                    s.broadcast_last(x),
                    s.splat(s.last_lane(y)),
                    z,
                ] {
                    out.extend_from_slice(&s.to_array(v));
                }
                for v in s.transpose4(x, y, z, s.sub(x, y)) {
                    out.extend_from_slice(&s.to_array(v));
                }
            }
            out
        }
    }

    fn bits(v: &[f64]) -> Vec<u64> {
        v.iter().map(|x| x.to_bits()).collect()
    }

    fn series() -> Vec<f64> {
        (0..4096)
            .map(|i| (f64::from(i) * 0.731).sin() * 1e3 + f64::from(i % 17))
            .collect()
    }

    #[test]
    fn dispatch_matches_the_baseline_bit_for_bit() {
        let xs = series();
        assert_eq!(
            bits(&dispatch(Recurrences { xs: &xs })),
            bits(&run_baseline(Recurrences { xs: &xs }))
        );
    }

    /// Each lane operation must give the same bits on every implementation,
    /// the awkward values included: signed zeros, infinities, `NaN` (where
    /// `max`/`min` must take the second operand), subnormals and a
    /// cancellation-heavy fused multiply-add.
    #[test]
    fn every_lane_operation_matches_the_portable_lanes() {
        let edge = [
            0.0,
            -0.0,
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::NAN,
            f64::MIN_POSITIVE / 8.0,
            1.0 + f64::EPSILON,
            -1.0,
        ];
        let mut a: Vec<f64> = series();
        let mut b: Vec<f64> = series().iter().rev().map(|x| x * 0.37 - 2.0).collect();
        for (i, &x) in edge.iter().enumerate() {
            for (j, &y) in edge.iter().enumerate() {
                a.extend_from_slice(&[x, y, 1.0 + f64::EPSILON, x]);
                b.extend_from_slice(&[
                    y,
                    x,
                    1.0 - f64::EPSILON,
                    f64::from(u8::try_from(i * 8 + j).unwrap()),
                ]);
            }
        }
        let fast = dispatch(AllOps { a: &a, b: &b });
        let base = run_baseline(AllOps { a: &a, b: &b });
        assert_eq!(fast.len(), base.len());
        for (k, (x, y)) in fast.iter().zip(&base).enumerate() {
            assert!(
                x.to_bits() == y.to_bits() || (x.is_nan() && y.is_nan()),
                "lane {k}: {x:e} vs {y:e}"
            );
        }
    }

    #[test]
    fn portable_lanes_follow_the_documented_rules() {
        let s = Portable;
        let x = [1.0, f64::NAN, -0.0, 3.0];
        let y = [2.0, 5.0, 0.0, f64::NAN];
        assert_eq!(bits(&s.max(x, y)), bits(&[2.0, 5.0, 0.0, f64::NAN]));
        assert_eq!(bits(&s.min(x, y)), bits(&[1.0, 5.0, 0.0, f64::NAN]));
        assert_eq!(s.shift1([1.0, 2.0, 3.0, 4.0]), [0.0, 1.0, 2.0, 3.0]);
        assert_eq!(s.shift2([1.0, 2.0, 3.0, 4.0]), [0.0, 0.0, 1.0, 2.0]);
        assert_eq!(s.broadcast_last([1.0, 2.0, 3.0, 4.0]), [4.0; 4]);
        assert_eq!(
            bits(&s.abs([-0.0, -2.0, 2.0, -f64::INFINITY])),
            bits(&[0.0, 2.0, 2.0, f64::INFINITY])
        );
        assert_eq!(
            s.transpose4(
                [1.0, 2.0, 3.0, 4.0],
                [5.0, 6.0, 7.0, 8.0],
                [9.0, 10.0, 11.0, 12.0],
                [13.0, 14.0, 15.0, 16.0]
            ),
            [
                [1.0, 5.0, 9.0, 13.0],
                [2.0, 6.0, 10.0, 14.0],
                [3.0, 7.0, 11.0, 15.0],
                [4.0, 8.0, 12.0, 16.0]
            ]
        );
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
