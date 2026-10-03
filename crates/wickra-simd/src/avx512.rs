//! The AVX2 lanes plus eight lanes in an AVX-512 `__m512d`.
//!
//! A kernel that opts in ([`crate::Kernel::WIDE`]) runs here on a CPU with
//! AVX-512F: its four-lane work is exactly the [`Avx2`] token's, its
//! element-wise eight-lane work one 512-bit instruction where AVX2 takes two.
//! An [`Avx512`] token is only created by [`run`], which the dispatcher calls
//! after `level()` found AVX-512F -- and with it AVX2 and FMA -- on this CPU;
//! that check is the safety argument for every block here.
#![cfg(all(target_arch = "x86_64", wickra_avx512))]
#![allow(unsafe_code)]

use std::arch::x86_64::{
    __m256d, __m512d, _mm512_add_pd, _mm512_cmp_pd_mask, _mm512_div_pd, _mm512_loadu_pd,
    _mm512_mask_blend_pd, _mm512_max_pd, _mm512_mul_pd, _mm512_set1_pd, _mm512_setzero_pd,
    _mm512_sqrt_pd, _mm512_storeu_pd, _mm512_sub_pd, _CMP_GT_OQ,
};

use crate::{Avx2, Kernel, Simd};

/// Proof that this CPU implements AVX-512F (and so AVX2 and FMA); only
/// [`crate::dispatch`] makes one.
#[derive(Debug, Clone, Copy)]
pub struct Avx512(());

/// Run `kernel` with an [`Avx512`] token inside a function compiled for AVX2,
/// FMA and AVX-512F. Only called by `dispatch`, after the CPU check.
#[inline]
pub(crate) fn run<K: Kernel>(kernel: K) -> K::Output {
    debug_assert_eq!(crate::level(), crate::Level::Avx512);
    // SAFETY: `dispatch` calls this only when `level()` reported AVX-512F,
    // which every CPU pairs with AVX2 and FMA.
    unsafe { run_avx512(kernel) }
}

/// # Safety
///
/// The CPU must support AVX2, FMA and AVX-512F.
#[target_feature(enable = "avx2,fma,avx512f")]
unsafe fn run_avx512<K: Kernel>(kernel: K) -> K::Output {
    kernel.run(Avx512(()))
}

/// The AVX2 token the AVX-512 check implies.
#[allow(clippy::inline_always)]
#[inline(always)]
const fn lanes() -> Avx2 {
    Avx2::implied()
}

// Every method must inline into the kernel that calls it for the kernel to be
// compiled as one unit; see `crate::Kernel`. Each `unsafe` block calls an
// intrinsic whose only precondition is the CPU feature the token proves.
#[allow(clippy::inline_always)]
impl Simd for Avx512 {
    type V = __m256d;

    #[inline(always)]
    fn splat(self, x: f64) -> __m256d {
        lanes().splat(x)
    }
    #[inline(always)]
    fn load(self, a: &[f64; 4]) -> __m256d {
        lanes().load(a)
    }
    #[inline(always)]
    fn store(self, v: __m256d, a: &mut [f64; 4]) {
        lanes().store(v, a);
    }
    #[inline(always)]
    fn to_array(self, v: __m256d) -> [f64; 4] {
        lanes().to_array(v)
    }
    #[inline(always)]
    fn add(self, a: __m256d, b: __m256d) -> __m256d {
        lanes().add(a, b)
    }
    #[inline(always)]
    fn sub(self, a: __m256d, b: __m256d) -> __m256d {
        lanes().sub(a, b)
    }
    #[inline(always)]
    fn mul(self, a: __m256d, b: __m256d) -> __m256d {
        lanes().mul(a, b)
    }
    #[inline(always)]
    fn div(self, a: __m256d, b: __m256d) -> __m256d {
        lanes().div(a, b)
    }
    #[inline(always)]
    fn sqrt(self, a: __m256d) -> __m256d {
        lanes().sqrt(a)
    }
    #[inline(always)]
    fn mul_add(self, a: __m256d, b: __m256d, c: __m256d) -> __m256d {
        lanes().mul_add(a, b, c)
    }
    #[inline(always)]
    fn max(self, a: __m256d, b: __m256d) -> __m256d {
        lanes().max(a, b)
    }
    #[inline(always)]
    fn min(self, a: __m256d, b: __m256d) -> __m256d {
        lanes().min(a, b)
    }
    #[inline(always)]
    fn abs(self, a: __m256d) -> __m256d {
        lanes().abs(a)
    }
    #[inline(always)]
    fn select_positive(self, test: __m256d, yes: __m256d, no: __m256d) -> __m256d {
        lanes().select_positive(test, yes, no)
    }
    #[inline(always)]
    fn shift1(self, v: __m256d) -> __m256d {
        lanes().shift1(v)
    }
    #[inline(always)]
    fn shift2(self, v: __m256d) -> __m256d {
        lanes().shift2(v)
    }
    #[inline(always)]
    fn broadcast_last(self, v: __m256d) -> __m256d {
        lanes().broadcast_last(v)
    }
    #[inline(always)]
    fn last_lane(self, v: __m256d) -> f64 {
        lanes().last_lane(v)
    }
    #[inline(always)]
    fn transpose4(self, a: __m256d, b: __m256d, c: __m256d, d: __m256d) -> [__m256d; 4] {
        lanes().transpose4(a, b, c, d)
    }

    type W = __m512d;

    #[inline(always)]
    fn load8(self, a: &[f64; 8]) -> __m512d {
        unsafe { _mm512_loadu_pd(a.as_ptr()) }
    }
    #[inline(always)]
    fn store8(self, v: __m512d, a: &mut [f64; 8]) {
        unsafe { _mm512_storeu_pd(a.as_mut_ptr(), v) }
    }
    #[inline(always)]
    fn splat8(self, x: f64) -> __m512d {
        unsafe { _mm512_set1_pd(x) }
    }
    #[inline(always)]
    fn add8(self, a: __m512d, b: __m512d) -> __m512d {
        unsafe { _mm512_add_pd(a, b) }
    }
    #[inline(always)]
    fn sub8(self, a: __m512d, b: __m512d) -> __m512d {
        unsafe { _mm512_sub_pd(a, b) }
    }
    #[inline(always)]
    fn mul8(self, a: __m512d, b: __m512d) -> __m512d {
        unsafe { _mm512_mul_pd(a, b) }
    }
    #[inline(always)]
    fn div8(self, a: __m512d, b: __m512d) -> __m512d {
        unsafe { _mm512_div_pd(a, b) }
    }
    #[inline(always)]
    fn sqrt8(self, a: __m512d) -> __m512d {
        unsafe { _mm512_sqrt_pd(a) }
    }
    #[inline(always)]
    fn max8(self, a: __m512d, b: __m512d) -> __m512d {
        // `VMAXPD` returns the second operand on a tie or a `NaN`, as `MAXPD`.
        unsafe { _mm512_max_pd(a, b) }
    }
    #[inline(always)]
    fn select_positive8(self, test: __m512d, yes: __m512d, no: __m512d) -> __m512d {
        // Ordered greater-than: a `NaN` lane compares false and keeps `no`.
        unsafe {
            let positive = _mm512_cmp_pd_mask::<_CMP_GT_OQ>(test, _mm512_setzero_pd());
            _mm512_mask_blend_pd(positive, no, yes)
        }
    }
}
