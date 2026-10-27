//! Elastic Volume-Weighted Moving Average (EVWMA).

use std::collections::VecDeque;

use crate::error::{Error, Result};
use crate::ohlcv::Candle;
use crate::traits::Indicator;

/// Christian P. Fries' Elastic Volume-Weighted Moving Average.
///
/// Unlike `VWMA` which is a per-bar weighted mean, `EVWMA` runs an
/// "elastic" recurrence whose smoothing weight is the bar's volume relative
/// to the running window-volume:
///
/// ```text
/// V_sum_t  = Σ volume_i over the last `period` candles
/// EVWMA_t  = ((V_sum_t - volume_t) * EVWMA_{t-1} + volume_t * close_t) / V_sum_t
/// ```
///
/// A bar whose volume is small compared to the window total barely moves the
/// average; a bar whose volume dominates the window pulls it strongly toward
/// the bar's close. The series is seeded with the close of the first candle
/// after the volume window has filled (i.e. after `period` candles).
///
/// If `V_sum_t == 0` (every candle in the window has zero volume), the
/// recurrence is undefined; the indicator holds its previous value.
///
/// Reference: Christian P. Fries, *Wilmott Magazine*, 2001.
///
/// # Example
///
/// ```
/// use wickra_core::{Candle, Evwma, Indicator};
///
/// let mut evwma = Evwma::new(20).unwrap();
/// let mut last = None;
/// for i in 0..40 {
///     let p = 100.0 + f64::from(i);
///     let candle = Candle::new(p, p + 1.0, p - 1.0, p, 10.0, i64::from(i)).unwrap();
///     last = evwma.update(candle);
/// }
/// assert!(last.is_some());
/// ```
#[derive(Debug, Clone)]
pub struct Evwma {
    period: usize,
    /// Rolling window of `(close, volume)` pairs, oldest at the front.
    window: VecDeque<(f64, f64)>,
    sum_v: f64,
    current: Option<f64>,
}

impl Evwma {
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
            window: VecDeque::with_capacity(period),
            sum_v: 0.0,
            current: None,
        })
    }

    /// Configured period.
    pub const fn period(&self) -> usize {
        self.period
    }

    /// Current value if available.
    pub const fn value(&self) -> Option<f64> {
        self.current
    }
}

impl Indicator for Evwma {
    type Input = Candle;
    type Output = f64;

    #[inline]
    fn update(&mut self, candle: Candle) -> Option<f64> {
        let close = candle.close;
        let volume = candle.volume;
        if self.window.len() == self.period {
            let (_, old_v) = self.window.pop_front().expect("window is non-empty");
            self.sum_v -= old_v;
        }
        self.window.push_back((close, volume));
        self.sum_v += volume;
        if self.window.len() < self.period {
            return None;
        }
        // The volume sum may be zero (every bar in the window had zero
        // volume); the recurrence is undefined, so seed/hold instead.
        if self.sum_v <= 0.0 {
            if self.current.is_none() {
                self.current = Some(close);
            }
            return self.current;
        }
        let prev = self.current.unwrap_or(close);
        let next = ((self.sum_v - volume) * prev + volume * close) / self.sum_v;
        self.current = Some(next);
        Some(next)
    }

    fn reset(&mut self) {
        self.window.clear();
        self.sum_v = 0.0;
        self.current = None;
    }

    #[inline]
    fn warmup_period(&self) -> usize {
        self.period
    }

    #[inline]
    fn is_ready(&self) -> bool {
        self.current.is_some()
    }

    #[inline]
    fn name(&self) -> &'static str {
        "EVWMA"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;
    use approx::assert_relative_eq;

    fn candle(close: f64, volume: f64, ts: i64) -> Candle {
        Candle::new(close, close, close, close, volume, ts).unwrap()
    }

    #[test]
    fn rejects_zero_period() {
        assert!(matches!(Evwma::new(0), Err(Error::PeriodZero)));
    }

    #[test]
    fn accessors_and_metadata() {
        let mut e = Evwma::new(5).unwrap();
        assert_eq!(e.period(), 5);
        assert_eq!(e.warmup_period(), 5);
        assert_eq!(e.name(), "EVWMA");
        assert_eq!(e.value(), None);
        for i in 0..5 {
            e.update(candle(10.0, 1.0, i));
        }
        assert!(e.value().is_some());
    }

    #[test]
    fn constant_series_yields_the_constant() {
        // A flat close — every (V_sum - v) * prev + v * close reduces to
        // V_sum * close, so the recurrence preserves the constant after the
        // first seeded sample.
        let mut e = Evwma::new(5).unwrap();
        let candles: Vec<Candle> = (0..30).map(|i| candle(42.0, 3.0, i)).collect();
        let out = e.batch(&candles);
        for v in out.iter().skip(4).flatten() {
            assert_relative_eq!(*v, 42.0, epsilon = 1e-12);
        }
    }

    #[test]
    fn reference_value_period_2() {
        // EVWMA(2). Bars: (close, volume) = (10, 1), (20, 3), (30, 1).
        //   Bar 1: window not full (size 1) -> None.
        //   Bar 2: window full, sum_v = 4, prev seeds to 20.
        //          EVWMA = ((4 - 3) * 20 + 3 * 20) / 4 = 80 / 4 = 20.
        //   Bar 3: window slides, sum_v = 4 (drops the 1, gains the 1).
        //          EVWMA = ((4 - 1) * 20 + 1 * 30) / 4 = (60 + 30) / 4 = 22.5.
        let mut e = Evwma::new(2).unwrap();
        assert_eq!(e.update(candle(10.0, 1.0, 0)), None);
        assert_relative_eq!(
            e.update(candle(20.0, 3.0, 1)).unwrap(),
            20.0,
            epsilon = 1e-12
        );
        assert_relative_eq!(
            e.update(candle(30.0, 1.0, 2)).unwrap(),
            22.5,
            epsilon = 1e-12
        );
    }

    #[test]
    fn warmup_emits_first_value_at_period() {
        let mut e = Evwma::new(4).unwrap();
        for i in 0..3 {
            assert_eq!(e.update(candle(10.0, 1.0, i)), None);
        }
        assert!(e.update(candle(10.0, 1.0, 3)).is_some());
    }

    #[test]
    fn zero_volume_window_holds_value() {
        // Every bar has zero volume: no participation, so the recurrence
        // can't move and EVWMA simply seeds to the first close.
        let mut e = Evwma::new(3).unwrap();
        e.update(candle(10.0, 0.0, 0));
        e.update(candle(15.0, 0.0, 1));
        let v = e.update(candle(20.0, 0.0, 2)).unwrap();
        assert_relative_eq!(v, 20.0, epsilon = 1e-12);
        // Next bar still flat-zero volume: holds 20.
        let v2 = e.update(candle(50.0, 0.0, 3)).unwrap();
        assert_relative_eq!(v2, 20.0, epsilon = 1e-12);
    }

    #[test]
    fn batch_equals_streaming() {
        let candles: Vec<Candle> = (0..60_i64)
            .map(|i| {
                let c = 100.0 + (i as f64 * 0.3).sin() * 8.0;
                candle(c, 1.0 + (i % 7) as f64, i)
            })
            .collect();
        let batch = Evwma::new(10).unwrap().batch(&candles);
        let mut b = Evwma::new(10).unwrap();
        let streamed: Vec<_> = candles.iter().map(|c| b.update(*c)).collect();
        assert_eq!(batch, streamed);
    }

    #[test]
    fn reset_clears_state() {
        let mut e = Evwma::new(3).unwrap();
        let candles: Vec<Candle> = (0..10).map(|i| candle(10.0 + i as f64, 2.0, i)).collect();
        e.batch(&candles);
        assert!(e.is_ready());
        e.reset();
        assert!(!e.is_ready());
        assert_eq!(e.update(candle(10.0, 1.0, 0)), None);
    }

    #[test]
    fn rejects_period_above_maximum() {
        assert!(matches!(
            Evwma::new(crate::error::MAX_PERIOD + 1),
            Err(Error::InvalidPeriod { .. })
        ));
    }
}
