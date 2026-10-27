//! Win Rate — the fraction of winning returns over a rolling window.

use std::collections::VecDeque;

use crate::error::{Error, Result};
use crate::traits::Indicator;

/// Win Rate — the fraction of strictly-positive returns among the last `period`
/// returns, in `[0, 1]`.
///
/// ```text
/// WinRate = #(rᵢ > 0) / period
/// ```
///
/// Feed a stream of per-trade or per-bar returns (or `PnL`); the indicator reports
/// the rolling hit rate. A return of exactly `0` is treated as a non-win (a
/// flat / scratch), so `WinRate` is the share of the window that strictly made
/// money — the most basic performance statistic and a building block for
/// [`Expectancy`](crate::Expectancy), Kelly sizing, and confidence filters.
///
/// Each `update` is O(1): the count of wins in the window is maintained
/// incrementally.
///
/// # Example
///
/// ```
/// use wickra_core::{Indicator, WinRate};
///
/// let mut indicator = WinRate::new(4).unwrap();
/// // returns: +, -, +, +  -> 3 of 4 win -> 0.75.
/// let out = indicator.batch(&[1.0, -1.0, 2.0, 1.0]);
/// # use wickra_core::BatchExt;
/// assert_eq!(out[3], Some(0.75));
/// ```
#[derive(Debug, Clone)]
pub struct WinRate {
    period: usize,
    window: VecDeque<f64>,
    wins: usize,
}

impl WinRate {
    /// Construct a new Win Rate over the given window.
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
            wins: 0,
        })
    }

    /// Configured period.
    pub const fn period(&self) -> usize {
        self.period
    }
}

impl Indicator for WinRate {
    type Input = f64;
    type Output = f64;

    #[inline]
    fn update(&mut self, ret: f64) -> Option<f64> {
        if !ret.is_finite() {
            return None;
        }
        if self.window.len() == self.period {
            let old = self.window.pop_front().expect("window is non-empty");
            if old > 0.0 {
                self.wins -= 1;
            }
        }
        self.window.push_back(ret);
        if ret > 0.0 {
            self.wins += 1;
        }
        if self.window.len() < self.period {
            return None;
        }
        Some(self.wins as f64 / self.period as f64)
    }

    fn reset(&mut self) {
        self.window.clear();
        self.wins = 0;
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
        "WinRate"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;
    use approx::assert_relative_eq;

    #[test]
    fn rejects_zero_period() {
        assert!(matches!(WinRate::new(0), Err(Error::PeriodZero)));
    }

    #[test]
    fn accessors_and_metadata() {
        let wr = WinRate::new(20).unwrap();
        assert_eq!(wr.period(), 20);
        assert_eq!(wr.warmup_period(), 20);
        assert_eq!(wr.name(), "WinRate");
        assert!(!wr.is_ready());
    }

    #[test]
    fn reference_value() {
        // +, -, +, + -> 3 wins of 4 -> 0.75.
        let mut wr = WinRate::new(4).unwrap();
        let out = wr.batch(&[1.0, -1.0, 2.0, 1.0]);
        assert_relative_eq!(out[3].unwrap(), 0.75, epsilon = 1e-12);
    }

    #[test]
    fn all_wins_is_one() {
        let mut wr = WinRate::new(5).unwrap();
        for v in wr.batch(&[1.0; 10]).into_iter().flatten() {
            assert_relative_eq!(v, 1.0, epsilon = 1e-12);
        }
    }

    #[test]
    fn all_losses_is_zero() {
        let mut wr = WinRate::new(5).unwrap();
        for v in wr.batch(&[-1.0; 10]).into_iter().flatten() {
            assert_relative_eq!(v, 0.0, epsilon = 1e-12);
        }
    }

    #[test]
    fn flat_returns_are_not_wins() {
        // Zeros count as non-wins: 2 wins, 2 flats -> 0.5.
        let mut wr = WinRate::new(4).unwrap();
        let out = wr.batch(&[1.0, 0.0, 2.0, 0.0]);
        assert_relative_eq!(out[3].unwrap(), 0.5, epsilon = 1e-12);
    }

    #[test]
    fn rolling_window_drops_old_wins() {
        // period 3: after [+,+,+] -> 1.0, then three losses slide the wins out.
        let mut wr = WinRate::new(3).unwrap();
        let out = wr.batch(&[1.0, 1.0, 1.0, -1.0, -1.0, -1.0]);
        assert_relative_eq!(out[2].unwrap(), 1.0, epsilon = 1e-12);
        assert_relative_eq!(out[5].unwrap(), 0.0, epsilon = 1e-12);
    }

    #[test]
    fn output_within_bounds() {
        let mut wr = WinRate::new(20).unwrap();
        let rets: Vec<f64> = (0..200).map(|i| (f64::from(i) * 0.7).sin()).collect();
        for v in wr.batch(&rets).into_iter().flatten() {
            assert!((0.0..=1.0).contains(&v), "out of bounds: {v}");
        }
    }

    #[test]
    fn reset_clears_state() {
        let mut wr = WinRate::new(5).unwrap();
        wr.batch(&[1.0, -1.0, 1.0, -1.0, 1.0]);
        assert!(wr.is_ready());
        wr.reset();
        assert!(!wr.is_ready());
        assert_eq!(wr.update(1.0), None);
    }

    #[test]
    fn batch_equals_streaming() {
        let rets: Vec<f64> = (0..60).map(|i| (f64::from(i) * 0.5).sin() * 2.0).collect();
        let batch = WinRate::new(14).unwrap().batch(&rets);
        let mut b = WinRate::new(14).unwrap();
        let streamed: Vec<_> = rets.iter().map(|p| b.update(*p)).collect();
        assert_eq!(batch, streamed);
    }

    #[test]
    fn rejects_period_above_maximum() {
        assert!(matches!(
            WinRate::new(crate::error::MAX_PERIOD + 1),
            Err(Error::InvalidPeriod { .. })
        ));
    }
}
