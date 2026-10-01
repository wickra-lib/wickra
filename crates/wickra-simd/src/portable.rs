//! Four lanes as a plain array: available on every target.

// Every function here must inline into the kernel that calls it for the
// kernel to be compiled as one unit; see `crate::Kernel`.
#![allow(clippy::inline_always)]

use crate::Simd;

/// Lanes as `[f64; 4]` in ordinary Rust arithmetic.
///
/// Available everywhere; on `aarch64` the compiler maps the lane loops onto
/// NEON registers. Each method performs exactly the IEEE-754 operation the
/// [`Simd`] contract names, lane by lane.
#[derive(Debug, Clone, Copy, Default)]
pub struct Portable;

#[inline(always)]
fn map(a: [f64; 4], f: impl Fn(f64) -> f64) -> [f64; 4] {
    [f(a[0]), f(a[1]), f(a[2]), f(a[3])]
}

#[inline(always)]
fn zip(a: [f64; 4], b: [f64; 4], f: impl Fn(f64, f64) -> f64) -> [f64; 4] {
    [f(a[0], b[0]), f(a[1], b[1]), f(a[2], b[2]), f(a[3], b[3])]
}

impl Simd for Portable {
    type V = [f64; 4];

    #[inline(always)]
    fn splat(self, x: f64) -> [f64; 4] {
        [x; 4]
    }
    #[inline(always)]
    fn load(self, a: &[f64; 4]) -> [f64; 4] {
        *a
    }
    #[inline(always)]
    fn store(self, v: [f64; 4], a: &mut [f64; 4]) {
        *a = v;
    }
    #[inline(always)]
    fn to_array(self, v: [f64; 4]) -> [f64; 4] {
        v
    }
    #[inline(always)]
    fn add(self, a: [f64; 4], b: [f64; 4]) -> [f64; 4] {
        zip(a, b, |x, y| x + y)
    }
    #[inline(always)]
    fn sub(self, a: [f64; 4], b: [f64; 4]) -> [f64; 4] {
        zip(a, b, |x, y| x - y)
    }
    #[inline(always)]
    fn mul(self, a: [f64; 4], b: [f64; 4]) -> [f64; 4] {
        zip(a, b, |x, y| x * y)
    }
    #[inline(always)]
    fn div(self, a: [f64; 4], b: [f64; 4]) -> [f64; 4] {
        zip(a, b, |x, y| x / y)
    }
    #[inline(always)]
    fn sqrt(self, a: [f64; 4]) -> [f64; 4] {
        map(a, f64::sqrt)
    }
    #[inline(always)]
    fn mul_add(self, a: [f64; 4], b: [f64; 4], c: [f64; 4]) -> [f64; 4] {
        [
            a[0].mul_add(b[0], c[0]),
            a[1].mul_add(b[1], c[1]),
            a[2].mul_add(b[2], c[2]),
            a[3].mul_add(b[3], c[3]),
        ]
    }
    #[inline(always)]
    fn max(self, a: [f64; 4], b: [f64; 4]) -> [f64; 4] {
        zip(a, b, |x, y| if x > y { x } else { y })
    }
    #[inline(always)]
    fn min(self, a: [f64; 4], b: [f64; 4]) -> [f64; 4] {
        zip(a, b, |x, y| if x < y { x } else { y })
    }
    #[inline(always)]
    fn abs(self, a: [f64; 4]) -> [f64; 4] {
        map(a, f64::abs)
    }
    #[inline(always)]
    fn select_positive(self, test: [f64; 4], yes: [f64; 4], no: [f64; 4]) -> [f64; 4] {
        let pick = |k: usize| if test[k] > 0.0 { yes[k] } else { no[k] };
        [pick(0), pick(1), pick(2), pick(3)]
    }
    #[inline(always)]
    fn shift1(self, v: [f64; 4]) -> [f64; 4] {
        [0.0, v[0], v[1], v[2]]
    }
    #[inline(always)]
    fn shift2(self, v: [f64; 4]) -> [f64; 4] {
        [0.0, 0.0, v[0], v[1]]
    }
    #[inline(always)]
    fn broadcast_last(self, v: [f64; 4]) -> [f64; 4] {
        [v[3]; 4]
    }
    #[inline(always)]
    fn last_lane(self, v: [f64; 4]) -> f64 {
        v[3]
    }
    #[inline(always)]
    fn transpose4(self, a: [f64; 4], b: [f64; 4], c: [f64; 4], d: [f64; 4]) -> [[f64; 4]; 4] {
        [
            [a[0], b[0], c[0], d[0]],
            [a[1], b[1], c[1], d[1]],
            [a[2], b[2], c[2], d[2]],
            [a[3], b[3], c[3], d[3]],
        ]
    }
}
