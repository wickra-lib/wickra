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

/// Whether every input is finite and within [`MAX_ABS`] (`NaN` fails the
/// comparison).
///
/// Each block is folded without an early exit, which is what lets the check
/// vectorize: a short-circuiting scan compares and branches once per value, a
/// cost on the order of the kernels it guards.
pub(crate) fn in_range(inputs: &[f64]) -> bool {
    inputs
        .chunks(BLOCK)
        .all(|block| block.iter().fold(true, |ok, x| ok & (x.abs() <= MAX_ABS)))
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
///
/// Every step is a multiplication and a separate addition, not a fused
/// multiply-add: the result has to be the same on every platform, and
/// WebAssembly has no fused instruction, so there it would be emulated in
/// software -- slow enough to make the scan lose to the plain recurrence it
/// replaces. With hardware FMA the split costs a few percent.
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
    // multiply and add the vector's last lane performs, but as a scalar it is
    // the only thing on the loop-carried path (the vector outputs hang off it).
    let mut carry = start;
    let head = xs.len() / 8 * 8;
    let (xs_head, xs_tail) = xs.split_at(head);
    let (out_head, out_tail) = out.split_at_mut(head);
    for (chunk, dest) in xs_head.chunks_exact(8).zip(out_head.chunks_exact_mut(8)) {
        let mut lo = simd.mul(gain_v, simd.load(quad(chunk)));
        let mut hi = simd.mul(gain_v, simd.load(quad(&chunk[4..])));
        lo = simd.add(simd.mul(decay_v, simd.shift1(lo)), lo);
        hi = simd.add(simd.mul(decay_v, simd.shift1(hi)), hi);
        lo = simd.add(simd.mul(decay2_v, simd.shift2(lo)), lo);
        hi = simd.add(simd.mul(decay2_v, simd.shift2(hi)), hi);
        hi = simd.add(simd.mul(simd.broadcast_last(lo), powers), hi);
        let carry_v = simd.splat(carry);
        let (dest_lo, dest_hi) = dest.split_at_mut(4);
        simd.store(simd.add(simd.mul(carry_v, powers), lo), quad_mut(dest_lo));
        simd.store(
            simd.add(simd.mul(carry_v, powers_hi), hi),
            quad_mut(dest_hi),
        );
        carry = carry * decay8 + simd.last_lane(hi);
    }
    let mut last = carry;
    for (slot, &value) in out_tail.iter_mut().zip(xs_tail) {
        last = decay * last + gain * value;
        *slot = last;
    }
    last
}

/// `Σ xs` over eight running lane sums (lane `j` of the two accumulators takes
/// `xs[8i + j]`), a remaining four-value step into the first, the lanes folded
/// in a fixed order and the last values added in order: the same bits on every
/// dispatch path. A serial sum is one addition's latency per value; this is
/// one per eight, which is what an anchor window costs on the kernels that
/// re-anchor every window.
#[inline(always)]
fn sum_lanes<S: Simd>(simd: S, xs: &[f64]) -> f64 {
    let mut low = simd.splat(0.0);
    let mut high = simd.splat(0.0);
    let mut chunks = xs.chunks_exact(8);
    for chunk in &mut chunks {
        low = simd.add(low, simd.load(quad(chunk)));
        high = simd.add(high, simd.load(quad(&chunk[4..])));
    }
    let mut rest = chunks.remainder();
    if rest.len() >= 4 {
        low = simd.add(low, simd.load(quad(rest)));
        rest = &rest[4..];
    }
    let lanes = simd.to_array(simd.add(low, high));
    let mut total = (lanes[0] + lanes[1]) + (lanes[2] + lanes[3]);
    for &value in rest {
        total += value;
    }
    total
}

/// Running sums `out[i] = start + xs[0] + … + xs[i]`; returns the last (or
/// `start` for an empty slice). [`lin_scan`] with unit decay and gain, without
/// its multiplications: in-vector steps by one and two lanes, the carry kept
/// as a scalar off the vector shuffles.
#[inline(always)]
pub(crate) fn prefix_sums<S: Simd>(simd: S, xs: &[f64], start: f64, out: &mut [f64]) -> f64 {
    debug_assert_eq!(xs.len(), out.len());
    let mut acc = start;
    let head = xs.len() / 8 * 8;
    let (xs_head, xs_tail) = xs.split_at(head);
    let (out_head, out_tail) = out.split_at_mut(head);
    for (chunk, dest) in xs_head.chunks_exact(8).zip(out_head.chunks_exact_mut(8)) {
        let mut lo = simd.load(quad(chunk));
        let mut hi = simd.load(quad(&chunk[4..]));
        lo = simd.add(lo, simd.shift1(lo));
        hi = simd.add(hi, simd.shift1(hi));
        lo = simd.add(lo, simd.shift2(lo));
        hi = simd.add(hi, simd.shift2(hi));
        let (dest_lo, dest_hi) = dest.split_at_mut(4);
        simd.store(simd.add(lo, simd.splat(acc)), quad_mut(dest_lo));
        let mid = acc + simd.last_lane(lo);
        simd.store(simd.add(hi, simd.splat(mid)), quad_mut(dest_hi));
        acc = mid + simd.last_lane(hi);
    }
    for (slot, &value) in out_tail.iter_mut().zip(xs_tail) {
        acc += value;
        *slot = acc;
    }
    acc
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
/// recomputed from the window it covers (by [`sum_lanes`]). Returns the
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
        let exact = sum_lanes(simd, &xs[anchor + 1 - period..=anchor]);
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

/// One segment of shifted power sums, handed to a moment kernel's finisher.
pub(crate) struct PowerSegment<'s> {
    /// Output indices this segment covers.
    pub(crate) outputs: std::ops::Range<usize>,
    /// The shift subtracted from every value (the segment's anchor-window mean).
    pub(crate) shift: f64,
    /// Window sums of `(x - shift)^k`: power `k` (1-based) for output `idx` at
    /// `sums[(k - 1) * stride + (idx - outputs.start)]`.
    pub(crate) sums: &'s [f64],
    pub(crate) stride: usize,
}

impl PowerSegment<'_> {
    /// The window sum of power `k` (1-based) at segment offset `pos`.
    #[inline(always)]
    fn sum(&self, k: usize, pos: usize) -> f64 {
        self.sums[(k - 1) * self.stride + pos]
    }

    /// Four window sums of power `k` (1-based) from segment offset `pos`.
    #[inline(always)]
    fn sum4<S: Simd>(&self, simd: S, k: usize, pos: usize) -> S::V {
        simd.load(quad(&self.sums[(k - 1) * self.stride + pos..]))
    }
}

/// Rolling sums of powers of shifted values, the heart of the moment kernels.
///
/// The series is cut into segments of `period` outputs, each taken about a
/// shift: the mean of its first window, the re-anchoring the exact
/// accumulators perform every `period` values. Within a segment the `POWERS`
/// window sums of `(x - shift)^k` advance by the entering value's power minus
/// the leaving one's, a prefix scan per power. At the next segment the sums
/// move to the new shift -- the old one plus the window's mean deviation --
/// through the binomial expansion ([`recentre`]); that shift is at most a
/// window's drift, so the move costs no accuracy, and every
/// [`fresh_cadence`] segments the sums are taken afresh from the window so
/// rounding cannot accumulate. Re-centring every window keeps each deviation within about a
/// window's spread, which is what keeps the higher powers from cancelling: a
/// shift held for `16 · period` values cost a skewness up to three orders of
/// magnitude of accuracy on a drifting series.
///
/// `finish` receives each segment. `scratch` must hold
/// [`power_scratch_len`]`(POWERS, period)` values.
#[inline(always)]
pub(crate) fn shifted_power_sums<S: Simd, const POWERS: usize>(
    simd: S,
    xs: &[f64],
    period: usize,
    scratch: &mut [f64],
    mut finish: impl FnMut(PowerSegment<'_>),
) {
    let len = xs.len();
    let stride = period;
    let count = period as f64;
    let (steps, sums) = scratch.split_at_mut(POWERS * stride);
    let mut anchor = period - 1;
    let (mut shift, mut window) = window_power_sums::<S, POWERS>(simd, &xs[..period]);
    let fresh_every = fresh_cadence(period);
    let mut since_fresh = 0;
    loop {
        let end = (anchor + period).min(len);
        let outputs = end - anchor;
        power_steps::<S, POWERS>(simd, xs, period, shift, anchor + 1..end, steps, stride);
        for (k, &start) in window.iter().enumerate() {
            let row = k * stride;
            sums[row] = start;
            prefix_sums(
                simd,
                &steps[row..row + outputs - 1],
                start,
                &mut sums[row + 1..row + outputs],
            );
        }
        finish(PowerSegment {
            outputs: anchor..end,
            shift,
            sums,
            stride,
        });
        if end == len {
            break;
        }
        // The window ending at `end`, still about `shift`.
        let (enter, leave) = (xs[end] - shift, xs[end - period] - shift);
        let (mut up, mut down) = (enter, leave);
        for (k, slot) in window.iter_mut().enumerate() {
            *slot = sums[k * stride + outputs - 1] + (up - down);
            up *= enter;
            down *= leave;
        }
        since_fresh += 1;
        if since_fresh == fresh_every {
            (shift, window) = window_power_sums::<S, POWERS>(simd, &xs[end + 1 - period..=end]);
            since_fresh = 0;
        } else {
            let next = shift + window[0] / count;
            window = recentre(window, shift - next, count);
            shift = next;
        }
        anchor = end;
    }
}

/// The mean of `window` and the sums of the first `POWERS` powers of its
/// values' deviations from it, over four lanes folded in a fixed order.
#[inline(always)]
fn window_power_sums<S: Simd, const POWERS: usize>(
    simd: S,
    window: &[f64],
) -> (f64, [f64; POWERS]) {
    let shift = sum_lanes(simd, window) / window.len() as f64;
    let shift_v = simd.splat(shift);
    let mut lanes = [simd.splat(0.0); POWERS];
    let mut chunks = window.chunks_exact(4);
    for chunk in &mut chunks {
        let dev = simd.sub(simd.load(quad(chunk)), shift_v);
        let mut power = dev;
        for lane in &mut lanes {
            *lane = simd.add(*lane, power);
            power = simd.mul(power, dev);
        }
    }
    let mut sums = [0.0; POWERS];
    for (sum, lane) in sums.iter_mut().zip(lanes) {
        let parts = simd.to_array(lane);
        *sum = (parts[0] + parts[1]) + (parts[2] + parts[3]);
    }
    for &value in chunks.remainder() {
        let dev = value - shift;
        let mut power = dev;
        for sum in &mut sums {
            *sum += power;
            power *= dev;
        }
    }
    (shift, sums)
}

/// For outputs `idx` in `range`: the entering value's powers minus the leaving
/// one's, `(xs[idx] - shift)^k - (xs[idx - period] - shift)^k`, into
/// `steps[(k - 1) * stride + idx - range.start]` for `k` in `1..=POWERS`.
/// Lane-wise subtractions and products only, so the vector and scalar steps
/// round alike.
#[inline(always)]
fn power_steps<S: Simd, const POWERS: usize>(
    simd: S,
    xs: &[f64],
    period: usize,
    shift: f64,
    range: std::ops::Range<usize>,
    steps: &mut [f64],
    stride: usize,
) {
    let shift_v = simd.splat(shift);
    let mut idx = range.start;
    while idx + 4 <= range.end {
        let enter = simd.sub(simd.load(quad(&xs[idx..])), shift_v);
        let leave = simd.sub(simd.load(quad(&xs[idx - period..])), shift_v);
        let (mut up, mut down) = (enter, leave);
        let at = idx - range.start;
        for k in 0..POWERS {
            simd.store(simd.sub(up, down), quad_mut(&mut steps[k * stride + at..]));
            up = simd.mul(up, enter);
            down = simd.mul(down, leave);
        }
        idx += 4;
    }
    while idx < range.end {
        let (enter, leave) = (xs[idx] - shift, xs[idx - period] - shift);
        let (mut up, mut down) = (enter, leave);
        let at = idx - range.start;
        for k in 0..POWERS {
            steps[k * stride + at] = up - down;
            up *= enter;
            down *= leave;
        }
        idx += 1;
    }
}

/// Window sums of the first `POWERS` powers moved to a shift `delta` lower:
/// `Σ (d + delta)^k = Σ_j C(k, j) · delta^(k - j) · S_j` with `S_0 = count`,
/// each power evaluated by Horner's rule in `delta`.
fn recentre<const POWERS: usize>(sums: [f64; POWERS], delta: f64, count: f64) -> [f64; POWERS] {
    const BINOMIAL: [[f64; 4]; 4] = [
        [1.0, 0.0, 0.0, 0.0],
        [1.0, 1.0, 0.0, 0.0],
        [1.0, 2.0, 1.0, 0.0],
        [1.0, 3.0, 3.0, 1.0],
    ];
    let mut moments = [0.0; 4];
    moments[0] = count;
    moments[1..=POWERS].copy_from_slice(&sums);
    let mut out = [0.0; POWERS];
    for (k, slot) in (1..=POWERS).zip(out.iter_mut()) {
        let mut acc = 0.0;
        for j in 0..=k {
            acc = acc * delta + BINOMIAL[k][j] * moments[j];
        }
        *slot = acc;
    }
    out
}

/// Segments between fresh window sums in the carried moment kernels.
///
/// A short window's spread is small next to the drift between windows, so the
/// rounding a carried sum picks up is large next to what it measures: at a
/// period of 2 a sixteen-segment carry cost a correlation five orders of
/// magnitude of accuracy. Below [`SHORT_PERIOD`] every segment starts from
/// fresh sums, which at those periods costs no more than the segment itself.
const fn fresh_cadence(period: usize) -> usize {
    if period < SHORT_PERIOD {
        1
    } else {
        RESEED_EVERY
    }
}

/// See [`fresh_cadence`].
const SHORT_PERIOD: usize = 16;

/// Scratch values [`shifted_power_sums`] needs for `powers` power series: a
/// segment of steps and a segment of sums per power.
pub(crate) const fn power_scratch_len(powers: usize, period: usize) -> usize {
    2 * powers * period
}

/// `if a > 0 { a } else { 0 }`, the scalar form of `simd.max(a, 0)`.
#[inline(always)]
fn clamp_at_zero(value: f64) -> f64 {
    if value > 0.0 {
        value
    } else {
        0.0
    }
}

kernel! {
    /// Bollinger bands: mean and population standard deviation of the window
    /// from shifted first and second power sums, four outputs per step; rows
    /// `[upper, middle, lower, stddev]`.
    BollingerFast {
        x: &'a [f64],
        period: usize,
        multiplier: f64,
        scratch: &'a mut [f64],
        out: &'a mut [f64],
    } -> () = |simd, kern| {
        let period = kern.period;
        let (inv, mult) = (1.0 / period as f64, kern.multiplier);
        let (inv_v, mult_v, zero) = (simd.splat(inv), simd.splat(mult), simd.splat(0.0));
        kern.out[..(period - 1) * 4].fill(f64::NAN);
        let out = &mut *kern.out;
        shifted_power_sums::<S, 2>(simd, kern.x, period, kern.scratch, |seg| {
            let shift_v = simd.splat(seg.shift);
            let count = seg.outputs.len();
            let mut pos = 0;
            while pos + 4 <= count {
                let mean_dev = simd.mul(seg.sum4(simd, 1, pos), inv_v);
                let second = simd.mul(seg.sum4(simd, 2, pos), inv_v);
                let var = simd.max(simd.sub(second, simd.mul(mean_dev, mean_dev)), zero);
                let stddev = simd.sqrt(var);
                let mean = simd.add(shift_v, mean_dev);
                let band = simd.mul(mult_v, stddev);
                let (upper, lower) = (simd.add(mean, band), simd.sub(mean, band));
                // Four outputs' rows at once: the transpose turns the four
                // field vectors into four `[upper, middle, lower, stddev]` rows.
                let rows = simd.transpose4(upper, mean, lower, stddev);
                let base = (seg.outputs.start + pos) * 4;
                for (lane, row) in rows.into_iter().enumerate() {
                    simd.store(row, quad_mut(&mut out[base + lane * 4..]));
                }
                pos += 4;
            }
            while pos < count {
                let mean_dev = seg.sum(1, pos) * inv;
                let second = seg.sum(2, pos) * inv;
                let stddev = clamp_at_zero(second - mean_dev * mean_dev).sqrt();
                let mean = seg.shift + mean_dev;
                let band = mult * stddev;
                let row = (seg.outputs.start + pos) * 4;
                out[row..row + 4].copy_from_slice(&[mean + band, mean, mean - band, stddev]);
                pos += 1;
            }
        });
    }
}

kernel! {
    /// Skewness `m3 / m2^1.5` of the window from shifted first to third power
    /// sums (`m2 · sqrt(m2)` for the power), 0 for a window with no dispersion;
    /// four outputs per step.
    SkewnessFast {
        x: &'a [f64],
        period: usize,
        scratch: &'a mut [f64],
        out: &'a mut [f64],
    } -> () = |simd, kern| {
        let period = kern.period;
        let inv = 1.0 / period as f64;
        let (inv_v, zero, three, two) = (simd.splat(inv), simd.splat(0.0), simd.splat(3.0), simd.splat(2.0));
        kern.out[..period - 1].fill(f64::NAN);
        let out = &mut *kern.out;
        shifted_power_sums::<S, 3>(simd, kern.x, period, kern.scratch, |seg| {
            let count = seg.outputs.len();
            let dest = &mut out[seg.outputs.clone()];
            let mut pos = 0;
            while pos + 4 <= count {
                let mean_dev = simd.mul(seg.sum4(simd, 1, pos), inv_v);
                let second = simd.mul(seg.sum4(simd, 2, pos), inv_v);
                let third = simd.mul(seg.sum4(simd, 3, pos), inv_v);
                let m2 = simd.max(simd.sub(second, simd.mul(mean_dev, mean_dev)), zero);
                let cube = simd.mul(simd.mul(mean_dev, mean_dev), mean_dev);
                let m3 = simd.add(
                    simd.sub(third, simd.mul(three, simd.mul(mean_dev, second))),
                    simd.mul(two, cube),
                );
                let skew = simd.to_array(simd.div(m3, simd.mul(m2, simd.sqrt(m2))));
                let m2 = simd.to_array(m2);
                for lane in 0..4 {
                    dest[pos + lane] = if m2[lane] == 0.0 { 0.0 } else { skew[lane] };
                }
                pos += 4;
            }
            while pos < count {
                let mean_dev = seg.sum(1, pos) * inv;
                let second = seg.sum(2, pos) * inv;
                let third = seg.sum(3, pos) * inv;
                let m2 = clamp_at_zero(second - mean_dev * mean_dev);
                let m3 = third - 3.0 * (mean_dev * second) + 2.0 * (mean_dev * mean_dev * mean_dev);
                dest[pos] = if m2 == 0.0 { 0.0 } else { m3 / (m2 * m2.sqrt()) };
                pos += 1;
            }
        });
    }
}

kernel! {
    /// Pearson correlation of two series over the window, from shifted sums of
    /// `a`, `b`, `a²`, `b²` and `a·b` (population moments), clamped to
    /// `[-1, 1]`, 0 when a channel is flat; four outputs per step.
    PearsonFast {
        a: &'a [f64],
        b: &'a [f64],
        period: usize,
        scratch: &'a mut [f64],
        out: &'a mut [f64],
    } -> () = |simd, kern| {
        let period = kern.period;
        let inv = 1.0 / period as f64;
        let (inv_v, zero, one, minus_one) = (simd.splat(inv), simd.splat(0.0), simd.splat(1.0), simd.splat(-1.0));
        let len = kern.a.len();
        let count_f = period as f64;
        // Carried, re-centred sums as in `shifted_power_sums`: rows `a`, `b`,
        // `a²`, `b²` and `a·b` of deviations from the two shifts.
        let stride = period;
        kern.out[..period - 1].fill(f64::NAN);
        let (steps, sums) = kern.scratch.split_at_mut(5 * stride);
        let mut anchor = period - 1;
        let (mut shift_a, mut shift_b, mut window) =
            pair_window_sums(simd, &kern.a[..period], &kern.b[..period]);
        let fresh_every = fresh_cadence(period);
        let mut since_fresh = 0;
        loop {
            let end = (anchor + period).min(len);
            let outputs = end - anchor;
            pair_steps(simd, kern.a, kern.b, period, (shift_a, shift_b), anchor + 1..end, steps, stride);
            for (k, &start) in window.iter().enumerate() {
                let row = k * stride;
                sums[row] = start;
                prefix_sums(simd, &steps[row..row + outputs - 1],
                    start,
                    &mut sums[row + 1..row + outputs],
                );
            }
            let seg = PowerSegment {
                outputs: anchor..end,
                shift: 0.0,
                sums,
                stride,
            };
            let count = end - anchor;
            let dest = &mut kern.out[anchor..end];
            let mut pos = 0;
            while pos + 4 <= count {
                let mean_a = simd.mul(seg.sum4(simd, 1, pos), inv_v);
                let mean_b = simd.mul(seg.sum4(simd, 2, pos), inv_v);
                let var_a = simd.max(
                    simd.sub(simd.mul(seg.sum4(simd, 3, pos), inv_v), simd.mul(mean_a, mean_a)),
                    zero,
                );
                let var_b = simd.max(
                    simd.sub(simd.mul(seg.sum4(simd, 4, pos), inv_v), simd.mul(mean_b, mean_b)),
                    zero,
                );
                let cov = simd.sub(simd.mul(seg.sum4(simd, 5, pos), inv_v), simd.mul(mean_a, mean_b));
                let denom = simd.sqrt(simd.mul(var_a, var_b));
                let corr = simd.to_array(simd.min(simd.max(simd.div(cov, denom), minus_one), one));
                let denom = simd.to_array(denom);
                for lane in 0..4 {
                    dest[pos + lane] = if denom[lane] == 0.0 { 0.0 } else { corr[lane] };
                }
                pos += 4;
            }
            while pos < count {
                let mean_a = seg.sum(1, pos) * inv;
                let mean_b = seg.sum(2, pos) * inv;
                let var_a = clamp_at_zero(seg.sum(3, pos) * inv - mean_a * mean_a);
                let var_b = clamp_at_zero(seg.sum(4, pos) * inv - mean_b * mean_b);
                let cov = seg.sum(5, pos) * inv - mean_a * mean_b;
                let denom = (var_a * var_b).sqrt();
                dest[pos] = if denom == 0.0 { 0.0 } else { (cov / denom).clamp(-1.0, 1.0) };
                pos += 1;
            }
            if end == len {
                break;
            }
            // The window ending at `end`, still about the current shifts.
            let (enter_a, leave_a) = (kern.a[end] - shift_a, kern.a[end - period] - shift_a);
            let (enter_b, leave_b) = (kern.b[end] - shift_b, kern.b[end - period] - shift_b);
            let last = outputs - 1;
            window = [
                sums[last] + (enter_a - leave_a),
                sums[stride + last] + (enter_b - leave_b),
                sums[2 * stride + last] + (enter_a * enter_a - leave_a * leave_a),
                sums[3 * stride + last] + (enter_b * enter_b - leave_b * leave_b),
                sums[4 * stride + last] + (enter_a * enter_b - leave_a * leave_b),
            ];
            since_fresh += 1;
            if since_fresh == fresh_every {
                let lo = end + 1 - period;
                (shift_a, shift_b, window) =
                    pair_window_sums(simd, &kern.a[lo..=end], &kern.b[lo..=end]);
                since_fresh = 0;
            } else {
                let (next_a, next_b) = (shift_a + window[0] / count_f, shift_b + window[1] / count_f);
                window = recentre_pair(window, shift_a - next_a, shift_b - next_b, count_f);
                (shift_a, shift_b) = (next_a, next_b);
            }
            anchor = end;
        }
    }
}

/// The means of the windows `a` and `b` and the sums of `da`, `db`, `da²`,
/// `db²` and `da·db` of their deviations from them, over four lanes folded in
/// a fixed order.
#[inline(always)]
fn pair_window_sums<S: Simd>(simd: S, a: &[f64], b: &[f64]) -> (f64, f64, [f64; 5]) {
    let count = a.len() as f64;
    let (shift_a, shift_b) = (sum_lanes(simd, a) / count, sum_lanes(simd, b) / count);
    let (sa, sb) = (simd.splat(shift_a), simd.splat(shift_b));
    let mut lanes = [simd.splat(0.0); 5];
    let mut chunks = a.chunks_exact(4).zip(b.chunks_exact(4));
    for (ca, cb) in &mut chunks {
        let da = simd.sub(simd.load(quad(ca)), sa);
        let db = simd.sub(simd.load(quad(cb)), sb);
        let terms = [da, db, simd.mul(da, da), simd.mul(db, db), simd.mul(da, db)];
        for (lane, term) in lanes.iter_mut().zip(terms) {
            *lane = simd.add(*lane, term);
        }
    }
    let mut sums = [0.0; 5];
    for (sum, lane) in sums.iter_mut().zip(lanes) {
        let parts = simd.to_array(lane);
        *sum = (parts[0] + parts[1]) + (parts[2] + parts[3]);
    }
    let head = a.len() / 4 * 4;
    for (&va, &vb) in a[head..].iter().zip(&b[head..]) {
        let (da, db) = (va - shift_a, vb - shift_b);
        for (sum, term) in sums.iter_mut().zip([da, db, da * da, db * db, da * db]) {
            *sum += term;
        }
    }
    (shift_a, shift_b, sums)
}

/// For outputs `idx` in `range`: the entering pair's terms minus the leaving
/// pair's (`da`, `db`, `da²`, `db²`, `da·db` about `shifts`), into
/// `steps[k * stride + idx - range.start]`. Lane-wise subtractions and products
/// only, so the vector and scalar steps round alike.
#[allow(clippy::too_many_arguments)]
#[inline(always)]
fn pair_steps<S: Simd>(
    simd: S,
    a: &[f64],
    b: &[f64],
    period: usize,
    shifts: (f64, f64),
    range: std::ops::Range<usize>,
    steps: &mut [f64],
    stride: usize,
) {
    let (sa, sb) = (simd.splat(shifts.0), simd.splat(shifts.1));
    let mut idx = range.start;
    while idx + 4 <= range.end {
        let ea = simd.sub(simd.load(quad(&a[idx..])), sa);
        let la = simd.sub(simd.load(quad(&a[idx - period..])), sa);
        let eb = simd.sub(simd.load(quad(&b[idx..])), sb);
        let lb = simd.sub(simd.load(quad(&b[idx - period..])), sb);
        let at = idx - range.start;
        let terms = [
            simd.sub(ea, la),
            simd.sub(eb, lb),
            simd.sub(simd.mul(ea, ea), simd.mul(la, la)),
            simd.sub(simd.mul(eb, eb), simd.mul(lb, lb)),
            simd.sub(simd.mul(ea, eb), simd.mul(la, lb)),
        ];
        for (k, term) in terms.into_iter().enumerate() {
            simd.store(term, quad_mut(&mut steps[k * stride + at..]));
        }
        idx += 4;
    }
    while idx < range.end {
        let (ea, la) = (a[idx] - shifts.0, a[idx - period] - shifts.0);
        let (eb, lb) = (b[idx] - shifts.1, b[idx - period] - shifts.1);
        let at = idx - range.start;
        let terms = [
            ea - la,
            eb - lb,
            ea * ea - la * la,
            eb * eb - lb * lb,
            ea * eb - la * lb,
        ];
        for (k, term) in terms.into_iter().enumerate() {
            steps[k * stride + at] = term;
        }
        idx += 1;
    }
}

/// The five pair sums moved to shifts `delta_a` / `delta_b` lower, by the
/// binomial expansion: `Σ(da + δa)² = S_aa + δa·(2·S_a + n·δa)` and
/// `Σ(da + δa)(db + δb) = S_ab + δa·S_b + δb·(S_a + n·δa)`.
fn recentre_pair(sums: [f64; 5], delta_a: f64, delta_b: f64, count: f64) -> [f64; 5] {
    let [sum_a, sum_b, sum_aa, sum_bb, sum_ab] = sums;
    [
        sum_a + count * delta_a,
        sum_b + count * delta_b,
        sum_aa + delta_a * (2.0 * sum_a + count * delta_a),
        sum_bb + delta_b * (2.0 * sum_b + count * delta_b),
        sum_ab + delta_a * sum_b + delta_b * (sum_a + count * delta_a),
    ]
}
kernel! {
    /// MACD rows `[macd, signal, histogram]`: the warmup until the signal EMA
    /// is seeded runs the exact recurrences (those rows match the exact batch
    /// to the bit), then blocks of the fast, slow and signal EMA scans. Returns
    /// the last fast, slow and signal EMA.
    MacdFast {
        x: &'a [f64],
        periods: (usize, usize, usize),
        alphas: (f64, f64, f64),
        out: &'a mut [f64],
    } -> (f64, f64, f64) = |simd, kern| {
        let (fast_period, slow_period, signal_period) = kern.periods;
        let (fast_alpha, slow_alpha, signal_alpha) = kern.alphas;
        let (fast_decay, slow_decay, signal_decay) = (1.0 - fast_alpha, 1.0 - slow_alpha, 1.0 - signal_alpha);
        let first_full = slow_period + signal_period - 2;
        kern.out[..first_full * 3].fill(f64::NAN);
        let mut fast = kern.x[..fast_period].iter().copied().sum::<f64>() / fast_period as f64;
        for &value in &kern.x[fast_period..slow_period] {
            fast = ema_step(fast_alpha, fast_decay, fast, value);
        }
        let mut slow = kern.x[..slow_period].iter().copied().sum::<f64>() / slow_period as f64;
        let mut signal_sum = -0.0 + (fast - slow);
        for &value in &kern.x[slow_period..=first_full] {
            fast = ema_step(fast_alpha, fast_decay, fast, value);
            slow = ema_step(slow_alpha, slow_decay, slow, value);
            signal_sum += fast - slow;
        }
        let mut signal = signal_sum / signal_period as f64;
        let macd = fast - slow;
        kern.out[first_full * 3..first_full * 3 + 3].copy_from_slice(&[macd, signal, macd - signal]);
        let mut fast_block = [0.0; BLOCK];
        let mut slow_block = [0.0; BLOCK];
        let mut signal_block = [0.0; BLOCK];
        let inputs = kern.x[first_full + 1..].chunks(BLOCK);
        for (block, rows) in inputs.zip(kern.out[(first_full + 1) * 3..].chunks_mut(BLOCK * 3)) {
            let len = block.len();
            fast = lin_scan(simd, fast_decay, fast_alpha, block, fast, &mut fast_block[..len]);
            slow = lin_scan(simd, slow_decay, slow_alpha, block, slow, &mut slow_block[..len]);
            for (line, &sl) in fast_block[..len].iter_mut().zip(&slow_block[..len]) {
                *line -= sl;
            }
            signal = lin_scan(
                simd,
                signal_decay,
                signal_alpha,
                &fast_block[..len],
                signal,
                &mut signal_block[..len],
            );
            let lines = fast_block[..len].iter().zip(&signal_block[..len]);
            for (row, (&line, &sig)) in rows.chunks_exact_mut(3).zip(lines) {
                row.copy_from_slice(&[line, sig, line - sig]);
            }
        }
        (fast, slow, signal)
    }
}

kernel! {
    /// ATR's steady state: blocks of true ranges (lane-parallel, each against
    /// the previous close) smoothed by a Wilder scan. `out` starts at the first
    /// value after the seed; returns the last average.
    AtrFast {
        high: &'a [f64],
        low: &'a [f64],
        prev_close: &'a [f64],
        seed: f64,
        n_minus_1: f64,
        inv_period: f64,
        out: &'a mut [f64],
    } -> f64 = |simd, kern| {
        let (decay, gain) = (kern.n_minus_1 * kern.inv_period, kern.inv_period);
        let mut avg = kern.seed;
        let mut ranges = [0.0; BLOCK];
        let columns = kern.high.chunks(BLOCK).zip(kern.low.chunks(BLOCK)).zip(kern.prev_close.chunks(BLOCK));
        for (((highs, lows), prevs), dest) in columns.zip(kern.out.chunks_mut(BLOCK)) {
            let len = dest.len();
            for (((range, &hi), &lo), &pc) in ranges.iter_mut().zip(highs).zip(lows).zip(prevs) {
                *range = (hi - lo).max((hi - pc).abs()).max((lo - pc).abs());
            }
            avg = lin_scan(simd, decay, gain, &ranges[..len], avg, dest);
        }
        avg
    }
}

/// The money-flow volume of one bar, exactly as `Adl::update` computes it.
#[inline(always)]
fn money_flow(high: f64, low: f64, close: f64, volume: f64) -> f64 {
    let range = high - low;
    if range == 0.0 {
        0.0
    } else {
        ((close - low) - (high - close)) / range * volume
    }
}

kernel! {
    /// Chaikin oscillator `EMA_fast(ADL) − EMA_slow(ADL)`: the warmup until the
    /// slow EMA is seeded runs the exact accumulation and recurrences, then
    /// blocks of money-flow volumes, their running sum (a scan with decay 1)
    /// and both EMA scans. Returns the last ADL, fast EMA and slow EMA.
    ChaikinFast {
        high: &'a [f64],
        low: &'a [f64],
        close: &'a [f64],
        volume: &'a [f64],
        periods: (usize, usize),
        alphas: (f64, f64),
        out: &'a mut [f64],
    } -> (f64, f64, f64) = |simd, kern| {
        let (fast_period, slow_period) = kern.periods;
        let (fast_alpha, slow_alpha) = kern.alphas;
        let (fast_decay, slow_decay) = (1.0 - fast_alpha, 1.0 - slow_alpha);
        let first = slow_period - 1;
        kern.out[..first].fill(f64::NAN);
        let mut adl = 0.0_f64;
        let (mut fast_sum, mut slow_sum) = (-0.0_f64, -0.0_f64);
        let mut fast = 0.0;
        for idx in 0..=first {
            adl += money_flow(kern.high[idx], kern.low[idx], kern.close[idx], kern.volume[idx]);
            if idx < fast_period {
                fast_sum += adl;
                if idx + 1 == fast_period {
                    fast = fast_sum / fast_period as f64;
                }
            } else {
                fast = ema_step(fast_alpha, fast_decay, fast, adl);
            }
            slow_sum += adl;
        }
        let mut slow = slow_sum / slow_period as f64;
        kern.out[first] = fast - slow;
        let mut flows = [0.0; BLOCK];
        let mut adl_block = [0.0; BLOCK];
        let mut fast_block = [0.0; BLOCK];
        let mut slow_block = [0.0; BLOCK];
        let start = first + 1;
        let mut pos = start;
        for dest in kern.out[start..].chunks_mut(BLOCK) {
            let len = dest.len();
            let bars = kern.high[pos..pos + len]
                .iter()
                .zip(&kern.low[pos..pos + len])
                .zip(&kern.close[pos..pos + len])
                .zip(&kern.volume[pos..pos + len]);
            for (flow, (((&hi, &lo), &cl), &vol)) in flows.iter_mut().zip(bars) {
                *flow = money_flow(hi, lo, cl, vol);
            }
            adl = prefix_sums(simd, &flows[..len], adl, &mut adl_block[..len]);
            fast = lin_scan(simd, fast_decay, fast_alpha, &adl_block[..len], fast, &mut fast_block[..len]);
            slow = lin_scan(simd, slow_decay, slow_alpha, &adl_block[..len], slow, &mut slow_block[..len]);
            for ((slot, &f), &s) in dest.iter_mut().zip(&fast_block[..len]).zip(&slow_block[..len]) {
                *slot = f - s;
            }
            pos += len;
        }
        (adl, fast, slow)
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

    /// OHLCV columns with a real range on every bar.
    fn bars(n: usize) -> (Vec<f64>, Vec<f64>, Vec<f64>, Vec<f64>) {
        let close = series(n);
        let high: Vec<f64> = close
            .iter()
            .enumerate()
            .map(|(i, c)| c + 0.5 + (f64::from(u32::try_from(i).unwrap()) * 0.7).sin().abs())
            .collect();
        let low: Vec<f64> = close
            .iter()
            .enumerate()
            .map(|(i, c)| c - 0.5 - (f64::from(u32::try_from(i).unwrap()) * 0.9).cos().abs())
            .collect();
        let volume: Vec<f64> = (0..n)
            .map(|i| 1000.0 + f64::from(u32::try_from(i % 97).unwrap()) * 10.0)
            .collect();
        (high, low, close, volume)
    }

    /// Same `NaN` placement and agreement within `tol` of `scale`.
    fn close_to(exact: &[f64], fast: &[f64], tol: f64, scale: f64) {
        assert_eq!(exact.len(), fast.len());
        for (i, (x, y)) in exact.iter().zip(fast).enumerate() {
            assert_eq!(x.is_nan(), y.is_nan(), "NaN mismatch at {i}");
            if x.is_finite() {
                assert!(
                    (x - y).abs() <= tol * scale.max(x.abs()),
                    "at {i}: {x} vs {y}"
                );
            }
        }
    }

    #[test]
    fn macd_fast_agrees_and_falls_back() {
        use crate::indicators::MacdIndicator;
        let xs = series(4003);
        for (fast, slow, signal) in [(12, 26, 9), (3, 7, 1), (2, 5, 4)] {
            let make = || MacdIndicator::new(fast, slow, signal).unwrap();
            let (mut exact, mut quick) = (make(), make());
            close_to(
                &exact.batch_macd(&xs),
                &quick.batch_macd_fast(&xs),
                1e-12,
                1.0,
            );
            let (u, v) = (exact.update(101.0).unwrap(), quick.update(101.0).unwrap());
            assert!((u.macd - v.macd).abs() < 1e-12 && (u.signal - v.signal).abs() < 1e-12);
            let mut warm = make();
            let _ = warm.update(100.0);
            let mut warm2 = warm.clone();
            assert_eq!(
                bits(&warm.batch_macd_fast(&xs)),
                bits(&warm2.batch_macd(&xs))
            );
            let mut wild = xs[..300].to_vec();
            wild[150] = 1e200;
            assert_eq!(
                bits(&make().batch_macd_fast(&wild)),
                bits(&make().batch_macd(&wild))
            );
            let short = &xs[..slow + signal - 2];
            assert_eq!(
                bits(&make().batch_macd_fast(short)),
                bits(&make().batch_macd(short))
            );
        }
    }

    #[test]
    fn bollinger_fast_agrees_and_falls_back() {
        use crate::indicators::BollingerBands;
        let xs = series(4003);
        for period in [2, 20, 33] {
            let make = || BollingerBands::new(period, 2.0).unwrap();
            let (mut exact, mut quick) = (make(), make());
            close_to(
                &exact.batch_bands(&xs),
                &quick.batch_bands_fast(&xs),
                1e-11,
                1.0,
            );
            let (u, v) = (exact.update(101.0).unwrap(), quick.update(101.0).unwrap());
            assert!((u.middle - v.middle).abs() < 1e-11 && (u.stddev - v.stddev).abs() < 1e-11);
            let mut warm = make();
            let _ = warm.update(100.0);
            let mut warm2 = warm.clone();
            assert_eq!(
                bits(&warm.batch_bands_fast(&xs)),
                bits(&warm2.batch_bands(&xs))
            );
            let mut wild = xs[..300].to_vec();
            wild[150] = f64::INFINITY;
            assert_eq!(
                bits(&make().batch_bands_fast(&wild)),
                bits(&make().batch_bands(&wild))
            );
            let short = &xs[..period - 1];
            assert_eq!(
                bits(&make().batch_bands_fast(short)),
                bits(&make().batch_bands(short))
            );
        }
    }

    #[test]
    fn skewness_fast_agrees_and_falls_back() {
        use crate::indicators::Skewness;
        // Skewness divides by m2^1.5; compare in absolute terms on its [-3, 3]
        // scale, where the fast and exact moments differ by ~1e-13.
        let xs = series(4003);
        for period in [3, 20, 33] {
            let make = || Skewness::new(period).unwrap();
            close_to(&make().batch_nan(&xs), &make().batch_fast(&xs), 1e-9, 1.0);
            let mut warm = make();
            let _ = warm.update(100.0);
            falls_back(&warm, &xs);
            let mut wild = xs[..300].to_vec();
            wild[150] = 1e200;
            falls_back(&make(), &wild);
            falls_back(&make(), &xs[..period - 1]);
        }
        // A flat window has no dispersion: 0, like the exact path.
        let flat = vec![5.0; 64];
        let fast = Skewness::new(8).unwrap().batch_fast(&flat);
        assert!(fast[7..].iter().all(|&v| v == 0.0));
    }

    #[test]
    fn pearson_fast_agrees_and_falls_back() {
        use crate::indicators::PearsonCorrelation;
        let a = series(4003);
        let b: Vec<f64> = a
            .iter()
            .enumerate()
            .map(|(i, v)| v * 0.5 + (f64::from(u32::try_from(i).unwrap()) * 0.21).cos())
            .collect();
        for period in [2, 20, 33] {
            let make = || PearsonCorrelation::new(period).unwrap();
            let (mut exact, mut quick) = (make(), make());
            let (mut ea, mut fa) = (vec![0.0; a.len()], vec![0.0; a.len()]);
            exact.batch_pairs_into(&a, &b, &mut ea);
            quick.batch_pairs_fast_into(&a, &b, &mut fa);
            close_to(&ea, &fa, 1e-9, 1.0);
            // The replayed window continues within the same tolerance (its
            // accumulator sums carry a different reseed history, so the last
            // bits may differ).
            let u = exact.update((101.0, 50.0)).unwrap();
            let v = quick.update((101.0, 50.0)).unwrap();
            assert!((u - v).abs() < 1e-9, "continuation {u} vs {v}");
            let mut warm = make();
            let _ = warm.update((1.0, 2.0));
            let mut warm2 = warm.clone();
            warm.batch_pairs_fast_into(&a, &b, &mut fa);
            warm2.batch_pairs_into(&a, &b, &mut ea);
            assert_eq!(bits(&ea), bits(&fa));
            let mut wild = b[..300].to_vec();
            wild[150] = 1e200;
            let (mut e2, mut f2) = (vec![0.0; 300], vec![0.0; 300]);
            make().batch_pairs_into(&a[..300], &wild, &mut e2);
            make().batch_pairs_fast_into(&a[..300], &wild, &mut f2);
            assert_eq!(bits(&e2), bits(&f2));
            let (mut e3, mut f3) = (vec![0.0; period - 1], vec![0.0; period - 1]);
            make().batch_pairs_into(&a[..period - 1], &b[..period - 1], &mut e3);
            make().batch_pairs_fast_into(&a[..period - 1], &b[..period - 1], &mut f3);
            assert_eq!(bits(&e3), bits(&f3));
        }
        // A flat channel: correlation undefined, reported as 0 on both paths.
        let flat = vec![3.0; 64];
        let mut out = vec![0.0; 64];
        PearsonCorrelation::new(8)
            .unwrap()
            .batch_pairs_fast_into(&a[..64], &flat, &mut out);
        assert!(out[7..].iter().all(|&v| v == 0.0));
    }

    #[test]
    fn atr_fast_agrees_and_falls_back() {
        use crate::indicators::Atr;
        use crate::ohlcv::Candle;
        let (high, low, close, _) = bars(4003);
        for period in [1, 14, 33] {
            let make = || Atr::new(period).unwrap();
            let (mut exact, mut quick) = (make(), make());
            close_to(
                &exact.batch_atr(&high, &low, &close),
                &quick.batch_atr_fast(&high, &low, &close),
                1e-12,
                1.0,
            );
            let bar = Candle::new_unchecked(100.0, 101.0, 99.0, 100.5, 0.0, 0);
            assert!((exact.update(bar).unwrap() - quick.update(bar).unwrap()).abs() < 1e-12);
            let mut warm = make();
            let _ = warm.update(bar);
            let mut warm2 = warm.clone();
            assert_eq!(
                bits(&warm.batch_atr_fast(&high, &low, &close)),
                bits(&warm2.batch_atr(&high, &low, &close))
            );
            let mut wild = high[..300].to_vec();
            wild[150] = 1e200;
            assert_eq!(
                bits(&make().batch_atr_fast(&wild, &low[..300], &close[..300])),
                bits(&make().batch_atr(&wild, &low[..300], &close[..300]))
            );
        }
        let short = Atr::new(14)
            .unwrap()
            .batch_atr_fast(&high[..13], &low[..13], &close[..13]);
        assert!(short.iter().all(|v| v.is_nan()));
    }

    #[test]
    fn chaikin_fast_agrees_and_falls_back() {
        use crate::indicators::ChaikinOscillator;
        use crate::ohlcv::Candle;
        let (high, low, close, volume) = bars(4003);
        let scale = volume.iter().sum::<f64>();
        for (fast, slow) in [(3, 10), (1, 2), (5, 34)] {
            let make = || ChaikinOscillator::new(fast, slow).unwrap();
            let (mut exact, mut quick) = (make(), make());
            let (mut ea, mut fa) = (vec![0.0; high.len()], vec![0.0; high.len()]);
            exact.batch_hlcv_into(&high, &low, &close, &volume, &mut ea);
            quick.batch_hlcv_fast_into(&high, &low, &close, &volume, &mut fa);
            close_to(&ea, &fa, 1e-13, scale);
            let bar = Candle::new_unchecked(100.0, 101.0, 99.0, 100.5, 5000.0, 0);
            assert!(
                (exact.update(bar).unwrap() - quick.update(bar).unwrap()).abs() < 1e-13 * scale
            );
            let mut warm = make();
            let _ = warm.update(bar);
            let mut warm2 = warm.clone();
            warm.batch_hlcv_fast_into(&high, &low, &close, &volume, &mut fa);
            warm2.batch_hlcv_into(&high, &low, &close, &volume, &mut ea);
            assert_eq!(bits(&ea), bits(&fa));
            let mut wild = volume[..300].to_vec();
            wild[150] = 1e200;
            let (mut e2, mut f2) = (vec![0.0; 300], vec![0.0; 300]);
            make().batch_hlcv_into(&high[..300], &low[..300], &close[..300], &wild, &mut e2);
            make().batch_hlcv_fast_into(&high[..300], &low[..300], &close[..300], &wild, &mut f2);
            assert_eq!(bits(&e2), bits(&f2));
            let n = slow - 1;
            let (mut e3, mut f3) = (vec![0.0; n], vec![0.0; n]);
            make().batch_hlcv_into(&high[..n], &low[..n], &close[..n], &volume[..n], &mut e3);
            make().batch_hlcv_fast_into(&high[..n], &low[..n], &close[..n], &volume[..n], &mut f3);
            assert_eq!(bits(&e3), bits(&f3));
        }
        // Flat bars carry no money flow: the zero-range branch.
        let flat = vec![7.0; 40];
        let mut out = vec![0.0; 40];
        ChaikinOscillator::classic().batch_hlcv_fast_into(&flat, &flat, &flat, &flat, &mut out);
        assert!(out[9..].iter().all(|&v| v == 0.0));
    }

    /// The recurrence kernels over columns (MACD, ATR, Chaikin) return the same
    /// bits through the dispatcher as in the portable baseline build.
    #[test]
    fn recurrence_column_kernels_are_identical_on_every_dispatch_path() {
        let xs = series(3001);
        let (high, low, close, volume) = bars(3001);
        let n = xs.len();
        let (mut a, mut b) = (vec![0.0; n * 3], vec![0.0; n * 3]);
        let ra = wickra_simd::dispatch(MacdFast {
            x: &xs,
            periods: (12, 26, 9),
            alphas: (2.0 / 13.0, 2.0 / 27.0, 0.2),
            out: &mut a[..n * 3],
            _borrow: PhantomData,
        });
        let rb = wickra_simd::run_baseline(MacdFast {
            x: &xs,
            periods: (12, 26, 9),
            alphas: (2.0 / 13.0, 2.0 / 27.0, 0.2),
            out: &mut b[..n * 3],
            _borrow: PhantomData,
        });
        assert_eq!(bits(&a[..n * 3]), bits(&b[..n * 3]));
        assert_eq!(bits(&[ra.0, ra.1, ra.2]), bits(&[rb.0, rb.1, rb.2]));
        let ra = wickra_simd::dispatch(AtrFast {
            high: &high[14..],
            low: &low[14..],
            prev_close: &close[13..n - 1],
            seed: 1.3,
            n_minus_1: 13.0,
            inv_period: 1.0 / 14.0,
            out: &mut a[..n - 14],
            _borrow: PhantomData,
        });
        let rb = wickra_simd::run_baseline(AtrFast {
            high: &high[14..],
            low: &low[14..],
            prev_close: &close[13..n - 1],
            seed: 1.3,
            n_minus_1: 13.0,
            inv_period: 1.0 / 14.0,
            out: &mut b[..n - 14],
            _borrow: PhantomData,
        });
        assert_eq!(bits(&a[..n - 14]), bits(&b[..n - 14]));
        assert_eq!(ra.to_bits(), rb.to_bits());
        let ra = wickra_simd::dispatch(ChaikinFast {
            high: &high,
            low: &low,
            close: &close,
            volume: &volume,
            periods: (3, 10),
            alphas: (0.5, 2.0 / 11.0),
            out: &mut a[..n],
            _borrow: PhantomData,
        });
        let rb = wickra_simd::run_baseline(ChaikinFast {
            high: &high,
            low: &low,
            close: &close,
            volume: &volume,
            periods: (3, 10),
            alphas: (0.5, 2.0 / 11.0),
            out: &mut b[..n],
            _borrow: PhantomData,
        });
        assert_eq!(bits(&a[..n]), bits(&b[..n]));
        assert_eq!(bits(&[ra.0, ra.1, ra.2]), bits(&[rb.0, rb.1, rb.2]));
    }

    /// The multi-output, candle and pair kernels return the same bits through
    /// the dispatcher as in the portable baseline build.
    #[test]
    fn column_kernels_are_identical_on_every_dispatch_path() {
        let xs = series(3001);
        let (_, _, close, _) = bars(3001);
        let n = xs.len();
        let (mut a, mut b) = (vec![0.0; n * 4], vec![0.0; n * 4]);
        let (mut sa, mut sb) = (
            vec![0.0; power_scratch_len(5, 20)],
            vec![0.0; power_scratch_len(5, 20)],
        );
        wickra_simd::dispatch(BollingerFast {
            x: &xs,
            period: 20,
            multiplier: 2.0,
            scratch: &mut sa,
            out: &mut a,
            _borrow: PhantomData,
        });
        wickra_simd::run_baseline(BollingerFast {
            x: &xs,
            period: 20,
            multiplier: 2.0,
            scratch: &mut sb,
            out: &mut b,
            _borrow: PhantomData,
        });
        assert_eq!(bits(&a), bits(&b));
        wickra_simd::dispatch(SkewnessFast {
            x: &xs,
            period: 20,
            scratch: &mut sa,
            out: &mut a[..n],
            _borrow: PhantomData,
        });
        wickra_simd::run_baseline(SkewnessFast {
            x: &xs,
            period: 20,
            scratch: &mut sb,
            out: &mut b[..n],
            _borrow: PhantomData,
        });
        assert_eq!(bits(&a[..n]), bits(&b[..n]));
        wickra_simd::dispatch(PearsonFast {
            a: &xs,
            b: &close,
            period: 20,
            scratch: &mut sa,
            out: &mut a[..n],
            _borrow: PhantomData,
        });
        wickra_simd::run_baseline(PearsonFast {
            a: &xs,
            b: &close,
            period: 20,
            scratch: &mut sb,
            out: &mut b[..n],
            _borrow: PhantomData,
        });
        assert_eq!(bits(&a[..n]), bits(&b[..n]));
    }
}
