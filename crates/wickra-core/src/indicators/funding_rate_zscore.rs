//! Funding Rate Z-Score — how extreme the latest funding rate is versus its
//! recent history.

use std::collections::VecDeque;

use crate::derivatives::DerivativesTick;
use crate::error::{Error, Result};
use crate::indicators::rolling_moments::ShiftedMoments;
use crate::traits::Indicator;

/// Funding Rate Z-Score — the latest funding rate expressed in standard
/// deviations from its rolling mean over the trailing window of `window` ticks.
///
/// ```text
/// zScore = (fundingRate − mean) / population_stddev   over the last `window` ticks
/// ```
///
/// A reading of `+2` means funding is two standard deviations richer than its
/// recent norm — an unusually crowded long, a contrarian fade signal; `−2` is
/// the mirror. Normalising the [funding rate] this way makes funding extremes
/// comparable across regimes and assets. A window with zero dispersion (a flat
/// funding series) yields `0`. The indicator warms up for `window` ticks, then
/// emits the rolling z-score, maintained in O(1) per tick.
///
/// `Input = DerivativesTick`, `Output = f64`.
///
/// [funding rate]: crate::FundingRate
///
/// # Example
///
/// ```
/// use wickra_core::{DerivativesTick, FundingRateZScore, Indicator};
///
/// fn tick(rate: f64) -> DerivativesTick {
///     DerivativesTick::new(rate, 100.0, 100.0, 100.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0)
///         .unwrap()
/// }
///
/// let mut z = FundingRateZScore::new(2).unwrap();
/// assert_eq!(z.update(tick(0.001)), None);
/// // Window [0.001, 0.003]: mean 0.002, population stddev 0.001 -> (0.003 - 0.002) / 0.001 = 1.
/// assert!((z.update(tick(0.003)).unwrap() - 1.0).abs() < 1e-9);
/// ```
#[derive(Debug, Clone)]
pub struct FundingRateZScore {
    window: usize,
    history: VecDeque<f64>,
    moments: ShiftedMoments,
}

impl FundingRateZScore {
    /// Construct a funding-rate z-score over a window of `window` ticks.
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
            moments: ShiftedMoments::new(),
        })
    }

    /// The configured window length, in ticks.
    #[must_use]
    pub fn window(&self) -> usize {
        self.window
    }
}

impl Indicator for FundingRateZScore {
    type Input = DerivativesTick;
    type Output = f64;

    #[inline]
    fn update(&mut self, tick: DerivativesTick) -> Option<f64> {
        let value = tick.funding_rate;
        if self.history.len() == self.window {
            let old = self.history.pop_front().expect("non-empty");
            self.moments.evict(old);
        }
        self.history.push_back(value);
        self.moments.push(value);
        if self.moments.needs_reseed(self.window) {
            self.moments.reseed(self.history.iter().copied());
        }
        if self.history.len() < self.window {
            return None;
        }
        let mean = self.moments.mean(self.window);
        let std = self.moments.std_dev(self.window);
        if std == 0.0 {
            // A window with no dispersion: funding is exactly its own mean.
            return Some(0.0);
        }
        Some((value - mean) / std)
    }

    fn reset(&mut self) {
        self.history.clear();
        self.moments.reset();
    }

    #[inline]
    fn warmup_period(&self) -> usize {
        self.window
    }

    #[inline]
    fn is_ready(&self) -> bool {
        self.history.len() == self.window
    }

    #[inline]
    fn name(&self) -> &'static str {
        "FundingRateZScore"
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
        assert!(matches!(FundingRateZScore::new(0), Err(Error::PeriodZero)));
    }

    #[test]
    fn accessors_and_metadata() {
        let z = FundingRateZScore::new(5).unwrap();
        assert_eq!(z.name(), "FundingRateZScore");
        assert_eq!(z.warmup_period(), 5);
        assert_eq!(z.window(), 5);
        assert!(!z.is_ready());
    }

    #[test]
    fn reference_value() {
        let mut z = FundingRateZScore::new(2).unwrap();
        assert_eq!(z.update(tick(0.001)), None);
        // Window [0.001, 0.003]: mean 0.002, var (1e-6 + 9e-6)/2 - 4e-6 = 1e-6,
        // stddev 0.001; latest 0.003 is (0.003 - 0.002) / 0.001 = 1.
        let out = z.update(tick(0.003)).unwrap();
        assert!((out - 1.0).abs() < 1e-9);
        assert!(z.is_ready());
    }

    #[test]
    fn flat_window_is_zero() {
        let mut z = FundingRateZScore::new(3).unwrap();
        z.update(tick(0.002));
        z.update(tick(0.002));
        assert_eq!(z.update(tick(0.002)), Some(0.0));
    }

    #[test]
    fn rolls_off_old_values() {
        let mut z = FundingRateZScore::new(2).unwrap();
        z.update(tick(0.001));
        z.update(tick(0.003));
        // Window now [0.003, 0.005]: mean 0.004, stddev 0.001 -> +1.
        let out = z.update(tick(0.005)).unwrap();
        assert!((out - 1.0).abs() < 1e-9);
    }

    #[test]
    fn batch_equals_streaming() {
        let ticks: Vec<DerivativesTick> = (0..30)
            .map(|i| tick(0.0001 * f64::from(i % 5) - 0.0002))
            .collect();
        let mut a = FundingRateZScore::new(6).unwrap();
        let mut b = FundingRateZScore::new(6).unwrap();
        assert_eq!(
            a.batch(&ticks),
            ticks.iter().map(|x| b.update(*x)).collect::<Vec<_>>()
        );
    }

    #[test]
    fn reset_clears_state() {
        let mut z = FundingRateZScore::new(2).unwrap();
        z.update(tick(0.001));
        z.update(tick(0.003));
        assert!(z.is_ready());
        z.reset();
        assert!(!z.is_ready());
        assert_eq!(z.update(tick(0.002)), None);
    }

    #[test]
    fn rejects_window_above_maximum() {
        assert!(matches!(
            FundingRateZScore::new(crate::error::MAX_PERIOD + 1),
            Err(Error::InvalidPeriod { .. })
        ));
    }
}
