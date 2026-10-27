//! Random Walk Index (RWI).

use std::collections::VecDeque;

use crate::error::{Error, Result};
use crate::ohlcv::Candle;
use crate::traits::Indicator;

/// Random Walk Index output: the bullish (high) and bearish (low) lines.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RwiOutput {
    /// `RWI_High` — strength of the trend up vs. a random walk.
    pub high: f64,
    /// `RWI_Low` — strength of the trend down vs. a random walk.
    pub low: f64,
}

/// Mike Poulos' Random Walk Index — a trend-vs.-random-walk indicator that
/// asks "how many standard deviations away from a random walk is the current
/// move?".
///
/// For each lookback `i ∈ [2, period]`, RWI computes the ratio of the actual
/// price displacement over `i` bars to the expected displacement of a random
/// walk of the same length:
///
/// ```text
/// RWI_High_t(i) = (high_t  − low_{t-i+1})  / (ATR_i(t) * sqrt(i))
/// RWI_Low_t(i)  = (high_{t-i+1} − low_t)   / (ATR_i(t) * sqrt(i))
/// ```
///
/// where `ATR_i(t)` is the simple average of true-range over the most recent
/// `i` bars. The reported `RWI_High_t` / `RWI_Low_t` are the maxima of these
/// ratios across all lookbacks `i ∈ [2, period]`.
///
/// `RWI_High` crossing above `RWI_Low` and exceeding 1 (`> 2` is the typical
/// strong-trend threshold) signals an uptrend dominating random-walk; the
/// mirror situation flags a downtrend. When both lines are below 1, neither
/// direction beats a random walk and the market is read as ranging.
///
/// The first output is emitted after `period` candles (the second one provides
/// the first `period = 2` lookback, so the indicator emits at index
/// `period - 1`).
///
/// # Example
///
/// ```
/// use wickra_core::{Candle, Indicator, Rwi};
///
/// let mut indicator = Rwi::new(14).unwrap();
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
pub struct Rwi {
    period: usize,
    /// Rolling window of the most recent `period` candles (oldest at the front).
    candles: VecDeque<Candle>,
    /// Rolling window of `period` true-range values aligned with `candles`
    /// after the first bar (so `tr[0]` corresponds to `candles[1]`).
    trs: VecDeque<f64>,
    /// Reusable contiguous copy of `trs`, so the per-lookback ATR can be
    /// summed over a slice without allocating per `update`.
    scratch: Vec<f64>,
    last: Option<RwiOutput>,
}

impl Rwi {
    /// Construct a new RWI with the given lookback period.
    ///
    /// # Errors
    ///
    /// Returns [`Error::PeriodZero`] if `period == 0`.
    /// Returns [`Error::InvalidPeriod`] if `period < 2` — RWI's shortest
    /// lookback is `i = 2`, so a one-bar window would emit nothing.
    pub fn new(period: usize) -> Result<Self> {
        if period == 0 {
            return Err(Error::PeriodZero);
        }
        if period > crate::error::MAX_PERIOD {
            return Err(Error::InvalidPeriod {
                message: crate::error::PERIOD_ABOVE_MAX,
            });
        }
        if period < 2 {
            return Err(Error::InvalidPeriod {
                message: "RWI requires period >= 2",
            });
        }
        Ok(Self {
            period,
            candles: VecDeque::with_capacity(period),
            trs: VecDeque::with_capacity(period),
            scratch: Vec::with_capacity(period),
            last: None,
        })
    }

    /// Configured period.
    pub const fn period(&self) -> usize {
        self.period
    }

    /// Current value if available.
    pub const fn value(&self) -> Option<RwiOutput> {
        self.last
    }
}

impl Indicator for Rwi {
    type Input = Candle;
    type Output = RwiOutput;

    fn update(&mut self, candle: Candle) -> Option<RwiOutput> {
        // Compute the true range of this candle vs. the previous close (if any),
        // then slide the windows.
        let tr = if let Some(prev) = self.candles.back() {
            candle.true_range(Some(prev.close))
        } else {
            candle.high - candle.low
        };

        if self.candles.len() == self.period {
            self.candles.pop_front();
        }
        self.candles.push_back(candle);

        // `trs` aligns with `candles` from index 1 onward; only push once we
        // have at least one previous candle (the bar's TR-vs-prev is what we
        // store). With the first bar in `candles`, no TR is recorded yet.
        if self.candles.len() >= 2 {
            if self.trs.len() == self.period - 1 {
                self.trs.pop_front();
            }
            self.trs.push_back(tr);
        }

        // Need a full `period` candles before we can scan lookbacks i ∈ [2,period].
        if self.candles.len() < self.period {
            return None;
        }

        // `candles` is indexed at single positions, which a `VecDeque` does
        // directly; only `trs` needs a contiguous slice, for the range sums.
        let candles = &self.candles;
        self.scratch.clear();
        self.scratch.extend(self.trs.iter().copied());
        let trs = &self.scratch;
        let n = candles.len(); // == self.period
        let last_high = candles[n - 1].high;
        let last_low = candles[n - 1].low;

        let mut rwi_high = 0.0_f64;
        let mut rwi_low = 0.0_f64;
        // For lookback i in [2, period]: compare bar `n - 1` to bar `n - i`.
        // The TRs covered are those at trs indices [n - i .. n - 1], which is
        // `i - 1` TR values (TR at index n - i is the TR of candle n - i + 1
        // vs. candle n - i, the first TR contributing to the i-bar ATR... or
        // strictly the ATR over the i-bar window is the mean of the i-1 TRs
        // _between_ those bars). We use the i-1-TR mean to keep the indicator
        // strictly causal.
        for i in 2..=self.period {
            // Trs slice indices (within trs Vec): start = n - i, end = n - 1 (excl.).
            // trs has length n - 1; trs[k] = TR of candle k+1 vs candle k.
            // count = i - 1, which is >= 1 for i >= 2.
            let tr_start = n - i;
            let tr_end = n - 1;
            let count = tr_end - tr_start;
            let atr_i: f64 = trs[tr_start..tr_end].iter().sum::<f64>() / (count as f64);
            let denom = atr_i * (i as f64).sqrt();
            if denom == 0.0 {
                continue;
            }
            let old_low = candles[n - i].low;
            let old_high = candles[n - i].high;
            let h = (last_high - old_low) / denom;
            let l = (old_high - last_low) / denom;
            if h > rwi_high {
                rwi_high = h;
            }
            if l > rwi_low {
                rwi_low = l;
            }
        }

        let out = RwiOutput {
            high: rwi_high,
            low: rwi_low,
        };
        self.last = Some(out);
        Some(out)
    }

    fn reset(&mut self) {
        self.candles.clear();
        self.trs.clear();
        self.scratch.clear();
        self.last = None;
    }

    #[inline]
    fn warmup_period(&self) -> usize {
        // First emission once the rolling window holds `period` candles.
        self.period
    }

    #[inline]
    fn is_ready(&self) -> bool {
        self.last.is_some()
    }

    #[inline]
    fn name(&self) -> &'static str {
        "RWI"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;

    fn candle(h: f64, l: f64, c: f64, ts: i64) -> Candle {
        Candle::new(c, h, l, c, 1.0, ts).unwrap()
    }

    #[test]
    fn rejects_zero_period() {
        assert!(matches!(Rwi::new(0), Err(Error::PeriodZero)));
    }

    #[test]
    fn rejects_period_one() {
        assert!(matches!(Rwi::new(1), Err(Error::InvalidPeriod { .. })));
    }

    #[test]
    fn accessors_and_metadata() {
        let mut r = Rwi::new(14).unwrap();
        assert_eq!(r.period(), 14);
        assert_eq!(r.warmup_period(), 14);
        assert_eq!(r.name(), "RWI");
        assert!(r.value().is_none());
        for i in 0..30_i64 {
            let p = 100.0 + (i as f64);
            r.update(candle(p + 1.0, p - 1.0, p, i));
        }
        assert!(r.value().is_some());
    }

    #[test]
    fn first_emission_at_warmup_period() {
        let candles: Vec<Candle> = (0..40_i64)
            .map(|i| {
                let p = 100.0 + ((i as f64) * 0.3).sin() * 5.0;
                candle(p + 1.0, p - 1.0, p, i)
            })
            .collect();
        let mut r = Rwi::new(5).unwrap();
        let out = r.batch(&candles);
        for v in out.iter().take(4) {
            assert!(v.is_none());
        }
        assert!(out[4].is_some());
    }

    #[test]
    fn constant_series_yields_zero_outputs() {
        // Flat market: ATR is zero, so all lookbacks short-circuit on the
        // denom-zero guard and both lines stay at 0.
        let candles: Vec<Candle> = (0..30_i64).map(|i| candle(10.0, 10.0, 10.0, i)).collect();
        let mut r = Rwi::new(5).unwrap();
        let last = r.batch(&candles).into_iter().flatten().last().unwrap();
        assert_eq!(last.high, 0.0);
        assert_eq!(last.low, 0.0);
    }

    #[test]
    fn pure_uptrend_high_dominates_low() {
        // A monotone uptrend should produce RWI_High >> RWI_Low.
        let candles: Vec<Candle> = (0..40_i64)
            .map(|i| {
                let base = 100.0 + (i as f64) * 2.0;
                candle(base + 1.0, base - 0.5, base + 0.5, i)
            })
            .collect();
        let mut r = Rwi::new(14).unwrap();
        let last = r.batch(&candles).into_iter().flatten().last().unwrap();
        assert!(
            last.high > last.low,
            "RWI_High {} should exceed RWI_Low {}",
            last.high,
            last.low
        );
        assert!(
            last.high > 1.0,
            "strong uptrend should exceed 1, got {}",
            last.high
        );
    }

    #[test]
    fn pure_downtrend_low_dominates_high() {
        let candles: Vec<Candle> = (0..40_i64)
            .rev()
            .map(|i| {
                let base = 100.0 + (i as f64) * 2.0;
                candle(base + 0.5, base - 1.0, base - 0.5, 40 - i)
            })
            .collect();
        let mut r = Rwi::new(14).unwrap();
        let last = r.batch(&candles).into_iter().flatten().last().unwrap();
        assert!(last.low > last.high);
        assert!(last.low > 1.0);
    }

    #[test]
    fn outputs_non_negative() {
        let candles: Vec<Candle> = (0..120_i64)
            .map(|i| {
                let p = 100.0 + ((i as f64) * 0.25).sin() * 6.0;
                candle(p + 1.5, p - 1.5, p, i)
            })
            .collect();
        let mut r = Rwi::new(10).unwrap();
        for v in r.batch(&candles).into_iter().flatten() {
            assert!(v.high >= 0.0 && v.low >= 0.0);
            assert!(v.high.is_finite() && v.low.is_finite());
        }
    }

    #[test]
    fn batch_equals_streaming() {
        let candles: Vec<Candle> = (0..80_i64)
            .map(|i| {
                let p = 100.0 + ((i as f64) * 0.3).sin() * 5.0;
                candle(p + 1.0, p - 1.0, p, i)
            })
            .collect();
        let mut a = Rwi::new(7).unwrap();
        let mut b = Rwi::new(7).unwrap();
        assert_eq!(
            a.batch(&candles),
            candles.iter().map(|c| b.update(*c)).collect::<Vec<_>>()
        );
    }

    #[test]
    fn reset_clears_state() {
        let candles: Vec<Candle> = (0..30_i64).map(|i| candle(11.0, 9.0, 10.0, i)).collect();
        let mut r = Rwi::new(5).unwrap();
        r.batch(&candles);
        assert!(r.is_ready());
        r.reset();
        assert!(!r.is_ready());
        assert_eq!(r.update(candles[0]), None);
    }

    #[test]
    fn rejects_period_above_maximum() {
        assert!(matches!(
            Rwi::new(crate::error::MAX_PERIOD + 1),
            Err(Error::InvalidPeriod { .. })
        ));
    }
}
