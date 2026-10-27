//! Choppiness Index.

use std::collections::VecDeque;

use crate::error::{Error, Result};
use crate::ohlcv::Candle;
use crate::traits::Indicator;

/// Choppiness Index — is the market trending or just chopping sideways?
///
/// ```text
/// CI = 100 · log10( Σ(TR, n) / (highest_high(n) − lowest_low(n)) ) / log10(n)
/// ```
///
/// The ratio compares the *distance price actually travelled* (the summed true
/// range) with the *net ground it covered* (the high-low span of the window).
/// A clean trend travels almost exactly its span, so the ratio is near `1` and
/// `CI` near `0`; a choppy market criss-crosses far more than its span, so the
/// ratio is large and `CI` climbs toward `100`. The conventional reading is
/// `CI > 61.8` ranging, `CI < 38.2` trending. A perfectly flat window yields
/// `100` by convention.
///
/// # Example
///
/// ```
/// use wickra_core::{Candle, Indicator, ChoppinessIndex};
///
/// let mut indicator = ChoppinessIndex::new(14).unwrap();
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
pub struct ChoppinessIndex {
    period: usize,
    log_n: f64,
    prev_close: Option<f64>,
    tr_window: VecDeque<f64>,
    tr_sum: f64,
    highs: VecDeque<f64>,
    lows: VecDeque<f64>,
}

impl ChoppinessIndex {
    /// Construct a new Choppiness Index over `period` bars.
    ///
    /// # Errors
    /// Returns [`Error::InvalidPeriod`] if `period < 2` — the `log10(period)`
    /// denominator is zero for `period == 1` and undefined for `period == 0`.
    pub fn new(period: usize) -> Result<Self> {
        if period < 2 {
            return Err(Error::InvalidPeriod {
                message: "choppiness index needs period >= 2",
            });
        }
        if period > crate::error::MAX_PERIOD {
            return Err(Error::InvalidPeriod {
                message: crate::error::PERIOD_ABOVE_MAX,
            });
        }
        Ok(Self {
            period,
            log_n: (period as f64).log10(),
            prev_close: None,
            tr_window: VecDeque::with_capacity(period),
            tr_sum: 0.0,
            highs: VecDeque::with_capacity(period),
            lows: VecDeque::with_capacity(period),
        })
    }

    /// Configured period.
    pub const fn period(&self) -> usize {
        self.period
    }
}

impl Indicator for ChoppinessIndex {
    type Input = Candle;
    type Output = f64;

    #[inline]
    fn update(&mut self, candle: Candle) -> Option<f64> {
        let tr = candle.true_range(self.prev_close);
        self.prev_close = Some(candle.close);

        if self.tr_window.len() == self.period {
            self.tr_sum -= self.tr_window.pop_front().expect("non-empty");
            self.highs.pop_front();
            self.lows.pop_front();
        }
        self.tr_window.push_back(tr);
        self.tr_sum += tr;
        self.highs.push_back(candle.high);
        self.lows.push_back(candle.low);

        if self.tr_window.len() < self.period {
            return None;
        }
        let highest = self.highs.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        let lowest = self.lows.iter().copied().fold(f64::INFINITY, f64::min);
        let span = highest - lowest;
        if span == 0.0 {
            // A perfectly flat window: maximal choppiness by convention.
            return Some(100.0);
        }
        Some(100.0 * (self.tr_sum / span).log10() / self.log_n)
    }

    fn reset(&mut self) {
        self.prev_close = None;
        self.tr_window.clear();
        self.tr_sum = 0.0;
        self.highs.clear();
        self.lows.clear();
    }

    #[inline]
    fn warmup_period(&self) -> usize {
        self.period
    }

    #[inline]
    fn is_ready(&self) -> bool {
        self.tr_window.len() == self.period
    }

    #[inline]
    fn name(&self) -> &'static str {
        "ChoppinessIndex"
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
    fn reference_value_equal_range_bars() {
        // Two H=11 L=9 C=10 bars: TR = 2 each, ΣTR = 4; span = 11 - 9 = 2.
        // CI = 100 · log10(4 / 2) / log10(2) = 100.
        let mut ci = ChoppinessIndex::new(2).unwrap();
        let out = ci.batch(&[c(11.0, 9.0, 10.0, 0), c(11.0, 9.0, 10.0, 1)]);
        assert!(out[0].is_none());
        assert_relative_eq!(out[1].unwrap(), 100.0, epsilon = 1e-9);
    }

    #[test]
    fn flat_window_yields_hundred() {
        let candles: Vec<Candle> = (0..20).map(|i| c(10.0, 10.0, 10.0, i)).collect();
        let mut ci = ChoppinessIndex::new(14).unwrap();
        for v in ci.batch(&candles).into_iter().flatten() {
            assert_relative_eq!(v, 100.0, epsilon = 1e-9);
        }
    }

    #[test]
    fn steady_trend_reads_low() {
        // A clean one-directional march travels close to its span -> low CI.
        let candles: Vec<Candle> = (0..60)
            .map(|i| {
                let base = 100.0 + i as f64;
                c(base + 1.0, base - 1.0, base, i)
            })
            .collect();
        let mut ci = ChoppinessIndex::new(14).unwrap();
        for v in ci.batch(&candles).into_iter().flatten() {
            assert!(v < 50.0, "a steady trend should read below 50, got {v}");
            assert!(v >= 0.0, "CI must be non-negative, got {v}");
        }
    }

    #[test]
    fn first_emission_matches_warmup_period() {
        let candles: Vec<Candle> = (0..20).map(|i| c(11.0, 9.0, 10.0, i)).collect();
        let mut ci = ChoppinessIndex::new(8).unwrap();
        let out = ci.batch(&candles);
        assert_eq!(ci.warmup_period(), 8);
        for (i, v) in out.iter().enumerate().take(7) {
            assert!(v.is_none(), "index {i} must be None during warmup");
        }
        assert!(out[7].is_some(), "first value lands at warmup_period - 1");
    }

    #[test]
    fn rejects_period_below_two() {
        assert!(ChoppinessIndex::new(0).is_err());
        assert!(ChoppinessIndex::new(1).is_err());
        assert!(ChoppinessIndex::new(2).is_ok());
    }

    /// Cover the const accessor `period` (73-75) and the Indicator-impl
    /// `name` body (125-127). `warmup_period` is exercised elsewhere.
    #[test]
    fn accessors_and_metadata() {
        let ci = ChoppinessIndex::new(14).unwrap();
        assert_eq!(ci.period(), 14);
        assert_eq!(ci.name(), "ChoppinessIndex");
    }

    #[test]
    fn reset_clears_state() {
        let candles: Vec<Candle> = (0..20).map(|i| c(11.0, 9.0, 10.0, i)).collect();
        let mut ci = ChoppinessIndex::new(14).unwrap();
        ci.batch(&candles);
        assert!(ci.is_ready());
        ci.reset();
        assert!(!ci.is_ready());
        assert_eq!(ci.update(candles[0]), None);
    }

    #[test]
    fn batch_equals_streaming() {
        let candles: Vec<Candle> = (0..80)
            .map(|i| {
                let mid = 100.0 + (i as f64 * 0.3).sin() * 8.0;
                c(mid + 1.5, mid - 1.5, mid + 0.5, i)
            })
            .collect();
        let mut a = ChoppinessIndex::new(14).unwrap();
        let mut b = ChoppinessIndex::new(14).unwrap();
        assert_eq!(
            a.batch(&candles),
            candles.iter().map(|x| b.update(*x)).collect::<Vec<_>>()
        );
    }

    #[test]
    fn rejects_period_above_maximum() {
        assert!(matches!(
            ChoppinessIndex::new(crate::error::MAX_PERIOD + 1),
            Err(Error::InvalidPeriod { .. })
        ));
    }
}
