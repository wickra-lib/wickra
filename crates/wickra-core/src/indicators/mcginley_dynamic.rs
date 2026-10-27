//! `McGinley` Dynamic — self-adjusting moving average.

use std::collections::VecDeque;

use crate::error::{Error, Result};
use crate::traits::Indicator;

/// John `McGinley`'s "Dynamic" — a self-adjusting moving average that speeds up
/// in downtrends and slows down in uptrends to track price more closely than
/// a fixed-period MA.
///
/// The recurrence is
///
/// ```text
/// MD_t = MD_{t-1} + (price_t - MD_{t-1}) / (K * period * (price_t / MD_{t-1})^4)
/// ```
///
/// where `K = 0.6` is `McGinley`'s original constant. The fourth-power ratio
/// term shrinks the divisor when price falls below the indicator (faster
/// catch-up) and inflates it when price runs above (more smoothing). The
/// indicator is seeded with the simple average of the first `period` inputs.
///
/// Reference: John R. `McGinley` Jr., *Technical Analysis of Stocks &
/// Commodities*, 1990.
///
/// # Example
///
/// ```
/// use wickra_core::{Indicator, McGinleyDynamic};
///
/// let mut md = McGinleyDynamic::new(10).unwrap();
/// let mut last = None;
/// for i in 0..40 {
///     last = md.update(100.0 + f64::from(i));
/// }
/// assert!(last.is_some());
/// ```
#[derive(Debug, Clone)]
pub struct McGinleyDynamic {
    period: usize,
    seed: VecDeque<f64>,
    seed_sum: f64,
    current: Option<f64>,
}

/// `McGinley`'s original constant `K` in the recurrence denominator.
const K: f64 = 0.6;

impl McGinleyDynamic {
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
            seed: VecDeque::with_capacity(period),
            seed_sum: 0.0,
            current: None,
        })
    }

    /// Configured period.
    pub const fn period(&self) -> usize {
        self.period
    }

    /// Current value if available.
    pub const fn value(&self) -> Option<f64> {
        self.current
    }
}

impl Indicator for McGinleyDynamic {
    type Input = f64;
    type Output = f64;

    #[inline]
    fn update(&mut self, input: f64) -> Option<f64> {
        if !input.is_finite() {
            return None;
        }
        if let Some(prev) = self.current {
            // The recurrence divides by `(price / prev)^4`; if either side is
            // zero or negative the formula blows up, so we hold the previous
            // value as a defensive fallback against degenerate price series.
            if prev <= 0.0 || input <= 0.0 {
                return self.current;
            }
            let ratio = input / prev;
            let divisor = K * (self.period as f64) * ratio.powi(4);
            let next = prev + (input - prev) / divisor;
            self.current = Some(next);
        } else {
            self.seed.push_back(input);
            self.seed_sum += input;
            if self.seed.len() == self.period {
                self.current = Some(self.seed_sum / self.period as f64);
            }
        }
        self.current
    }

    fn reset(&mut self) {
        self.seed.clear();
        self.seed_sum = 0.0;
        self.current = None;
    }

    #[inline]
    fn warmup_period(&self) -> usize {
        self.period
    }

    #[inline]
    fn is_ready(&self) -> bool {
        self.current.is_some()
    }

    #[inline]
    fn name(&self) -> &'static str {
        "McGinleyDynamic"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;
    use approx::assert_relative_eq;

    #[test]
    fn rejects_zero_period() {
        assert!(matches!(McGinleyDynamic::new(0), Err(Error::PeriodZero)));
    }

    #[test]
    fn accessors_and_metadata() {
        let mut md = McGinleyDynamic::new(10).unwrap();
        assert_eq!(md.period(), 10);
        assert_eq!(md.warmup_period(), 10);
        assert_eq!(md.name(), "McGinleyDynamic");
        assert_eq!(md.value(), None);
        for i in 1..=10 {
            md.update(f64::from(i));
        }
        assert!(md.value().is_some());
    }

    #[test]
    fn constant_series_yields_the_constant() {
        // ratio = 1, so the recurrence collapses to MD + 0 / divisor = MD.
        let mut md = McGinleyDynamic::new(5).unwrap();
        let out = md.batch(&[42.0_f64; 30]);
        for v in out.iter().skip(4).flatten() {
            assert_relative_eq!(*v, 42.0, epsilon = 1e-12);
        }
    }

    #[test]
    fn warmup_emits_first_value_at_period() {
        let mut md = McGinleyDynamic::new(3).unwrap();
        // Seed = SMA([10, 20, 30]) = 20.0.
        assert_eq!(md.update(10.0), None);
        assert_eq!(md.update(20.0), None);
        assert_eq!(md.update(30.0), Some(20.0));
    }

    #[test]
    fn reference_value_recurrence() {
        // Period 3, seed = SMA([10, 20, 30]) = 20.0. Then on price = 40.0:
        //   ratio   = 40 / 20 = 2
        //   divisor = 0.6 * 3 * 2^4 = 0.6 * 3 * 16 = 28.8
        //   next    = 20 + (40 - 20) / 28.8 = 20.694444...
        let mut md = McGinleyDynamic::new(3).unwrap();
        md.batch(&[10.0_f64, 20.0, 30.0]);
        let v = md.update(40.0).unwrap();
        let expected = 20.0 + 20.0 / (0.6 * 3.0 * 16.0);
        assert_relative_eq!(v, expected, epsilon = 1e-12);
    }

    #[test]
    fn batch_equals_streaming() {
        let prices: Vec<f64> = (1..=80)
            .map(|i| 100.0 + (f64::from(i) * 0.2).sin() * 5.0)
            .collect();
        let mut a = McGinleyDynamic::new(10).unwrap();
        let mut b = McGinleyDynamic::new(10).unwrap();
        assert_eq!(
            a.batch(&prices),
            prices.iter().map(|p| b.update(*p)).collect::<Vec<_>>()
        );
    }

    #[test]
    fn reset_clears_state() {
        let mut md = McGinleyDynamic::new(5).unwrap();
        md.batch(&(1..=30).map(f64::from).collect::<Vec<_>>());
        assert!(md.is_ready());
        md.reset();
        assert!(!md.is_ready());
        assert_eq!(md.update(1.0), None);
    }

    #[test]
    fn ignores_non_finite_input() {
        let mut md = McGinleyDynamic::new(3).unwrap();
        md.batch(&[10.0_f64, 20.0, 30.0]);
        md.value().unwrap();
        assert_eq!(md.update(f64::NAN), None);
        assert_eq!(md.update(f64::INFINITY), None);
    }

    #[test]
    fn holds_value_when_input_is_non_positive() {
        // Defensive: a zero or negative price would make the (price/prev)^4
        // divisor zero or otherwise blow up; the recurrence holds steady.
        let mut md = McGinleyDynamic::new(3).unwrap();
        md.batch(&[10.0_f64, 20.0, 30.0]);
        let before = md.value().unwrap();
        assert_eq!(md.update(0.0), Some(before));
        assert_eq!(md.update(-5.0), Some(before));
        // Once a positive price arrives the recurrence resumes normally.
        let after = md.update(40.0).unwrap();
        assert!(after > before);
    }

    #[test]
    fn rejects_period_above_maximum() {
        assert!(matches!(
            McGinleyDynamic::new(crate::error::MAX_PERIOD + 1),
            Err(Error::InvalidPeriod { .. })
        ));
    }
}
