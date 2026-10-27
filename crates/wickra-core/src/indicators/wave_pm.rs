//! Wave PM — Cynthia Kase's peak-momentum statistic (Wickra reconstruction).

use std::collections::VecDeque;

use crate::error::{Error, Result};
use crate::indicators::ema::Ema;
use crate::traits::Indicator;

/// Wave PM (Peak Momentum): a `0..100` statistic that rises when the current
/// `length`-bar momentum is large relative to its own recent energy — Cynthia
/// Kase's gauge of how "peaked" the move is.
///
/// ```text
/// m       = close_t - close_{t-length}                  (length-bar momentum)
/// energy  = EMA(m^2, length)                            (mean squared momentum)
/// raw     = 1 - exp( -m^2 / (2 * energy) )      (0 if energy == 0)
/// WavePM  = 100 * EMA(raw, smoothing)
/// ```
///
/// The momentum `m` is normalised by its recent variance (`energy`): a move that
/// merely matches its typical energy sits at the baseline
/// `100·(1 − e^{−1/2}) ≈ 39.35`, while a momentum *spike* that exceeds recent
/// energy drives the reading toward `100`. A flat market (`m = 0`) reads `0`.
/// High readings mark a peaking, possibly exhausted move rather than a fresh one.
///
/// Kase's published `WavePM` is platform-specific; this is Wickra's faithful
/// reconstruction of its variance-normalised peak-momentum form. The exact
/// constants differ from any single vendor implementation, but the shape — flat
/// at zero, a fixed baseline on a steady trend, and saturation on an
/// acceleration — matches the indicator's intent.
///
/// Reference: Cynthia Kase, *Trading with the Odds*, 1996 (Wickra reconstruction).
///
/// # Example
///
/// ```
/// use wickra_core::{Indicator, WavePm};
///
/// let mut indicator = WavePm::new(10, 3).unwrap();
/// let mut last = None;
/// for i in 0..60 {
///     last = indicator.update(100.0 + f64::from(i));
/// }
/// assert!(last.is_some());
/// ```
#[derive(Debug, Clone)]
pub struct WavePm {
    length: usize,
    smoothing: usize,
    closes: VecDeque<f64>,
    energy_ema: Ema,
    smooth_ema: Ema,
}

impl WavePm {
    /// Construct a Wave PM with the momentum `length` and the output `smoothing`
    /// period.
    ///
    /// # Errors
    ///
    /// Returns [`Error::PeriodZero`] if `length == 0` or `smoothing == 0`.
    pub fn new(length: usize, smoothing: usize) -> Result<Self> {
        if length == 0 {
            return Err(Error::PeriodZero);
        }
        if length > crate::error::MAX_PERIOD {
            return Err(Error::InvalidPeriod {
                message: crate::error::PERIOD_ABOVE_MAX,
            });
        }
        Ok(Self {
            length,
            smoothing,
            closes: VecDeque::with_capacity(length + 1),
            energy_ema: Ema::new(length)?,
            smooth_ema: Ema::new(smoothing)?,
        })
    }

    /// Configured `(length, smoothing)`.
    pub const fn periods(&self) -> (usize, usize) {
        (self.length, self.smoothing)
    }
}

impl Indicator for WavePm {
    type Input = f64;
    type Output = f64;

    #[inline]
    fn update(&mut self, close: f64) -> Option<f64> {
        if !close.is_finite() {
            // There was no guard here at all, so a single NaN entered the
            // window and poisoned every value that followed it.
            return None;
        }
        self.closes.push_back(close);
        if self.closes.len() > self.length + 1 {
            self.closes.pop_front();
        }
        if self.closes.len() <= self.length {
            return None;
        }

        let oldest = *self.closes.front().unwrap_or(&close);
        let momentum = close - oldest;
        let energy = self.energy_ema.update(momentum * momentum)?;
        let raw = if energy <= 0.0 {
            0.0
        } else {
            1.0 - (-(momentum * momentum) / (2.0 * energy)).exp()
        };
        self.smooth_ema.update(raw).map(|v| v * 100.0)
    }

    fn reset(&mut self) {
        self.closes.clear();
        self.energy_ema.reset();
        self.smooth_ema.reset();
    }

    #[inline]
    fn warmup_period(&self) -> usize {
        2 * self.length + self.smoothing - 1
    }

    #[inline]
    fn is_ready(&self) -> bool {
        self.smooth_ema.is_ready()
    }

    #[inline]
    fn name(&self) -> &'static str {
        "WavePm"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;
    use approx::assert_relative_eq;

    #[test]
    fn rejects_zero_period() {
        assert!(matches!(WavePm::new(0, 3), Err(Error::PeriodZero)));
        assert!(matches!(WavePm::new(10, 0), Err(Error::PeriodZero)));
    }

    #[test]
    fn accessors_and_metadata() {
        let w = WavePm::new(10, 3).unwrap();
        assert_eq!(w.periods(), (10, 3));
        // 2*10 + 3 - 1 = 22.
        assert_eq!(w.warmup_period(), 22);
        assert_eq!(w.name(), "WavePm");
        assert!(!w.is_ready());
    }

    #[test]
    fn warmup_emits_at_expected_bar() {
        let mut w = WavePm::new(3, 2).unwrap();
        // warmup = 2*3 + 2 - 1 = 7 -> first value at input 7 (index 6).
        let inputs: Vec<f64> = (0..12).map(f64::from).collect();
        let out = w.batch(&inputs);
        assert!(out[5].is_none());
        assert!(out[6].is_some());
    }

    #[test]
    fn flat_market_reads_zero() {
        let mut w = WavePm::new(4, 2).unwrap();
        let inputs = [50.0; 20];
        let last = w.batch(&inputs).last().unwrap().unwrap();
        assert_relative_eq!(last, 0.0, epsilon = 1e-12);
    }

    #[test]
    fn steady_trend_reads_baseline() {
        // Constant-slope ramp: momentum equals its own energy every bar, so the
        // reading pins to the baseline 100*(1 - e^-0.5).
        let mut w = WavePm::new(10, 3).unwrap();
        let inputs: Vec<f64> = (0..60).map(|i| f64::from(i) * 5.0).collect();
        let last = w.batch(&inputs).last().unwrap().unwrap();
        let baseline = 100.0 * (1.0 - (-0.5_f64).exp());
        assert_relative_eq!(last, baseline, epsilon = 1e-9);
    }

    #[test]
    fn acceleration_reads_above_baseline() {
        // A quadratic path: momentum keeps outrunning its lagged energy, so the
        // reading sits above the steady-trend baseline.
        let mut w = WavePm::new(10, 3).unwrap();
        let inputs: Vec<f64> = (0..60).map(|i| f64::from(i * i) * 0.1).collect();
        let last = w.batch(&inputs).last().unwrap().unwrap();
        let baseline = 100.0 * (1.0 - (-0.5_f64).exp());
        assert!(
            last > baseline,
            "accelerating wpm {last} should exceed {baseline}"
        );
        assert!(last <= 100.0, "wpm {last} must stay <= 100");
    }

    #[test]
    fn reset_clears_state() {
        let mut w = WavePm::new(10, 3).unwrap();
        let inputs: Vec<f64> = (0..60).map(|i| f64::from(i) * 5.0).collect();
        w.batch(&inputs);
        assert!(w.is_ready());
        w.reset();
        assert!(!w.is_ready());
    }

    #[test]
    fn batch_equals_streaming() {
        let inputs: Vec<f64> = (0..80)
            .map(|i| 100.0 + (f64::from(i) * 0.2).sin() * 5.0)
            .collect();
        let mut a = WavePm::new(10, 3).unwrap();
        let mut b = WavePm::new(10, 3).unwrap();
        assert_eq!(
            a.batch(&inputs),
            inputs.iter().map(|x| b.update(*x)).collect::<Vec<_>>()
        );
    }

    #[test]
    fn rejects_length_above_maximum() {
        assert!(matches!(
            WavePm::new(crate::error::MAX_PERIOD + 1, 3),
            Err(Error::InvalidPeriod { .. })
        ));
    }
}
