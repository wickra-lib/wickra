//! Running Drawdown — since-inception maximum drawdown over an unbounded stream.

use crate::traits::Indicator;

/// Since-inception maximum drawdown, with the current drawdown and running peak.
///
/// The input is treated as an equity-curve sample (or any non-negative value
/// series). Unlike [`MaxDrawdown`](crate::MaxDrawdown), which looks at a
/// trailing `period`-bar window, this indicator never forgets: it folds every
/// value since construction (or the last `reset`) into three numbers and keeps
/// them in O(1) memory:
///
/// ```text
/// peak_t        = max(input over [0..=t])
/// current_t     = (peak_t − input_t) / peak_t     (0 while peak_t ≤ 0)
/// max_t         = max(current over [0..=t])
/// ```
///
/// `update` returns the maximum drawdown so far as a non-negative fraction
/// (`0.20` = 20 % drop from peak), the same convention as
/// [`MaxDrawdown`](crate::MaxDrawdown). A monotonically rising equity curve
/// has a maximum drawdown of `0`. The result equals `MaxDrawdown` with a
/// `period` at least as long as the stream.
///
/// There are no parameters and no warmup: a value is emitted from the first
/// finite input on, and each `update` is O(1).
///
/// # Example
///
/// ```
/// use wickra_core::{Indicator, RunningDrawdown};
///
/// let mut dd = RunningDrawdown::new();
/// assert_eq!(dd.update(100.0), Some(0.0));
/// // Equity peaks at 110 then drops to 88 — a 20% drawdown.
/// assert_eq!(dd.update(110.0), Some(0.0));
/// assert!((dd.update(88.0).unwrap() - 0.20).abs() < 1e-9);
/// // Recovering shrinks the current drawdown but not the maximum.
/// dd.update(99.0);
/// assert!((dd.max_drawdown().unwrap() - 0.20).abs() < 1e-9);
/// assert!((dd.current_drawdown().unwrap() - 0.10).abs() < 1e-9);
/// assert_eq!(dd.peak(), Some(110.0));
/// ```
#[derive(Debug, Clone)]
pub struct RunningDrawdown {
    peak: f64,
    current: f64,
    max: f64,
    seen: bool,
}

impl RunningDrawdown {
    /// Construct a new Running Drawdown.
    pub const fn new() -> Self {
        Self {
            peak: f64::NEG_INFINITY,
            current: 0.0,
            max: 0.0,
            seen: false,
        }
    }

    /// Maximum drawdown since the first value, as a non-negative fraction.
    pub const fn max_drawdown(&self) -> Option<f64> {
        if self.seen {
            Some(self.max)
        } else {
            None
        }
    }

    /// Drawdown of the latest value from the running peak, as a non-negative
    /// fraction (`0` at a new peak).
    pub const fn current_drawdown(&self) -> Option<f64> {
        if self.seen {
            Some(self.current)
        } else {
            None
        }
    }

    /// Highest value seen since the first value.
    pub const fn peak(&self) -> Option<f64> {
        if self.seen {
            Some(self.peak)
        } else {
            None
        }
    }
}

impl Default for RunningDrawdown {
    fn default() -> Self {
        Self::new()
    }
}

impl Indicator for RunningDrawdown {
    type Input = f64;
    type Output = f64;

    #[inline]
    fn update(&mut self, input: f64) -> Option<f64> {
        if !input.is_finite() {
            return None;
        }
        if input > self.peak {
            self.peak = input;
        }
        // Same guard as `MaxDrawdown`: a non-positive peak has no fractional
        // drawdown, so it stays at 0.
        self.current = if self.peak > 0.0 {
            (self.peak - input) / self.peak
        } else {
            0.0
        };
        if self.current > self.max {
            self.max = self.current;
        }
        self.seen = true;
        Some(self.max)
    }

    fn reset(&mut self) {
        *self = Self::new();
    }

    #[inline]
    fn warmup_period(&self) -> usize {
        1
    }

    #[inline]
    fn is_ready(&self) -> bool {
        self.seen
    }

    #[inline]
    fn name(&self) -> &'static str {
        "RunningDrawdown"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::indicators::MaxDrawdown;
    use crate::traits::BatchExt;
    use approx::assert_relative_eq;

    #[test]
    fn accessors_and_metadata() {
        let mut dd = RunningDrawdown::new();
        assert_eq!(dd.name(), "RunningDrawdown");
        assert_eq!(dd.warmup_period(), 1);
        assert_eq!(dd.max_drawdown(), None);
        assert_eq!(dd.current_drawdown(), None);
        assert_eq!(dd.peak(), None);
        dd.update(100.0);
        assert_eq!(dd.max_drawdown(), Some(0.0));
        assert_eq!(dd.current_drawdown(), Some(0.0));
        assert_eq!(dd.peak(), Some(100.0));
    }

    #[test]
    fn default_matches_new() {
        let mut a = RunningDrawdown::default();
        let mut b = RunningDrawdown::new();
        assert_eq!(a.warmup_period(), b.warmup_period());
        assert_eq!(a.is_ready(), b.is_ready());
        assert_eq!(a.peak(), b.peak());
        assert_eq!(a.update(100.0), b.update(100.0));
        assert_eq!(a.update(80.0), b.update(80.0));
    }

    #[test]
    fn first_value_emits_immediately() {
        let mut dd = RunningDrawdown::new();
        assert_eq!(dd.update(100.0), Some(0.0));
    }

    #[test]
    fn reference_drawdown() {
        // Peak 120, trough 90 -> 25 %; recovery to 110 leaves the max at 25 %
        // and the current drawdown at (120 - 110) / 120.
        let mut dd = RunningDrawdown::new();
        let out = dd.batch(&[100.0, 120.0, 90.0, 110.0]);
        assert_eq!(out[0], Some(0.0));
        assert_eq!(out[1], Some(0.0));
        assert_relative_eq!(out[2].unwrap(), 0.25, epsilon = 1e-12);
        assert_relative_eq!(out[3].unwrap(), 0.25, epsilon = 1e-12);
        assert_relative_eq!(
            dd.current_drawdown().unwrap(),
            10.0 / 120.0,
            epsilon = 1e-12
        );
        assert_eq!(dd.peak(), Some(120.0));
    }

    #[test]
    fn remembers_a_drawdown_older_than_any_window() {
        let mut dd = RunningDrawdown::new();
        dd.batch(&[100.0, 50.0]);
        for v in 0..1000 {
            dd.update(100.0 + f64::from(v));
        }
        assert_relative_eq!(dd.max_drawdown().unwrap(), 0.5, epsilon = 1e-12);
        assert_relative_eq!(dd.current_drawdown().unwrap(), 0.0, epsilon = 1e-12);
    }

    #[test]
    fn pure_uptrend_yields_zero() {
        let mut dd = RunningDrawdown::new();
        let out = dd.batch(&(1..=20).map(f64::from).collect::<Vec<_>>());
        for v in out.into_iter().flatten() {
            assert_relative_eq!(v, 0.0, epsilon = 1e-12);
        }
    }

    #[test]
    fn non_positive_peak_yields_zero() {
        // Peak never exceeds 0, so the fractional drawdown is undefined and
        // stays 0 -- the same as `MaxDrawdown`.
        let mut dd = RunningDrawdown::new();
        let out = dd.batch(&[-1.0, -5.0, -3.0, 0.0, -2.0]);
        for v in out.into_iter().flatten() {
            assert_eq!(v, 0.0);
        }
        assert_eq!(dd.current_drawdown(), Some(0.0));
    }

    #[test]
    fn ignores_non_finite_input() {
        let mut dd = RunningDrawdown::new();
        dd.batch(&[100.0, 90.0, 80.0]);
        let (max, cur, peak) = (dd.max_drawdown(), dd.current_drawdown(), dd.peak());
        assert_eq!(dd.update(f64::NAN), None);
        assert_eq!(dd.update(f64::INFINITY), None);
        assert_eq!(dd.update(f64::NEG_INFINITY), None);
        // The rejected input must not have disturbed the state.
        assert_eq!(dd.max_drawdown(), max);
        assert_eq!(dd.current_drawdown(), cur);
        assert_eq!(dd.peak(), peak);
    }

    #[test]
    fn non_finite_first_input_leaves_it_unseeded() {
        let mut dd = RunningDrawdown::new();
        assert_eq!(dd.update(f64::NAN), None);
        assert!(!dd.is_ready());
        assert_eq!(dd.peak(), None);
    }

    #[test]
    fn reset_clears_state() {
        let mut dd = RunningDrawdown::new();
        dd.batch(&[100.0, 90.0, 80.0]);
        assert!(dd.is_ready());
        dd.reset();
        assert!(!dd.is_ready());
        assert_eq!(dd.max_drawdown(), None);
        assert_eq!(dd.peak(), None);
        assert_eq!(dd.update(100.0), Some(0.0));
    }

    #[test]
    fn batch_equals_streaming() {
        let prices: Vec<f64> = (0..60)
            .map(|i| 100.0 + (f64::from(i) * 0.3).sin() * 10.0)
            .collect();
        let batch = RunningDrawdown::new().batch(&prices);
        let mut s = RunningDrawdown::new();
        let streamed: Vec<_> = prices.iter().map(|p| s.update(*p)).collect();
        assert_eq!(batch, streamed);
    }

    #[test]
    fn matches_max_drawdown_with_a_period_covering_the_stream() {
        // Includes a non-positive stretch and a recovery, so the guard and the
        // peak tracking are both exercised against the windowed indicator.
        let prices: Vec<f64> = (0..200)
            .map(|i| 50.0 + (f64::from(i) * 0.17).sin() * 60.0 + f64::from(i) * 0.1)
            .collect();
        let period = prices.len();
        let mut windowed = MaxDrawdown::new(period).unwrap();
        let mut running = RunningDrawdown::new();
        for (i, &p) in prices.iter().enumerate() {
            let w = windowed.update(p);
            let r = running.update(p);
            // `MaxDrawdown` is silent until its window fills; compare once it
            // has a value, and on the last bar always.
            if let Some(w) = w {
                assert_relative_eq!(r.unwrap(), w, epsilon = 1e-12);
            } else {
                assert!(i + 1 < period);
            }
        }
        assert!(windowed.value().is_some());
    }

    #[test]
    fn matches_max_drawdown_at_every_bar_via_growing_periods() {
        // For each prefix length n, MaxDrawdown(n) over exactly that prefix is
        // the since-inception value at bar n.
        let prices: Vec<f64> = (0..40)
            .map(|i| 100.0 + (f64::from(i) * 0.5).sin() * 25.0)
            .collect();
        let mut running = RunningDrawdown::new();
        for (i, &p) in prices.iter().enumerate() {
            let r = running.update(p).unwrap();
            let w = MaxDrawdown::new(i + 1)
                .unwrap()
                .batch(&prices[..=i])
                .pop()
                .flatten()
                .unwrap();
            assert_relative_eq!(r, w, epsilon = 1e-12);
        }
    }
}
