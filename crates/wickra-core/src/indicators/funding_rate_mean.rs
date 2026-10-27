//! Funding Rate Rolling Mean — average funding rate over a trailing window.

use std::collections::VecDeque;

use crate::derivatives::DerivativesTick;
use crate::error::{Error, Result};
use crate::indicators::rolling_moments::RollingSum;
use crate::traits::Indicator;

/// Funding Rate Rolling Mean — the arithmetic mean of the funding rate over the
/// trailing window of `window` ticks.
///
/// ```text
/// mean = (1 / window) · Σ fundingRate over the last `window` ticks
/// ```
///
/// Smoothing the raw [funding rate] reveals the persistent carry regime — a
/// sustained positive mean marks a crowded-long market paying to hold the
/// perpetual, a sustained negative mean a crowded-short one. The indicator warms
/// up for `window` ticks — `update` returns `None` until the window is full —
/// then emits the rolling mean, maintained in O(1) per tick via a running sum.
///
/// `Input = DerivativesTick`, `Output = f64`.
///
/// [funding rate]: crate::FundingRate
///
/// # Example
///
/// ```
/// use wickra_core::{DerivativesTick, FundingRateMean, Indicator};
///
/// fn tick(rate: f64) -> DerivativesTick {
///     DerivativesTick::new(rate, 100.0, 100.0, 100.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0)
///         .unwrap()
/// }
///
/// let mut frm = FundingRateMean::new(2).unwrap();
/// assert_eq!(frm.update(tick(0.001)), None);
/// // Window full: (0.001 + 0.003) / 2 = 0.002.
/// assert_eq!(frm.update(tick(0.003)), Some(0.002));
/// ```
#[derive(Debug, Clone)]
pub struct FundingRateMean {
    window: usize,
    history: VecDeque<f64>,
    sum: RollingSum,
}

impl FundingRateMean {
    /// Construct a funding-rate rolling mean over a window of `window` ticks.
    ///
    /// # Errors
    ///
    /// Returns [`Error::PeriodZero`] if `window` is zero.
    pub fn new(window: usize) -> Result<Self> {
        if window == 0 {
            return Err(Error::PeriodZero);
        }
        if window > crate::error::MAX_PERIOD {
            return Err(Error::InvalidPeriod {
                message: crate::error::PERIOD_ABOVE_MAX,
            });
        }
        Ok(Self {
            window,
            history: VecDeque::with_capacity(window),
            sum: RollingSum::new(),
        })
    }

    /// The configured window length, in ticks.
    #[must_use]
    pub fn window(&self) -> usize {
        self.window
    }
}

impl Indicator for FundingRateMean {
    type Input = DerivativesTick;
    type Output = f64;

    #[inline]
    fn update(&mut self, tick: DerivativesTick) -> Option<f64> {
        self.history.push_back(tick.funding_rate);
        self.sum.push(tick.funding_rate);
        if self.sum.needs_reseed(self.window) {
            self.sum.reseed(self.history.iter().copied());
        }
        if self.history.len() > self.window {
            let old = self.history.pop_front().expect("window >= 1, len > window");
            self.sum.evict(old);
        }
        if self.history.len() < self.window {
            return None;
        }
        Some(self.sum.value() / self.window as f64)
    }

    fn reset(&mut self) {
        self.history.clear();
        self.sum.reset();
    }

    #[inline]
    fn warmup_period(&self) -> usize {
        self.window
    }

    #[inline]
    fn is_ready(&self) -> bool {
        self.history.len() >= self.window
    }

    #[inline]
    fn name(&self) -> &'static str {
        "FundingRateMean"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;

    fn tick(rate: f64) -> DerivativesTick {
        DerivativesTick::new_unchecked(
            rate, 100.0, 100.0, 100.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0,
        )
    }

    #[test]
    fn rejects_zero_window() {
        assert!(matches!(FundingRateMean::new(0), Err(Error::PeriodZero)));
    }

    #[test]
    fn accessors_and_metadata() {
        let frm = FundingRateMean::new(5).unwrap();
        assert_eq!(frm.name(), "FundingRateMean");
        assert_eq!(frm.warmup_period(), 5);
        assert_eq!(frm.window(), 5);
        assert!(!frm.is_ready());
    }

    #[test]
    fn warms_up_then_emits_mean() {
        let mut frm = FundingRateMean::new(2).unwrap();
        assert_eq!(frm.update(tick(0.001)), None);
        assert!(!frm.is_ready());
        assert_eq!(frm.update(tick(0.003)), Some(0.002));
        assert!(frm.is_ready());
    }

    #[test]
    fn rolls_off_old_values() {
        let mut frm = FundingRateMean::new(2).unwrap();
        frm.update(tick(0.001));
        frm.update(tick(0.003)); // mean 0.002
        let out = frm.update(tick(0.005)).unwrap(); // window [0.003, 0.005] -> 0.004
        assert!((out - 0.004).abs() < 1e-12);
    }

    #[test]
    fn handles_negative_rates() {
        let mut frm = FundingRateMean::new(2).unwrap();
        frm.update(tick(-0.002));
        let out = frm.update(tick(0.004)).unwrap();
        assert!((out - 0.001).abs() < 1e-12);
    }

    #[test]
    fn batch_equals_streaming() {
        let ticks: Vec<DerivativesTick> = (0..30)
            .map(|i| tick(0.0001 * f64::from(i % 7) - 0.0003))
            .collect();
        let mut a = FundingRateMean::new(5).unwrap();
        let mut b = FundingRateMean::new(5).unwrap();
        assert_eq!(
            a.batch(&ticks),
            ticks.iter().map(|x| b.update(*x)).collect::<Vec<_>>()
        );
    }

    #[test]
    fn reset_clears_state() {
        let mut frm = FundingRateMean::new(2).unwrap();
        frm.update(tick(0.001));
        frm.update(tick(0.003));
        assert!(frm.is_ready());
        frm.reset();
        assert!(!frm.is_ready());
        assert_eq!(frm.update(tick(0.002)), None);
    }

    #[test]
    fn rejects_window_above_maximum() {
        assert!(matches!(
            FundingRateMean::new(crate::error::MAX_PERIOD + 1),
            Err(Error::InvalidPeriod { .. })
        ));
    }
}
