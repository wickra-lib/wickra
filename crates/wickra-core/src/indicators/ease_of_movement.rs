//! Ease of Movement (Arms).

use std::collections::VecDeque;

use crate::error::{Error, Result};
use crate::indicators::rolling_moments::RollingSum;
use crate::ohlcv::Candle;
use crate::traits::Indicator;

/// Richard Arms' Ease of Movement — how far price travels per unit of volume.
///
/// ```text
/// distance_t = (high_t + low_t)/2 − (high_{t−1} + low_{t−1})/2
/// EMV_t      = distance_t · (high_t − low_t) · divisor / volume_t
/// EOM_t      = SMA(EMV, period)_t
/// ```
///
/// A large positive EMV means price climbed a long way on light volume — it
/// moved "easily"; a value near zero means heavy volume was needed to shift
/// price at all. The `divisor` only rescales the output: the conventional
/// `1e8` keeps `EMV` in a readable range for typical share volumes. A bar with
/// zero volume contributes `EMV = 0` (no trading carries no signal), as does a
/// zero-range bar. The first candle only seeds the previous midpoint, so the
/// first value appears on candle `period + 1`.
///
/// # Example
///
/// ```
/// use wickra_core::{Candle, Indicator, EaseOfMovement};
///
/// let mut indicator = EaseOfMovement::new(14).unwrap();
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
pub struct EaseOfMovement {
    period: usize,
    divisor: f64,
    prev_mid: Option<f64>,
    window: VecDeque<f64>,
    sum: RollingSum,
}

impl EaseOfMovement {
    /// Construct an Ease of Movement with the conventional `1e8` volume divisor.
    ///
    /// # Errors
    /// Returns [`Error::PeriodZero`] if `period == 0`.
    pub fn new(period: usize) -> Result<Self> {
        Self::with_divisor(period, 100_000_000.0)
    }

    /// Construct an Ease of Movement with an explicit volume divisor. The
    /// divisor is a pure output-scaling constant; pick whatever keeps `EMV`
    /// readable for your instrument's volume magnitude.
    ///
    /// # Errors
    /// Returns [`Error::PeriodZero`] if `period == 0` and
    /// [`Error::NonPositiveMultiplier`] if `divisor` is not strictly positive
    /// and finite.
    pub fn with_divisor(period: usize, divisor: f64) -> Result<Self> {
        if period == 0 {
            return Err(Error::PeriodZero);
        }
        if period > crate::error::MAX_PERIOD {
            return Err(Error::InvalidPeriod {
                message: crate::error::PERIOD_ABOVE_MAX,
            });
        }
        if !divisor.is_finite() || divisor <= 0.0 {
            return Err(Error::NonPositiveMultiplier);
        }
        Ok(Self {
            period,
            divisor,
            prev_mid: None,
            window: VecDeque::with_capacity(period),
            sum: RollingSum::new(),
        })
    }

    /// Configured period.
    pub const fn period(&self) -> usize {
        self.period
    }

    /// Configured volume divisor.
    pub const fn divisor(&self) -> f64 {
        self.divisor
    }
}

impl Indicator for EaseOfMovement {
    type Input = Candle;
    type Output = f64;

    #[inline]
    fn update(&mut self, candle: Candle) -> Option<f64> {
        let mid = f64::midpoint(candle.high, candle.low);
        let Some(prev_mid) = self.prev_mid else {
            // The first candle only establishes the previous midpoint.
            self.prev_mid = Some(mid);
            return None;
        };
        let distance = mid - prev_mid;
        let range = candle.high - candle.low;
        let emv = if candle.volume == 0.0 {
            // No volume traded — the move carries no ease-of-movement signal.
            0.0
        } else {
            distance * range * self.divisor / candle.volume
        };
        self.prev_mid = Some(mid);

        if self.window.len() == self.period {
            let oldest = self.window.pop_front().expect("non-empty");
            self.sum.evict(oldest);
        }
        self.window.push_back(emv);
        self.sum.push(emv);
        if self.sum.needs_reseed(self.period) {
            self.sum.reseed(self.window.iter().copied());
        }
        if self.window.len() < self.period {
            return None;
        }
        Some(self.sum.value() / self.period as f64)
    }

    fn reset(&mut self) {
        self.prev_mid = None;
        self.window.clear();
        self.sum.reset();
    }

    #[inline]
    fn warmup_period(&self) -> usize {
        // One seed candle establishes the first previous midpoint, then
        // `period` EMV values fill the averaging window.
        self.period + 1
    }

    #[inline]
    fn is_ready(&self) -> bool {
        self.window.len() == self.period
    }

    #[inline]
    fn name(&self) -> &'static str {
        "EaseOfMovement"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;
    use approx::assert_relative_eq;

    fn candle(open: f64, high: f64, low: f64, close: f64, volume: f64, ts: i64) -> Candle {
        Candle::new(open, high, low, close, volume, ts).unwrap()
    }

    #[test]
    fn reference_values() {
        // EOM(period = 1, divisor = 1): one EMV value is its own average.
        //   candle 1: midpoint (10 + 8)/2 = 9 only seeds the previous mid.
        //   candle 2: mid = (14 + 10)/2 = 12, distance = 3, range = 4,
        //             EMV = 3 * 4 * 1 / 100 = 0.12.
        let mut eom = EaseOfMovement::with_divisor(1, 1.0).unwrap();
        let out = eom.batch(&[
            candle(9.0, 10.0, 8.0, 9.0, 50.0, 0),
            candle(12.0, 14.0, 10.0, 12.0, 100.0, 1),
        ]);
        assert!(out[0].is_none());
        assert_relative_eq!(out[1].unwrap(), 0.12, epsilon = 1e-12);
    }

    #[test]
    fn rising_midpoints_yield_positive_eom() {
        // Strictly rising midpoints on constant volume -> every EMV is
        // positive, so the averaged EOM is positive.
        let candles: Vec<Candle> = (0..40)
            .map(|i| {
                let base = 100.0 + i as f64;
                candle(base, base + 1.0, base - 1.0, base, 100.0, i)
            })
            .collect();
        let mut eom = EaseOfMovement::new(14).unwrap();
        for v in eom.batch(&candles).into_iter().flatten() {
            assert!(v > 0.0, "EOM {v} should be positive on a rising series");
        }
    }

    #[test]
    fn constant_series_yields_zero() {
        // Unchanging candles -> zero distance -> EMV is zero throughout.
        let candles: Vec<Candle> = (0..30)
            .map(|i| candle(10.0, 11.0, 9.0, 10.0, 50.0, i))
            .collect();
        let mut eom = EaseOfMovement::new(10).unwrap();
        for v in eom.batch(&candles).into_iter().flatten() {
            assert_relative_eq!(v, 0.0, epsilon = 1e-12);
        }
    }

    #[test]
    fn zero_volume_contributes_zero() {
        // A zero-volume bar yields EMV = 0 instead of dividing by zero.
        let candles: Vec<Candle> = (0..20)
            .map(|i| {
                let base = 100.0 + i as f64;
                candle(base, base + 1.0, base - 1.0, base, 0.0, i)
            })
            .collect();
        let mut eom = EaseOfMovement::new(10).unwrap();
        for v in eom.batch(&candles).into_iter().flatten() {
            assert_relative_eq!(v, 0.0, epsilon = 1e-12);
        }
    }

    #[test]
    fn first_value_on_period_plus_one_candle() {
        let candles: Vec<Candle> = (0..12)
            .map(|i| {
                let base = 100.0 + i as f64;
                candle(base, base + 1.0, base - 1.0, base, 50.0, i)
            })
            .collect();
        let mut eom = EaseOfMovement::new(5).unwrap();
        let out = eom.batch(&candles);
        for (i, v) in out.iter().enumerate().take(5) {
            assert!(v.is_none(), "index {i} must be None during warmup");
        }
        assert!(out[5].is_some(), "first EOM lands at index period");
        assert_eq!(eom.warmup_period(), 6);
    }

    #[test]
    fn rejects_invalid_input() {
        assert!(EaseOfMovement::new(0).is_err());
        assert!(EaseOfMovement::with_divisor(14, 0.0).is_err());
        assert!(EaseOfMovement::with_divisor(14, -1.0).is_err());
        assert!(EaseOfMovement::with_divisor(14, f64::NAN).is_err());
    }

    /// Cover the const accessors `period` / `divisor` (82-90) and the
    /// Indicator-impl `name` body (141-143). Existing tests inspect EMV
    /// output but never query the metadata methods.
    #[test]
    fn accessors_and_metadata() {
        let emv = EaseOfMovement::new(14).unwrap();
        assert_eq!(emv.period(), 14);
        // The canonical divisor (per the new() default) — keep in sync with src.
        assert_relative_eq!(emv.divisor(), 100_000_000.0, epsilon = 1e-6);
        assert_eq!(emv.name(), "EaseOfMovement");
    }

    #[test]
    fn reset_clears_state() {
        let candles: Vec<Candle> = (0..30)
            .map(|i| {
                let base = 100.0 + i as f64;
                candle(base, base + 1.0, base - 1.0, base, 50.0, i)
            })
            .collect();
        let mut eom = EaseOfMovement::new(10).unwrap();
        eom.batch(&candles);
        assert!(eom.is_ready());
        eom.reset();
        assert!(!eom.is_ready());
        assert_eq!(eom.update(candles[0]), None);
    }

    #[test]
    fn batch_equals_streaming() {
        let candles: Vec<Candle> = (0..80)
            .map(|i| {
                let mid = 100.0 + (i as f64 * 0.3).sin() * 8.0;
                candle(
                    mid,
                    mid + 2.0,
                    mid - 2.0,
                    mid + 0.5,
                    10.0 + (i % 5) as f64,
                    i,
                )
            })
            .collect();
        let mut a = EaseOfMovement::new(14).unwrap();
        let mut b = EaseOfMovement::new(14).unwrap();
        assert_eq!(
            a.batch(&candles),
            candles.iter().map(|x| b.update(*x)).collect::<Vec<_>>()
        );
    }

    #[test]
    fn rejects_period_above_maximum() {
        assert!(matches!(
            EaseOfMovement::with_divisor(crate::error::MAX_PERIOD + 1, 1.0),
            Err(Error::InvalidPeriod { .. })
        ));
    }
}
