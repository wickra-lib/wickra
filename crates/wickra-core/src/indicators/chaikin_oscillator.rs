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
        for (i, slot) in out.iter_mut().enumerate() {
            let candle = Candle::new_unchecked(close[i], high[i], low[i], close[i], volume[i], 0);
            *slot = self.update(candle).unwrap_or(f64::NAN);
        }
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
        if self.adl.value().is_some()
            || !self.fast.is_fresh()
            || !self.slow.is_fresh()
            || n < self.slow_period
            || ![high, low, close, volume]
                .iter()
                .all(|col| crate::fast::in_range(col))
        {
            self.batch_hlcv_into(high, low, close, volume, out);
            return;
        }
        let (adl, fast, slow) = wickra_simd::dispatch(crate::fast::ChaikinFast {
            high,
            low,
            close,
            volume,
            periods: (self.fast_period, self.slow_period),
            alphas: (self.fast.alpha(), self.slow.alpha()),
            out,
            _borrow: std::marker::PhantomData,
        });
        self.adl.resume_at(adl);
        self.fast.seed_to(fast);
        self.slow.seed_to(slow);
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
}
