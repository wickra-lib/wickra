//! Volume-Weighted Moving Average.

use std::collections::VecDeque;

use crate::error::{Error, Result};
use crate::indicators::rolling_moments::RollingSum;
use crate::ohlcv::Candle;
use crate::traits::Indicator;

/// Volume-Weighted Moving Average over a rolling window of `period` candles.
///
/// Each close is weighted by its own bar volume:
///
/// ```text
/// VWMA_t = Σ(close_i · volume_i) / Σ(volume_i)   over the last `period` bars
/// ```
///
/// High-volume bars pull the average toward their close, so VWMA reacts to
/// price moves that the market actually participated in and largely ignores
/// thin, low-conviction bars.
///
/// If every candle in the window has zero volume the weighted mean is
/// undefined; the indicator then falls back to the **unweighted** mean of the
/// `period` closes, so the output is always finite. The first output lands
/// after exactly `period` candles.
///
/// # Example
///
/// ```
/// use wickra_core::{Candle, Indicator, Vwma};
///
/// let mut indicator = Vwma::new(5).unwrap();
/// let mut last = None;
/// for i in 0..40 {
///     let p = 100.0 + f64::from(i);
///     let candle = Candle::new(p, p + 1.0, p - 1.0, p, 10.0, i64::from(i)).unwrap();
///     last = indicator.update(candle);
/// }
/// assert!(last.is_some());
/// ```
#[derive(Debug, Clone)]
pub struct Vwma {
    period: usize,
    /// Rolling window of `(close, volume)` pairs, oldest at the front.
    window: VecDeque<(f64, f64)>,
    sum_pv: RollingSum,
    sum_v: RollingSum,
    sum_close: RollingSum,
    current: Option<f64>,
}

impl Vwma {
    /// Construct a new VWMA with the given period.
    ///
    /// # Errors
    ///
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
            sum_pv: RollingSum::new(),
            sum_v: RollingSum::new(),
            sum_close: RollingSum::new(),
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

impl Indicator for Vwma {
    type Input = Candle;
    type Output = f64;

    #[inline]
    fn update(&mut self, candle: Candle) -> Option<f64> {
        let close = candle.close;
        let volume = candle.volume;
        if self.window.len() == self.period {
            let (old_close, old_volume) = self.window.pop_front().expect("window is non-empty");
            self.sum_pv.evict(old_close * old_volume);
            self.sum_v.evict(old_volume);
            self.sum_close.evict(old_close);
        }
        self.window.push_back((close, volume));
        self.sum_pv.push(close * volume);
        self.sum_v.push(volume);
        self.sum_close.push(close);
        if self.sum_pv.needs_reseed(self.period) {
            self.sum_pv.reseed(self.window.iter().map(|&(c, v)| c * v));
            self.sum_v.reseed(self.window.iter().map(|&(_, v)| v));
            self.sum_close.reseed(self.window.iter().map(|&(c, _)| c));
        }
        if self.window.len() < self.period {
            return None;
        }
        let value = if self.sum_v.value() > 0.0 {
            self.sum_pv.value() / self.sum_v.value()
        } else {
            // Degenerate window: every bar had zero volume. Fall back to the
            // plain mean of the closes so the output stays finite.
            self.sum_close.value() / self.period as f64
        };
        self.current = Some(value);
        Some(value)
    }

    fn reset(&mut self) {
        self.window.clear();
        self.sum_pv.reset();
        self.sum_v.reset();
        self.sum_close.reset();
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
        "VWMA"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;
    use approx::assert_relative_eq;

    /// Build a flat candle with a given close and volume.
    fn candle(close: f64, volume: f64, ts: i64) -> Candle {
        Candle::new(close, close, close, close, volume, ts).unwrap()
    }

    #[test]
    fn new_rejects_zero_period() {
        assert!(matches!(Vwma::new(0), Err(Error::PeriodZero)));
    }

    /// Cover the const accessors `period` / `value` (72-79) and the
    /// Indicator-impl `name` body (129-131). Existing tests inspect
    /// VWMA output but never query the metadata.
    #[test]
    fn accessors_and_metadata() {
        let mut v = Vwma::new(5).unwrap();
        assert_eq!(v.period(), 5);
        assert_eq!(v.name(), "VWMA");
        assert_eq!(v.value(), None);
        for i in 1..=5i64 {
            let p = 100.0 + i as f64;
            v.update(Candle::new(p, p, p, p, 1.0, i).unwrap());
        }
        assert!(v.value().is_some());
    }

    #[test]
    fn reference_value() {
        // VWMA(2): (10·1 + 20·3) / (1 + 3) = 70 / 4 = 17.5.
        let mut vwma = Vwma::new(2).unwrap();
        assert_eq!(vwma.update(candle(10.0, 1.0, 0)), None);
        assert_relative_eq!(
            vwma.update(candle(20.0, 3.0, 1)).unwrap(),
            17.5,
            epsilon = 1e-12
        );
        // Window slides: (20·3 + 30·1) / (3 + 1) = 90 / 4 = 22.5.
        assert_relative_eq!(
            vwma.update(candle(30.0, 1.0, 2)).unwrap(),
            22.5,
            epsilon = 1e-12
        );
    }

    #[test]
    fn zero_volume_window_falls_back_to_unweighted_mean() {
        let mut vwma = Vwma::new(2).unwrap();
        assert_eq!(vwma.update(candle(10.0, 0.0, 0)), None);
        // Both bars have zero volume: fall back to mean(10, 20) = 15.
        assert_relative_eq!(
            vwma.update(candle(20.0, 0.0, 1)).unwrap(),
            15.0,
            epsilon = 1e-12
        );
    }

    #[test]
    fn constant_series_yields_the_constant() {
        let mut vwma = Vwma::new(5).unwrap();
        let candles: Vec<Candle> = (0..30).map(|i| candle(42.0, 3.0, i)).collect();
        let out = vwma.batch(&candles);
        for x in out.iter().skip(4).flatten() {
            assert_relative_eq!(*x, 42.0, epsilon = 1e-12);
        }
    }

    #[test]
    fn high_volume_bar_pulls_the_average() {
        // A heavy bar at a higher close drags VWMA above the simple mean.
        let mut vwma = Vwma::new(3).unwrap();
        vwma.update(candle(10.0, 1.0, 0));
        vwma.update(candle(10.0, 1.0, 1));
        let v = vwma.update(candle(20.0, 100.0, 2)).unwrap();
        let simple_mean = (10.0 + 10.0 + 20.0) / 3.0;
        assert!(
            v > simple_mean,
            "{v} should exceed simple mean {simple_mean}"
        );
    }

    #[test]
    fn first_emission_at_warmup_period() {
        let mut vwma = Vwma::new(4).unwrap();
        assert_eq!(vwma.warmup_period(), 4);
        for i in 0..3 {
            assert_eq!(vwma.update(candle(10.0, 1.0, i)), None);
        }
        assert!(vwma.update(candle(10.0, 1.0, 3)).is_some());
    }

    #[test]
    fn reset_clears_state() {
        let mut vwma = Vwma::new(3).unwrap();
        let candles: Vec<Candle> = (0..10).map(|i| candle(10.0 + i as f64, 2.0, i)).collect();
        vwma.batch(&candles);
        assert!(vwma.is_ready());
        vwma.reset();
        assert!(!vwma.is_ready());
        assert_eq!(vwma.update(candle(10.0, 1.0, 0)), None);
    }

    #[test]
    fn batch_equals_streaming() {
        let candles: Vec<Candle> = (0..50_i64)
            .map(|i| {
                let c = 100.0 + (i as f64 * 0.3).sin() * 8.0;
                candle(c, 1.0 + (i % 7) as f64, i)
            })
            .collect();
        let batch = Vwma::new(8).unwrap().batch(&candles);
        let mut b = Vwma::new(8).unwrap();
        let streamed: Vec<_> = candles.iter().map(|c| b.update(*c)).collect();
        assert_eq!(batch, streamed);
    }

    #[test]
    fn rejects_period_above_maximum() {
        assert!(matches!(
            Vwma::new(crate::error::MAX_PERIOD + 1),
            Err(Error::InvalidPeriod { .. })
        ));
    }
}
