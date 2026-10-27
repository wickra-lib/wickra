//! Gain-to-Pain Ratio (Schwager) — sum of returns over the sum of losses.

use std::collections::VecDeque;

use crate::error::{Error, Result};
use crate::traits::Indicator;

/// Gain-to-Pain Ratio — Jack Schwager's measure of return per unit of downside:
/// the sum of all returns divided by the sum of the absolute *negative* returns.
///
/// ```text
/// GPR = Σ returns / Σ |negative returns|        over the window
/// ```
///
/// Where the [`GainLossRatio`](crate::GainLossRatio) compares *average* win to
/// *average* loss and the [`ProfitFactor`](crate::ProfitFactor) compares gross
/// profit to gross loss, the Gain-to-Pain Ratio puts the **net** result over the
/// total pain endured to earn it. Schwager treats a GPR above `1.0` as good and
/// above `2.0` as excellent for a monthly return series: the strategy made more
/// than it lost on the way, and twice as much when GPR is `2`. A flat series, or
/// one with no losses, has no measurable pain and reports `0` (undefined).
///
/// The output is unbounded and may be negative (a net-losing window). The first
/// value lands after `period` returns; each `update` is O(1).
///
/// # Example
///
/// ```
/// use wickra_core::{Indicator, GainToPainRatio};
///
/// let mut indicator = GainToPainRatio::new(12).unwrap();
/// let mut last = None;
/// for i in 0..24 {
///     last = indicator.update((f64::from(i) * 0.5).sin() * 0.02);
/// }
/// assert!(last.is_some());
/// ```
#[derive(Debug, Clone)]
pub struct GainToPainRatio {
    period: usize,
    window: VecDeque<f64>,
    sum_all: f64,
    sum_pain: f64,
}

impl GainToPainRatio {
    /// Construct a Gain-to-Pain Ratio over `period` returns.
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
            window: VecDeque::with_capacity(period),
            sum_all: 0.0,
            sum_pain: 0.0,
        })
    }

    /// Configured window of returns.
    pub const fn period(&self) -> usize {
        self.period
    }
}

impl Indicator for GainToPainRatio {
    type Input = f64;
    type Output = f64;

    #[inline]
    fn update(&mut self, ret: f64) -> Option<f64> {
        if !ret.is_finite() {
            return None;
        }
        if self.window.len() == self.period {
            let old = self.window.pop_front().expect("non-empty");
            self.sum_all -= old;
            if old < 0.0 {
                self.sum_pain -= -old;
            }
        }
        self.window.push_back(ret);
        self.sum_all += ret;
        if ret < 0.0 {
            self.sum_pain += -ret;
        }
        if self.window.len() < self.period {
            return None;
        }
        Some(self.compute())
    }

    fn reset(&mut self) {
        self.window.clear();
        self.sum_all = 0.0;
        self.sum_pain = 0.0;
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
        "GainToPainRatio"
    }
}

impl GainToPainRatio {
    fn compute(&self) -> f64 {
        if self.sum_pain > 0.0 {
            self.sum_all / self.sum_pain
        } else {
            0.0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;
    use approx::assert_relative_eq;

    #[test]
    fn rejects_zero_period() {
        assert!(matches!(GainToPainRatio::new(0), Err(Error::PeriodZero)));
    }

    #[test]
    fn accessors_and_metadata() {
        let g = GainToPainRatio::new(12).unwrap();
        assert_eq!(g.period(), 12);
        assert_eq!(g.warmup_period(), 12);
        assert_eq!(g.name(), "GainToPainRatio");
        assert!(!g.is_ready());
    }

    #[test]
    fn first_emission_at_warmup_period() {
        let mut g = GainToPainRatio::new(4).unwrap();
        let out = g.batch(&[0.01, -0.01, 0.02, -0.01, 0.03]);
        for v in out.iter().take(3) {
            assert!(v.is_none());
        }
        assert!(out[3].is_some());
    }

    #[test]
    fn reference_value() {
        // returns: +0.04, -0.02 -> sum_all = 0.02, pain = 0.02 -> GPR = 1.0.
        let mut g = GainToPainRatio::new(2).unwrap();
        let out = g.batch(&[0.04, -0.02]);
        assert_relative_eq!(out[1].unwrap(), 1.0, epsilon = 1e-9);
    }

    #[test]
    fn net_losing_window_is_negative() {
        let mut g = GainToPainRatio::new(3).unwrap();
        let last = g
            .batch(&[-0.03, 0.01, -0.02])
            .into_iter()
            .flatten()
            .last()
            .unwrap();
        assert!(last < 0.0);
    }

    #[test]
    fn no_pain_is_zero() {
        let mut g = GainToPainRatio::new(3).unwrap();
        let last = g
            .batch(&[0.01, 0.02, 0.03])
            .into_iter()
            .flatten()
            .last()
            .unwrap();
        assert_relative_eq!(last, 0.0, epsilon = 1e-12);
    }

    #[test]
    fn ignores_non_finite() {
        let mut g = GainToPainRatio::new(2).unwrap();
        let _ready = g
            .batch(&[0.04, -0.02])
            .into_iter()
            .flatten()
            .last()
            .unwrap();
        assert_eq!(g.update(f64::NAN), None);
    }

    #[test]
    fn non_finite_before_ready_is_none() {
        // A non-finite value arriving before the window fills yields None.
        let mut g = GainToPainRatio::new(3).unwrap();
        assert_eq!(g.update(0.02), None);
        assert_eq!(g.update(f64::NAN), None);
    }

    #[test]
    fn reset_clears_state() {
        let mut g = GainToPainRatio::new(2).unwrap();
        g.batch(&[0.04, -0.02]);
        assert!(g.is_ready());
        g.reset();
        assert!(!g.is_ready());
        assert_eq!(g.update(0.01), None);
    }

    #[test]
    fn batch_equals_streaming() {
        let rets: Vec<f64> = (0..60).map(|i| (f64::from(i) * 0.3).sin() * 0.02).collect();
        let batch = GainToPainRatio::new(12).unwrap().batch(&rets);
        let mut b = GainToPainRatio::new(12).unwrap();
        let streamed: Vec<_> = rets.iter().map(|r| b.update(*r)).collect();
        assert_eq!(batch, streamed);
    }

    #[test]
    fn rejects_period_above_maximum() {
        assert!(matches!(
            GainToPainRatio::new(crate::error::MAX_PERIOD + 1),
            Err(Error::InvalidPeriod { .. })
        ));
    }
}
