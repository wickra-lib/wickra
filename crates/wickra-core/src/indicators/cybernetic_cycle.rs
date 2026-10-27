//! Ehlers Cybernetic Cycle Component.
#![allow(clippy::doc_markdown)]

use crate::error::{Error, Result};
use crate::traits::Indicator;

/// Ehlers' Cybernetic Cycle Component (CCC).
///
/// Classic EasyLanguage construct from *Cybernetic Analysis for Stocks and
/// Futures* (Ehlers 2004, ch. 4):
///
/// ```text
/// smooth[t] = (x[t] + 2*x[t-1] + 2*x[t-2] + x[t-3]) / 6
/// cycle[t]  = (1 - alpha/2)^2 * (smooth[t] - 2*smooth[t-1] + smooth[t-2])
///           + 2 * (1 - alpha) * cycle[t-1]
///           - (1 - alpha)^2 * cycle[t-2]
/// ```
///
/// The result is a near-zero-mean oscillator that tracks the dominant cycle
/// component while filtering trend. `alpha` is a smoothing fraction in
/// `(0, 1]`; Ehlers recommends `2 / (period + 1)` for a given critical period.
///
/// The first six outputs follow Ehlers' "use the input directly" initial
/// condition so downstream consumers stay reactive.
///
/// # Example
///
/// ```
/// use wickra_core::{Indicator, CyberneticCycle};
///
/// let mut cc = CyberneticCycle::new(10).unwrap();
/// let mut last = None;
/// for i in 0..30 {
///     last = cc.update(100.0 + (f64::from(i) * 0.3).sin() * 5.0);
/// }
/// assert!(last.is_some());
/// ```
#[derive(Debug, Clone)]
pub struct CyberneticCycle {
    period: usize,
    alpha: f64,
    in_buf: [Option<f64>; 4],
    smooth_buf: [Option<f64>; 3],
    cycle_buf: [Option<f64>; 3],
    count: usize,
    last_value: Option<f64>,
}

impl CyberneticCycle {
    /// Construct with the dominant-cycle period (alpha = 2 / (period + 1)).
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
            in_buf: [None; 4],
            smooth_buf: [None; 3],
            cycle_buf: [None; 3],
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

    /// Shift in `x` at position 0 of a 3-slot buffer.
    fn push3(buf: &mut [Option<f64>; 3], x: f64) {
        buf[2] = buf[1];
        buf[1] = buf[0];
        buf[0] = Some(x);
    }
    fn push4(buf: &mut [Option<f64>; 4], x: f64) {
        buf[3] = buf[2];
        buf[2] = buf[1];
        buf[1] = buf[0];
        buf[0] = Some(x);
    }
}

impl Indicator for CyberneticCycle {
    type Input = f64;
    type Output = f64;

    fn update(&mut self, input: f64) -> Option<f64> {
        if !input.is_finite() {
            return None;
        }
        self.count += 1;
        Self::push4(&mut self.in_buf, input);

        // Smooth needs four prior inputs (positions 0..=3).
        let smooth = if let (Some(a), Some(b), Some(c), Some(d)) = (
            self.in_buf[0],
            self.in_buf[1],
            self.in_buf[2],
            self.in_buf[3],
        ) {
            (a + 2.0 * b + 2.0 * c + d) / 6.0
        } else {
            // Initial condition: use the raw input.
            input
        };
        Self::push3(&mut self.smooth_buf, smooth);

        // Cycle needs two prior smooths and two prior cycles.
        let one_minus_half_alpha = 1.0 - self.alpha / 2.0;
        let one_minus_alpha = 1.0 - self.alpha;
        let drv = one_minus_half_alpha * one_minus_half_alpha;

        // The 3-slot `smooth_buf` and `cycle_buf` ring buffers fill within a
        // few updates, so the pattern match only fails during warmup. The
        // `else` branch is therefore the Ehlers initial condition: the
        // second-difference of the raw input series, scaled by 0.5 — matches
        // the EasyLanguage implementation's first-bar fallback.
        let cycle = if let (Some(s0), Some(s1), Some(s2), Some(c1), Some(c2)) = (
            self.smooth_buf[0],
            self.smooth_buf[1],
            self.smooth_buf[2],
            self.cycle_buf[0],
            self.cycle_buf[1],
        ) {
            drv * (s0 - 2.0 * s1 + s2) + 2.0 * one_minus_alpha * c1
                - one_minus_alpha * one_minus_alpha * c2
        } else {
            let (x0, x1, x2) = (
                self.in_buf[0].unwrap_or(input),
                self.in_buf[1].unwrap_or(input),
                self.in_buf[2].unwrap_or(input),
            );
            (x0 - 2.0 * x1 + x2) / 4.0
        };

        Self::push3(&mut self.cycle_buf, cycle);
        self.last_value = Some(cycle);
        Some(cycle)
    }

    fn reset(&mut self) {
        self.in_buf = [None; 4];
        self.smooth_buf = [None; 3];
        self.cycle_buf = [None; 3];
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
        "CyberneticCycle"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;
    use approx::assert_relative_eq;

    #[test]
    fn new_rejects_zero_period() {
        assert!(matches!(CyberneticCycle::new(0), Err(Error::PeriodZero)));
    }

    #[test]
    fn accessors_and_metadata() {
        let mut cc = CyberneticCycle::new(10).unwrap();
        assert_eq!(cc.period(), 10);
        assert_relative_eq!(cc.alpha(), 2.0 / 11.0, epsilon = 1e-15);
        assert_eq!(cc.warmup_period(), 1);
        assert_eq!(cc.name(), "CyberneticCycle");
        assert!(!cc.is_ready());
        cc.update(100.0);
        assert!(cc.is_ready());
    }

    #[test]
    fn constant_series_converges_to_zero() {
        let mut cc = CyberneticCycle::new(10).unwrap();
        let out = cc.batch(&[50.0_f64; 200]);
        for x in out.iter().skip(50).flatten() {
            assert_relative_eq!(*x, 0.0, epsilon = 1e-9);
        }
    }

    #[test]
    fn batch_equals_streaming() {
        let prices: Vec<f64> = (0..120)
            .map(|i| 100.0 + (f64::from(i) * 0.25).sin() * 5.0)
            .collect();
        let mut a = CyberneticCycle::new(15).unwrap();
        let mut b = CyberneticCycle::new(15).unwrap();
        let batch = a.batch(&prices);
        let streamed: Vec<_> = prices.iter().map(|p| b.update(*p)).collect();
        assert_eq!(batch, streamed);
    }

    #[test]
    fn ignores_non_finite_input() {
        let mut cc = CyberneticCycle::new(10).unwrap();
        cc.batch(&(1..=30).map(f64::from).collect::<Vec<_>>());
        let before = cc.value();
        assert!(before.is_some());
        assert_eq!(cc.update(f64::NAN), None);
    }

    #[test]
    fn reset_clears_state() {
        let mut cc = CyberneticCycle::new(10).unwrap();
        cc.batch(&(1..=30).map(f64::from).collect::<Vec<_>>());
        assert!(cc.is_ready());
        cc.reset();
        assert!(!cc.is_ready());
    }

    #[test]
    fn rejects_period_above_maximum() {
        assert!(matches!(
            CyberneticCycle::new(crate::error::MAX_PERIOD + 1),
            Err(Error::InvalidPeriod { .. })
        ));
    }
}
