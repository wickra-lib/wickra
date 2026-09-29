//! Building blocks of the opt-in `batch_fast` kernels.
//!
//! A `batch_fast` kernel may reassociate an indicator's arithmetic so it runs
//! in SIMD lanes; every value then agrees with the exact batch to within a few
//! units in the last place instead of bit for bit. What stays exact is the
//! *shape*: warmup positions, `NaN` placement and the output length match the
//! exact batch. And the kernels are deterministic across platforms: they are
//! written once against [`wickra_simd::Simd`], whose lane operations are exact
//! per lane, so the AVX2 build and the portable build (every other CPU,
//! including NEON on `aarch64`) return the same bits.
//!
//! Two scans do the heavy lifting:
//!
//! - [`lin_scan`] evaluates a first-order linear recurrence
//!   `y[i] = c * y[i - 1] + a * x[i]` — an EMA, a Wilder average — eight values
//!   per step: a Hillis–Steele scan inside each four-lane vector, the carry
//!   applied through precomputed powers of `c`.
//! - [`window_sums`] produces rolling window sums as a prefix scan of
//!   `x[i] - x[i - period]`, re-anchored on an exact window sum every
//!   [`RESEED_EVERY`] · `period` values so rounding cannot accumulate over a
//!   long series (the same cadence the exact SMA reseeds on).
//!
//! Composite kernels (DEMA, TEMA, HMA, TRIMA …) chain the scans over blocks of
//! [`BLOCK`] values in stack buffers, carrying each stage's state from block to
//! block, so they allocate nothing.

// Every helper and kernel here must inline into the function `dispatch` runs
// it in, which is what compiles it with that function's instruction set; see
// `wickra_simd::Kernel`.
#![allow(clippy::inline_always)]

use wickra_simd::Simd;

/// Values beyond this magnitude send a `batch_fast` call to the exact batch.
///
/// Prices never come near it; it keeps every intermediate sum, square and
/// power far from overflow, so the kernels never have to handle an infinity
/// the exact path would have treated differently. A non-finite input fails the
/// comparison too.
pub(crate) const MAX_ABS: f64 = 1e100;

/// Re-anchor rolling sums on an exact window sum every `RESEED_EVERY * period`
/// values.
pub(crate) const RESEED_EVERY: usize = 16;

/// Values per block in the composite kernels' stack buffers.
pub(crate) const BLOCK: usize = 512;

/// Whether every input is finite and within [`MAX_ABS`].
pub(crate) fn in_range(inputs: &[f64]) -> bool {
    inputs.iter().all(|x| x.abs() <= MAX_ABS)
}

/// Four consecutive values as an array reference.
#[inline(always)]
fn quad(values: &[f64]) -> &[f64; 4] {
    values[..4].try_into().expect("a four-value window")
}

/// Four consecutive writable values as an array reference.
#[inline(always)]
fn quad_mut(values: &mut [f64]) -> &mut [f64; 4] {
    (&mut values[..4]).try_into().expect("a four-value window")
}

/// `out[i] = decay * y[i - 1] + gain * xs[i]` with `y[-1] = start`; returns the
/// last `y` (or `start` for an empty slice).
///
/// Eight values per step: inside each vector a two-step Hillis–Steele scan with
/// weights `decay` and `decay²`, the second vector continued from the first
/// through `decay¹..decay⁴`, and the carry from the previous step folded in
/// through `decay¹..decay⁸`. The tail of fewer than eight values runs as a
/// scalar recurrence.
#[inline(always)]
pub(crate) fn lin_scan<S: Simd>(
    simd: S,
    decay: f64,
    gain: f64,
    xs: &[f64],
    start: f64,
    out: &mut [f64],
) -> f64 {
    debug_assert_eq!(xs.len(), out.len());
    let decay2 = decay * decay;
    let decay4 = decay2 * decay2;
    let decay8 = decay4 * decay4;
    let decay_v = simd.splat(decay);
    let decay2_v = simd.splat(decay2);
    let powers = simd.load(&[decay, decay2, decay * decay2, decay4]);
    let powers_hi = simd.mul(powers, simd.splat(decay4));
    let gain_v = simd.splat(gain);
    // The carry is kept as a scalar: `carry * decay⁸ + hi[3]` is exactly the
    // fused multiply-add the vector's last lane performs, but as a scalar it is
    // the only thing on the loop-carried path (the vector outputs hang off it).
    let mut carry = start;
    let head = xs.len() / 8 * 8;
    let (xs_head, xs_tail) = xs.split_at(head);
    let (out_head, out_tail) = out.split_at_mut(head);
    for (chunk, dest) in xs_head.chunks_exact(8).zip(out_head.chunks_exact_mut(8)) {
        let mut lo = simd.mul(gain_v, simd.load(quad(chunk)));
        let mut hi = simd.mul(gain_v, simd.load(quad(&chunk[4..])));
        lo = simd.mul_add(decay_v, simd.shift1(lo), lo);
        hi = simd.mul_add(decay_v, simd.shift1(hi), hi);
        lo = simd.mul_add(decay2_v, simd.shift2(lo), lo);
        hi = simd.mul_add(decay2_v, simd.shift2(hi), hi);
        hi = simd.mul_add(simd.broadcast_last(lo), powers, hi);
        let carry_v = simd.splat(carry);
        let (dest_lo, dest_hi) = dest.split_at_mut(4);
        simd.store(simd.mul_add(carry_v, powers, lo), quad_mut(dest_lo));
        simd.store(simd.mul_add(carry_v, powers_hi, hi), quad_mut(dest_hi));
        carry = carry.mul_add(decay8, simd.last_lane(hi));
    }
    let mut last = carry;
    for (slot, &value) in out_tail.iter_mut().zip(xs_tail) {
        last = decay.mul_add(last, gain * value);
        *slot = last;
    }
    last
}

/// For `idx` in `range`: `out[idx] = scale * sum[idx]` with
/// `sum[idx] = sum[idx - 1] + (xs[idx] - xs[idx - lag])` and
/// `sum[range.start - 1] = start`. Returns the unscaled last sum (or `start`
/// for an empty range).
#[inline(always)]
fn diff_prefix<S: Simd>(
    simd: S,
    xs: &[f64],
    lag: usize,
    range: std::ops::Range<usize>,
    start: f64,
    scale: f64,
    out: &mut [f64],
) -> f64 {
    let scale_v = simd.splat(scale);
    // Scalar carry, as in `lin_scan`: `carry + local[3]` is the addition the
    // vector's last lane performs, kept off the vector shuffles.
    let mut acc = start;
    let mut idx = range.start;
    while idx + 8 <= range.end {
        let diff_lo = simd.sub(
            simd.load(quad(&xs[idx..])),
            simd.load(quad(&xs[idx - lag..])),
        );
        let diff_hi = simd.sub(
            simd.load(quad(&xs[idx + 4..])),
            simd.load(quad(&xs[idx + 4 - lag..])),
        );
        let mut local_lo = simd.add(diff_lo, simd.shift1(diff_lo));
        let mut local_hi = simd.add(diff_hi, simd.shift1(diff_hi));
        local_lo = simd.add(local_lo, simd.shift2(local_lo));
        local_hi = simd.add(local_hi, simd.shift2(local_hi));
        let sums_lo = simd.add(local_lo, simd.splat(acc));
        simd.store(simd.mul(sums_lo, scale_v), quad_mut(&mut out[idx..]));
        let mid = acc + simd.last_lane(local_lo);
        let sums_hi = simd.add(local_hi, simd.splat(mid));
        simd.store(simd.mul(sums_hi, scale_v), quad_mut(&mut out[idx + 4..]));
        acc = mid + simd.last_lane(local_hi);
        idx += 8;
    }
    while idx < range.end {
        acc += xs[idx] - xs[idx - lag];
        out[idx] = acc * scale;
        idx += 1;
    }
    acc
}

/// Rolling sums of `period` values times `scale`: `out[idx]` for
/// `idx >= period - 1`. Every [`RESEED_EVERY`] · `period` values the sum is
/// recomputed exactly (in input order) from the window it covers. Returns the
/// unscaled last window sum; `out[..period - 1]` is left untouched.
///
/// Requires `xs.len() >= period >= 1`.
#[inline(always)]
pub(crate) fn window_sums<S: Simd>(
    simd: S,
    xs: &[f64],
    period: usize,
    scale: f64,
    out: &mut [f64],
) -> f64 {
    let len = xs.len();
    let segment = RESEED_EVERY * period;
    let mut anchor = period - 1;
    let mut last = 0.0;
    while anchor < len {
        let end = (anchor + segment).min(len);
        let exact: f64 = xs[anchor + 1 - period..=anchor].iter().sum();
        out[anchor] = exact * scale;
        last = diff_prefix(simd, xs, period, anchor + 1..end, exact, scale, out);
        anchor = end;
    }
    last
}

/// Weighted-moving-average numerators `Σ (k + 1) · xs[idx - period + 1 + k]`
/// times `scale`, for `idx >= period - 1`, from the window sums already in
/// `out` (`out[idx]` holding the unscaled sum ending at `idx`), overwriting
/// them.
///
/// `N[idx] = N[idx - 1] + period · xs[idx] - S[idx - 1]`, re-anchored on the
/// exact numerator at the same cadence as the sums. Returns the unscaled last
/// numerator.
#[inline(always)]
fn wma_numerators<S: Simd>(simd: S, xs: &[f64], period: usize, scale: f64, out: &mut [f64]) -> f64 {
    let len = xs.len();
    let segment = RESEED_EVERY * period;
    let weight = period as f64;
    let weight_v = simd.splat(weight);
    let scale_v = simd.splat(scale);
    let mut anchor = period - 1;
    let mut last = 0.0;
    while anchor < len {
        let end = (anchor + segment).min(len);
        let exact: f64 = xs[anchor + 1 - period..=anchor]
            .iter()
            .enumerate()
            .map(|(k, v)| (k as f64 + 1.0) * v)
            .sum();
        // `out[anchor]` still holds S[anchor]: the first "previous sum".
        let mut prev_sum = out[anchor];
        out[anchor] = exact * scale;
        // Scalar carry, as in `diff_prefix`.
        let mut acc = exact;
        let mut idx = anchor + 1;
        while idx + 4 <= end {
            let sums = simd.load(quad(&out[idx..]));
            let prev = simd.add(simd.shift1(sums), simd.load(&[prev_sum, 0.0, 0.0, 0.0]));
            prev_sum = simd.last_lane(sums);
            let step = simd.sub(simd.mul(weight_v, simd.load(quad(&xs[idx..]))), prev);
            let mut local = simd.add(step, simd.shift1(step));
            local = simd.add(local, simd.shift2(local));
            let numerators = simd.add(local, simd.splat(acc));
            simd.store(simd.mul(numerators, scale_v), quad_mut(&mut out[idx..]));
            acc += simd.last_lane(local);
            idx += 4;
        }
        while idx < end {
            let sum_here = out[idx];
            acc += weight * xs[idx] - prev_sum;
            prev_sum = sum_here;
            out[idx] = acc * scale;
            idx += 1;
        }
        last = acc;
        anchor = end;
    }
    last
}

/// Weighted moving average of `period` over `xs` into `out[period - 1..]`
/// (weights `1..=period`, newest heaviest). `out[..period - 1]` is left
/// untouched. Requires `xs.len() >= period`.
#[inline(always)]
pub(crate) fn wma<S: Simd>(simd: S, xs: &[f64], period: usize, out: &mut [f64]) {
    let weight = period as f64;
    window_sums(simd, xs, period, 1.0, out);
    wma_numerators(simd, xs, period, 1.0 / (weight * (weight + 1.0) / 2.0), out);
}

thread_local! {
    /// Per-thread scratch space for kernels that need a full-length temporary
    /// (the FIR composites). Reused across calls, so a batch pays for its
    /// pages once per thread instead of on every call.
    static SCRATCH: std::cell::RefCell<Vec<f64>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// Run `f` with a scratch slice of `len` values (contents unspecified). The
/// kernels that use it never nest, so the per-thread buffer is never borrowed
/// twice.
pub(crate) fn with_scratch<R>(len: usize, f: impl FnOnce(&mut [f64]) -> R) -> R {
    SCRATCH.with(|cell| {
        let mut buf = cell.borrow_mut();
        if buf.len() < len {
            buf.resize(len, 0.0);
        }
        f(&mut buf[..len])
    })
}

/// Reset `indicator` and replay `tail` through `update`. For an indicator
/// whose state depends only on its last `tail.len()` inputs (a finite window),
/// this leaves exactly the state a full replay would.
pub(crate) fn replay_tail<I: crate::traits::Indicator<Input = f64>>(
    indicator: &mut I,
    tail: &[f64],
) {
    indicator.reset();
    for &x in tail {
        let _ = indicator.update(x);
    }
}

/// Declare a `batch_fast` kernel: a struct of borrowed inputs and a
/// [`wickra_simd::Kernel`] impl whose body runs with the dispatched lanes.
macro_rules! kernel {
    (
        $(#[$meta:meta])*
        $name:ident { $($field:ident: $ty:ty),* $(,)? } -> $ret:ty = |$simd:ident, $k:ident| $body:block
    ) => {
        $(#[$meta])*
        pub(crate) struct $name<'a> {
            $(pub(crate) $field: $ty,)*
            pub(crate) _borrow: std::marker::PhantomData<&'a ()>,
        }

        // Inlining into the dispatching function is what compiles the body
        // with its features; see `wickra_simd::Kernel`.
        #[allow(clippy::inline_always)]
        impl wickra_simd::Kernel for $name<'_> {
            type Output = $ret;

            #[inline(always)]
            fn run<S: Simd>(self, $simd: S) -> $ret {
                let $k = self;
                $body
            }
        }
    };
}

kernel! {
    /// SMA: rolling sums scaled by `1 / period`.
    SmaFast { x: &'a [f64], period: usize, out: &'a mut [f64] } -> () = |simd, kern| {
        let period = kern.period;
        kern.out[..period - 1].fill(f64::NAN);
        window_sums(simd, kern.x, period, 1.0 / period as f64, kern.out);
    }
}

kernel! {
    /// EMA: the seed mean (summed from `-0.0` in input order, as the streaming
    /// EMA does), then the recurrence as a linear-recurrence scan. Returns the
    /// last EMA and the seed sum.
    EmaFast {
        x: &'a [f64],
        period: usize,
        alpha: f64,
        one_minus_alpha: f64,
        out: &'a mut [f64],
    } -> (f64, f64) = |simd, kern| {
        let period = kern.period;
        let seed_sum = kern.x[..period].iter().copied().sum::<f64>();
        let seed = seed_sum / period as f64;
        kern.out[..period - 1].fill(f64::NAN);
        kern.out[period - 1] = seed;
        let last = lin_scan(
            simd,
            kern.one_minus_alpha,
            kern.alpha,
            &kern.x[period..],
            seed,
            &mut kern.out[period..],
        );
        (last, seed_sum)
    }
}

kernel! {
    /// WMA: window sums, then the numerator scan, scaled by `1 / Σ weights`.
    WmaFast { x: &'a [f64], period: usize, out: &'a mut [f64] } -> () = |simd, kern| {
        kern.out[..kern.period - 1].fill(f64::NAN);
        wma(simd, kern.x, kern.period, kern.out);
    }
}

kernel! {
    /// HMA: `WMA(2 · WMA(half) − WMA(full), smooth)`, the inner half WMA and
    /// the difference series in `tmp`.
    HmaFast {
        x: &'a [f64],
        half: usize,
        full: usize,
        smooth: usize,
        tmp: &'a mut [f64],
        out: &'a mut [f64],
    } -> () = |simd, kern| {
        let first = kern.full - 1;
        wma(simd, kern.x, kern.full, kern.out);
        wma(simd, kern.x, kern.half, kern.tmp);
        for (diff, &full) in kern.tmp[first..].iter_mut().zip(&kern.out[first..]) {
            *diff = 2.0 * *diff - full;
        }
        wma(simd, &kern.tmp[first..], kern.smooth, &mut kern.out[first..]);
        kern.out[..first + kern.smooth - 1].fill(f64::NAN);
    }
}

kernel! {
    /// TRIMA: an SMA of an SMA, the inner one in `tmp`.
    TrimaFast {
        x: &'a [f64],
        inner: usize,
        outer: usize,
        tmp: &'a mut [f64],
        out: &'a mut [f64],
    } -> () = |simd, kern| {
        let first = kern.inner - 1;
        window_sums(simd, kern.x, kern.inner, 1.0 / kern.inner as f64, kern.tmp);
        window_sums(
            simd,
            &kern.tmp[first..],
            kern.outer,
            1.0 / kern.outer as f64,
            &mut kern.out[first..],
        );
        kern.out[..first + kern.outer - 1].fill(f64::NAN);
    }
}

kernel! {
    /// SMMA (Wilder's smoothing): the seed mean, then
    /// `y = ((period − 1) / period) · y + x / period` as a scan. Returns the
    /// last value.
    SmmaFast { x: &'a [f64], period: usize, seed_sum: f64, out: &'a mut [f64] } -> f64 = |simd, kern| {
        let period = kern.period;
        let weight = period as f64;
        let seed = kern.seed_sum / weight;
        kern.out[..period - 1].fill(f64::NAN);
        kern.out[period - 1] = seed;
        lin_scan(
            simd,
            (weight - 1.0) / weight,
            1.0 / weight,
            &kern.x[period..],
            seed,
            &mut kern.out[period..],
        )
    }
}

/// One EMA step, exactly as `Ema::update` computes it.
#[inline(always)]
fn ema_step(alpha: f64, one_minus_alpha: f64, prev: f64, value: f64) -> f64 {
    alpha.mul_add(value, one_minus_alpha * prev)
}

kernel! {
    /// DEMA `2 · e1 − e2`: the warmup until both EMAs are seeded runs the
    /// exact recurrence (so those first values match the exact batch to the
    /// bit), then blocks of both scans chained through stack buffers. Returns
    /// the last `e1` and `e2`.
    DemaFast { x: &'a [f64], period: usize, alpha: f64, out: &'a mut [f64] } -> (f64, f64) = |simd, kern| {
        let (period, alpha) = (kern.period, kern.alpha);
        let decay = 1.0 - alpha;
        let first = 2 * period - 2;
        let mut ema1 = kern.x[..period].iter().copied().sum::<f64>() / period as f64;
        let mut ema2_sum = -0.0 + ema1;
        for &value in &kern.x[period..=first] {
            ema1 = ema_step(alpha, decay, ema1, value);
            ema2_sum += ema1;
        }
        let mut ema2 = ema2_sum / period as f64;
        kern.out[..first].fill(f64::NAN);
        kern.out[first] = 2.0 * ema1 - ema2;
        let mut stage1 = [0.0; BLOCK];
        let mut stage2 = [0.0; BLOCK];
        let inputs = kern.x[first + 1..].chunks(BLOCK);
        for (block, dest) in inputs.zip(kern.out[first + 1..].chunks_mut(BLOCK)) {
            let len = block.len();
            ema1 = lin_scan(simd, decay, alpha, block, ema1, &mut stage1[..len]);
            ema2 = lin_scan(simd, decay, alpha, &stage1[..len], ema2, &mut stage2[..len]);
            for ((slot, &one), &two) in dest.iter_mut().zip(&stage1[..len]).zip(&stage2[..len]) {
                *slot = 2.0 * one - two;
            }
        }
        (ema1, ema2)
    }
}

kernel! {
    /// TEMA `3 · e1 − 3 · e2 + e3`, built like [`DemaFast`] with a third EMA.
    /// Returns the last `e1`, `e2` and `e3`.
    TemaFast { x: &'a [f64], period: usize, alpha: f64, out: &'a mut [f64] } -> (f64, f64, f64) = |simd, kern| {
        let (period, alpha) = (kern.period, kern.alpha);
        let decay = 1.0 - alpha;
        let (second_seed, first) = (2 * period - 2, 3 * period - 3);
        let mut ema1 = kern.x[..period].iter().copied().sum::<f64>() / period as f64;
        let mut ema2_sum = -0.0 + ema1;
        for &value in &kern.x[period..=second_seed] {
            ema1 = ema_step(alpha, decay, ema1, value);
            ema2_sum += ema1;
        }
        let mut ema2 = ema2_sum / period as f64;
        let mut ema3_sum = -0.0 + ema2;
        for &value in &kern.x[second_seed + 1..=first] {
            ema1 = ema_step(alpha, decay, ema1, value);
            ema2 = ema_step(alpha, decay, ema2, ema1);
            ema3_sum += ema2;
        }
        let mut ema3 = ema3_sum / period as f64;
        kern.out[..first].fill(f64::NAN);
        kern.out[first] = 3.0 * ema1 - 3.0 * ema2 + ema3;
        let mut stage1 = [0.0; BLOCK];
        let mut stage2 = [0.0; BLOCK];
        let mut stage3 = [0.0; BLOCK];
        let inputs = kern.x[first + 1..].chunks(BLOCK);
        for (block, dest) in inputs.zip(kern.out[first + 1..].chunks_mut(BLOCK)) {
            let len = block.len();
            ema1 = lin_scan(simd, decay, alpha, block, ema1, &mut stage1[..len]);
            ema2 = lin_scan(simd, decay, alpha, &stage1[..len], ema2, &mut stage2[..len]);
            ema3 = lin_scan(simd, decay, alpha, &stage2[..len], ema3, &mut stage3[..len]);
            let stages = stage1[..len].iter().zip(&stage2[..len]).zip(&stage3[..len]);
            for (slot, ((&one, &two), &three)) in dest.iter_mut().zip(stages) {
                *slot = 3.0 * one - 3.0 * two + three;
            }
        }
        (ema1, ema2, ema3)
    }
}

kernel! {
    /// RSI: the exact seed over the first `period` changes, then blocks of
    /// gains and losses smoothed by two Wilder scans and combined as
    /// `100 · ag / (ag + al)` (50 when both are zero). Returns the averages.
    RsiFast {
        x: &'a [f64],
        period: usize,
        avg_gain: f64,
        avg_loss: f64,
        n_minus_1: f64,
        inv_period: f64,
        out: &'a mut [f64],
    } -> (f64, f64) = |simd, kern| {
        let (decay, gain) = (kern.n_minus_1 * kern.inv_period, kern.inv_period);
        let (mut avg_gain, mut avg_loss) = (kern.avg_gain, kern.avg_loss);
        let mut gains = [0.0; BLOCK];
        let mut losses = [0.0; BLOCK];
        let mut smoothed_gains = [0.0; BLOCK];
        let mut smoothed_losses = [0.0; BLOCK];
        let start = kern.period + 1;
        let mut pos = start;
        for dest in kern.out[start..].chunks_mut(BLOCK) {
            let len = dest.len();
            let changes = kern.x[pos..pos + len].iter().zip(&kern.x[pos - 1..pos + len - 1]);
            for ((up, down), (&now, &before)) in gains.iter_mut().zip(losses.iter_mut()).zip(changes) {
                let diff = now - before;
                *up = if diff > 0.0 { diff } else { 0.0 };
                *down = if diff < 0.0 { -diff } else { 0.0 };
            }
            avg_gain = lin_scan(simd, decay, gain, &gains[..len], avg_gain, &mut smoothed_gains[..len]);
            avg_loss = lin_scan(simd, decay, gain, &losses[..len], avg_loss, &mut smoothed_losses[..len]);
            let smoothed = smoothed_gains[..len].iter().zip(&smoothed_losses[..len]);
            for (slot, (&up, &down)) in dest.iter_mut().zip(smoothed) {
                let denom = up + down;
                *slot = if denom == 0.0 { 50.0 } else { 100.0 * up / denom };
            }
            pos += len;
        }
        (avg_gain, avg_loss)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::indicators::{Dema, Ema, Hma, Rsi, Sma, Smma, Tema, Trima, Wma};
    use crate::traits::{BatchNanExt, Indicator};
    use std::marker::PhantomData;

    /// A wandering price path; its length is neither a multiple of four nor of
    /// eight and exceeds `RESEED_EVERY * period` for every period tested, so
    /// every scan's vector loop, scalar tail and re-anchoring run.
    fn series(n: usize) -> Vec<f64> {
        (0..n)
            .map(|i| {
                let t = f64::from(u32::try_from(i).unwrap());
                100.0 + (t * 0.0137).sin() * 5.0 + (t * 0.37).cos() + (t * 0.0011).sin() * 20.0
            })
            .collect()
    }

    fn bits(v: &[f64]) -> Vec<u64> {
        v.iter().map(|x| x.to_bits()).collect()
    }

    /// The fast batch must put `NaN` exactly where the exact batch does, agree
    /// with it to `tol` (relative, against a unit floor) everywhere else, and
    /// leave a state whose next updates agree to the same tolerance.
    fn agrees<I>(ind: &I, xs: &[f64], tol: f64)
    where
        I: Indicator<Input = f64, Output = f64> + Clone,
    {
        let (mut exact, mut fast) = (ind.clone(), ind.clone());
        let a = exact.batch_nan(xs);
        let b = fast.batch_fast(xs);
        assert_eq!(a.len(), b.len());
        for (i, (x, y)) in a.iter().zip(&b).enumerate() {
            assert_eq!(x.is_nan(), y.is_nan(), "{} NaN mismatch at {i}", ind.name());
            if x.is_finite() {
                let err = (x - y).abs() / x.abs().max(1.0);
                assert!(err <= tol, "{} at {i}: {x} vs {y}", ind.name());
            }
        }
        for &x in &[101.5, 99.25, 100.75, 104.0] {
            let (u, v) = (exact.update(x).unwrap(), fast.update(x).unwrap());
            assert!(
                (u - v).abs() / u.abs().max(1.0) <= tol,
                "{} continuation",
                ind.name()
            );
        }
    }

    /// Where the kernel does not apply, `batch_fast` must be the exact batch.
    fn falls_back<I>(ind: &I, xs: &[f64])
    where
        I: Indicator<Input = f64, Output = f64> + Clone,
    {
        assert_eq!(
            bits(&ind.clone().batch_fast(xs)),
            bits(&ind.clone().batch_nan(xs)),
            "{}",
            ind.name()
        );
    }

    fn every_path<I>(make: impl Fn(usize) -> I, periods: &[usize], min_len: impl Fn(usize) -> usize)
    where
        I: Indicator<Input = f64, Output = f64> + Clone,
    {
        let xs = series(4003);
        for &p in periods {
            agrees(&make(p), &xs, 1e-11);
            // Not fresh: one update first.
            let mut warm = make(p);
            let _ = warm.update(100.0);
            falls_back(&warm, &xs);
            // A value out of range, and one non-finite value.
            let mut wild = xs[..600].to_vec();
            wild[300] = 1e200;
            falls_back(&make(p), &wild);
            wild[300] = f64::NAN;
            falls_back(&make(p), &wild);
            // Too short for the kernel.
            falls_back(&make(p), &xs[..min_len(p) - 1]);
        }
    }

    #[test]
    fn scalar_kernels_agree_with_the_exact_batch_and_fall_back_where_they_must() {
        let periods = [1, 2, 7, 14, 33];
        every_path(|p| Sma::new(p).unwrap(), &periods, |p| p);
        every_path(|p| Ema::new(p).unwrap(), &periods, |p| p);
        every_path(|p| Wma::new(p).unwrap(), &periods, |p| p);
        every_path(|p| Smma::new(p).unwrap(), &periods, |p| p);
        every_path(|p| Dema::new(p).unwrap(), &periods, |p| 2 * p - 1);
        every_path(|p| Tema::new(p).unwrap(), &periods, |p| 3 * p - 2);
        every_path(|p| Rsi::new(p).unwrap(), &periods, |p| p + 1);
        every_path(
            |p| Hma::new(p).unwrap(),
            &periods,
            |p| p + ((p as f64).sqrt().round() as usize).max(1) - 1,
        );
        every_path(
            |p| Trima::new(p).unwrap(),
            &periods,
            |p| if p % 2 == 1 { p.div_ceil(2) * 2 - 1 } else { p },
        );
    }

    /// Every kernel returns the same bits through the dispatcher (AVX2 + FMA
    /// where the CPU has them) as in the portable baseline build: the
    /// determinism `batch_fast` promises across platforms.
    #[test]
    fn kernels_are_identical_on_every_dispatch_path() {
        let xs = series(3001);
        let n = xs.len();
        macro_rules! both {
            ($make:expr) => {{
                let (mut a, mut b) = (vec![0.0; n], vec![0.0; n]);
                let ra = wickra_simd::dispatch($make(&mut a));
                let rb = wickra_simd::run_baseline($make(&mut b));
                assert_eq!(bits(&a), bits(&b));
                (ra, rb)
            }};
        }
        let _ = both!(|out| SmaFast {
            x: &xs,
            period: 20,
            out,
            _borrow: PhantomData
        });
        let (ra, rb) = both!(|out| EmaFast {
            x: &xs,
            period: 20,
            alpha: 2.0 / 21.0,
            one_minus_alpha: 19.0 / 21.0,
            out,
            _borrow: PhantomData,
        });
        assert_eq!(bits(&[ra.0, ra.1]), bits(&[rb.0, rb.1]));
        let _ = both!(|out| WmaFast {
            x: &xs,
            period: 20,
            out,
            _borrow: PhantomData
        });
        let seed_sum = xs[..14].iter().sum();
        let (ra, rb) = both!(|out| SmmaFast {
            x: &xs,
            period: 14,
            seed_sum,
            out,
            _borrow: PhantomData
        });
        assert_eq!(ra.to_bits(), rb.to_bits());
        let (ra, rb) = both!(|out| DemaFast {
            x: &xs,
            period: 14,
            alpha: 2.0 / 15.0,
            out,
            _borrow: PhantomData
        });
        assert_eq!(bits(&[ra.0, ra.1]), bits(&[rb.0, rb.1]));
        let (ra, rb) = both!(|out| TemaFast {
            x: &xs,
            period: 14,
            alpha: 2.0 / 15.0,
            out,
            _borrow: PhantomData
        });
        assert_eq!(bits(&[ra.0, ra.1, ra.2]), bits(&[rb.0, rb.1, rb.2]));
        let (ra, rb) = both!(|out| RsiFast {
            x: &xs,
            period: 14,
            avg_gain: 0.4,
            avg_loss: 0.6,
            n_minus_1: 13.0,
            inv_period: 1.0 / 14.0,
            out,
            _borrow: PhantomData,
        });
        assert_eq!(bits(&[ra.0, ra.1]), bits(&[rb.0, rb.1]));
    }

    /// The FIR composites (HMA, TRIMA) through their scratch buffers: same
    /// bits on every dispatch path.
    #[test]
    fn composite_kernels_are_identical_on_every_dispatch_path() {
        let xs = series(3001);
        let n = xs.len();
        let (mut ta, mut tb) = (vec![0.0; n], vec![0.0; n]);
        let (mut a, mut b) = (vec![0.0; n], vec![0.0; n]);
        wickra_simd::dispatch(HmaFast {
            x: &xs,
            half: 10,
            full: 20,
            smooth: 4,
            tmp: &mut ta,
            out: &mut a,
            _borrow: PhantomData,
        });
        wickra_simd::run_baseline(HmaFast {
            x: &xs,
            half: 10,
            full: 20,
            smooth: 4,
            tmp: &mut tb,
            out: &mut b,
            _borrow: PhantomData,
        });
        assert_eq!(bits(&a), bits(&b));
        wickra_simd::dispatch(TrimaFast {
            x: &xs,
            inner: 10,
            outer: 11,
            tmp: &mut ta,
            out: &mut a,
            _borrow: PhantomData,
        });
        wickra_simd::run_baseline(TrimaFast {
            x: &xs,
            inner: 10,
            outer: 11,
            tmp: &mut tb,
            out: &mut b,
            _borrow: PhantomData,
        });
        assert_eq!(bits(&a), bits(&b));
    }

    /// The fast WMA re-anchors on exact window values, so over a long series
    /// it stays at the definition to a few ulps.
    #[test]
    fn fast_wma_stays_at_the_definition_on_a_long_series() {
        let xs = series(20_003);
        let p = 14;
        let got = Wma::new(p).unwrap().batch_fast(&xs);
        let total = (p * (p + 1) / 2) as f64;
        for i in (p - 1..xs.len()).step_by(97) {
            let def: f64 = xs[i + 1 - p..=i]
                .iter()
                .enumerate()
                .map(|(k, v)| (k as f64 + 1.0) * v)
                .sum::<f64>()
                / total;
            assert!(((got[i] - def) / def).abs() < 1e-13, "at {i}");
        }
    }

    #[test]
    fn scratch_grows_and_is_reused() {
        let first = with_scratch(8, |buf| {
            buf.fill(1.0);
            buf.len()
        });
        let second = with_scratch(16, |buf| buf.len());
        assert_eq!((first, second), (8, 16));
    }
}
