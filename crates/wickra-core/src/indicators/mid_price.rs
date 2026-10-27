//! Midpoint Price (MIDPRICE) over a rolling window of high/low extremes.

use std::collections::VecDeque;

use crate::error::{Error, Result};
use crate::ohlcv::Candle;
use crate::traits::Indicator;

/// Midpoint Price (`MIDPRICE`): the average of the highest high and the lowest
/// low over the last `period` candles.
///
/// ```text
/// MIDPRICE = (highest(high, period) + lowest(low, period)) / 2
/// ```
///
/// Unlike [`MedianPrice`](crate::MedianPrice), which averages a single bar's own
/// high and low, `MIDPRICE` averages the *window* extremes — it is numerically
/// the centre line of [`Donchian`](crate::Donchian) channels, exposed as a
/// standalone scalar for TA-Lib parity. The first value is emitted once `period`
/// candles have been seen.
///
/// # Example
///
/// ```
/// use wickra_core::{Candle, Indicator, MidPrice};
///
/// let mut indicator = MidPrice::new(5).unwrap();
/// let mut last = None;
/// for i in 0..40 {
///     let base = 100.0 + f64::from(i);
///     let candle =
///         Candle::new(base, base + 2.0, base - 2.0, base + 1.0, 10.0, i64::from(i)).unwrap();
///     last = indicator.update(candle);
/// }
/// assert!(last.is_some());
/// ```
#[derive(Debug, Clone)]
pub struct MidPrice {
    period: usize,
    candles: VecDeque<Candle>,
}

impl MidPrice {
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
            candles: VecDeque::with_capacity(period),
        })
    }

    /// Configured period.
    pub const fn period(&self) -> usize {
        self.period
    }
}

impl Indicator for MidPrice {
    type Input = Candle;
    type Output = f64;

    #[inline]
    fn update(&mut self, candle: Candle) -> Option<f64> {
        if self.candles.len() == self.period {
            self.candles.pop_front();
        }
        self.candles.push_back(candle);
        if self.candles.len() < self.period {
            return None;
        }
        let highest = self
            .candles
            .iter()
            .map(|c| c.high)
            .fold(f64::NEG_INFINITY, f64::max);
        let lowest = self
            .candles
            .iter()
            .map(|c| c.low)
            .fold(f64::INFINITY, f64::min);
        Some(f64::midpoint(highest, lowest))
    }

    fn reset(&mut self) {
        self.candles.clear();
    }

    #[inline]
    fn warmup_period(&self) -> usize {
        self.period
    }

    #[inline]
    fn is_ready(&self) -> bool {
        self.candles.len() == self.period
    }

    #[inline]
    fn name(&self) -> &'static str {
        "MIDPRICE"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;
    use approx::assert_relative_eq;

    fn c(h: f64, l: f64, cl: f64) -> Candle {
        Candle::new(cl, h, l, cl, 1.0, 0).unwrap()
    }

    #[test]
    fn rejects_zero_period() {
        assert!(matches!(MidPrice::new(0), Err(Error::PeriodZero)));
    }

    #[test]
    fn accessors_report_config() {
        let mp = MidPrice::new(7).unwrap();
        assert_eq!(mp.period(), 7);
        assert_eq!(mp.name(), "MIDPRICE");
        assert_eq!(mp.warmup_period(), 7);
        assert!(!mp.is_ready());
    }

    #[test]
    fn averages_window_extremes() {
        // Window highs {12, 14, 16}, lows {8, 9, 10}: highest 16, lowest 8 -> 12.
        let candles = [c(12.0, 8.0, 10.0), c(14.0, 9.0, 11.0), c(16.0, 10.0, 12.0)];
        let mut mp = MidPrice::new(3).unwrap();
        let out: Vec<Option<f64>> = mp.batch(&candles);
        assert_eq!(out[0], None);
        assert_eq!(out[1], None);
        assert_relative_eq!(out[2].unwrap(), 12.0, epsilon = 1e-12);
        assert!(mp.is_ready());
    }

    #[test]
    fn window_slides_and_drops_old_extremes() {
        // After the spike leaves the window the midpoint falls back.
        let candles = [
            c(30.0, 10.0, 20.0),
            c(12.0, 8.0, 10.0),
            c(14.0, 9.0, 11.0),
            c(16.0, 10.0, 12.0),
        ];
        let mut mp = MidPrice::new(3).unwrap();
        let out: Vec<Option<f64>> = mp.batch(&candles);
        // Last window {12,14,16}/{8,9,10}: (16 + 8) / 2 = 12.
        assert_relative_eq!(out[3].unwrap(), 12.0, epsilon = 1e-12);
    }

    #[test]
    fn reset_clears_state() {
        let candles = [c(12.0, 8.0, 10.0), c(14.0, 9.0, 11.0), c(16.0, 10.0, 12.0)];
        let mut mp = MidPrice::new(3).unwrap();
        let _ = mp.batch(&candles);
        assert!(mp.is_ready());
        mp.reset();
        assert!(!mp.is_ready());
        assert_eq!(mp.update(candles[0]), None);
    }

    #[test]
    fn rejects_period_above_maximum() {
        assert!(matches!(
            MidPrice::new(crate::error::MAX_PERIOD + 1),
            Err(Error::InvalidPeriod { .. })
        ));
    }
}
