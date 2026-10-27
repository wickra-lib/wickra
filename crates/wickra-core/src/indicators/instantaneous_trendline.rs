//! Ehlers Instantaneous Trendline (ITrend).
#![allow(clippy::doc_markdown)]

use crate::error::{Error, Result};
use crate::traits::Indicator;

/// Ehlers' Instantaneous Trendline (ITrend).
///
/// A 2-pole IIR that approximates a lag-free trend line:
///
/// ```text
/// itrend[t] = (alpha - alpha^2/4) * x[t]
///           + 0.5 * alpha^2 * x[t-1]
///           - (alpha - 0.75*alpha^2) * x[t-2]
///           + 2*(1 - alpha) * itrend[t-1]
///           - (1 - alpha)^2 * itrend[t-2]
/// ```
///
/// where `alpha = 2 / (period + 1)`. From *Cybernetic Analysis for Stocks
/// and Futures* (Ehlers 2004, ch. 8). During the first six bars the output
/// uses the EasyLanguage initial condition `(x[t] + 2*x[t-1] + x[t-2]) / 4`.
///
/// # Example
///
/// ```
/// use wickra_core::{Indicator, InstantaneousTrendline};
///
/// let mut it = InstantaneousTrendline::new(20).unwrap();
/// let mut last = None;
/// for i in 0..40 {
///     last = it.update(100.0 + f64::from(i) * 0.5);
/// }
/// assert!(last.is_some());
/// ```
#[derive(Debug, Clone)]
pub struct InstantaneousTrendline {
    period: usize,
    alpha: f64,
    in_buf: [Option<f64>; 3],
    out_buf: [Option<f64>; 2],
    count: usize,
    last_value: Option<f64>,
}

impl InstantaneousTrendline {
    /// Construct with the dominant-cycle period.
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
        let alpha = 2.0 / (period as f64 + 1.0);
        Ok(Self {
            period,
            alpha,
            in_buf: [None; 3],
            out_buf: [None; 2],
            count: 0,
            last_value: None,
        })
    }

    /// Configured period.
    pub const fn period(&self) -> usize {
        self.period
    }

    /// Smoothing alpha.
    pub const fn alpha(&self) -> f64 {
        self.alpha
    }

    /// Current value if available.
    pub const fn value(&self) -> Option<f64> {
        self.last_value
    }
}

impl Indicator for InstantaneousTrendline {
    type Input = f64;
    type Output = f64;

    fn update(&mut self, input: f64) -> Option<f64> {
        if !input.is_finite() {
            return None;
        }
        self.count += 1;

        // Shift input buffer (position 0 = most recent).
        self.in_buf[2] = self.in_buf[1];
        self.in_buf[1] = self.in_buf[0];
        self.in_buf[0] = Some(input);

        let alpha = self.alpha;
        let v = if self.count >= 7 {
            // Full recursive formula.
            let (x0, x1, x2) = (
                self.in_buf[0].expect("filled"),
                self.in_buf[1].expect("filled"),
                self.in_buf[2].expect("filled"),
            );
            let (y1, y2) = (
                self.out_buf[0].expect("filled"),
                self.out_buf[1].expect("filled"),
            );
            (alpha - alpha * alpha / 4.0) * x0 + 0.5 * alpha * alpha * x1
                - (alpha - 0.75 * alpha * alpha) * x2
                + 2.0 * (1.0 - alpha) * y1
                - (1.0 - alpha) * (1.0 - alpha) * y2
        } else {
            // Initial condition: 4-point weighted average of the most recent
            // inputs (Ehlers EasyLanguage default).
            let x0 = self.in_buf[0].expect("just pushed");
            let x1 = self.in_buf[1].unwrap_or(x0);
            let x2 = self.in_buf[2].unwrap_or(x0);
            (x0 + 2.0 * x1 + x2) / 4.0
        };

        self.out_buf[1] = self.out_buf[0];
        self.out_buf[0] = Some(v);
        self.last_value = Some(v);
        Some(v)
    }

    fn reset(&mut self) {
        self.in_buf = [None; 3];
        self.out_buf = [None; 2];
        self.count = 0;
        self.last_value = None;
    }

    #[inline]
    fn warmup_period(&self) -> usize {
        1
    }

    #[inline]
    fn is_ready(&self) -> bool {
        self.last_value.is_some()
    }

    #[inline]
    fn name(&self) -> &'static str {
        "InstantaneousTrendline"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;
    use approx::assert_relative_eq;

    #[test]
    fn new_rejects_zero_period() {
        assert!(matches!(
            InstantaneousTrendline::new(0),
            Err(Error::PeriodZero)
        ));
    }

    #[test]
    fn accessors_and_metadata() {
        let mut it = InstantaneousTrendline::new(20).unwrap();
        assert_eq!(it.period(), 20);
        assert_relative_eq!(it.alpha(), 2.0 / 21.0, epsilon = 1e-15);
        assert_eq!(it.warmup_period(), 1);
        assert_eq!(it.name(), "InstantaneousTrendline");
        assert!(!it.is_ready());
        it.update(100.0);
        assert!(it.is_ready());
    }

    #[test]
    fn constant_series_passes_through() {
        // Coefficients sum to 1, so a flat input stays flat after warmup.
        let mut it = InstantaneousTrendline::new(20).unwrap();
        let out = it.batch(&[42.0_f64; 200]);
        for x in out.iter().skip(20).flatten() {
            assert_relative_eq!(*x, 42.0, epsilon = 1e-6);
        }
    }

    #[test]
    fn batch_equals_streaming() {
        let prices: Vec<f64> = (0..120)
            .map(|i| 100.0 + (f64::from(i) * 0.2).cos() * 5.0)
            .collect();
        let mut a = InstantaneousTrendline::new(15).unwrap();
        let mut b = InstantaneousTrendline::new(15).unwrap();
        let batch = a.batch(&prices);
        let streamed: Vec<_> = prices.iter().map(|p| b.update(*p)).collect();
        assert_eq!(batch, streamed);
    }

    #[test]
    fn ignores_non_finite_input() {
        let mut it = InstantaneousTrendline::new(20).unwrap();
        it.batch(&(1..=40).map(f64::from).collect::<Vec<_>>());
        let before = it.value();
        assert!(before.is_some());
        assert_eq!(it.update(f64::NAN), None);
    }

    #[test]
    fn reset_clears_state() {
        let mut it = InstantaneousTrendline::new(20).unwrap();
        it.batch(&(1..=40).map(f64::from).collect::<Vec<_>>());
        assert!(it.is_ready());
        it.reset();
        assert!(!it.is_ready());
    }

    #[test]
    fn rejects_period_above_maximum() {
        assert!(matches!(
            InstantaneousTrendline::new(crate::error::MAX_PERIOD + 1),
            Err(Error::InvalidPeriod { .. })
        ));
    }
}
