//! Hurst exponent of the spread of two series, for pairs-trading regime detection.

use std::collections::VecDeque;

use crate::error::{Error, Result};
use crate::traits::Indicator;

/// Hurst exponent of the spread `a − b` over a rolling window.
///
/// Each `update` takes one `(a, b)` price pair and forms the spread
/// `sₜ = aₜ − bₜ`. Over the trailing window of `period` spreads the indicator
/// estimates the Hurst exponent `H` from how the variance of `τ`-lagged
/// differences grows with the lag `τ`:
///
/// ```text
/// V(τ) = mean_t (s_{t+τ} − s_t)²   ∝   τ^(2H)
/// H    = slope of log V(τ) on log τ, divided by two
/// ```
///
/// `H` classifies the spread's regime:
///
/// * `H < 0.5` — **mean-reverting** (anti-persistent): the spread snaps back,
///   the regime pairs traders want.
/// * `H ≈ 0.5` — a **random walk**: no exploitable structure.
/// * `H > 0.5` — **trending** (persistent): the spread keeps diverging.
///
/// The fit uses lags `1..=period/4` (at least two). When the spread is flat —
/// every lagged difference is zero, so the log-regression has fewer than two
/// usable points — the indicator returns the neutral `0.5`. The output is
/// clamped to `[0, 1]`.
///
/// Each `update` is `O(period · period/4)`, bounded by the fixed window.
///
/// # Example
///
/// ```
/// use wickra_core::{Indicator, SpreadHurst};
///
/// let mut h = SpreadHurst::new(60).unwrap();
/// let mut last = None;
/// for t in 0..200 {
///     let b = 100.0 + f64::from(t);
///     // A tight oscillating spread is anti-persistent ⇒ H < 0.5.
///     let a = b + 3.0 * (f64::from(t) * 0.8).sin();
///     last = h.update((a, b));
/// }
/// assert!(last.unwrap() < 0.5);
/// ```
#[derive(Debug, Clone)]
pub struct SpreadHurst {
    period: usize,
    max_lag: usize,
    window: VecDeque<f64>,
    /// Reusable scratch buffer to avoid allocating per `update`.
    scratch: Vec<f64>,
    /// Reusable `(log lag, log variance)` buffers for the regression.
    log_lag: Vec<f64>,
    log_var: Vec<f64>,
}

impl SpreadHurst {
    /// Construct a new spread Hurst estimator.
    ///
    /// # Errors
    /// Returns [`Error::InvalidPeriod`] if `period < 8` — fewer than eight
    /// spreads cannot support a two-lag log–log regression.
    pub fn new(period: usize) -> Result<Self> {
        if period < 8 {
            return Err(Error::InvalidPeriod {
                message: "spread Hurst needs period >= 8",
            });
        }
        if period > crate::error::MAX_PERIOD {
            return Err(Error::InvalidPeriod {
                message: crate::error::PERIOD_ABOVE_MAX,
            });
        }
        Ok(Self {
            period,
            max_lag: (period / 4).max(2),
            window: VecDeque::with_capacity(period),
            scratch: Vec::with_capacity(period),
            log_lag: Vec::with_capacity((period / 4).max(2)),
            log_var: Vec::with_capacity((period / 4).max(2)),
        })
    }

    /// Configured look-back window of spreads.
    pub const fn period(&self) -> usize {
        self.period
    }
}

impl Indicator for SpreadHurst {
    type Input = (f64, f64);
    type Output = f64;

    fn update(&mut self, input: (f64, f64)) -> Option<f64> {
        let (a, b) = input;
        if !a.is_finite() || !b.is_finite() {
            return None;
        }
        if self.window.len() == self.period {
            self.window.pop_front();
        }
        self.window.push_back(a - b);
        if self.window.len() < self.period {
            return None;
        }
        self.scratch.clear();
        self.scratch.extend(self.window.iter().copied());
        let spreads = &self.scratch;
        // Collect (log τ, log V(τ)) for every lag whose variance is positive.
        let log_lag = &mut self.log_lag;
        let log_var = &mut self.log_var;
        log_lag.clear();
        log_var.clear();
        for lag in 1..=self.max_lag {
            let mut sum_sq = 0.0;
            let mut count = 0.0;
            for pair in spreads.windows(lag + 1) {
                let diff = pair[lag] - pair[0];
                sum_sq += diff * diff;
                count += 1.0;
            }
            let var = sum_sq / count;
            if var > 0.0 {
                log_lag.push((lag as f64).ln());
                log_var.push(var.ln());
            }
        }
        if log_lag.len() < 2 {
            // Degenerate (flat) spread: report the random-walk midpoint.
            return Some(0.5);
        }
        let n = log_lag.len() as f64;
        let mean_lag = log_lag.iter().sum::<f64>() / n;
        let mean_var = log_var.iter().sum::<f64>() / n;
        let mut cov = 0.0;
        let mut var_lag = 0.0;
        for (lx, lv) in log_lag.iter().zip(log_var.iter()) {
            cov += (lx - mean_lag) * (lv - mean_var);
            var_lag += (lx - mean_lag) * (lx - mean_lag);
        }
        // `log_lag` holds at least two *distinct* lag logarithms, so the lag
        // variance is strictly positive — no degenerate-slope guard is needed.
        let slope = cov / var_lag;
        Some((slope / 2.0).clamp(0.0, 1.0))
    }

    fn reset(&mut self) {
        self.window.clear();
        self.scratch.clear();
        self.log_lag.clear();
        self.log_var.clear();
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
        "SpreadHurst"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;
    use approx::assert_relative_eq;

    #[test]
    fn rejects_period_below_eight() {
        assert!(SpreadHurst::new(7).is_err());
        assert!(SpreadHurst::new(8).is_ok());
    }

    #[test]
    fn accessors_and_metadata() {
        let h = SpreadHurst::new(40).unwrap();
        assert_eq!(h.period(), 40);
        assert_eq!(h.warmup_period(), 40);
        assert_eq!(h.name(), "SpreadHurst");
        assert!(!h.is_ready());
    }

    #[test]
    fn warmup_returns_none() {
        let mut h = SpreadHurst::new(8).unwrap();
        for t in 0..7 {
            assert_eq!(h.update((f64::from(t), 0.0)), None);
        }
        assert!(h.update((7.0, 0.0)).is_some());
        assert!(h.is_ready());
    }

    #[test]
    fn oscillating_spread_is_anti_persistent() {
        let pairs: Vec<(f64, f64)> = (0..200)
            .map(|t| {
                let b = 100.0 + f64::from(t);
                (b + 3.0 * (f64::from(t) * 0.8).sin(), b)
            })
            .collect();
        let last = SpreadHurst::new(60)
            .unwrap()
            .batch(&pairs)
            .into_iter()
            .flatten()
            .last()
            .unwrap();
        assert!(last < 0.5, "H {last}");
    }

    #[test]
    fn linear_trend_spread_is_persistent() {
        // Spread = a − b = t ⇒ τ-lagged differences are all τ ⇒ V(τ) = τ² ⇒ H = 1.
        let pairs: Vec<(f64, f64)> = (0..40)
            .map(|t| (2.0 * f64::from(t), f64::from(t)))
            .collect();
        let last = SpreadHurst::new(20)
            .unwrap()
            .batch(&pairs)
            .into_iter()
            .flatten()
            .last()
            .unwrap();
        assert_relative_eq!(last, 1.0, epsilon = 1e-9);
    }

    #[test]
    fn flat_spread_returns_midpoint() {
        // a − b is constant ⇒ all lagged differences zero ⇒ neutral 0.5.
        let pairs: Vec<(f64, f64)> = (0..30)
            .map(|t| (5.0 + f64::from(t), f64::from(t)))
            .collect();
        let last = SpreadHurst::new(16)
            .unwrap()
            .batch(&pairs)
            .into_iter()
            .flatten()
            .last()
            .unwrap();
        assert_relative_eq!(last, 0.5, epsilon = 1e-12);
    }

    #[test]
    fn output_in_unit_range() {
        let pairs: Vec<(f64, f64)> = (0..150)
            .map(|t| {
                let b = 50.0 + 0.3 * f64::from(t);
                (
                    b + (f64::from(t) * 0.5).sin() * 2.0 + (f64::from(t) * 0.13).cos(),
                    b,
                )
            })
            .collect();
        let mut h = SpreadHurst::new(48).unwrap();
        for v in h.batch(&pairs).into_iter().flatten() {
            assert!((0.0..=1.0).contains(&v));
        }
    }

    #[test]
    fn reset_clears_state() {
        let mut h = SpreadHurst::new(8).unwrap();
        for t in 0..12 {
            h.update((f64::from(t) + (f64::from(t) * 0.7).sin(), f64::from(t)));
        }
        assert!(h.is_ready());
        h.reset();
        assert!(!h.is_ready());
        assert_eq!(h.update((1.0, 0.0)), None);
    }

    #[test]
    fn batch_equals_streaming() {
        let pairs: Vec<(f64, f64)> = (0..100)
            .map(|t| {
                let b = 30.0 + 0.7 * f64::from(t);
                (b + (f64::from(t) * 0.4).sin() * 1.5, b)
            })
            .collect();
        let batch = SpreadHurst::new(32).unwrap().batch(&pairs);
        let mut h = SpreadHurst::new(32).unwrap();
        let streamed: Vec<_> = pairs.iter().map(|p| h.update(*p)).collect();
        assert_eq!(batch, streamed);
    }

    #[test]
    fn non_finite_input_returns_none() {
        let mut h = SpreadHurst::new(8).unwrap();
        assert_eq!(h.update((f64::NAN, 1.0)), None);
        assert_eq!(h.update((1.0, f64::INFINITY)), None);
        // The rejected ticks leave no trace: a fresh window still warms up.
        for t in 0..7 {
            assert_eq!(h.update((f64::from(t), 0.0)), None);
        }
        assert!(h.update((7.0, 0.0)).is_some());
    }

    #[test]
    fn rejects_period_above_maximum() {
        assert!(matches!(
            SpreadHurst::new(crate::error::MAX_PERIOD + 1),
            Err(Error::InvalidPeriod { .. })
        ));
    }
}
