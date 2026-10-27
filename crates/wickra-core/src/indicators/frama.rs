//! Fractal Adaptive Moving Average (FRAMA).

use std::collections::VecDeque;

use crate::error::{Error, Result};
use crate::traits::Indicator;

/// Ehlers' Fractal Adaptive Moving Average.
///
/// FRAMA picks its smoothing constant from the fractal dimension `D` of the
/// recent window: in a trending (low-`D`) market it follows price tightly, in
/// a choppy (high-`D`) market it smooths heavily. The window of `period`
/// closes is split into two equal halves; the fractal dimension comes from
/// the price ranges of the halves vs. the whole window:
///
/// ```text
/// N1 = (max(first half)  - min(first half))  / (period / 2)
/// N2 = (max(second half) - min(second half)) / (period / 2)
/// N3 = (max(window)      - min(window))      / period
/// D  = (log(N1 + N2) - log(N3)) / log(2)
/// alpha = exp(-4.6 * (D - 1))   clamped to [0.01, 1.0]
/// ```
///
/// The output is an EMA-like recurrence
/// `FRAMA_t = alpha * close_t + (1 - alpha) * FRAMA_{t - 1}`, seeded with the
/// first close. `period` must be even and at least 2.
///
/// Reference: John F. Ehlers, *Fractal Adaptive Moving Average*, 2005.
///
/// # Example
///
/// ```
/// use wickra_core::{Frama, Indicator};
///
/// let mut frama = Frama::new(16).unwrap();
/// let mut last = None;
/// for i in 0..40 {
///     last = frama.update(100.0 + f64::from(i));
/// }
/// assert!(last.is_some());
/// ```
#[derive(Debug, Clone)]
pub struct Frama {
    period: usize,
    half: usize,
    window: VecDeque<f64>,
    current: Option<f64>,
}

impl Frama {
    /// # Errors
    /// - [`Error::PeriodZero`] if `period == 0`.
    /// - [`Error::InvalidPeriod`] if `period` is odd or below 2.
    pub fn new(period: usize) -> Result<Self> {
        if period == 0 {
            return Err(Error::PeriodZero);
        }
        if period > crate::error::MAX_PERIOD {
            return Err(Error::InvalidPeriod {
                message: crate::error::PERIOD_ABOVE_MAX,
            });
        }
        if period < 2 {
            return Err(Error::InvalidPeriod {
                message: "FRAMA period must be at least 2",
            });
        }
        if period % 2 != 0 {
            return Err(Error::InvalidPeriod {
                message: "FRAMA period must be even",
            });
        }
        Ok(Self {
            period,
            half: period / 2,
            window: VecDeque::with_capacity(period),
            current: None,
        })
    }

    /// Configured period.
    pub const fn period(&self) -> usize {
        self.period
    }
}

impl Indicator for Frama {
    type Input = f64;
    type Output = f64;

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

        let half = self.half;
        let mut h_first = f64::NEG_INFINITY;
        let mut l_first = f64::INFINITY;
        let mut h_second = f64::NEG_INFINITY;
        let mut l_second = f64::INFINITY;
        let mut h_whole = f64::NEG_INFINITY;
        let mut l_whole = f64::INFINITY;
        for (i, &p) in self.window.iter().enumerate() {
            if p > h_whole {
                h_whole = p;
            }
            if p < l_whole {
                l_whole = p;
            }
            if i < half {
                if p > h_first {
                    h_first = p;
                }
                if p < l_first {
                    l_first = p;
                }
            } else {
                if p > h_second {
                    h_second = p;
                }
                if p < l_second {
                    l_second = p;
                }
            }
        }

        let half_f = half as f64;
        let period_f = self.period as f64;
        let n1 = (h_first - l_first) / half_f;
        let n2 = (h_second - l_second) / half_f;
        let n3 = (h_whole - l_whole) / period_f;

        let alpha = if n1 > 0.0 && n2 > 0.0 && n3 > 0.0 {
            let d = ((n1 + n2).ln() - n3.ln()) / 2.0_f64.ln();
            (-4.6 * (d - 1.0)).exp().clamp(0.01, 1.0)
        } else {
            // Degenerate (perfectly flat half or whole window): use the slowest
            // smoothing so the indicator coasts on its previous value.
            0.01
        };

        let prev = self.current.unwrap_or(input);
        let next = alpha * input + (1.0 - alpha) * prev;
        self.current = Some(next);
        Some(next)
    }

    fn reset(&mut self) {
        self.window.clear();
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
        "FRAMA"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;
    use approx::assert_relative_eq;

    #[test]
    fn rejects_zero_period() {
        assert!(matches!(Frama::new(0), Err(Error::PeriodZero)));
    }

    #[test]
    fn rejects_invalid_period() {
        assert!(matches!(Frama::new(1), Err(Error::InvalidPeriod { .. })));
        assert!(matches!(Frama::new(3), Err(Error::InvalidPeriod { .. })));
        assert!(matches!(Frama::new(15), Err(Error::InvalidPeriod { .. })));
    }

    #[test]
    fn accessors_and_metadata() {
        let frama = Frama::new(16).unwrap();
        assert_eq!(frama.period(), 16);
        assert_eq!(frama.warmup_period(), 16);
        assert_eq!(frama.name(), "FRAMA");
    }

    #[test]
    fn constant_series_yields_the_constant() {
        // Flat input -> alpha clamps to 0.01 (degenerate ranges) and the
        // EMA recurrence holds the seed value forever.
        let mut frama = Frama::new(4).unwrap();
        let out = frama.batch(&[42.0_f64; 30]);
        for v in out.iter().skip(3).flatten() {
            assert_relative_eq!(*v, 42.0, epsilon = 1e-12);
        }
    }

    #[test]
    fn warmup_emits_first_value_at_period() {
        let mut frama = Frama::new(4).unwrap();
        assert_eq!(frama.update(1.0), None);
        assert_eq!(frama.update(2.0), None);
        assert_eq!(frama.update(3.0), None);
        assert!(frama.update(4.0).is_some());
    }

    #[test]
    fn pure_uptrend_alpha_close_to_one() {
        // A strict monotonic uptrend has fractal dimension ~1, so alpha is
        // pushed to 1.0 and FRAMA reduces to the latest price.
        let mut frama = Frama::new(4).unwrap();
        let prices: Vec<f64> = (1..=8).map(f64::from).collect();
        let out = frama.batch(&prices);
        let last = out.last().unwrap().unwrap();
        assert!(
            (last - 8.0).abs() < 0.05,
            "FRAMA on a clean uptrend should hug the latest close: {last}"
        );
    }

    #[test]
    fn batch_equals_streaming() {
        let prices: Vec<f64> = (1..=80)
            .map(|i| 100.0 + (f64::from(i) * 0.2).sin() * 5.0)
            .collect();
        let mut a = Frama::new(8).unwrap();
        let mut b = Frama::new(8).unwrap();
        assert_eq!(
            a.batch(&prices),
            prices.iter().map(|p| b.update(*p)).collect::<Vec<_>>()
        );
    }

    #[test]
    fn reset_clears_state() {
        let mut frama = Frama::new(4).unwrap();
        frama.batch(&(1..=20).map(f64::from).collect::<Vec<_>>());
        assert!(frama.is_ready());
        frama.reset();
        assert!(!frama.is_ready());
        assert_eq!(frama.update(1.0), None);
    }

    #[test]
    fn ignores_non_finite_input() {
        let mut frama = Frama::new(4).unwrap();
        frama.batch(&[1.0, 2.0, 3.0, 4.0]);
        frama.update(5.0).unwrap();
        assert_eq!(frama.update(f64::NAN), None);
        assert_eq!(frama.update(f64::INFINITY), None);
    }

    #[test]
    fn rejects_period_above_maximum() {
        assert!(matches!(
            Frama::new(crate::error::MAX_PERIOD + 1),
            Err(Error::InvalidPeriod { .. })
        ));
    }
}
