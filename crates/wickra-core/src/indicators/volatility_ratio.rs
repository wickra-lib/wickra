//! Schwager's Volatility Ratio — today's true range versus its typical level.

use crate::error::{Error, Result};
use crate::ohlcv::Candle;
use crate::traits::Indicator;

/// Schwager's Volatility Ratio — the current bar's true range divided by the
/// exponential moving average of the *prior* true ranges.
///
/// ```text
/// TR_t = true range of bar t
/// VR_t = TR_t / EMA_n(TR through bar t−1)
/// ```
///
/// Jack Schwager's volatility ratio measures how today's range compares to its
/// recent typical level: a reading above `2.0` marks a **wide-ranging day** —
/// today's true range is more than twice the smoothed average — which often
/// precedes or accompanies a reversal. The denominator is the exponential
/// moving average of true range *excluding the current bar*, seeded with the
/// simple average of the first `period` true ranges, so a single large bar
/// stands out instead of inflating its own benchmark.
///
/// True range is `max(high − low, |high − prev_close|, |low − prev_close|)`,
/// identical to the [`Atr`](crate::Atr) building block, but here it is compared
/// to a *standard* EMA (smoothing `2 / (period + 1)`) rather than Wilder
/// smoothing, which keeps the ratio distinct from `TR / ATR`. Each `update` is
/// O(1).
///
/// A flat market drives every true range — and the EMA — to `0`; the ratio is
/// then `0.0` rather than an undefined `0 / 0`. `Candle::new` rejects non-finite
/// fields, so no in-method finiteness guard is needed.
///
/// # Example
///
/// ```
/// use wickra_core::{Candle, Indicator, VolatilityRatio};
///
/// let mut indicator = VolatilityRatio::new(14).unwrap();
/// let mut last = None;
/// for i in 0..40 {
///     let base = 100.0 + f64::from(i);
///     let candle = Candle::new(base, base + 2.0, base - 1.0, base + 0.5, 1_000.0, 0).unwrap();
///     last = indicator.update(candle);
/// }
/// assert!(last.is_some());
/// ```
#[derive(Debug, Clone)]
pub struct VolatilityRatio {
    period: usize,
    alpha: f64,
    prev_close: Option<f64>,
    /// Sum and count of the first `period` true ranges, used to seed the EMA.
    seed_sum: f64,
    seed_count: usize,
    /// EMA of true range through the previous bar; `None` until seeded.
    ema: Option<f64>,
    last: Option<f64>,
}

impl VolatilityRatio {
    /// Construct a new volatility-ratio indicator.
    ///
    /// `period` is the number of true ranges that seed and smooth the
    /// denominator EMA.
    ///
    /// # Errors
    /// Returns [`Error::PeriodZero`] if `period == 0`.
    pub fn new(period: usize) -> Result<Self> {
        if period == 0 {
            return Err(Error::PeriodZero);
        }
        if period > crate::error::MAX_PERIOD {
            return Err(Error::InvalidPeriod {
                message: crate::error::PERIOD_ABOVE_MAX,
            });
        }
        Ok(Self {
            period,
            alpha: 2.0 / (period as f64 + 1.0),
            prev_close: None,
            seed_sum: 0.0,
            seed_count: 0,
            ema: None,
            last: None,
        })
    }

    /// Configured period.
    pub const fn period(&self) -> usize {
        self.period
    }

    /// Current value if available.
    pub const fn value(&self) -> Option<f64> {
        self.last
    }
}

impl Indicator for VolatilityRatio {
    type Input = Candle;
    type Output = f64;

    #[inline]
    fn update(&mut self, candle: Candle) -> Option<f64> {
        // The first bar has no previous close, so no true range can be formed.
        let Some(prev_close) = self.prev_close else {
            self.prev_close = Some(candle.close);
            return None;
        };
        let tr = candle.true_range(Some(prev_close));
        self.prev_close = Some(candle.close);

        match self.ema {
            None => {
                // Seeding the EMA with the simple average of the first `period`
                // true ranges; emit nothing until it is established.
                self.seed_sum += tr;
                self.seed_count += 1;
                if self.seed_count == self.period {
                    self.ema = Some(self.seed_sum / self.period as f64);
                }
                None
            }
            Some(prev_ema) => {
                // Denominator excludes the current bar (it is the EMA through the
                // previous bar). A flat benchmark yields 0.0, not 0/0.
                let vr = if prev_ema > 0.0 { tr / prev_ema } else { 0.0 };
                self.ema = Some(self.alpha * tr + (1.0 - self.alpha) * prev_ema);
                self.last = Some(vr);
                Some(vr)
            }
        }
    }

    fn reset(&mut self) {
        self.prev_close = None;
        self.seed_sum = 0.0;
        self.seed_count = 0;
        self.ema = None;
        self.last = None;
    }

    #[inline]
    fn warmup_period(&self) -> usize {
        // Bar 1 sets the previous close; bars 2..=period+1 seed the EMA; the
        // first ratio is emitted on bar period + 2.
        self.period + 2
    }

    #[inline]
    fn is_ready(&self) -> bool {
        self.last.is_some()
    }

    #[inline]
    fn name(&self) -> &'static str {
        "VolatilityRatio"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;
    use approx::assert_relative_eq;

    /// Build a candle with the given high/low/close (open = low, fixed volume).
    fn candle(high: f64, low: f64, close: f64) -> Candle {
        Candle::new_unchecked(low, high, low, close, 1_000.0, 0)
    }

    #[test]
    fn rejects_zero_period() {
        assert!(matches!(VolatilityRatio::new(0), Err(Error::PeriodZero)));
    }

    #[test]
    fn accessors_and_metadata() {
        let vr = VolatilityRatio::new(14).unwrap();
        assert_eq!(vr.period(), 14);
        assert_eq!(vr.warmup_period(), 16);
        assert_eq!(vr.name(), "VolatilityRatio");
        assert!(!vr.is_ready());
        assert_eq!(vr.value(), None);
    }

    #[test]
    fn first_emission_at_warmup_period() {
        let mut vr = VolatilityRatio::new(3).unwrap();
        // Build enough constant-range candles to reach warmup.
        let candles: Vec<Candle> = (0..10)
            .map(|i| {
                let base = 100.0 + f64::from(i);
                candle(base + 1.0, base - 1.0, base)
            })
            .collect();
        let out = vr.batch(&candles);
        // warmup_period == period + 2 == 5: the first emission is at index 4.
        let warmup = vr.warmup_period();
        assert_eq!(warmup, 5);
        for v in out.iter().take(warmup - 1) {
            assert!(v.is_none());
        }
        assert!(out[warmup - 1].is_some());
    }

    #[test]
    fn wide_ranging_day_exceeds_two() {
        // Steady true range of 2.0 seeds the EMA, then one bar with a far wider
        // range pushes the ratio above 2.0.
        let mut vr = VolatilityRatio::new(3).unwrap();
        let mut candles: Vec<Candle> = (0..6)
            .map(|i| {
                let base = 100.0 + f64::from(i);
                candle(base + 1.0, base - 1.0, base) // TR = 2.0 each
            })
            .collect();
        // A wide bar: range 10 around the last close (~105).
        candles.push(candle(110.0, 100.0, 105.0));
        let out = vr.batch(&candles);
        let last = out.last().unwrap().unwrap();
        assert!(last > 2.0, "wide-ranging day should exceed 2.0, got {last}");
    }

    #[test]
    fn steady_range_ratio_is_one() {
        // Constant true range -> EMA equals it -> ratio is exactly 1.0.
        let mut vr = VolatilityRatio::new(3).unwrap();
        let candles: Vec<Candle> = (0..12)
            .map(|i| {
                let base = 100.0 + f64::from(i);
                candle(base + 1.0, base - 1.0, base) // TR = 2.0 each
            })
            .collect();
        let out = vr.batch(&candles);
        assert_relative_eq!(out.last().unwrap().unwrap(), 1.0, epsilon = 1e-9);
    }

    #[test]
    fn flat_market_yields_zero() {
        // Zero-range candles: TR = 0, EMA = 0, ratio guarded to 0.0.
        let mut vr = VolatilityRatio::new(3).unwrap();
        let candles: Vec<Candle> = (0..10).map(|_| candle(100.0, 100.0, 100.0)).collect();
        let out = vr.batch(&candles);
        for v in out.into_iter().flatten() {
            assert_relative_eq!(v, 0.0, epsilon = 1e-12);
        }
    }

    #[test]
    fn output_is_non_negative() {
        let mut vr = VolatilityRatio::new(14).unwrap();
        let candles: Vec<Candle> = (0..200)
            .map(|i| {
                let base = 100.0 + (f64::from(i) * 0.3).sin() * 12.0;
                candle(base + 2.0, base - 2.0, base + 0.5)
            })
            .collect();
        for v in vr.batch(&candles).into_iter().flatten() {
            assert!(v >= 0.0, "volatility ratio must be non-negative, got {v}");
        }
    }

    #[test]
    fn reset_clears_state() {
        let mut vr = VolatilityRatio::new(3).unwrap();
        let candles: Vec<Candle> = (0..10)
            .map(|i| {
                let base = 100.0 + f64::from(i);
                candle(base + 1.0, base - 1.0, base)
            })
            .collect();
        vr.batch(&candles);
        assert!(vr.is_ready());
        vr.reset();
        assert!(!vr.is_ready());
        assert_eq!(vr.value(), None);
        assert_eq!(vr.update(candle(101.0, 99.0, 100.0)), None);
    }

    #[test]
    fn batch_equals_streaming() {
        let candles: Vec<Candle> = (0..120)
            .map(|i| {
                let base = 100.0 + (f64::from(i) * 0.25).sin() * 9.0;
                candle(base + 2.0, base - 1.5, base + 0.5)
            })
            .collect();
        let batch = VolatilityRatio::new(14).unwrap().batch(&candles);
        let mut b = VolatilityRatio::new(14).unwrap();
        let streamed: Vec<_> = candles.iter().map(|c| b.update(*c)).collect();
        assert_eq!(batch, streamed);
    }

    #[test]
    fn rejects_period_above_maximum() {
        assert!(matches!(
            VolatilityRatio::new(crate::error::MAX_PERIOD + 1),
            Err(Error::InvalidPeriod { .. })
        ));
    }
}
