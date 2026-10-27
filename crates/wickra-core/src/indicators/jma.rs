//! Jurik Moving Average (JMA).

use crate::error::{Error, Result};
use crate::traits::Indicator;

/// Mark Jurik's adaptive moving average. The original algorithm is proprietary
/// and Jurik Research has never published the full source. This implementation
/// follows the widely-used three-stage filter reconstruction circulated since
/// the 1999 TASC article on the indicator — the same form used by most
/// open-source ports (`TradingView` Pine, `pandas-ta`, various MQL ports):
///
/// ```text
/// beta        = 0.45 * (period - 1) / (0.45 * (period - 1) + 2)
/// alpha       = beta ^ power
/// phase_ratio = clamp(phase / 100 + 1.5, 0.5, 2.5)
///
/// e0_t = (1 - alpha) * x_t + alpha * e0_{t-1}
/// e1_t = (x_t - e0_t) * (1 - beta) + beta * e1_{t-1}
/// e2_t = (e0_t + phase_ratio * e1_t - JMA_{t-1}) * (1 - alpha)^2 + alpha^2 * e2_{t-1}
/// JMA_t = JMA_{t-1} + e2_t
/// ```
///
/// The state is seeded by setting `e0 = JMA = first input`, so a constant
/// input stream is reproduced exactly from the first output onward.
///
/// # Parameters
///
/// - `period`: smoothing length (default 14).
/// - `phase`: phase shift in `[-100, 100]`. Values outside this range are
///   clamped to the boundary `phase_ratio` so the constructor never fails on
///   a finite `phase`.
/// - `power`: kernel exponent in `1..=4` (default 2 matches the popular
///   reconstruction).
///
/// # Example
///
/// ```
/// use wickra_core::{Indicator, Jma};
///
/// let mut jma = Jma::new(14, 0.0, 2).unwrap();
/// let mut last = None;
/// for i in 0..40 {
///     last = jma.update(100.0 + f64::from(i));
/// }
/// assert!(last.is_some());
/// ```
#[derive(Debug, Clone)]
pub struct Jma {
    period: usize,
    phase: f64,
    power: u32,
    beta: f64,
    alpha: f64,
    phase_ratio: f64,
    e0: f64,
    e1: f64,
    e2: f64,
    output: Option<f64>,
}

impl Jma {
    /// # Errors
    /// - [`Error::PeriodZero`] if `period == 0`.
    /// - [`Error::InvalidPeriod`] if `phase` is non-finite or `power` is
    ///   outside `1..=4`.
    pub fn new(period: usize, phase: f64, power: u32) -> Result<Self> {
        if period == 0 {
            return Err(Error::PeriodZero);
        }
        if period > crate::error::MAX_PERIOD {
            return Err(Error::InvalidPeriod {
                message: crate::error::PERIOD_ABOVE_MAX,
            });
        }
        if !phase.is_finite() {
            return Err(Error::InvalidPeriod {
                message: "JMA phase must be a finite value",
            });
        }
        if !(1..=4).contains(&power) {
            return Err(Error::InvalidPeriod {
                message: "JMA power must be in 1..=4",
            });
        }
        let len = period as f64 - 1.0;
        let beta = 0.45 * len / (0.45 * len + 2.0);
        let alpha = beta.powi(i32::try_from(power).expect("power is in 1..=4"));
        let phase_ratio = (phase / 100.0 + 1.5).clamp(0.5, 2.5);
        Ok(Self {
            period,
            phase,
            power,
            beta,
            alpha,
            phase_ratio,
            e0: 0.0,
            e1: 0.0,
            e2: 0.0,
            output: None,
        })
    }

    /// Construct JMA with the popular defaults `(period = 14, phase = 0, power = 2)`.
    pub fn classic() -> Self {
        Self::new(14, 0.0, 2).expect("classic JMA parameters are valid")
    }

    /// Configured `(period, phase, power)`.
    pub const fn params(&self) -> (usize, f64, u32) {
        (self.period, self.phase, self.power)
    }
}

impl Indicator for Jma {
    type Input = f64;
    type Output = f64;

    #[inline]
    fn update(&mut self, input: f64) -> Option<f64> {
        if !input.is_finite() {
            return None;
        }
        let Some(prev_jma) = self.output else {
            // Seed e0 and JMA to the first input so a flat series is
            // reproduced exactly.
            self.e0 = input;
            self.output = Some(input);
            return self.output;
        };
        self.e0 = (1.0 - self.alpha) * input + self.alpha * self.e0;
        self.e1 = (input - self.e0) * (1.0 - self.beta) + self.beta * self.e1;
        let one_minus_alpha = 1.0 - self.alpha;
        self.e2 =
            (self.e0 + self.phase_ratio * self.e1 - prev_jma) * one_minus_alpha * one_minus_alpha
                + self.alpha * self.alpha * self.e2;
        let next = prev_jma + self.e2;
        self.output = Some(next);
        Some(next)
    }

    fn reset(&mut self) {
        self.e0 = 0.0;
        self.e1 = 0.0;
        self.e2 = 0.0;
        self.output = None;
    }

    #[inline]
    fn warmup_period(&self) -> usize {
        1
    }

    #[inline]
    fn is_ready(&self) -> bool {
        self.output.is_some()
    }

    #[inline]
    fn name(&self) -> &'static str {
        "JMA"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;
    use approx::assert_relative_eq;

    #[test]
    fn rejects_zero_period() {
        assert!(matches!(Jma::new(0, 0.0, 2), Err(Error::PeriodZero)));
    }

    #[test]
    fn rejects_non_finite_phase() {
        assert!(matches!(
            Jma::new(14, f64::NAN, 2),
            Err(Error::InvalidPeriod { .. })
        ));
        assert!(matches!(
            Jma::new(14, f64::INFINITY, 2),
            Err(Error::InvalidPeriod { .. })
        ));
    }

    #[test]
    fn rejects_invalid_power() {
        assert!(matches!(
            Jma::new(14, 0.0, 0),
            Err(Error::InvalidPeriod { .. })
        ));
        assert!(matches!(
            Jma::new(14, 0.0, 5),
            Err(Error::InvalidPeriod { .. })
        ));
    }

    #[test]
    fn accessors_and_metadata() {
        let jma = Jma::new(14, 0.0, 2).unwrap();
        assert_eq!(jma.params(), (14, 0.0, 2));
        assert_eq!(jma.warmup_period(), 1);
        assert_eq!(jma.name(), "JMA");
    }

    #[test]
    fn classic_factory() {
        let jma = Jma::classic();
        assert_eq!(jma.params(), (14, 0.0, 2));
    }

    #[test]
    fn constant_series_yields_the_constant() {
        // Seeding e0 = JMA = first input means the recurrence stays exactly
        // on the constant from the very first sample.
        let mut jma = Jma::new(14, 0.0, 2).unwrap();
        let out = jma.batch(&[42.0_f64; 60]);
        for x in out.iter().flatten() {
            assert_relative_eq!(*x, 42.0, epsilon = 1e-12);
        }
    }

    #[test]
    fn extreme_phase_is_clamped() {
        // phase outside [-100, 100] must produce a finite JMA series (phase
        // ratio clamps to [0.5, 2.5]) rather than blow up the recurrence.
        let mut a = Jma::new(14, 250.0, 2).unwrap();
        let mut b = Jma::new(14, -250.0, 2).unwrap();
        let prices: Vec<f64> = (1..=40).map(f64::from).collect();
        for &p in &prices {
            let va = a.update(p).unwrap();
            let vb = b.update(p).unwrap();
            assert!(va.is_finite(), "JMA(phase=+250) emitted {va}");
            assert!(vb.is_finite(), "JMA(phase=-250) emitted {vb}");
        }
    }

    #[test]
    fn pure_uptrend_tracks_close() {
        // Monotonic uptrend, period 5, power 2 — after enough samples the
        // smoothed JMA sits close to the latest input.
        let mut jma = Jma::new(5, 0.0, 2).unwrap();
        let prices: Vec<f64> = (1..=80).map(f64::from).collect();
        let out = jma.batch(&prices);
        let last = out.last().unwrap().unwrap();
        let latest = *prices.last().unwrap();
        assert!(
            (latest - last).abs() < 5.0,
            "JMA on a long clean uptrend should track close: {last} vs {latest}"
        );
    }

    #[test]
    fn batch_equals_streaming() {
        let prices: Vec<f64> = (1..=80)
            .map(|i| 100.0 + (f64::from(i) * 0.2).sin() * 5.0)
            .collect();
        let mut a = Jma::new(14, 0.0, 2).unwrap();
        let mut b = Jma::new(14, 0.0, 2).unwrap();
        assert_eq!(
            a.batch(&prices),
            prices.iter().map(|p| b.update(*p)).collect::<Vec<_>>()
        );
    }

    #[test]
    fn reset_clears_state() {
        let mut jma = Jma::new(14, 0.0, 2).unwrap();
        jma.batch(&(1..=30).map(f64::from).collect::<Vec<_>>());
        assert!(jma.is_ready());
        jma.reset();
        assert!(!jma.is_ready());
        assert_eq!(jma.e0, 0.0);
    }

    #[test]
    fn ignores_non_finite_input() {
        let mut jma = Jma::new(14, 0.0, 2).unwrap();
        jma.batch(&(1..=15).map(f64::from).collect::<Vec<_>>());
        jma.update(16.0).unwrap();
        assert_eq!(jma.update(f64::NAN), None);
        assert_eq!(jma.update(f64::INFINITY), None);
    }

    #[test]
    fn period_one_is_pass_through() {
        // beta = 0, alpha = 0 -> e2 collapses to (input - prev) and the
        // recurrence reduces to JMA_t = input.
        let mut jma = Jma::new(1, 0.0, 2).unwrap();
        assert_eq!(jma.update(5.0), Some(5.0));
        assert_relative_eq!(jma.update(10.0).unwrap(), 10.0, epsilon = 1e-12);
        assert_relative_eq!(jma.update(7.0).unwrap(), 7.0, epsilon = 1e-12);
    }

    #[test]
    fn rejects_period_above_maximum() {
        assert!(matches!(
            Jma::new(crate::error::MAX_PERIOD + 1, 0.0, 2),
            Err(Error::InvalidPeriod { .. })
        ));
    }
}
