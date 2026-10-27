//! `HiLo` Activator (Crabel).

use std::collections::VecDeque;

use crate::error::{Error, Result};
use crate::indicators::rolling_moments::RollingSum;
use crate::ohlcv::Candle;
use crate::traits::Indicator;

/// `HiLo` Activator — Robert Krausz's adaptation of Linda Bradford Raschke and
/// Larry Connors' "`HiLo`" rule, popularised by Toby Crabel. Two simple moving
/// averages — of the high and of the low — bracket price; the trailing stop
/// for a long sits at the SMA-of-low, and for a short at the SMA-of-high.
///
/// ```text
/// hi_sma = SMA(high, period)        // potential short stop
/// lo_sma = SMA(low,  period)        // potential long stop
///
/// state-machine:
///   long  while close > hi_sma_prev   ->  emit lo_sma_prev
///   short while close < lo_sma_prev   ->  emit hi_sma_prev
///   else: hold the previous side
/// ```
///
/// Comparing the close to the *previous* bar's SMA avoids look-ahead and gives
/// the indicator a one-bar lag — the classic Crabel formulation. A long signal
/// fires the bar after price closes above the high-SMA; the stop then trails
/// at the low-SMA. The first input that fills the SMA window seeds a long.
/// A common configuration is a `3`-period window.
///
/// # Example
///
/// ```
/// use wickra_core::{Candle, Indicator, HiLoActivator};
///
/// let mut indicator = HiLoActivator::new(3).unwrap();
/// let mut last = None;
/// for i in 0..40 {
///     let base = 100.0 + f64::from(i);
///     let candle =
///         Candle::new(base, base + 1.0, base - 1.0, base, 10.0, i64::from(i)).unwrap();
///     last = indicator.update(candle);
/// }
/// assert!(last.is_some());
/// ```
#[derive(Debug, Clone)]
pub struct HiLoActivator {
    period: usize,
    highs: VecDeque<f64>,
    lows: VecDeque<f64>,
    sum_high: RollingSum,
    sum_low: RollingSum,
    /// Last bar's `(hi_sma, lo_sma)`, used so today's signal is based on
    /// yesterday's SMAs (no look-ahead).
    prev_smas: Option<(f64, f64)>,
    /// `true` while the current trail is on the long side.
    long: bool,
    /// `true` once a signal has been emitted at least once.
    started: bool,
}

impl HiLoActivator {
    /// Construct a `HiLo` Activator with an explicit SMA window.
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
            highs: VecDeque::with_capacity(period),
            lows: VecDeque::with_capacity(period),
            sum_high: RollingSum::new(),
            sum_low: RollingSum::new(),
            prev_smas: None,
            long: true,
            started: false,
        })
    }

    /// Crabel's classic configuration: a `3`-bar window.
    pub fn classic() -> Self {
        Self::new(3).expect("classic period is valid")
    }

    /// Configured SMA window.
    pub const fn period(&self) -> usize {
        self.period
    }
}

impl Indicator for HiLoActivator {
    type Input = Candle;
    type Output = f64;

    #[inline]
    fn update(&mut self, candle: Candle) -> Option<f64> {
        if self.highs.len() == self.period {
            let old_high = self.highs.pop_front().expect("non-empty by check");
            let old_low = self.lows.pop_front().expect("non-empty by check");
            self.sum_high.evict(old_high);
            self.sum_low.evict(old_low);
        }
        self.highs.push_back(candle.high);
        self.lows.push_back(candle.low);
        self.sum_high.push(candle.high);
        self.sum_low.push(candle.low);
        if self.sum_high.needs_reseed(self.period) {
            self.sum_high.reseed(self.highs.iter().copied());
            self.sum_low.reseed(self.lows.iter().copied());
        }

        // Need today's SMA + yesterday's SMA to compare close vs the *previous*
        // bar's bands — so the very first ready bar only computes today's SMA
        // and stores it; emission begins on the next bar.
        if self.highs.len() < self.period {
            return None;
        }
        let p = self.period as f64;
        let hi_sma = self.sum_high.value() / p;
        let lo_sma = self.sum_low.value() / p;

        let out = if let Some((prev_hi, prev_lo)) = self.prev_smas {
            if candle.close > prev_hi {
                self.long = true;
            } else if candle.close < prev_lo {
                self.long = false;
            }
            self.started = true;
            if self.long {
                prev_lo
            } else {
                prev_hi
            }
        } else {
            // First SMA-ready bar seeds yesterday's bands for the next call.
            self.prev_smas = Some((hi_sma, lo_sma));
            return None;
        };
        self.prev_smas = Some((hi_sma, lo_sma));
        Some(out)
    }

    fn reset(&mut self) {
        self.highs.clear();
        self.lows.clear();
        self.sum_high.reset();
        self.sum_low.reset();
        self.prev_smas = None;
        self.long = true;
        self.started = false;
    }

    #[inline]
    fn warmup_period(&self) -> usize {
        self.period + 1
    }

    #[inline]
    fn is_ready(&self) -> bool {
        self.started
    }

    #[inline]
    fn name(&self) -> &'static str {
        "HiLoActivator"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;
    use approx::assert_relative_eq;

    fn c(high: f64, low: f64, close: f64, ts: i64) -> Candle {
        Candle::new(f64::midpoint(high, low), high, low, close, 1.0, ts).unwrap()
    }

    #[test]
    fn rejects_zero_period() {
        assert!(HiLoActivator::new(0).is_err());
    }

    #[test]
    fn accessors_and_metadata() {
        let s = HiLoActivator::classic();
        assert_eq!(s.period(), 3);
        assert_eq!(s.warmup_period(), 4);
        assert_eq!(s.name(), "HiLoActivator");
    }

    #[test]
    fn warmup_emits_none_until_period_plus_one() {
        let mut s = HiLoActivator::new(3).unwrap();
        // The first 3 candles fill the SMA; the 4th is the first emission.
        let candles: Vec<Candle> = (0..6)
            .map(|i| {
                let base = 100.0 + i as f64;
                c(base + 1.0, base - 1.0, base, i)
            })
            .collect();
        let out = s.batch(&candles);
        assert!(out[0].is_none());
        assert!(out[1].is_none());
        assert!(out[2].is_none());
        assert!(out[3].is_some(), "first emission lands at index period");
    }

    #[test]
    fn constant_series_stays_long_on_lo_sma() {
        let mut s = HiLoActivator::new(3).unwrap();
        // Flat candles: H=11, L=9, C=10. Both SMAs are constant.
        let candles: Vec<Candle> = (0..10).map(|i| c(11.0, 9.0, 10.0, i)).collect();
        for v in s.batch(&candles).into_iter().flatten() {
            // close (10) is not > 11 nor < 9, so the long seed persists -> lo_sma = 9.
            assert_relative_eq!(v, 9.0, epsilon = 1e-12);
        }
    }

    #[test]
    fn uptrend_keeps_emitting_low_sma_below_close() {
        let mut s = HiLoActivator::new(3).unwrap();
        let candles: Vec<Candle> = (0..30)
            .map(|i| {
                let base = 100.0 + i as f64;
                c(base + 1.0, base - 1.0, base, i)
            })
            .collect();
        let paired: Vec<(f64, f64)> = s
            .batch(&candles)
            .into_iter()
            .zip(candles.iter())
            .filter_map(|(o, c)| o.map(|v| (v, c.close)))
            .collect();
        assert!(
            paired.iter().all(|(stop, close)| stop < close),
            "uptrend stop should sit below the close"
        );
    }

    #[test]
    fn reset_clears_state() {
        let mut s = HiLoActivator::new(3).unwrap();
        let candles: Vec<Candle> = (0..20)
            .map(|i| {
                let base = 100.0 + i as f64;
                c(base + 1.0, base - 1.0, base, i)
            })
            .collect();
        s.batch(&candles);
        assert!(s.is_ready());
        s.reset();
        assert!(!s.is_ready());
        assert_eq!(s.update(candles[0]), None);
    }

    #[test]
    fn batch_equals_streaming() {
        let candles: Vec<Candle> = (0..80)
            .map(|i| {
                let mid = 100.0 + (i as f64 * 0.3).sin() * 8.0;
                c(mid + 1.5, mid - 1.5, mid + 0.5, i)
            })
            .collect();
        let mut a = HiLoActivator::classic();
        let mut b = HiLoActivator::classic();
        assert_eq!(
            a.batch(&candles),
            candles.iter().map(|x| b.update(*x)).collect::<Vec<_>>()
        );
    }

    #[test]
    fn rejects_period_above_maximum() {
        assert!(matches!(
            HiLoActivator::new(crate::error::MAX_PERIOD + 1),
            Err(Error::InvalidPeriod { .. })
        ));
    }
}
