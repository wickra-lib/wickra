//! Expectancy — expected return per unit of average loss (R-multiple).

use std::collections::VecDeque;

use crate::error::{Error, Result};
use crate::traits::Indicator;

/// Expectancy — the expected return per trade expressed in units of average
/// loss (the "R-multiple" expectancy) over the last `period` returns.
///
/// ```text
/// mean    = average of the `period` returns
/// avgLoss = average of the absolute losing returns (rᵢ < 0)
/// E       = mean / avgLoss          (0 when there are no losing returns)
/// ```
///
/// Feed a stream of per-trade or per-bar returns. Expectancy answers "how much
/// do I make per trade for every unit I typically risk": `E = 0.3` means the
/// system nets `0.3R` per trade on average, where `R` is the average loss.
/// Dividing the mean return by the average loss makes the figure comparable
/// across systems with different bet sizes — unlike the raw mean return (which
/// is just an SMA of the series). A positive `E` is a profitable edge, a
/// negative `E` a losing one.
///
/// When the window contains **no** losing returns there is no risk reference to
/// normalise against, so the indicator returns `0` (undefined R-multiple)
/// rather than dividing by zero.
///
/// Each `update` is O(1): the running sum and the loss aggregates are
/// maintained incrementally.
///
/// # Example
///
/// ```
/// use wickra_core::{BatchExt, Indicator, Expectancy};
///
/// let mut indicator = Expectancy::new(4).unwrap();
/// // returns +2, -1, +2, -1: mean 0.5, avg loss 1 -> E = 0.5.
/// let out = indicator.batch(&[2.0, -1.0, 2.0, -1.0]);
/// assert_eq!(out[3], Some(0.5));
/// ```
#[derive(Debug, Clone)]
pub struct Expectancy {
    period: usize,
    window: VecDeque<f64>,
    sum: f64,
    sum_abs_loss: f64,
    loss_count: usize,
}

impl Expectancy {
    /// Construct a new Expectancy over the given window.
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
            sum: 0.0,
            sum_abs_loss: 0.0,
            loss_count: 0,
        })
    }

    /// Configured period.
    pub const fn period(&self) -> usize {
        self.period
    }
}

impl Indicator for Expectancy {
    type Input = f64;
    type Output = f64;

    #[inline]
    fn update(&mut self, ret: f64) -> Option<f64> {
        if !ret.is_finite() {
            return None;
        }
        if self.window.len() == self.period {
            let old = self.window.pop_front().expect("window is non-empty");
            self.sum -= old;
            if old < 0.0 {
                self.sum_abs_loss -= -old;
                self.loss_count -= 1;
            }
        }
        self.window.push_back(ret);
        self.sum += ret;
        if ret < 0.0 {
            self.sum_abs_loss += -ret;
            self.loss_count += 1;
        }
        if self.window.len() < self.period {
            return None;
        }
        if self.loss_count == 0 {
            // No losing returns: no risk reference to express the edge in.
            return Some(0.0);
        }
        let mean = self.sum / self.period as f64;
        let avg_loss = self.sum_abs_loss / self.loss_count as f64;
        Some(mean / avg_loss)
    }

    fn reset(&mut self) {
        self.window.clear();
        self.sum = 0.0;
        self.sum_abs_loss = 0.0;
        self.loss_count = 0;
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
        "Expectancy"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;
    use approx::assert_relative_eq;

    #[test]
    fn rejects_zero_period() {
        assert!(matches!(Expectancy::new(0), Err(Error::PeriodZero)));
    }

    #[test]
    fn accessors_and_metadata() {
        let e = Expectancy::new(20).unwrap();
        assert_eq!(e.period(), 20);
        assert_eq!(e.warmup_period(), 20);
        assert_eq!(e.name(), "Expectancy");
        assert!(!e.is_ready());
    }

    #[test]
    fn positive_edge() {
        // +2, -1, +2, -1: mean 0.5, avgLoss 1 -> 0.5.
        let mut e = Expectancy::new(4).unwrap();
        let out = e.batch(&[2.0, -1.0, 2.0, -1.0]);
        assert_relative_eq!(out[3].unwrap(), 0.5, epsilon = 1e-12);
    }

    #[test]
    fn negative_edge() {
        // +1, -2, +1, -2: mean -0.5, avgLoss 2 -> -0.25.
        let mut e = Expectancy::new(4).unwrap();
        let out = e.batch(&[1.0, -2.0, 1.0, -2.0]);
        assert_relative_eq!(out[3].unwrap(), -0.25, epsilon = 1e-12);
    }

    #[test]
    fn no_losses_returns_zero() {
        // All winning returns: no risk reference -> 0.
        let mut e = Expectancy::new(5).unwrap();
        for v in e.batch(&[1.0, 2.0, 3.0, 1.0, 2.0]).into_iter().flatten() {
            assert_relative_eq!(v, 0.0, epsilon = 1e-12);
        }
    }

    #[test]
    fn flat_returns_are_not_losses() {
        // Zeros are not losses: mean (2+0+2+0)/4 = 1, but no losing returns
        // -> 0 (undefined R-multiple).
        let mut e = Expectancy::new(4).unwrap();
        let out = e.batch(&[2.0, 0.0, 2.0, 0.0]);
        assert_relative_eq!(out[3].unwrap(), 0.0, epsilon = 1e-12);
    }

    #[test]
    fn rolling_window_evicts_old_losses() {
        // period 4. Window [+2,-1,+2,-1] -> 0.5; then push +3,+3,+3,+3 to evict
        // all losses -> no losses -> 0.
        let mut e = Expectancy::new(4).unwrap();
        let out = e.batch(&[2.0, -1.0, 2.0, -1.0, 3.0, 3.0, 3.0, 3.0]);
        assert_relative_eq!(out[3].unwrap(), 0.5, epsilon = 1e-12);
        assert_relative_eq!(out[7].unwrap(), 0.0, epsilon = 1e-12);
    }

    #[test]
    fn reset_clears_state() {
        let mut e = Expectancy::new(5).unwrap();
        e.batch(&[1.0, -1.0, 2.0, -2.0, 1.0]);
        assert!(e.is_ready());
        e.reset();
        assert!(!e.is_ready());
        assert_eq!(e.update(1.0), None);
    }

    #[test]
    fn batch_equals_streaming() {
        let rets: Vec<f64> = (0..60).map(|i| (f64::from(i) * 0.5).sin() * 2.0).collect();
        let batch = Expectancy::new(14).unwrap().batch(&rets);
        let mut b = Expectancy::new(14).unwrap();
        let streamed: Vec<_> = rets.iter().map(|p| b.update(*p)).collect();
        assert_eq!(batch, streamed);
    }

    #[test]
    fn rejects_period_above_maximum() {
        assert!(matches!(
            Expectancy::new(crate::error::MAX_PERIOD + 1),
            Err(Error::InvalidPeriod { .. })
        ));
    }
}
