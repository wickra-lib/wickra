#![allow(clippy::doc_markdown)]

//! Tom DeMark Range Expansion Index (TD REI).
//!
//! The TD REI is a `period`-bar bounded oscillator in `[-100, 100]` that
//! detects exhaustion via comparisons of the current bar's range to the bars
//! two, five-or-six and seven-or-eight bars earlier. The canonical TD REI uses
//! a `period` of 5.
//!
//! Per bar `i` (requires history through `i - 8`):
//!
//! ```text
//! overlap      = (high[i]   >= low[i-5]   OR high[i]   >= low[i-6])
//!            AND (low[i]    <= high[i-5]  OR low[i]    <= high[i-6])
//! overlap_back = (high[i-2] >= close[i-7] OR high[i-2] >= close[i-8])
//!            AND (low[i-2]  <= close[i-7] OR low[i-2]  <= close[i-8])
//!
//! if overlap OR overlap_back:
//!     numerator   = (high[i] - high[i-2]) + (low[i] - low[i-2])
//! else:
//!     numerator   = 0
//!
//! denominator = |high[i] - high[i-2]| + |low[i] - low[i-2]|     (every bar)
//!
//! REI(i) = 100 * sum(numerator, period) / sum(denominator, period)
//! ```
//!
//! When the windowed denominator is zero the indicator falls back to `0` (the
//! neutral midpoint). Readings above `+60` are typically considered
//! overbought; below `-60` oversold.

use std::collections::VecDeque;

use crate::error::{Error, Result};
use crate::ohlcv::Candle;
use crate::traits::Indicator;

/// TD Range Expansion Index oscillator.
#[derive(Debug, Clone)]
pub struct TdRei {
    period: usize,
    // Need at least the last 9 candles for the lookback comparisons; we keep a
    // rolling window long enough for the rule plus enough numerator/
    // denominator history.
    candles: VecDeque<Candle>,
    numerators: VecDeque<f64>,
    denominators: VecDeque<f64>,
    last_value: Option<f64>,
}

/// Minimum history required to evaluate the TD REI per-bar rule. The
/// numerator and denominator reference `bar[i-2]`, the first condition
/// `bar[i-5]` / `bar[i-6]` and the alternative condition the closes of
/// `bar[i-7]` / `bar[i-8]`, so the candle eight bars back must be available.
const LOOKBACK: usize = 9;

impl TdRei {
    /// Construct a TD REI with the given averaging window. The classic
    /// DeMark configuration is `period = 5`.
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
            candles: VecDeque::with_capacity(LOOKBACK),
            numerators: VecDeque::with_capacity(period),
            denominators: VecDeque::with_capacity(period),
            last_value: None,
        })
    }

    /// DeMark's classic configuration: `period = 5`.
    pub fn classic() -> Self {
        Self::new(5).expect("classic TD REI parameters are valid")
    }

    /// Configured window.
    pub const fn period(&self) -> usize {
        self.period
    }

    /// Latest emitted value if available.
    pub const fn value(&self) -> Option<f64> {
        self.last_value
    }
}

impl Indicator for TdRei {
    type Input = Candle;
    type Output = f64;

    fn update(&mut self, candle: Candle) -> Option<f64> {
        // Maintain a rolling window of the last `LOOKBACK` candles (front =
        // 6 bars ago when full).
        if self.candles.len() == LOOKBACK {
            self.candles.pop_front();
        }
        if self.candles.len() < LOOKBACK - 1 {
            // Need 8 previous candles before we can evaluate the rule on the
            // current one.
            self.candles.push_back(candle);
            return None;
        }
        // `candles` holds the 8 previous bars, oldest first: index 0 is bar
        // i-8, index 7 is bar i-1.
        let prev2 = self.candles[6];
        let prev5 = self.candles[3];
        let prev6 = self.candles[2];
        let close7 = self.candles[1].close;
        let close8 = self.candles[0].close;

        // The bar's range overlaps the range of 5-6 bars earlier ...
        let overlap = (candle.high >= prev5.low || candle.high >= prev6.low)
            && (candle.low <= prev5.high || candle.low <= prev6.high);
        // ... or the bar two back overlaps the closes of 7-8 bars earlier.
        let overlap_back = (prev2.high >= close7 || prev2.high >= close8)
            && (prev2.low <= close7 || prev2.low <= close8);

        let raw_num = (candle.high - prev2.high) + (candle.low - prev2.low);
        let denominator = (candle.high - prev2.high).abs() + (candle.low - prev2.low).abs();
        let numerator = if overlap || overlap_back {
            raw_num
        } else {
            0.0
        };

        if self.numerators.len() == self.period {
            self.numerators.pop_front();
            self.denominators.pop_front();
        }
        self.numerators.push_back(numerator);
        self.denominators.push_back(denominator);
        self.candles.push_back(candle);

        if self.numerators.len() < self.period {
            return None;
        }
        let sum_num: f64 = self.numerators.iter().sum();
        let sum_den: f64 = self.denominators.iter().sum();
        let v = if sum_den == 0.0 {
            0.0
        } else {
            // |numerator| <= denominator bar by bar, so the ratio is bounded;
            // the clamp only absorbs the last-bit rounding of the two sums.
            (100.0 * sum_num / sum_den).clamp(-100.0, 100.0)
        };
        self.last_value = Some(v);
        Some(v)
    }

    fn reset(&mut self) {
        self.candles.clear();
        self.numerators.clear();
        self.denominators.clear();
        self.last_value = None;
    }

    #[inline]
    fn warmup_period(&self) -> usize {
        // 8 bars to fill the lookback plus `period` updates to fill the
        // numerator / denominator buffers.
        (LOOKBACK - 1) + self.period
    }

    #[inline]
    fn is_ready(&self) -> bool {
        self.last_value.is_some()
    }

    #[inline]
    fn name(&self) -> &'static str {
        "TDREI"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;
    use approx::assert_relative_eq;

    fn c(high: f64, low: f64, close: f64, ts: i64) -> Candle {
        Candle::new_unchecked(close, high, low, close, 0.0, ts)
    }

    #[test]
    fn flat_market_yields_neutral_zero() {
        // All highs and lows equal -> denominator is identically zero, so the
        // indicator emits its neutral fallback of 0.
        let candles: Vec<Candle> = (0..40).map(|i| c(11.0, 9.0, 10.0, i)).collect();
        let mut rei = TdRei::classic();
        let out = rei.batch(&candles);
        for v in out.iter().skip(rei.warmup_period()).copied().flatten() {
            assert_relative_eq!(v, 0.0, epsilon = 1e-12);
        }
    }

    #[test]
    fn pure_uptrend_pegs_indicator_at_100() {
        // Every bar makes strictly higher highs and lows. Both range-overlap
        // conditions hold (current high > all previous lows; current low > all
        // previous highs is false, but we need current low <= some prev
        // high). For a slow steady uptrend cond2 still holds because
        // current low < prev5/prev6 highs as long as the slope is moderate.
        // With slope 1 and spread 2 (low to high), cond2 fails after ~3 bars.
        // Use a smaller slope so cond2 holds throughout.
        let candles: Vec<Candle> = (0..40)
            .map(|i| {
                let m = 100.0 + f64::from(i) * 0.1;
                c(m + 1.0, m - 1.0, m, i64::from(i))
            })
            .collect();
        let mut rei = TdRei::classic();
        let last = rei.batch(&candles).into_iter().flatten().last().unwrap();
        // Every numerator is positive (price moving up) and equals the
        // denominator in magnitude (no sign flips), so REI saturates at 100.
        assert_relative_eq!(last, 100.0, epsilon = 1e-9);
    }

    #[test]
    fn pure_downtrend_pegs_indicator_at_minus_100() {
        let candles: Vec<Candle> = (0..40)
            .map(|i| {
                let m = 100.0 - f64::from(i) * 0.1;
                c(m + 1.0, m - 1.0, m, i64::from(i))
            })
            .collect();
        let mut rei = TdRei::classic();
        let last = rei.batch(&candles).into_iter().flatten().last().unwrap();
        assert_relative_eq!(last, -100.0, epsilon = 1e-9);
    }

    #[test]
    fn stays_in_minus_100_to_100() {
        let candles: Vec<Candle> = (0..200)
            .map(|i| {
                let m = 50.0 + (f64::from(i) * 0.2).sin() * 5.0;
                c(m + 1.0, m - 1.0, m, i64::from(i))
            })
            .collect();
        let mut rei = TdRei::classic();
        for v in rei.batch(&candles).into_iter().flatten() {
            assert!((-100.0..=100.0).contains(&v), "out of range: {v}");
        }
    }

    #[test]
    fn batch_equals_streaming() {
        let candles: Vec<Candle> = (0..80)
            .map(|i| {
                let m = 100.0 + (f64::from(i) * 0.3).sin() * 5.0;
                c(m + 1.0, m - 1.0, m, i64::from(i))
            })
            .collect();
        let mut a = TdRei::classic();
        let mut b = TdRei::classic();
        assert_eq!(
            a.batch(&candles),
            candles.iter().map(|x| b.update(*x)).collect::<Vec<_>>()
        );
    }

    #[test]
    fn rejects_zero_period() {
        assert!(matches!(TdRei::new(0), Err(Error::PeriodZero)));
    }

    #[test]
    fn reset_clears_state() {
        let candles: Vec<Candle> = (0..40)
            .map(|i| {
                let m = 100.0 + f64::from(i) * 0.1;
                c(m + 1.0, m - 1.0, m, i64::from(i))
            })
            .collect();
        let mut rei = TdRei::classic();
        rei.batch(&candles);
        assert!(rei.is_ready());
        rei.reset();
        assert!(!rei.is_ready());
        assert_eq!(rei.update(candles[0]), None);
        assert_eq!(rei.value(), None);
    }

    #[test]
    fn accessors_and_metadata() {
        let rei = TdRei::classic();
        assert_eq!(rei.period(), 5);
        assert_eq!(rei.warmup_period(), 8 + 5);
        assert_eq!(rei.name(), "TDREI");
    }

    /// Eight base bars (idx 0..=7): idx 0 and 1 close at `far` (range +-1),
    /// idx 2..=7 are h 11, l 9, c 10.
    fn base(far: f64) -> Vec<Candle> {
        (0..8)
            .map(|i| {
                let m = if i < 2 { far } else { 10.0 };
                c(m + 1.0, m - 1.0, m, i64::from(i))
            })
            .collect()
    }

    #[test]
    fn hand_computed_overlap_mixed_sign() {
        // period 1, bar 8 = h 13, l 8.5. prev2 = idx 6 (h 11, l 9).
        // overlap: 13 >= 9 and 8.5 <= 11 -> numerator counts.
        // numerator = (13 - 11) + (8.5 - 9) = 1.5; denominator = 2 + 0.5 = 2.5
        // REI = 100 * 1.5 / 2.5 = 60.
        let mut rei = TdRei::new(1).unwrap();
        let mut candles = base(10.0);
        candles.push(c(13.0, 8.5, 10.0, 8));
        let out = rei.batch(&candles);
        assert_relative_eq!(out[8].unwrap(), 60.0, epsilon = 1e-12);
    }

    #[test]
    fn numerator_is_gated_but_denominator_counts() {
        // Bars 0, 1 close at 50, so overlap_back fails: prev2.high 11 < 50.
        // Bar 8 = h 30, l 25: low 25 > highs 11 of idx 2/3 -> no overlap.
        // numerator = 0; denominator = (30 - 11) + (25 - 9) = 35 -> REI 0
        // (a genuine zero, not the empty-denominator fallback).
        let mut rei = TdRei::new(1).unwrap();
        let mut candles = base(50.0);
        candles.push(c(30.0, 25.0, 27.0, 8));
        assert_eq!(rei.batch(&candles)[8], Some(0.0));
        assert_eq!(rei.denominators.back().copied(), Some(35.0));
        assert_eq!(rei.numerators.back().copied(), Some(0.0));
    }

    #[test]
    fn overlap_back_alone_enables_numerator() {
        // Same bar 8 as above (no overlap), but idx 0, 1 close at 10, so
        // prev2 (h 11, l 9) brackets close[i-7] = 10 -> overlap_back holds.
        // numerator = denominator = 19 + 16 = 35 -> REI 100.
        let mut rei = TdRei::new(1).unwrap();
        let mut candles = base(10.0);
        candles.push(c(30.0, 25.0, 27.0, 8));
        assert_eq!(rei.batch(&candles)[8], Some(100.0));
    }

    #[test]
    fn hand_computed_period_two_window() {
        // period 2, bars 0, 1 close at 50.
        // Bar 8 (h 30, l 25, c 27): gated as above -> num 0, den 35.
        // Bar 9 (h 12, l 10, c 11): prev2 = idx 7 (11, 9); prev5 = idx 4,
        // prev6 = idx 3 (h 11, l 9): 12 >= 9 and 10 <= 11 -> overlap.
        // num = (12 - 11) + (10 - 9) = 2, den = 2.
        // REI = 100 * (0 + 2) / (35 + 2) = 200 / 37.
        let mut rei = TdRei::new(2).unwrap();
        let mut candles = base(50.0);
        candles.push(c(30.0, 25.0, 27.0, 8));
        candles.push(c(12.0, 10.0, 11.0, 9));
        let out = rei.batch(&candles);
        assert_eq!(rei.warmup_period(), 10);
        assert!(out[..9].iter().all(Option::is_none));
        assert_relative_eq!(out[9].unwrap(), 200.0 / 37.0, epsilon = 1e-12);
    }

    #[test]
    fn first_value_lands_at_warmup_minus_one() {
        let candles: Vec<Candle> = (0..40)
            .map(|i| {
                let m = 100.0 + (f64::from(i) * 0.4).sin() * 3.0;
                c(m + 1.0, m - 1.0, m, i64::from(i))
            })
            .collect();
        for period in [1, 2, 5, 14] {
            let mut rei = TdRei::new(period).unwrap();
            let warm = rei.warmup_period();
            assert_eq!(warm, 8 + period);
            let out = rei.batch(&candles);
            assert!(out[..warm - 1].iter().all(Option::is_none));
            assert!(out[warm - 1..].iter().all(Option::is_some));
        }
    }

    #[test]
    fn rejects_period_above_max() {
        let err = TdRei::new(crate::error::MAX_PERIOD + 1).unwrap_err();
        assert!(matches!(err, Error::InvalidPeriod { .. }));
    }

    #[test]
    fn reset_reproduces_fresh_run() {
        let candles: Vec<Candle> = (0..60)
            .map(|i| {
                let m = 100.0 + (f64::from(i) * 0.6).sin() * 4.0;
                c(m + 1.5, m - 0.5, m, i64::from(i))
            })
            .collect();
        let mut fresh = TdRei::classic();
        let expected = fresh.batch(&candles);
        let mut rei = TdRei::classic();
        rei.batch(&candles[..23]);
        rei.reset();
        assert_eq!(rei.batch(&candles), expected);
    }

    #[test]
    fn batch_nan_into_matches_streaming() {
        let candles: Vec<Candle> = (0..80)
            .map(|i| {
                let m = 100.0 + (f64::from(i) * 0.9).sin() * 6.0;
                c(m + 1.5, m - 0.5, m, i64::from(i))
            })
            .collect();
        let mut a = TdRei::classic();
        let mut out = vec![0.0; candles.len()];
        a.batch_nan_into(&candles, &mut out);
        let mut b = TdRei::classic();
        let streamed: Vec<f64> = candles
            .iter()
            .map(|x| b.update(*x).unwrap_or(f64::NAN))
            .collect();
        assert!(out
            .iter()
            .zip(&streamed)
            .all(|(x, y)| x.to_bits() == y.to_bits()));
    }
}
