//! Chaikin Oscillator.

use crate::error::{Error, Result};
use crate::indicators::adl::Adl;
use crate::indicators::ema::Ema;
use crate::ohlcv::Candle;
use crate::traits::Indicator;

/// Chaikin Oscillator — the MACD of the Accumulation/Distribution Line.
///
/// ```text
/// ChaikinOsc_t = EMA(ADL, fast)_t − EMA(ADL, slow)_t
/// ```
///
/// It turns the unbounded, ever-drifting [`Adl`](crate::Adl) into a
/// zero-centred momentum oscillator: positive when short-term accumulation
/// outpaces the longer trend, negative when distribution leads. Because the
/// ADL emits from the very first candle, the slow EMA gates the first output —
/// the warmup period is exactly `slow`. Chaikin's classic configuration is
/// `fast = 3`, `slow = 10`.
///
/// # Example
///
/// ```
/// use wickra_core::{Candle, Indicator, ChaikinOscillator};
///
/// let mut indicator = ChaikinOscillator::classic();
/// let mut last = None;
/// for i in 0..80 {
///     let base = 100.0 + f64::from(i);
///     let candle =
///         Candle::new(base, base + 2.0, base - 2.0, base + 1.0, 10.0, i64::from(i)).unwrap();
///     last = indicator.update(candle);
/// }
/// assert!(last.is_some());
/// ```
#[derive(Debug, Clone)]
pub struct ChaikinOscillator {
    adl: Adl,
    fast: Ema,
    slow: Ema,
    fast_period: usize,
    slow_period: usize,
}

impl ChaikinOscillator {
    /// Construct a Chaikin Oscillator with explicit fast / slow EMA periods.
    ///
    /// # Errors
    /// Returns [`Error::PeriodZero`] if either period is zero, or
    /// [`Error::InvalidPeriod`] if `fast >= slow`.
    pub fn new(fast: usize, slow: usize) -> Result<Self> {
        if fast == 0 || slow == 0 {
            return Err(Error::PeriodZero);
        }
        if fast >= slow {
            return Err(Error::InvalidPeriod {
                message: "Chaikin Oscillator needs fast < slow",
            });
        }
        Ok(Self {
            adl: Adl::new(),
            fast: Ema::new(fast)?,
            slow: Ema::new(slow)?,
            fast_period: fast,
            slow_period: slow,
        })
    }

    /// Chaikin's classic configuration: `EMA(ADL, 3) − EMA(ADL, 10)`.
    pub fn classic() -> Self {
        Self::new(3, 10).expect("classic Chaikin Oscillator params are valid")
    }

    /// Configured `(fast, slow)` periods.
    pub const fn periods(&self) -> (usize, usize) {
        (self.fast_period, self.slow_period)
    }
}

impl ChaikinOscillator {
    /// Exact batch over high/low/close/volume columns: one output per bar
    /// (`NaN` during warmup), bit for bit what replaying `update` over the same
    /// candles gives (the oscillator reads no open or timestamp). The caller
    /// guarantees valid OHLCV bars, as the bindings validate them once up
    /// front.
    ///
    /// # Panics
    ///
    /// Panics if the five slices differ in length.
    pub fn batch_hlcv_into(
        &mut self,
        high: &[f64],
        low: &[f64],
        close: &[f64],
        volume: &[f64],
        out: &mut [f64],
    ) {
        let n = high.len();
        assert!(
            low.len() == n && close.len() == n && volume.len() == n && out.len() == n,
            "high, low, close, volume and the output must be equal length"
        );
        let seeded = self.warm_up(high, low, close, volume, out);
        if seeded == n {
            return;
        }
        let (adl, fast, slow) = wickra_simd::dispatch(ChaikinTail {
            high: &high[seeded..],
            low: &low[seeded..],
            close: &close[seeded..],
            volume: &volume[seeded..],
            out: &mut out[seeded..],
            state: self.steady_state(),
            fast: (self.fast.alpha(), self.fast.one_minus_alpha()),
            slow: (self.slow.alpha(), self.slow.one_minus_alpha()),
        });
        self.adl.resume_at(adl);
        self.fast.seed_to(fast);
        self.slow.seed_to(slow);
    }

    /// Replay `update` until both EMAs are seeded (or the bars run out),
    /// writing those outputs; returns how many bars it consumed.
    fn warm_up(
        &mut self,
        high: &[f64],
        low: &[f64],
        close: &[f64],
        volume: &[f64],
        out: &mut [f64],
    ) -> usize {
        let mut idx = 0;
        while idx < out.len() && !self.is_ready() {
            let candle =
                Candle::new_unchecked(close[idx], high[idx], low[idx], close[idx], volume[idx], 0);
            out[idx] = self.update(candle).unwrap_or(f64::NAN);
            idx += 1;
        }
        idx
    }

    /// The accumulation line and both EMAs once both EMAs are seeded.
    fn steady_state(&self) -> (f64, f64, f64) {
        (
            self.adl.value().expect("the ADL emits from its first bar"),
            self.fast.value().expect("the fast EMA is seeded"),
            self.slow.value().expect("the slow EMA is seeded"),
        )
    }

    /// [`batch_hlcv_into`](Self::batch_hlcv_into) over full OHLCV columns that
    /// have not been validated: returns `true` with `out` filled if every bar
    /// is one [`Candle::new`] accepts, or `false` -- the indicator untouched and
    /// `out` as it was -- otherwise.
    ///
    /// # Panics
    ///
    /// Panics if the six slices differ in length.
    pub fn batch_ohlcv_into(
        &mut self,
        open: &[f64],
        high: &[f64],
        low: &[f64],
        close: &[f64],
        volume: &[f64],
        out: &mut [f64],
    ) -> bool {
        assert_ohlcv(open, high, low, close, volume, out);
        if !Candle::all_valid(open, high, low, close, volume) {
            return false;
        }
        self.batch_hlcv_into(high, low, close, volume, out);
        true
    }

    /// [`batch_hlcv_fast_into`](Self::batch_hlcv_fast_into) over full OHLCV
    /// columns that have not been validated, like
    /// [`batch_ohlcv_into`](Self::batch_ohlcv_into): `true` with `out` filled
    /// -- bit for bit what `batch_hlcv_fast_into` gives -- or `false`, the
    /// indicator untouched, if any bar is invalid. Whether the bars are valid
    /// and whether they are inside the kernel's range is one pass over the
    /// columns, not two.
    ///
    /// # Panics
    ///
    /// Panics if the six slices differ in length.
    pub fn batch_ohlcv_fast_into(
        &mut self,
        open: &[f64],
        high: &[f64],
        low: &[f64],
        close: &[f64],
        volume: &[f64],
        out: &mut [f64],
    ) -> bool {
        let n = assert_ohlcv(open, high, low, close, volume, out);
        if !self.fast_path_open(n)
            || !Candle::all_valid_within(open, high, low, close, volume, crate::fast::MAX_ABS)
        {
            return self.batch_ohlcv_into(open, high, low, close, volume, out);
        }
        self.fast_batch(high, low, close, volume, out);
        true
    }

    /// Whether a fast batch of `n` bars may take the kernel: a fresh indicator
    /// and a series reaching the first output.
    fn fast_path_open(&self, n: usize) -> bool {
        self.adl.value().is_none()
            && self.fast.is_fresh()
            && self.slow.is_fresh()
            && n >= self.slow_period
    }

    /// The fast batch of a fresh indicator over bars it may take: the exact
    /// warmup up to the first output, then the kernel from that state, leaving
    /// the state where the kernel ends.
    fn fast_batch(
        &mut self,
        high: &[f64],
        low: &[f64],
        close: &[f64],
        volume: &[f64],
        out: &mut [f64],
    ) {
        let first = self.slow_period;
        self.warm_up(high, low, close, volume, &mut out[..first]);
        let (adl, fast, slow) = wickra_simd::dispatch(crate::fast::ChaikinFast {
            high: &high[first..],
            low: &low[first..],
            close: &close[first..],
            volume: &volume[first..],
            state: self.steady_state(),
            alphas: (self.fast.alpha(), self.slow.alpha()),
            out: &mut out[first..],
            _borrow: std::marker::PhantomData,
        });
        self.adl.resume_at(adl);
        self.fast.seed_to(fast);
        self.slow.seed_to(slow);
    }

    /// Opt-in fast variant of [`batch_hlcv_into`](Self::batch_hlcv_into): after
    /// the exact warmup, blocks of money-flow volumes, the accumulation line as
    /// a running-sum scan and both EMAs as SIMD linear-recurrence scans. Every
    /// value agrees with the exact batch to within a few units in the last
    /// place relative to the accumulation line; the first value, warmup `NaN`s
    /// and length are identical, and the result is the same on every platform.
    /// Only a fresh indicator over finite values within `1e100`, at least
    /// `slow` bars long, takes the kernel; anything else is the exact batch.
    /// Afterwards the oscillator continues streaming from the kernel's last
    /// values.
    ///
    /// # Panics
    ///
    /// Panics if the five slices differ in length.
    pub fn batch_hlcv_fast_into(
        &mut self,
        high: &[f64],
        low: &[f64],
        close: &[f64],
        volume: &[f64],
        out: &mut [f64],
    ) {
        let n = high.len();
        assert!(
            low.len() == n && close.len() == n && volume.len() == n && out.len() == n,
            "high, low, close, volume and the output must be equal length"
        );
        if !self.fast_path_open(n)
            || ![high, low, close, volume]
                .iter()
                .all(|col| crate::fast::in_range(col))
        {
            self.batch_hlcv_into(high, low, close, volume, out);
            return;
        }
        self.fast_batch(high, low, close, volume, out);
    }
}

/// The common length of OHLCV columns and their output.
fn assert_ohlcv(
    open: &[f64],
    high: &[f64],
    low: &[f64],
    close: &[f64],
    volume: &[f64],
    out: &[f64],
) -> usize {
    let n = open.len();
    assert!(
        high.len() == n
            && low.len() == n
            && close.len() == n
            && volume.len() == n
            && out.len() == n,
        "open, high, low, close, volume and the output must be equal length"
    );
    n
}

/// The exact oscillator past its warmup as a [`wickra_simd::Kernel`]: the
/// accumulation line and both EMA recurrences fused into one pass, with the
/// arithmetic of `update` in the same order -- so the same bits -- and each
/// `mul_add` a hardware FMA where the CPU has one. Returns the last ADL, fast
/// and slow EMA.
struct ChaikinTail<'a> {
    high: &'a [f64],
    low: &'a [f64],
    close: &'a [f64],
    volume: &'a [f64],
    out: &'a mut [f64],
    state: (f64, f64, f64),
    /// `(alpha, 1 - alpha)` of the fast EMA.
    fast: (f64, f64),
    /// `(alpha, 1 - alpha)` of the slow EMA.
    slow: (f64, f64),
}

// Inlining into the dispatching function is what compiles the body with its
// features; see `wickra_simd::Kernel`.
#[allow(clippy::inline_always)]
impl wickra_simd::Kernel for ChaikinTail<'_> {
    type Output = (f64, f64, f64);

    #[inline(always)]
    fn run<S: wickra_simd::Simd>(self, _simd: S) -> (f64, f64, f64) {
        let (mut adl, mut fast, mut slow) = self.state;
        let ((fast_alpha, fast_oma), (slow_alpha, slow_oma)) = (self.fast, self.slow);
        let bars = self
            .high
            .iter()
            .zip(self.low)
            .zip(self.close)
            .zip(self.volume);
        for (slot, (((&high, &low), &close), &volume)) in self.out.iter_mut().zip(bars) {
            adl += crate::fast::money_flow(high, low, close, volume);
            // `Ema::update` ignores a non-finite input, so an accumulation line
            // that overflowed leaves both EMAs where they were and emits nothing.
            *slot = if adl.is_finite() {
                fast = fast_alpha.mul_add(adl, fast_oma * fast);
                slow = slow_alpha.mul_add(adl, slow_oma * slow);
                fast - slow
            } else {
                f64::NAN
            };
        }
        (adl, fast, slow)
    }
}

impl Indicator for ChaikinOscillator {
    type Input = Candle;
    type Output = f64;

    #[inline]
    fn update(&mut self, candle: Candle) -> Option<f64> {
        // The ADL emits a value from the very first candle, so both EMAs are
        // fed on every bar and warm up in parallel.
        let adl = self.adl.update(candle)?;
        let fast = self.fast.update(adl);
        let slow = self.slow.update(adl);
        Some(fast? - slow?)
    }

    fn reset(&mut self) {
        self.adl.reset();
        self.fast.reset();
        self.slow.reset();
    }

    #[inline]
    fn warmup_period(&self) -> usize {
        // ADL is ready at candle 1; the slow EMA gates the first emission.
        self.slow_period
    }

    #[inline]
    fn is_ready(&self) -> bool {
        self.fast.is_ready() && self.slow.is_ready()
    }

    #[inline]
    fn name(&self) -> &'static str {
        "ChaikinOscillator"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;
    use approx::assert_relative_eq;

    fn cdl(base: f64, volume: f64, ts: i64) -> Candle {
        Candle::new(base, base + 1.0, base - 1.0, base, volume, ts).unwrap()
    }

    fn flat(price: f64, ts: i64) -> Candle {
        Candle::new(price, price, price, price, 100.0, ts).unwrap()
    }

    #[test]
    fn matches_independent_adl_and_emas() {
        // The oscillator must equal feeding a standalone ADL into two
        // standalone EMAs and differencing them once both are ready.
        let candles: Vec<Candle> = (0..80)
            .map(|i| {
                let mid = 100.0 + (i as f64 * 0.2).sin() * 6.0;
                Candle::new(
                    mid,
                    mid + 1.5,
                    mid - 1.5,
                    mid + 0.3,
                    10.0 + (i % 6) as f64,
                    i,
                )
                .unwrap()
            })
            .collect();
        let mut osc = ChaikinOscillator::classic();
        let mut adl = Adl::new();
        let mut fast = Ema::new(3).unwrap();
        let mut slow = Ema::new(10).unwrap();
        for (i, candle) in candles.iter().enumerate() {
            let got = osc.update(*candle);
            let a = adl.update(*candle).expect("ADL emits from candle 1");
            let f = fast.update(a);
            let s = slow.update(a);
            match (f, s) {
                (Some(fv), Some(sv)) => {
                    assert_relative_eq!(
                        got.expect("oscillator ready once slow EMA is"),
                        fv - sv,
                        epsilon = 1e-9
                    );
                }
                _ => assert!(got.is_none(), "must be None until slow EMA ready (i={i})"),
            }
        }
    }

    #[test]
    fn flat_market_yields_zero() {
        // A flat candle has zero money-flow volume, so the ADL never moves and
        // both EMAs of a constant-zero series stay at zero.
        let candles: Vec<Candle> = (0..60).map(|i| flat(10.0, i)).collect();
        let mut osc = ChaikinOscillator::classic();
        for v in osc.batch(&candles).into_iter().flatten() {
            assert_relative_eq!(v, 0.0, epsilon = 1e-9);
        }
    }

    #[test]
    fn first_emission_matches_warmup_period() {
        let candles: Vec<Candle> = (0..40).map(|i| cdl(100.0 + i as f64, 50.0, i)).collect();
        let mut osc = ChaikinOscillator::classic();
        let out = osc.batch(&candles);
        assert_eq!(osc.warmup_period(), 10);
        for (i, v) in out.iter().enumerate().take(9) {
            assert!(v.is_none(), "index {i} must be None during warmup");
        }
        assert!(out[9].is_some(), "first value lands at warmup_period - 1");
    }

    #[test]
    fn rejects_invalid_params() {
        assert!(ChaikinOscillator::new(0, 10).is_err());
        assert!(ChaikinOscillator::new(3, 0).is_err());
        assert!(ChaikinOscillator::new(10, 3).is_err());
        assert!(ChaikinOscillator::new(5, 5).is_err());
    }

    /// Cover the const accessor `periods` (76-78) and the Indicator-impl
    /// `name` body (109-111). `warmup_period` is exercised elsewhere.
    #[test]
    fn accessors_and_metadata() {
        let osc = ChaikinOscillator::classic();
        assert_eq!(osc.periods(), (3, 10));
        assert_eq!(osc.name(), "ChaikinOscillator");
    }

    #[test]
    fn reset_clears_state() {
        let candles: Vec<Candle> = (0..40).map(|i| cdl(100.0 + i as f64, 50.0, i)).collect();
        let mut osc = ChaikinOscillator::classic();
        osc.batch(&candles);
        assert!(osc.is_ready());
        osc.reset();
        assert!(!osc.is_ready());
        assert_eq!(osc.update(candles[0]), None);
    }

    #[test]
    fn batch_equals_streaming() {
        let candles: Vec<Candle> = (0..80)
            .map(|i| {
                let mid = 100.0 + (i as f64 * 0.3).sin() * 8.0;
                Candle::new(
                    mid,
                    mid + 2.0,
                    mid - 2.0,
                    mid + 0.5,
                    10.0 + (i % 5) as f64,
                    i,
                )
                .unwrap()
            })
            .collect();
        let mut a = ChaikinOscillator::classic();
        let mut b = ChaikinOscillator::classic();
        assert_eq!(
            a.batch(&candles),
            candles.iter().map(|x| b.update(*x)).collect::<Vec<_>>()
        );
    }

    /// OHLCV columns of `n` valid bars that are not all alike.
    fn columns(n: usize) -> [Vec<f64>; 5] {
        let mid: Vec<f64> = (0..n)
            .map(|i| 100.0 + (i as f64 * 0.37).sin() * 9.0)
            .collect();
        let open = mid.iter().map(|m| m - 0.25).collect();
        let high = mid
            .iter()
            .enumerate()
            .map(|(i, m)| m + 1.0 + (i % 3) as f64)
            .collect();
        let low = mid
            .iter()
            .enumerate()
            .map(|(i, m)| m - 1.0 - (i % 4) as f64 * 0.5)
            .collect();
        let close = mid
            .iter()
            .enumerate()
            .map(|(i, m)| m + ((i % 7) as f64 - 3.0) * 0.2)
            .collect();
        let volume = (0..n).map(|i| 1000.0 + (i % 11) as f64 * 37.0).collect();
        [open, high, low, close, volume]
    }

    fn bits(values: &[f64]) -> Vec<u64> {
        values.iter().map(|v| v.to_bits()).collect()
    }

    /// The `update` replay over the bars the columns describe, as the column
    /// batches see them (open is the close, timestamps are zero).
    fn replay(
        osc: &mut ChaikinOscillator,
        [_, high, low, close, volume]: &[Vec<f64>; 5],
    ) -> Vec<f64> {
        (0..high.len())
            .map(|i| {
                let candle =
                    Candle::new_unchecked(close[i], high[i], low[i], close[i], volume[i], 0);
                osc.update(candle).unwrap_or(f64::NAN)
            })
            .collect()
    }

    #[test]
    fn column_batch_is_the_update_replay_bit_for_bit() {
        let cols = columns(3000);
        let [_, high, low, close, volume] = &cols;
        let mut out = vec![0.0; 3000];
        let mut osc = ChaikinOscillator::classic();
        osc.batch_hlcv_into(high, low, close, volume, &mut out);
        let mut reference = ChaikinOscillator::classic();
        assert_eq!(bits(&out), bits(&replay(&mut reference, &cols)));
        // The state it leaves continues exactly like the replay's.
        let next = Candle::new(100.0, 103.0, 98.0, 101.0, 500.0, 0).unwrap();
        assert_eq!(
            osc.update(next).map(f64::to_bits),
            reference.update(next).map(f64::to_bits)
        );
    }

    #[test]
    fn column_batch_resumes_mid_warmup_and_mid_series() {
        let cols = columns(400);
        let [_, high, low, close, volume] = &cols;
        let mut whole = vec![0.0; 400];
        ChaikinOscillator::classic().batch_hlcv_into(high, low, close, volume, &mut whole);
        // Split inside the warmup and again past it: the pieces join up.
        let mut split = vec![0.0; 400];
        let mut osc = ChaikinOscillator::classic();
        for (start, end) in [(0, 4), (4, 4), (4, 150), (150, 400)] {
            osc.batch_hlcv_into(
                &high[start..end],
                &low[start..end],
                &close[start..end],
                &volume[start..end],
                &mut split[start..end],
            );
        }
        assert_eq!(bits(&split), bits(&whole));
    }

    #[test]
    fn column_batch_skips_an_overflowed_accumulation_line() {
        // A full-range bar with a huge volume adds ±volume to the ADL; two of
        // them overflow it to infinity, which the EMAs refuse, as in `update`.
        let n = 30;
        let high = vec![2.0; n];
        let low = vec![0.0; n];
        let close = vec![2.0; n];
        let mut volume = vec![1.0; n];
        volume[20] = f64::MAX;
        volume[21] = f64::MAX;
        let cols = [
            close.clone(),
            high.clone(),
            low.clone(),
            close.clone(),
            volume.clone(),
        ];
        let mut out = vec![0.0; n];
        ChaikinOscillator::classic().batch_hlcv_into(&high, &low, &close, &volume, &mut out);
        assert!(out[21].is_nan() && out[29].is_nan());
        assert_eq!(
            bits(&out),
            bits(&replay(&mut ChaikinOscillator::classic(), &cols))
        );
    }

    #[test]
    fn checked_ohlcv_batches_equal_the_column_batches() {
        // Long enough for many kernel blocks, with a ragged last one.
        let n = 12_345;
        let [open, high, low, close, volume] = columns(n);
        let mut column = vec![0.0; n];
        let mut checked = vec![0.0; n];
        ChaikinOscillator::classic().batch_hlcv_into(&high, &low, &close, &volume, &mut column);
        assert!(ChaikinOscillator::classic().batch_ohlcv_into(
            &open,
            &high,
            &low,
            &close,
            &volume,
            &mut checked
        ));
        assert_eq!(bits(&checked), bits(&column));

        let mut a = ChaikinOscillator::classic();
        let mut b = ChaikinOscillator::classic();
        a.batch_hlcv_fast_into(&high, &low, &close, &volume, &mut column);
        assert!(b.batch_ohlcv_fast_into(&open, &high, &low, &close, &volume, &mut checked));
        assert_eq!(bits(&checked), bits(&column));
        let next = Candle::new(100.0, 103.0, 98.0, 101.0, 500.0, 0).unwrap();
        assert_eq!(
            a.update(next).map(f64::to_bits),
            b.update(next).map(f64::to_bits)
        );
    }

    #[test]
    fn checked_ohlcv_batches_reject_an_invalid_bar_untouched() {
        let n = 9_000;
        let [open, mut high, low, close, volume] = columns(n);
        // Valid until late in the series, where a high falls below its low.
        high[6_010] = low[6_010] - 1.0;
        let mut out = vec![0.0; n];
        for fast in [false, true] {
            let mut osc = ChaikinOscillator::classic();
            let ok = if fast {
                osc.batch_ohlcv_fast_into(&open, &high, &low, &close, &volume, &mut out)
            } else {
                osc.batch_ohlcv_into(&open, &high, &low, &close, &volume, &mut out)
            };
            assert!(!ok);
            assert!(osc.adl.value().is_none() && osc.fast.is_fresh() && osc.slow.is_fresh());
            assert!(out.iter().all(|&v| v == 0.0));
        }
    }

    #[test]
    fn checked_fast_batch_outside_the_kernel_range_is_the_exact_batch() {
        let n = 4_600;
        let [open, high, low, close, mut volume] = columns(n);
        let mut exact = vec![0.0; n];
        let mut fast = vec![0.0; n];
        // A valid but huge volume in the warmup, then one past it: both send
        // the whole series to the exact batch.
        for at in [5, 4_120] {
            volume[at] = 1e200;
            ChaikinOscillator::classic().batch_hlcv_into(&high, &low, &close, &volume, &mut exact);
            assert!(ChaikinOscillator::classic()
                .batch_ohlcv_fast_into(&open, &high, &low, &close, &volume, &mut fast));
            assert_eq!(bits(&fast), bits(&exact));
            volume[at] = 1000.0;
        }
        // A series too short for an output, or an indicator past its first
        // bar, is the checked exact batch as well.
        let mut osc = ChaikinOscillator::classic();
        assert!(osc.batch_ohlcv_fast_into(
            &open[..5],
            &high[..5],
            &low[..5],
            &close[..5],
            &volume[..5],
            &mut fast[..5]
        ));
        assert!(osc.batch_ohlcv_fast_into(&open, &high, &low, &close, &volume, &mut fast));
        let mut reference = ChaikinOscillator::classic();
        reference.batch_hlcv_into(
            &high[..5],
            &low[..5],
            &close[..5],
            &volume[..5],
            &mut exact[..5],
        );
        reference.batch_hlcv_into(&high, &low, &close, &volume, &mut exact);
        assert_eq!(bits(&fast), bits(&exact));
    }
}
