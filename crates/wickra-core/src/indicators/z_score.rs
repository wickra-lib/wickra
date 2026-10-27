//! Z-Score.

use std::collections::VecDeque;

use crate::error::{Error, Result};
use crate::indicators::rolling_moments::ShiftedMoments;
use crate::traits::Indicator;

/// Z-Score — how many standard deviations the latest price sits from its
/// rolling mean.
///
/// ```text
/// ZScore = (price − SMA(price, n)) / population_stddev(price, n)
/// ```
///
/// A reading of `+2` means price is two standard deviations above its recent
/// average — statistically stretched to the upside; `−2` is the mirror. It is
/// the standard normalisation behind mean-reversion strategies: a large
/// magnitude flags an extension, a return toward `0` flags reversion. A window
/// with zero dispersion (a flat series) yields `0`.
///
/// # Example
///
/// ```
/// use wickra_core::{Indicator, ZScore};
///
/// let mut indicator = ZScore::new(20).unwrap();
/// let mut last = None;
/// for i in 0..80 {
///     last = indicator.update(f64::from(i));
/// }
/// assert!(last.is_some());
/// ```
#[derive(Debug, Clone)]
pub struct ZScore {
    period: usize,
    window: VecDeque<f64>,
    moments: ShiftedMoments,
}

impl ZScore {
    /// Construct a new Z-Score over a rolling window of `period` prices.
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
            window: VecDeque::with_capacity(period),
            moments: ShiftedMoments::new(),
        })
    }

    /// Configured period.
    pub const fn period(&self) -> usize {
        self.period
    }
}

impl Indicator for ZScore {
    type Input = f64;
    type Output = f64;

    #[inline]
    fn update(&mut self, value: f64) -> Option<f64> {
        if !value.is_finite() {
            return None;
        }
        if self.window.len() == self.period {
            let old = self.window.pop_front().expect("non-empty");
            self.moments.evict(old);
        }
        self.window.push_back(value);
        self.moments.push(value);
        if self.moments.needs_reseed(self.period) {
            self.moments.reseed(self.window.iter().copied());
        }
        if self.window.len() < self.period {
            return None;
        }
        let mean = self.moments.mean(self.period);
        let std = self.moments.std_dev(self.period);
        if std == 0.0 {
            // A window with no dispersion: the price is exactly its own mean.
            return Some(0.0);
        }
        Some((value - mean) / std)
    }

    fn reset(&mut self) {
        self.window.clear();
        self.moments.reset();
    }

    #[inline]
    fn warmup_period(&self) -> usize {
        self.period
    }

    #[inline]
    fn is_ready(&self) -> bool {
        self.window.len() == self.period
    }

    #[inline]
    fn name(&self) -> &'static str {
        "ZScore"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;
    use approx::assert_relative_eq;

    #[test]
    fn reference_values() {
        // Window [1, 3]: mean 2, population variance (1 + 9)/2 − 4 = 1,
        // stddev 1; the latest price 3 is (3 − 2) / 1 = 1 stddev above.
        let mut z = ZScore::new(2).unwrap();
        let out = z.batch(&[1.0, 3.0]);
        assert!(out[0].is_none());
        assert_relative_eq!(out[1].unwrap(), 1.0, epsilon = 1e-12);
    }

    #[test]
    fn constant_series_yields_zero() {
        let mut z = ZScore::new(10).unwrap();
        for v in z.batch(&[42.0; 30]).into_iter().flatten() {
            assert_relative_eq!(v, 0.0, epsilon = 1e-12);
        }
    }

    #[test]
    fn rising_price_is_above_its_mean() {
        // A monotonically rising series always sits above its trailing mean.
        let prices: Vec<f64> = (0..40).map(f64::from).collect();
        let mut z = ZScore::new(10).unwrap();
        for v in z.batch(&prices).into_iter().flatten() {
            assert!(
                v > 0.0,
                "a rising price should score above its mean, got {v}"
            );
        }
    }

    #[test]
    fn first_value_on_period_th_input() {
        let mut z = ZScore::new(5).unwrap();
        let out = z.batch(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0]);
        for (i, v) in out.iter().enumerate().take(4) {
            assert!(v.is_none(), "index {i} must be None during warmup");
        }
        assert!(out[4].is_some(), "first value lands at index period - 1");
        assert_eq!(z.warmup_period(), 5);
    }

    #[test]
    fn rejects_zero_period() {
        assert!(ZScore::new(0).is_err());
    }

    /// Cover the const accessor `period` (59-61) and the Indicator-impl
    /// `name` body (106-108). `warmup_period` is exercised elsewhere.
    #[test]
    fn accessors_and_metadata() {
        let z = ZScore::new(20).unwrap();
        assert_eq!(z.period(), 20);
        assert_eq!(z.name(), "ZScore");
    }

    #[test]
    fn reset_clears_state() {
        let mut z = ZScore::new(5).unwrap();
        z.batch(&[1.0, 2.0, 3.0, 4.0, 5.0]);
        assert!(z.is_ready());
        z.reset();
        assert!(!z.is_ready());
        assert_eq!(z.update(1.0), None);
    }

    #[test]
    fn batch_equals_streaming() {
        let prices: Vec<f64> = (0..60)
            .map(|i| 50.0 + (f64::from(i) * 0.3).sin() * 10.0)
            .collect();
        let mut a = ZScore::new(20).unwrap();
        let mut b = ZScore::new(20).unwrap();
        assert_eq!(
            a.batch(&prices),
            prices.iter().map(|x| b.update(*x)).collect::<Vec<_>>()
        );
    }

    #[test]
    fn rejects_period_above_maximum() {
        assert!(matches!(
            ZScore::new(crate::error::MAX_PERIOD + 1),
            Err(Error::InvalidPeriod { .. })
        ));
    }
}
