//! Dynamic Momentum Index (Chande's volatility-adaptive RSI).

use std::collections::VecDeque;

use crate::error::{Error, Result};
use crate::indicators::sma::Sma;
use crate::indicators::std_dev::StdDev;
use crate::traits::Indicator;

// Chande's definitional constants.
const STD_PERIOD: usize = 5; // volatility window
const STD_AVG_PERIOD: usize = 10; // smoothing of the volatility
const MIN_PERIOD: usize = 5; // fastest RSI lookback
const MAX_RSI_LOOKBACK: usize = 30; // slowest RSI lookback

/// Dynamic Momentum Index — Tushar Chande's RSI whose lookback shrinks in
/// volatile markets and lengthens in calm ones.
///
/// A standard RSI uses a fixed period; the DMI varies it from the recent
/// volatility so the oscillator stays responsive when the market is fast and
/// smooth when it is quiet:
///
/// ```text
/// vol     = StdDev(close, 5)
/// vol_avg = SMA(vol, 10)
/// Vi      = vol / vol_avg                       (volatility index)
/// td      = clamp(round(period / Vi), 5, 30)    (dynamic lookback)
/// avg_gain, avg_loss = simple means of the last `td` price changes
/// DMI     = 100 * avg_gain / (avg_gain + avg_loss)
/// ```
///
/// High volatility (`Vi > 1`) shortens `td` toward `5` (faster); low volatility
/// lengthens it toward `30` (slower). The averages of gains and losses are
/// simple means over the last `td` changes (not Wilder-smoothed), recomputed as
/// the window length flexes. Output is bounded in `[0, 100]`; a flat market
/// returns the neutral `50`.
///
/// The first value lands after `MAX_RSI_LOOKBACK + 1 = 31` inputs, so the change
/// buffer always holds enough history for any dynamic lookback up to `30`.
///
/// # Example
///
/// ```
/// use wickra_core::{DynamicMomentumIndex, Indicator};
///
/// let mut dmi = DynamicMomentumIndex::new(14).unwrap();
/// let mut last = None;
/// for i in 0..80 {
///     last = dmi.update(100.0 + (f64::from(i) * 0.2).sin() * 5.0);
/// }
/// assert!(last.is_some());
/// ```
#[derive(Debug, Clone)]
pub struct DynamicMomentumIndex {
    period: usize,
    vol: StdDev,
    vol_avg: Sma,
    prev_close: Option<f64>,
    /// The last `MAX_RSI_LOOKBACK` price changes, oldest at the front.
    changes: VecDeque<f64>,
    last_vol_avg: Option<f64>,
    last_value: Option<f64>,
}

impl DynamicMomentumIndex {
    /// Construct a DMI with the given base RSI period (Chande uses 14).
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
            vol: StdDev::new(STD_PERIOD)?,
            vol_avg: Sma::new(STD_AVG_PERIOD)?,
            prev_close: None,
            changes: VecDeque::with_capacity(MAX_RSI_LOOKBACK),
            last_vol_avg: None,
            last_value: None,
        })
    }

    /// Configured base period.
    pub const fn period(&self) -> usize {
        self.period
    }

    /// Current value if available.
    pub const fn value(&self) -> Option<f64> {
        self.last_value
    }

    /// Dynamic lookback for the current volatility, clamped to `[5, 30]`.
    fn dynamic_period(&self, vol: f64, vol_avg: f64) -> usize {
        if vol_avg <= 0.0 || vol <= 0.0 {
            // No measurable volatility -> slowest (calmest) lookback.
            return MAX_RSI_LOOKBACK;
        }
        let vi = vol / vol_avg;
        let td = (self.period as f64 / vi).round();
        // td is finite and positive here; clamp into the valid band.
        (td as usize).clamp(MIN_PERIOD, MAX_RSI_LOOKBACK)
    }
}

impl Indicator for DynamicMomentumIndex {
    type Input = f64;
    type Output = f64;

    fn update(&mut self, input: f64) -> Option<f64> {
        if !input.is_finite() {
            return None;
        }
        // Track the smoothed volatility on every close.
        if let Some(v) = self.vol.update(input) {
            self.last_vol_avg = self.vol_avg.update(v);
        }

        // Record the price change.
        if let Some(prev) = self.prev_close {
            let change = input - prev;
            if self.changes.len() == MAX_RSI_LOOKBACK {
                self.changes.pop_front();
            }
            self.changes.push_back(change);
        }
        self.prev_close = Some(input);

        let vol = self.vol.value()?;
        let vol_avg = self.last_vol_avg?;
        if self.changes.len() < MAX_RSI_LOOKBACK {
            return None;
        }

        let td = self.dynamic_period(vol, vol_avg);
        // Average gains and losses over the last `td` changes.
        let mut sum_gain = 0.0;
        let mut sum_loss = 0.0;
        for &c in self.changes.iter().skip(MAX_RSI_LOOKBACK - td) {
            if c > 0.0 {
                sum_gain += c;
            } else if c < 0.0 {
                sum_loss -= c;
            }
        }
        let denom = sum_gain + sum_loss;
        let v = if denom == 0.0 {
            50.0
        } else {
            // Ratio first, then scale, so `100 * g / g` cannot round above 100.
            100.0 * (sum_gain / denom)
        };
        self.last_value = Some(v);
        Some(v)
    }

    fn reset(&mut self) {
        self.vol.reset();
        self.vol_avg.reset();
        self.prev_close = None;
        self.changes.clear();
        self.last_vol_avg = None;
        self.last_value = None;
    }

    #[inline]
    fn warmup_period(&self) -> usize {
        // The change buffer (MAX_RSI_LOOKBACK changes => MAX_RSI_LOOKBACK + 1 inputs) is the
        // binding constraint; the volatility chain (5 + 10 - 1 = 14) is shorter.
        MAX_RSI_LOOKBACK + 1
    }

    #[inline]
    fn is_ready(&self) -> bool {
        self.last_value.is_some()
    }

    #[inline]
    fn name(&self) -> &'static str {
        "DynamicMomentumIndex"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;
    use approx::assert_relative_eq;

    #[test]
    fn rejects_zero_period() {
        assert!(matches!(
            DynamicMomentumIndex::new(0),
            Err(Error::PeriodZero)
        ));
    }

    /// Cover the const accessors `period` + `value` and the Indicator-impl
    /// `warmup_period` + `name`.
    #[test]
    fn accessors_and_metadata() {
        let dmi = DynamicMomentumIndex::new(14).unwrap();
        assert_eq!(dmi.period(), 14);
        assert_eq!(dmi.value(), None);
        assert_eq!(dmi.warmup_period(), 31);
        assert_eq!(dmi.name(), "DynamicMomentumIndex");
    }

    #[test]
    fn first_emission_matches_warmup_period() {
        let prices: Vec<f64> = (0..50)
            .map(|i| 100.0 + (f64::from(i) * 0.4).sin() * 6.0)
            .collect();
        let mut dmi = DynamicMomentumIndex::new(14).unwrap();
        let out = dmi.batch(&prices);
        for (i, v) in out.iter().enumerate().take(30) {
            assert!(v.is_none(), "index {i} must be None during warmup");
        }
        assert!(out[30].is_some(), "first value at warmup_period - 1 = 30");
    }

    #[test]
    fn pure_uptrend_is_one_hundred() {
        // Every change positive -> avg_loss 0 -> 100, regardless of dynamic period.
        let prices: Vec<f64> = (1..=60).map(f64::from).collect();
        let mut dmi = DynamicMomentumIndex::new(14).unwrap();
        let last = dmi.batch(&prices).into_iter().flatten().last().unwrap();
        assert_relative_eq!(last, 100.0, epsilon = 1e-9);
    }

    #[test]
    fn flat_market_is_neutral() {
        // Constant prices: no volatility (dynamic period -> max) and no changes
        // -> neutral 50.
        let mut dmi = DynamicMomentumIndex::new(14).unwrap();
        let last = dmi.batch(&[42.0; 50]).into_iter().flatten().last().unwrap();
        assert_relative_eq!(last, 50.0, epsilon = 1e-12);
    }

    #[test]
    fn output_stays_in_range() {
        let prices: Vec<f64> = (0..120)
            .map(|i| 100.0 + (f64::from(i) * 0.3).sin() * 10.0 + (f64::from(i) * 0.07).cos() * 4.0)
            .collect();
        let mut dmi = DynamicMomentumIndex::new(14).unwrap();
        for v in dmi.batch(&prices).into_iter().flatten() {
            assert!((0.0..=100.0).contains(&v), "DMI {v} left [0, 100]");
        }
    }

    #[test]
    fn high_volatility_shortens_period() {
        let dmi = DynamicMomentumIndex::new(14).unwrap();
        // Vi = 2 (vol twice its average) -> td = round(14 / 2) = 7.
        assert_eq!(dmi.dynamic_period(2.0, 1.0), 7);
        // Vi = 0.5 (calm) -> td = round(14 / 0.5) = 28.
        assert_eq!(dmi.dynamic_period(0.5, 1.0), 28);
        // Extreme calm clamps to MAX_RSI_LOOKBACK; extreme volatility clamps to MIN.
        assert_eq!(dmi.dynamic_period(0.1, 1.0), MAX_RSI_LOOKBACK);
        assert_eq!(dmi.dynamic_period(100.0, 1.0), MIN_PERIOD);
        // Zero volatility -> slowest lookback.
        assert_eq!(dmi.dynamic_period(0.0, 1.0), MAX_RSI_LOOKBACK);
        assert_eq!(dmi.dynamic_period(1.0, 0.0), MAX_RSI_LOOKBACK);
    }

    #[test]
    fn ignores_non_finite_input() {
        let mut dmi = DynamicMomentumIndex::new(14).unwrap();
        let _ready = dmi
            .batch(&(0..40).map(|i| 100.0 + f64::from(i)).collect::<Vec<_>>())
            .into_iter()
            .flatten()
            .last()
            .unwrap();
        assert_eq!(dmi.update(f64::NAN), None);
        assert_eq!(dmi.update(f64::INFINITY), None);
    }

    #[test]
    fn reset_clears_state() {
        let mut dmi = DynamicMomentumIndex::new(14).unwrap();
        dmi.batch(&(0..40).map(|i| 100.0 + f64::from(i)).collect::<Vec<_>>());
        assert!(dmi.is_ready());
        dmi.reset();
        assert!(!dmi.is_ready());
        assert_eq!(dmi.update(1.0), None);
    }

    #[test]
    fn batch_equals_streaming() {
        let prices: Vec<f64> = (0..80)
            .map(|i| 50.0 + (f64::from(i) * 0.5).sin() * 10.0)
            .collect();
        let mut a = DynamicMomentumIndex::new(14).unwrap();
        let mut b = DynamicMomentumIndex::new(14).unwrap();
        assert_eq!(
            a.batch(&prices),
            prices.iter().map(|p| b.update(*p)).collect::<Vec<_>>()
        );
    }

    #[test]
    fn rejects_period_above_maximum() {
        assert!(matches!(
            DynamicMomentumIndex::new(crate::error::MAX_PERIOD + 1),
            Err(Error::InvalidPeriod { .. })
        ));
    }
}
