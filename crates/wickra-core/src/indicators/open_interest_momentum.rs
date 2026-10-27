//! Open-Interest Momentum — the rate of change of open interest over a lookback.

use std::collections::VecDeque;

use crate::derivatives::DerivativesTick;
use crate::error::{Error, Result};
use crate::traits::Indicator;

/// Open-Interest Momentum — the percentage rate of change of open interest over a
/// `period`-tick lookback.
///
/// ```text
/// OIM = 100 · (OI_t − OI_{t−period}) / OI_{t−period}
/// ```
///
/// Where [`OpenInterestDelta`](crate::OpenInterestDelta) reports the single-tick change in open
/// interest, OI Momentum measures the trend in positioning over a window: positive
/// values mean open interest is expanding (new money entering — a position build
/// that fuels the prevailing move), negative values mean it is contracting
/// (positions being closed — deleveraging or short-covering). Read alongside price:
/// rising OI with rising price is a strong new-long trend, while rising price with
/// falling OI is a short-covering rally on borrowed time.
///
/// The output is a percentage and may be negative. A zero base open interest
/// `period` ticks ago reports `0` rather than dividing by zero. The first value
/// lands after `period + 1` inputs. Each `update` is O(1).
///
/// # Example
///
/// ```
/// use wickra_core::{DerivativesTick, Indicator, OpenInterestMomentum};
///
/// let mut indicator = OpenInterestMomentum::new(5).unwrap();
/// let mut last = None;
/// for i in 0..20 {
///     let oi = 1_000.0 + f64::from(i) * 100.0;
///     let tick = DerivativesTick::new(0.0, 100.0, 100.0, 100.0, oi, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0).unwrap();
///     last = indicator.update(tick);
/// }
/// assert!(last.unwrap() > 0.0); // expanding OI
/// ```
#[derive(Debug, Clone)]
pub struct OpenInterestMomentum {
    period: usize,
    window: VecDeque<f64>,
    last: Option<f64>,
}

impl OpenInterestMomentum {
    /// Construct an OI Momentum over a `period`-tick lookback.
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
            window: VecDeque::with_capacity(period + 1),
            last: None,
        })
    }

    /// Configured lookback period.
    pub const fn period(&self) -> usize {
        self.period
    }

    /// Current value if available.
    pub const fn value(&self) -> Option<f64> {
        self.last
    }
}

impl Indicator for OpenInterestMomentum {
    type Input = DerivativesTick;
    type Output = f64;

    #[inline]
    fn update(&mut self, tick: DerivativesTick) -> Option<f64> {
        if self.window.len() == self.period + 1 {
            self.window.pop_front();
        }
        self.window.push_back(tick.open_interest);
        if self.window.len() < self.period + 1 {
            return None;
        }
        let base = *self.window.front().expect("non-empty");
        let current = tick.open_interest;
        let oim = if base > 0.0 {
            100.0 * (current - base) / base
        } else {
            0.0
        };
        self.last = Some(oim);
        Some(oim)
    }

    fn reset(&mut self) {
        self.window.clear();
        self.last = None;
    }

    #[inline]
    fn warmup_period(&self) -> usize {
        self.period + 1
    }

    #[inline]
    fn is_ready(&self) -> bool {
        self.last.is_some()
    }

    #[inline]
    fn name(&self) -> &'static str {
        "OpenInterestMomentum"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;
    use approx::assert_relative_eq;

    fn tick(oi: f64) -> DerivativesTick {
        DerivativesTick::new_unchecked(
            0.0, 100.0, 100.0, 100.0, oi, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0,
        )
    }

    #[test]
    fn rejects_zero_period() {
        assert!(matches!(
            OpenInterestMomentum::new(0),
            Err(Error::PeriodZero)
        ));
    }

    #[test]
    fn accessors_and_metadata() {
        let o = OpenInterestMomentum::new(5).unwrap();
        assert_eq!(o.period(), 5);
        assert_eq!(o.warmup_period(), 6);
        assert_eq!(o.name(), "OpenInterestMomentum");
        assert!(!o.is_ready());
        assert_eq!(o.value(), None);
    }

    #[test]
    fn first_emission_at_warmup_period() {
        let mut o = OpenInterestMomentum::new(3).unwrap();
        let ticks: Vec<DerivativesTick> = (0..6)
            .map(|i| tick(1_000.0 + f64::from(i) * 100.0))
            .collect();
        let out = o.batch(&ticks);
        for v in out.iter().take(3) {
            assert!(v.is_none());
        }
        assert!(out[3].is_some());
    }

    #[test]
    fn reference_value() {
        // period 2: OI 1000 -> 1200 over the window -> +20%.
        let mut o = OpenInterestMomentum::new(2).unwrap();
        let out = o.batch(&[tick(1_000.0), tick(1_100.0), tick(1_200.0)]);
        assert_relative_eq!(out[2].unwrap(), 20.0, epsilon = 1e-9);
    }

    #[test]
    fn expanding_oi_is_positive() {
        let mut o = OpenInterestMomentum::new(5).unwrap();
        let ticks: Vec<DerivativesTick> = (0..20)
            .map(|i| tick(1_000.0 + f64::from(i) * 100.0))
            .collect();
        let last = o.batch(&ticks).into_iter().flatten().last().unwrap();
        assert!(last > 0.0);
    }

    #[test]
    fn contracting_oi_is_negative() {
        let mut o = OpenInterestMomentum::new(5).unwrap();
        let ticks: Vec<DerivativesTick> = (0..20)
            .map(|i| tick(3_000.0 - f64::from(i) * 100.0))
            .collect();
        let last = o.batch(&ticks).into_iter().flatten().last().unwrap();
        assert!(last < 0.0);
    }

    #[test]
    fn zero_base_is_zero() {
        let mut o = OpenInterestMomentum::new(2).unwrap();
        let out = o.batch(&[tick(0.0), tick(100.0), tick(200.0)]);
        assert_relative_eq!(out[2].unwrap(), 0.0, epsilon = 1e-12);
    }

    #[test]
    fn reset_clears_state() {
        let mut o = OpenInterestMomentum::new(3).unwrap();
        o.batch(
            &(0..10)
                .map(|i| tick(1_000.0 + f64::from(i) * 50.0))
                .collect::<Vec<_>>(),
        );
        assert!(o.is_ready());
        o.reset();
        assert!(!o.is_ready());
        assert_eq!(o.value(), None);
        assert_eq!(o.update(tick(1_000.0)), None);
    }

    #[test]
    fn batch_equals_streaming() {
        let ticks: Vec<DerivativesTick> = (0..80)
            .map(|i| tick(1_000.0 + (f64::from(i) * 0.25).sin() * 300.0))
            .collect();
        let batch = OpenInterestMomentum::new(10).unwrap().batch(&ticks);
        let mut b = OpenInterestMomentum::new(10).unwrap();
        let streamed: Vec<_> = ticks.iter().map(|x| b.update(*x)).collect();
        assert_eq!(batch, streamed);
    }

    #[test]
    fn rejects_period_above_maximum() {
        assert!(matches!(
            OpenInterestMomentum::new(crate::error::MAX_PERIOD + 1),
            Err(Error::InvalidPeriod { .. })
        ));
    }
}
