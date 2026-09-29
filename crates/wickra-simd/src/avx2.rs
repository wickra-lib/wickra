//! Four lanes in an AVX `__m256d`, with FMA. The only `unsafe` in the crate.
//!
//! Every intrinsic below requires AVX (and `_mm256_fmadd_pd` FMA) at run time.
//! An [`Avx2`] token can only be created by [`run`], which the dispatcher calls
//! after `level()` confirmed AVX2 and FMA on this CPU, and it cannot be built
//! anywhere else (its field is private). Holding the token therefore proves the
//! instructions exist, which is the whole safety argument for each block here.
#![cfg(target_arch = "x86_64")]
#![allow(unsafe_code)]

use std::arch::x86_64::{
    __m256d, _mm256_add_pd, _mm256_andnot_pd, _mm256_blend_pd, _mm256_div_pd,
    _mm256_extractf128_pd, _mm256_fmadd_pd, _mm256_loadu_pd, _mm256_max_pd, _mm256_min_pd,
    _mm256_mul_pd, _mm256_permute2f128_pd, _mm256_permute4x64_pd, _mm256_set1_pd,
    _mm256_setzero_pd, _mm256_sqrt_pd, _mm256_storeu_pd, _mm256_sub_pd, _mm256_unpackhi_pd,
    _mm256_unpacklo_pd, _mm_cvtsd_f64, _mm_unpackhi_pd,
};

use crate::{Kernel, Simd};

/// Proof that this CPU implements AVX2 and FMA; only [`crate::dispatch`] makes
/// one.
#[derive(Debug, Clone, Copy)]
pub struct Avx2(());

/// Run `kernel` with an [`Avx2`] token inside a function compiled for AVX2 and
/// FMA. Only called by `dispatch`, after the CPU check.
#[inline]
pub(crate) fn run<K: Kernel>(kernel: K) -> K::Output {
    debug_assert_eq!(crate::level(), crate::Level::Avx2Fma);
    // SAFETY: `dispatch` calls this only when `level()` reported AVX2 and FMA.
    unsafe { run_avx2_fma(kernel) }
}

/// # Safety
///
/// The CPU must support AVX2 and FMA.
#[target_feature(enable = "avx2,fma")]
unsafe fn run_avx2_fma<K: Kernel>(kernel: K) -> K::Output {
    kernel.run(Avx2(()))
}

// Every method must inline into the kernel that calls it for the kernel to be
// compiled as one unit; see `crate::Kernel`. Each `unsafe` block calls an
// intrinsic whose only precondition is the CPU feature the token proves.
#[allow(clippy::inline_always)]
impl Simd for Avx2 {
    type V = __m256d;

    #[inline(always)]
    fn splat(self, x: f64) -> __m256d {
        unsafe { _mm256_set1_pd(x) }
    }
    #[inline(always)]
    fn load(self, a: &[f64; 4]) -> __m256d {
        // SAFETY (pointer): `a` is four valid, initialised `f64`s; `loadu`
        // accepts any alignment.
        unsafe { _mm256_loadu_pd(a.as_ptr()) }
    }
    #[inline(always)]
    fn store(self, v: __m256d, a: &mut [f64; 4]) {
        // SAFETY (pointer): `a` is four writable `f64`s; `storeu` accepts any
        // alignment.
        unsafe { _mm256_storeu_pd(a.as_mut_ptr(), v) }
    }
    #[inline(always)]
    fn to_array(self, v: __m256d) -> [f64; 4] {
        let mut a = [0.0; 4];
        self.store(v, &mut a);
        a
    }
    #[inline(always)]
    fn add(self, a: __m256d, b: __m256d) -> __m256d {
        unsafe { _mm256_add_pd(a, b) }
    }
    #[inline(always)]
    fn sub(self, a: __m256d, b: __m256d) -> __m256d {
        unsafe { _mm256_sub_pd(a, b) }
    }
    #[inline(always)]
    fn mul(self, a: __m256d, b: __m256d) -> __m256d {
        unsafe { _mm256_mul_pd(a, b) }
    }
    #[inline(always)]
    fn div(self, a: __m256d, b: __m256d) -> __m256d {
        unsafe { _mm256_div_pd(a, b) }
    }
    #[inline(always)]
    fn sqrt(self, a: __m256d) -> __m256d {
        unsafe { _mm256_sqrt_pd(a) }
    }
    #[inline(always)]
    fn mul_add(self, a: __m256d, b: __m256d, c: __m256d) -> __m256d {
        unsafe { _mm256_fmadd_pd(a, b, c) }
    }
    #[inline(always)]
    fn max(self, a: __m256d, b: __m256d) -> __m256d {
        unsafe { _mm256_max_pd(a, b) }
    }
    #[inline(always)]
    fn min(self, a: __m256d, b: __m256d) -> __m256d {
        unsafe { _mm256_min_pd(a, b) }
    }
    #[inline(always)]
    fn abs(self, a: __m256d) -> __m256d {
        unsafe { _mm256_andnot_pd(_mm256_set1_pd(-0.0), a) }
    }
    #[inline(always)]
    fn shift1(self, v: __m256d) -> __m256d {
        // permute to [v0, v0, v1, v2], then blend lane 0 from zero.
        unsafe {
            _mm256_blend_pd::<0b0001>(
                _mm256_permute4x64_pd::<0b10_01_00_00>(v),
                _mm256_setzero_pd(),
            )
        }
    }
    #[inline(always)]
    fn shift2(self, v: __m256d) -> __m256d {
        // permute to [v0, v0, v0, v1], then blend lanes 0 and 1 from zero.
        unsafe {
            _mm256_blend_pd::<0b0011>(
                _mm256_permute4x64_pd::<0b01_00_00_00>(v),
                _mm256_setzero_pd(),
            )
        }
    }
    #[inline(always)]
    fn broadcast_last(self, v: __m256d) -> __m256d {
        unsafe { _mm256_permute4x64_pd::<0b11_11_11_11>(v) }
    }
    #[inline(always)]
    fn last_lane(self, v: __m256d) -> f64 {
        // upper 128-bit half [v2, v3], then its high element.
        unsafe {
            let hi = _mm256_extractf128_pd::<1>(v);
            _mm_cvtsd_f64(_mm_unpackhi_pd(hi, hi))
        }
    }
    #[inline(always)]
    fn transpose4(self, a: __m256d, b: __m256d, c: __m256d, d: __m256d) -> [__m256d; 4] {
        // Interleave pairs within each 128-bit half, then swap the halves:
        // [a0 b0 a2 b2], [a1 b1 a3 b3], [c0 d0 c2 d2], [c1 d1 c3 d3] →
        // [a0 b0 c0 d0], [a1 b1 c1 d1], [a2 b2 c2 d2], [a3 b3 c3 d3].
        unsafe {
            let ab_even = _mm256_unpacklo_pd(a, b);
            let ab_odd = _mm256_unpackhi_pd(a, b);
            let cd_even = _mm256_unpacklo_pd(c, d);
            let cd_odd = _mm256_unpackhi_pd(c, d);
            [
                _mm256_permute2f128_pd::<0x20>(ab_even, cd_even),
                _mm256_permute2f128_pd::<0x20>(ab_odd, cd_odd),
                _mm256_permute2f128_pd::<0x31>(ab_even, cd_even),
                _mm256_permute2f128_pd::<0x31>(ab_odd, cd_odd),
            ]
        }
    }
}
