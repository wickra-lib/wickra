//! Volume-Weighted Average Price (VWAP).
//!
//! Two variants are offered: a cumulative `Vwap` that runs forever (the
//! intraday convention), and a rolling-window `RollingVwap` for streaming bots
//! that need a finite-memory price benchmark.

use std::collections::VecDeque;

use crate::error::{Error, Result};
use crate::indicators::rolling_moments::RollingSum;
use crate::ohlcv::Candle;
use crate::traits::Indicator;

/// Cumulative session VWAP. Call [`Indicator::reset`] at the start of each
/// session (e.g. trading-day boundary) to restart the accumulation.
///
/// # Example
///
/// ```
/// use wickra_core::{Candle, Indicator, Vwap};
///
/// let mut indicator = Vwap::new();
/// let mut last = None;
/// for i in 0..80 {
///     let base = 100.0 + f64::from(i);
///     let candle =
///         Candle::new(base, base + 2.0, base - 2.0, base + 1.0, 10.0, i64::from(i)).unwrap();
///     last = indicator.update(candle);
/// }
/// assert!(last.is_some());
/// ```
#[derive(Debug, Clone, Default)]
pub struct Vwap {
    sum_pv: f64,
    sum_v: f64,
    has_emitted: bool,
}

impl Vwap {
    /// Construct a fresh cumulative VWAP.
    pub const fn new() -> Self {
        Self {
            sum_pv: 0.0,
            sum_v: 0.0,
            has_emitted: false,
        }
    }

    /// Current VWAP if at least one candle with non-zero volume has been observed.
    pub fn value(&self) -> Option<f64> {
        if self.sum_v == 0.0 {
            None
        } else {
            Some(self.sum_pv / self.sum_v)
        }
    }
}

impl Indicator for Vwap {
    type Input = Candle;
    type Output = f64;

    #[inline]
    fn update(&mut self, candle: Candle) -> Option<f64> {
        let tp = candle.typical_price();
        self.sum_pv += tp * candle.volume;
        self.sum_v += candle.volume;
        if self.sum_v == 0.0 {
            return None;
        }
        self.has_emitted = true;
        Some(self.sum_pv / self.sum_v)
    }

    fn reset(&mut self) {
        self.sum_pv = 0.0;
        self.sum_v = 0.0;
        self.has_emitted = false;
    }

    #[inline]
    fn warmup_period(&self) -> usize {
        1
    }

    #[inline]
    fn is_ready(&self) -> bool {
        self.has_emitted
    }

    #[inline]
    fn name(&self) -> &'static str {
        "VWAP"
    }
}

/// Rolling-window VWAP: a finite-memory variant for bots that don't want
/// unbounded accumulation.
///
/// # Example
///
/// ```
/// use wickra_core::{Candle, Indicator, RollingVwap};
///
/// let mut indicator = RollingVwap::new(5).unwrap();
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
pub struct RollingVwap {
    period: usize,
    window: VecDeque<(f64, f64)>, // (typical_price * volume, volume)
    sum_pv: RollingSum,
    sum_v: RollingSum,
}

impl RollingVwap {
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
            sum_pv: RollingSum::new(),
            sum_v: RollingSum::new(),
        })
    }

    /// Configured rolling window length.
    pub const fn period(&self) -> usize {
        self.period
    }
}

impl Indicator for RollingVwap {
    type Input = Candle;
    type Output = f64;

    #[inline]
    fn update(&mut self, candle: Candle) -> Option<f64> {
        let pv = candle.typical_price() * candle.volume;
        if self.window.len() == self.period {
            let (old_pv, old_v) = self.window.pop_front().expect("non-empty");
            self.sum_pv.evict(old_pv);
            self.sum_v.evict(old_v);
        }
        self.window.push_back((pv, candle.volume));
        self.sum_pv.push(pv);
        self.sum_v.push(candle.volume);
        if self.sum_pv.needs_reseed(self.period) {
            self.sum_pv.reseed(self.window.iter().map(|&(p, _)| p));
            self.sum_v.reseed(self.window.iter().map(|&(_, v)| v));
        }
        if self.window.len() < self.period || self.sum_v.value() == 0.0 {
            return None;
        }
        Some(self.sum_pv.value() / self.sum_v.value())
    }

    fn reset(&mut self) {
        self.window.clear();
        self.sum_pv.reset();
        self.sum_v.reset();
    }

    #[inline]
    fn warmup_period(&self) -> usize {
        self.period
    }

    #[inline]
    fn is_ready(&self) -> bool {
        self.window.len() == self.period && self.sum_v.value() > 0.0
    }

    #[inline]
    fn name(&self) -> &'static str {
        "RollingVWAP"
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
    fn cumulative_vwap_equal_volumes_equals_mean() {
        let candles = vec![c(10.0, 1.0), c(20.0, 1.0), c(30.0, 1.0)];
        let mut v = Vwap::new();
        let out = v.batch(&candles);
        assert_relative_eq!(out[2].unwrap(), 20.0, epsilon = 1e-12);
    }

    /// Cover the `Some` branch of `Vwap::value()` (line 53). The only other
    /// test that calls `value()` is `cumulative_reset_clears_state`, which
    /// calls it after `reset()` so `sum_v == 0` and the `None` branch fires.
    #[test]
    fn cumulative_value_some_branch_after_update() {
        let mut v = Vwap::new();
        // typical_price of a flat OHLC bar equals the price itself.
        v.update(c(42.0, 5.0));
        assert_relative_eq!(v.value().expect("non-zero volume"), 42.0, epsilon = 1e-12);
    }

    /// Cover the `return None` early-out inside `Vwap::update` (line 67),
    /// reached when the running `sum_v` is still 0 after adding the latest
    /// candle's volume — i.e. the first candle has volume 0. Existing tests
    /// only use strictly positive volumes, so the early-return never fired.
    #[test]
    fn cumulative_zero_volume_first_candle_returns_none() {
        let mut v = Vwap::new();
        let out = v.update(c(42.0, 0.0));
        assert_eq!(out, None);
        assert!(!v.is_ready());
        // Adding a non-zero candle afterwards still works as expected.
        let out2 = v.update(c(10.0, 4.0));
        assert_relative_eq!(out2.expect("now warmed"), 10.0, epsilon = 1e-12);
    }

    /// Cover the cumulative `Vwap` Indicator-impl metadata: `warmup_period`
    /// (lines 79-81) and `name` (lines 87-89). Existing tests inspected
    /// only the numeric output, never the metadata surface.
    #[test]
    fn cumulative_metadata() {
        let v = Vwap::new();
        assert_eq!(v.warmup_period(), 1);
        assert_eq!(v.name(), "VWAP");
    }

    #[test]
    fn cumulative_vwap_weighted() {
        // Two candles: 10@1 and 20@3 -> (10*1 + 20*3) / (1+3) = 70/4 = 17.5
        let candles = vec![c(10.0, 1.0), c(20.0, 3.0)];
        let mut v = Vwap::new();
        let out = v.batch(&candles);
        assert_relative_eq!(out[1].unwrap(), 17.5, epsilon = 1e-12);
    }

    /// Cover the `RollingVwap` accessors and metadata: `period`
    /// (lines 134-136), `warmup_period` (165-167), `name` (173-175).
    /// Existing rolling tests called `update`/`batch`/`reset`/`is_ready`
    /// only, never queried the configuration or metadata.
    #[test]
    fn rolling_accessors_and_metadata() {
        let v = RollingVwap::new(7).unwrap();
        assert_eq!(v.period(), 7);
        assert_eq!(v.warmup_period(), 7);
        assert_eq!(v.name(), "RollingVWAP");
    }

    #[test]
    fn rolling_vwap_window_slides() {
        let candles = vec![c(10.0, 1.0), c(20.0, 1.0), c(30.0, 1.0), c(40.0, 1.0)];
        let mut v = RollingVwap::new(3).unwrap();
        let out = v.batch(&candles);
        assert!(out[1].is_none());
        // index 2 -> (10+20+30)/3 = 20
        assert_relative_eq!(out[2].unwrap(), 20.0, epsilon = 1e-12);
        // index 3 -> (20+30+40)/3 = 30
        assert_relative_eq!(out[3].unwrap(), 30.0, epsilon = 1e-12);
    }

    #[test]
    fn batch_equals_streaming_cumulative() {
        let candles: Vec<Candle> = (1..20).map(|i| c(f64::from(i), 1.0)).collect();
        let mut a = Vwap::new();
        let mut b = Vwap::new();
        assert_eq!(
            a.batch(&candles),
            candles.iter().map(|x| b.update(*x)).collect::<Vec<_>>()
        );
    }

    #[test]
    fn batch_equals_streaming_rolling() {
        let candles: Vec<Candle> = (1..30)
            .map(|i| c(f64::from(i), f64::from(i % 5 + 1)))
            .collect();
        let mut a = RollingVwap::new(10).unwrap();
        let mut b = RollingVwap::new(10).unwrap();
        assert_eq!(
            a.batch(&candles),
            candles.iter().map(|x| b.update(*x)).collect::<Vec<_>>()
        );
    }

    #[test]
    fn rolling_rejects_zero_period() {
        assert!(RollingVwap::new(0).is_err());
    }

    #[test]
    fn cumulative_reset_clears_state() {
        let candles = vec![c(10.0, 1.0), c(20.0, 1.0), c(30.0, 1.0)];
        let mut v = Vwap::new();
        v.batch(&candles);
        assert!(v.is_ready());
        v.reset();
        assert!(!v.is_ready());
        assert_eq!(v.value(), None);
    }

    #[test]
    fn rolling_reset_clears_state() {
        let candles: Vec<Candle> = (1..=10).map(|i| c(f64::from(i), 1.0)).collect();
        let mut v = RollingVwap::new(5).unwrap();
        v.batch(&candles);
        assert!(v.is_ready());
        v.reset();
        assert!(!v.is_ready());
        assert_eq!(v.update(candles[0]), None);
    }

    #[test]
    fn rejects_period_above_maximum() {
        assert!(matches!(
            RollingVwap::new(crate::error::MAX_PERIOD + 1),
            Err(Error::InvalidPeriod { .. })
        ));
    }
}
