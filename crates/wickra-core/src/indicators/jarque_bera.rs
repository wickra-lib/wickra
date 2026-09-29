//! Jarque-Bera — a normality-test statistic on a rolling window.

use std::collections::VecDeque;

use crate::error::{Error, Result};
use crate::traits::Indicator;

/// Jarque-Bera — the Jarque-Bera test statistic measuring how far a window's
/// distribution departs from normal, via its **skewness** and **excess
/// kurtosis**.
///
/// ```text
/// S  = skewness         = m3 / m2^(3/2)
/// K  = excess kurtosis  = m4 / m2²  − 3
/// JB = (period / 6) · ( S² + K²/4 )
/// ```
///
/// where `m2`, `m3`, `m4` are the second, third and fourth central moments of the
/// window. A perfectly normal sample has zero skew and zero excess kurtosis, so
/// `JB = 0`; the statistic grows as the distribution becomes asymmetric (non-zero
/// skew) or fat- or thin-tailed (non-zero excess kurtosis). Under the null of
/// normality `JB` is asymptotically χ² with two degrees of freedom, so values
/// above roughly `6` reject normality at the 95% level — a useful streaming flag
/// for fat-tail / crash-risk regimes in a return series.
///
/// The statistic is `≥ 0`. A degenerate window with zero variance (`m2 == 0`)
/// returns `0`. The first value lands after `period` inputs; each `update`
/// recomputes the four moments over the window in O(`period`).
///
/// # Example
///
/// ```
/// use wickra_core::{Indicator, JarqueBera};
///
/// let mut indicator = JarqueBera::new(50).unwrap();
/// let mut last = None;
/// for i in 0..80 {
///     last = indicator.update((f64::from(i) * 0.3).sin());
/// }
/// assert!(last.is_some());
/// ```
#[derive(Debug, Clone)]
pub struct JarqueBera {
    period: usize,
    window: VecDeque<f64>,
    last: Option<f64>,
}

impl JarqueBera {
    /// Construct a rolling Jarque-Bera over `period` values.
    ///
    /// # Errors
    ///
    /// Returns [`Error::PeriodZero`] if `period == 0` and
    /// [`Error::InvalidPeriod`] if `period < 4` (the statistic is degenerate on
    /// fewer than four points).
    pub fn new(period: usize) -> Result<Self> {
        if period == 0 {
            return Err(Error::PeriodZero);
        }
        if period > crate::error::MAX_PERIOD {
            return Err(Error::InvalidPeriod {
                message: crate::error::PERIOD_ABOVE_MAX,
            });
        }
        if period < 4 {
            return Err(Error::InvalidPeriod {
                message: "Jarque-Bera needs period >= 4",
            });
        }
        Ok(Self {
            period,
            window: VecDeque::with_capacity(period),
            last: None,
        })
    }

    /// Configured window length.
    pub const fn period(&self) -> usize {
        self.period
    }

    /// Current value if available.
    pub const fn value(&self) -> Option<f64> {
        self.last
    }

    fn compute(&self) -> f64 {
        let n = self.period as f64;
        let mean = self.window.iter().sum::<f64>() / n;
        let mut m2 = 0.0;
        let mut m3 = 0.0;
        let mut m4 = 0.0;
        for &v in &self.window {
            let d = v - mean;
            let d2 = d * d;
            m2 += d2;
            m3 += d2 * d;
            m4 += d2 * d2;
        }
        m2 /= n;
        m3 /= n;
        m4 /= n;
        if m2 == 0.0 {
            return 0.0;
        }
        // `m2^1.5` as `m2 * sqrt(m2)`, correctly rounded on every platform
        // where `powf` is the platform libm's.
        let skew = m3 / (m2 * m2.sqrt());
        let excess_kurt = m4 / (m2 * m2) - 3.0;
        (n / 6.0) * (skew * skew + excess_kurt * excess_kurt / 4.0)
    }
}

impl Indicator for JarqueBera {
    type Input = f64;
    type Output = f64;

    #[inline]
    fn update(&mut self, input: f64) -> Option<f64> {
        if !input.is_finite() {
            return None;
        }
        if self.window.len() == self.period {
            self.window.pop_front();
        }
        self.window.push_back(input);
        if self.window.len() < self.period {
            return None;
        }
        let out = self.compute();
        self.last = Some(out);
        Some(out)
    }

    fn reset(&mut self) {
        self.window.clear();
        self.last = None;
    }

    #[inline]
    fn warmup_period(&self) -> usize {
        self.period
    }

    #[inline]
    fn is_ready(&self) -> bool {
        self.last.is_some()
    }

    #[inline]
    fn name(&self) -> &'static str {
        "JarqueBera"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;
    use approx::assert_relative_eq;

    #[test]
    fn rejects_invalid_period() {
        assert!(matches!(JarqueBera::new(0), Err(Error::PeriodZero)));
        assert!(matches!(
            JarqueBera::new(3),
            Err(Error::InvalidPeriod { .. })
        ));
        assert!(JarqueBera::new(4).is_ok());
    }

    #[test]
    fn accessors_and_metadata() {
        let jb = JarqueBera::new(50).unwrap();
        assert_eq!(jb.period(), 50);
        assert_eq!(jb.warmup_period(), 50);
        assert_eq!(jb.name(), "JarqueBera");
        assert!(!jb.is_ready());
        assert_eq!(jb.value(), None);
    }

    #[test]
    fn first_emission_at_warmup_period() {
        let mut jb = JarqueBera::new(4).unwrap();
        let out = jb.batch(&[1.0, 2.0, 3.0, 4.0, 5.0]);
        for v in out.iter().take(3) {
            assert!(v.is_none());
        }
        assert!(out[3].is_some());
    }

    #[test]
    fn constant_window_is_zero() {
        let mut jb = JarqueBera::new(8).unwrap();
        let last = jb.batch(&[5.0; 12]).into_iter().flatten().last().unwrap();
        assert_relative_eq!(last, 0.0, epsilon = 1e-12);
    }

    #[test]
    fn output_is_non_negative() {
        let mut jb = JarqueBera::new(30).unwrap();
        for v in jb
            .batch(
                &(0..200)
                    .map(|i| (f64::from(i) * 0.3).sin() * 5.0)
                    .collect::<Vec<_>>(),
            )
            .into_iter()
            .flatten()
        {
            assert!(v >= 0.0, "JB must be non-negative, got {v}");
        }
    }

    #[test]
    fn skewed_window_exceeds_symmetric() {
        // A symmetric window vs. one with a heavy outlier (high skew + kurtosis).
        let symmetric: Vec<f64> = vec![-3.0, -1.0, 0.0, 1.0, 3.0, -2.0, 2.0, 0.0];
        let skewed: Vec<f64> = vec![0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 20.0];
        let jb_sym = JarqueBera::new(8)
            .unwrap()
            .batch(&symmetric)
            .into_iter()
            .flatten()
            .last()
            .unwrap();
        let jb_skew = JarqueBera::new(8)
            .unwrap()
            .batch(&skewed)
            .into_iter()
            .flatten()
            .last()
            .unwrap();
        assert!(
            jb_skew > jb_sym,
            "skewed ({jb_skew}) should exceed symmetric ({jb_sym})"
        );
    }

    #[test]
    fn ignores_non_finite() {
        let mut jb = JarqueBera::new(4).unwrap();
        let _ready = jb
            .batch(&[1.0, 2.0, 3.0, 5.0])
            .into_iter()
            .flatten()
            .last()
            .unwrap();
        assert_eq!(jb.update(f64::NAN), None);
    }

    #[test]
    fn reset_clears_state() {
        let mut jb = JarqueBera::new(4).unwrap();
        jb.batch(&[1.0, 2.0, 3.0, 5.0]);
        assert!(jb.is_ready());
        jb.reset();
        assert!(!jb.is_ready());
        assert_eq!(jb.value(), None);
        assert_eq!(jb.update(1.0), None);
    }

    #[test]
    fn batch_equals_streaming() {
        let xs: Vec<f64> = (0..120)
            .map(|i| (f64::from(i) * 0.25).sin() * 9.0)
            .collect();
        let batch = JarqueBera::new(30).unwrap().batch(&xs);
        let mut b = JarqueBera::new(30).unwrap();
        let streamed: Vec<_> = xs.iter().map(|x| b.update(*x)).collect();
        assert_eq!(batch, streamed);
    }
}
