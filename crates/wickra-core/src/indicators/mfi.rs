//! Money Flow Index (MFI).

use std::collections::VecDeque;

use crate::error::{Error, Result};
use crate::indicators::rolling_moments::RollingSum;
use crate::ohlcv::Candle;
use crate::traits::Indicator;

/// Money Flow Index: a volume-weighted version of RSI.
///
/// `MFI = 100 - 100 / (1 + positive_money_flow / negative_money_flow)` where
/// money flow is `typical_price * volume`, classified positive when TP increases
/// and negative when it decreases.
///
/// # Example
///
/// ```
/// use wickra_core::{Candle, Indicator, Mfi};
///
/// let mut indicator = Mfi::new(5).unwrap();
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
pub struct Mfi {
    period: usize,
    prev_tp: Option<f64>,
    pos_window: VecDeque<f64>,
    neg_window: VecDeque<f64>,
    pos_sum: RollingSum,
    neg_sum: RollingSum,
}

impl Mfi {
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
            prev_tp: None,
            pos_window: VecDeque::with_capacity(period),
            neg_window: VecDeque::with_capacity(period),
            pos_sum: RollingSum::new(),
            neg_sum: RollingSum::new(),
        })
    }

    /// Configured period.
    pub const fn period(&self) -> usize {
        self.period
    }
}

impl Indicator for Mfi {
    type Input = Candle;
    type Output = f64;

    fn update(&mut self, candle: Candle) -> Option<f64> {
        let tp = candle.typical_price();

        // The very first candle only establishes the previous typical price.
        // It carries no money-flow direction, so it is not pushed into the
        // window. This matches TA-Lib / pandas-ta, which need `period + 1`
        // candles before the first MFI value.
        let Some(prev) = self.prev_tp else {
            self.prev_tp = Some(tp);
            return None;
        };

        let mf = tp * candle.volume;
        let (pos_flow, neg_flow) = if tp > prev {
            (mf, 0.0)
        } else if tp < prev {
            (0.0, mf)
        } else {
            (0.0, 0.0)
        };

        if self.pos_window.len() == self.period {
            let old_pos = self.pos_window.pop_front().expect("non-empty");
            let old_neg = self.neg_window.pop_front().expect("non-empty");
            self.pos_sum.evict(old_pos);
            self.neg_sum.evict(old_neg);
        }
        self.pos_window.push_back(pos_flow);
        self.neg_window.push_back(neg_flow);
        self.pos_sum.push(pos_flow);
        self.neg_sum.push(neg_flow);
        if self.pos_sum.needs_reseed(self.period) {
            self.pos_sum.reseed(self.pos_window.iter().copied());
            self.neg_sum.reseed(self.neg_window.iter().copied());
        }

        self.prev_tp = Some(tp);

        if self.pos_window.len() < self.period {
            return None;
        }
        // A fully flat window (every typical price equal) has zero flow on
        // both sides; by convention MFI is then 50.
        let (pos_sum, neg_sum) = (self.pos_sum.value(), self.neg_sum.value());
        if pos_sum == 0.0 && neg_sum == 0.0 {
            return Some(50.0);
        }
        if neg_sum == 0.0 {
            return Some(100.0);
        }
        let mr = pos_sum / neg_sum;
        Some(100.0 - 100.0 / (1.0 + mr))
    }

    fn reset(&mut self) {
        self.prev_tp = None;
        self.pos_window.clear();
        self.neg_window.clear();
        self.pos_sum.reset();
        self.neg_sum.reset();
    }

    #[inline]
    fn warmup_period(&self) -> usize {
        // One seed candle establishes the first previous typical price, then
        // `period` flow comparisons fill the window.
        self.period + 1
    }

    #[inline]
    fn is_ready(&self) -> bool {
        self.pos_window.len() == self.period
    }

    #[inline]
    fn name(&self) -> &'static str {
        "MFI"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;
    use approx::assert_relative_eq;

    fn c(price: f64, volume: f64) -> Candle {
        Candle::new(price, price, price, price, volume, 0).unwrap()
    }

    #[test]
    fn pure_uptrend_yields_high_mfi() {
        let candles: Vec<Candle> = (1..30).map(|i| c(f64::from(i), 100.0)).collect();
        let mut mfi = Mfi::new(14).unwrap();
        let last = mfi.batch(&candles).into_iter().flatten().last().unwrap();
        assert_relative_eq!(last, 100.0, epsilon = 1e-9);
    }

    #[test]
    fn pure_downtrend_yields_low_mfi() {
        let candles: Vec<Candle> = (1..30).rev().map(|i| c(f64::from(i), 100.0)).collect();
        let mut mfi = Mfi::new(14).unwrap();
        let last = mfi.batch(&candles).into_iter().flatten().last().unwrap();
        assert_relative_eq!(last, 0.0, epsilon = 1e-9);
    }

    #[test]
    fn batch_equals_streaming() {
        let candles: Vec<Candle> = (0..40).map(|i| c(f64::from(i) + 10.0, 50.0)).collect();
        let mut a = Mfi::new(14).unwrap();
        let mut b = Mfi::new(14).unwrap();
        assert_eq!(
            a.batch(&candles),
            candles.iter().map(|x| b.update(*x)).collect::<Vec<_>>()
        );
    }

    #[test]
    fn reset_clears_state() {
        let candles: Vec<Candle> = (1..30).map(|i| c(f64::from(i), 100.0)).collect();
        let mut mfi = Mfi::new(14).unwrap();
        mfi.batch(&candles);
        assert!(mfi.is_ready());
        mfi.reset();
        assert!(!mfi.is_ready());
    }

    /// Cover the const accessor `period` (58-60) and the Indicator-impl
    /// `name` body (132-134). `warmup_period` is already covered elsewhere.
    #[test]
    fn accessors_and_metadata() {
        let mfi = Mfi::new(14).unwrap();
        assert_eq!(mfi.period(), 14);
        assert_eq!(mfi.name(), "MFI");
    }

    /// Cover the `tp == prev` arm (line 85) — when typical price equals
    /// the previous typical price, both flows are 0 — and the all-zero-
    /// flow fallback `Some(50.0)` (line 105). Existing tests use varying
    /// candles so the flat-TP arm and the zero-flow fallback never fired.
    #[test]
    fn flat_typical_prices_default_to_50() {
        let mut mfi = Mfi::new(3).unwrap();
        let candles: Vec<Candle> = (0..6)
            .map(|i| Candle::new(10.0, 10.0, 10.0, 10.0, 1.0, i).unwrap())
            .collect();
        let last = mfi
            .batch(&candles)
            .into_iter()
            .flatten()
            .last()
            .expect("emits");
        assert_eq!(last, 50.0);
    }

    #[test]
    fn rejects_zero_period() {
        assert!(Mfi::new(0).is_err());
    }

    #[test]
    fn first_value_emitted_on_period_plus_one_candle() {
        // The seed candle plus `period` flow comparisons -> first MFI on the
        // (period + 1)-th candle (index `period`).
        let candles: Vec<Candle> = (1..=20).map(|i| c(f64::from(i), 100.0)).collect();
        let mut mfi = Mfi::new(5).unwrap();
        let out = mfi.batch(&candles);
        for (i, v) in out.iter().enumerate().take(5) {
            assert!(v.is_none(), "candle index {i} must be None during warmup");
        }
        assert!(
            out[5].is_some(),
            "first MFI value lands at index period (5)"
        );
        assert_eq!(mfi.warmup_period(), 6);
    }

    #[test]
    fn known_value_period_2() {
        // Three candles, MFI(2). Candle 1 (tp=10) only seeds the previous TP.
        // Candle 2 (tp=12 > 10): positive money flow 12 * 100 = 1200.
        // Candle 3 (tp=11 < 12): negative money flow 11 * 100 = 1100.
        // money ratio = 1200 / 1100; MFI = 100 - 100 / (1 + 1200/1100) = 1200/23.
        let candles = vec![c(10.0, 100.0), c(12.0, 100.0), c(11.0, 100.0)];
        let mut mfi = Mfi::new(2).unwrap();
        let out = mfi.batch(&candles);
        assert!(out[0].is_none());
        assert!(out[1].is_none());
        assert_relative_eq!(out[2].unwrap(), 1200.0 / 23.0, epsilon = 1e-9);
    }

    #[test]
    fn rejects_period_above_maximum() {
        assert!(matches!(
            Mfi::new(crate::error::MAX_PERIOD + 1),
            Err(Error::InvalidPeriod { .. })
        ));
    }
}
