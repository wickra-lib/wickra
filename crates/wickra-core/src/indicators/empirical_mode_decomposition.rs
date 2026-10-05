//! Ehlers Empirical Mode Decomposition (bandpass trend component).

use std::collections::VecDeque;
use std::f64::consts::PI;

use crate::error::{Error, Result};
use crate::traits::Indicator;

/// Ehlers' half-bandwidth `Delta` of the bandpass (his published default).
const DELTA: f64 = 0.1;

/// Length of the peak / valley averages.
const PEAK_AVG_LEN: usize = 50;

/// Ehlers' adaptation of Empirical Mode Decomposition (EMD).
///
/// From John Ehlers & Ric Way, *"Empirical Mode Decomposition"*, Technical
/// Analysis of Stocks & Commodities, March 2010:
///
/// ```text
/// β  = cos(2π / Period),  γ = 1 / cos(4π · Delta / Period),  α = γ − √(γ² − 1)
/// BP = 0.5·(1 − α)·(Price − Price[2]) + β·(1 + α)·BP[1] − α·BP[2]
/// Mean   = SMA(BP, 2 · Period)                          (the trend component)
/// Peak   = BP[1] at a local maximum of BP, otherwise the previous Peak
/// Valley = BP[1] at a local minimum of BP, otherwise the previous Valley
/// Upper  = Fraction · SMA(Peak, 50)
/// Lower  = Fraction · SMA(Valley, 50)
/// ```
///
/// `Delta` is Ehlers' `0.1`. The output is `Mean`; the two thresholds are
/// available from [`upper`](Self::upper) and [`lower`](Self::lower) after each
/// update. The market is in a trend mode while `Mean` sits above `Upper`
/// (bullish) or below `Lower` (bearish), and in a cycle mode between them.
///
/// # Example
///
/// ```
/// use wickra_core::{Indicator, EmpiricalModeDecomposition};
///
/// let mut emd = EmpiricalModeDecomposition::new(20, 0.1).unwrap();
/// let mut last = None;
/// for i in 0..200 {
///     last = emd.update(100.0 + (f64::from(i) * 0.3).sin() * 5.0);
/// }
/// assert!(last.is_some());
/// ```
#[derive(Debug, Clone)]
pub struct EmpiricalModeDecomposition {
    period: usize,
    fraction: f64,
    beta: f64,
    alpha: f64,
    prev_in_1: Option<f64>,
    prev_in_2: Option<f64>,
    prev_bp_1: f64,
    prev_bp_2: f64,
    peak: f64,
    valley: f64,
    bp_window: VecDeque<f64>,
    bp_sum: f64,
    peak_window: VecDeque<f64>,
    peak_sum: f64,
    valley_window: VecDeque<f64>,
    valley_sum: f64,
    upper: f64,
    lower: f64,
    last_value: Option<f64>,
}

impl EmpiricalModeDecomposition {
    /// Construct with the bandpass centre period and the threshold fraction.
    ///
    /// `fraction` scales the averaged peaks and valleys into the trend-mode
    /// thresholds; Ehlers uses `0.1`.
    ///
    /// # Errors
    ///
    /// Returns [`Error::PeriodZero`] if `period == 0`, and
    /// [`Error::InvalidPeriod`] if `fraction` is not in `(0, 1]`.
    pub fn new(period: usize, fraction: f64) -> Result<Self> {
        if period == 0 {
            return Err(Error::PeriodZero);
        }
        if period > crate::error::MAX_PERIOD {
            return Err(Error::InvalidPeriod {
                message: crate::error::PERIOD_ABOVE_MAX,
            });
        }
        if !fraction.is_finite() || fraction <= 0.0 || fraction > 1.0 {
            return Err(Error::InvalidPeriod {
                message: "fraction must be in (0, 1]",
            });
        }
        let beta = (2.0 * PI / period as f64).cos();
        let gamma = 1.0 / (4.0 * PI * DELTA / period as f64).cos();
        let alpha = gamma - (gamma * gamma - 1.0).sqrt();
        Ok(Self {
            period,
            fraction,
            beta,
            alpha,
            prev_in_1: None,
            prev_in_2: None,
            prev_bp_1: 0.0,
            prev_bp_2: 0.0,
            peak: 0.0,
            valley: 0.0,
            bp_window: VecDeque::with_capacity(2 * period),
            bp_sum: 0.0,
            peak_window: VecDeque::with_capacity(PEAK_AVG_LEN),
            peak_sum: 0.0,
            valley_window: VecDeque::with_capacity(PEAK_AVG_LEN),
            valley_sum: 0.0,
            upper: 0.0,
            lower: 0.0,
            last_value: None,
        })
    }

    /// Configured period.
    pub const fn period(&self) -> usize {
        self.period
    }

    /// Configured fraction.
    pub const fn fraction(&self) -> f64 {
        self.fraction
    }

    /// Current value (the trend component `Mean`) if available.
    pub const fn value(&self) -> Option<f64> {
        self.last_value
    }

    /// Upper trend threshold `Fraction · SMA(Peak, 50)` after the last update.
    pub const fn upper(&self) -> f64 {
        self.upper
    }

    /// Lower trend threshold `Fraction · SMA(Valley, 50)` after the last update.
    pub const fn lower(&self) -> f64 {
        self.lower
    }

    /// Push `x` into a fixed-length window with a running sum.
    fn push(window: &mut VecDeque<f64>, sum: &mut f64, len: usize, x: f64) {
        if window.len() == len {
            *sum -= window.pop_front().expect("window is non-empty");
        }
        window.push_back(x);
        *sum += x;
    }
}

impl Indicator for EmpiricalModeDecomposition {
    type Input = f64;
    type Output = f64;

    fn update(&mut self, input: f64) -> Option<f64> {
        if !input.is_finite() {
            return None;
        }
        // 2nd-order resonant bandpass.
        let bp = if let Some(x2) = self.prev_in_2 {
            0.5 * (1.0 - self.alpha) * (input - x2)
                + self.beta * (1.0 + self.alpha) * self.prev_bp_1
                - self.alpha * self.prev_bp_2
        } else {
            0.0
        };
        // Peak / valley of the previous bandpass value.
        if self.prev_bp_1 > bp && self.prev_bp_1 > self.prev_bp_2 {
            self.peak = self.prev_bp_1;
        }
        if self.prev_bp_1 < bp && self.prev_bp_1 < self.prev_bp_2 {
            self.valley = self.prev_bp_1;
        }
        self.prev_bp_2 = self.prev_bp_1;
        self.prev_bp_1 = bp;
        self.prev_in_2 = self.prev_in_1;
        self.prev_in_1 = Some(input);

        Self::push(&mut self.bp_window, &mut self.bp_sum, 2 * self.period, bp);
        Self::push(
            &mut self.peak_window,
            &mut self.peak_sum,
            PEAK_AVG_LEN,
            self.peak,
        );
        Self::push(
            &mut self.valley_window,
            &mut self.valley_sum,
            PEAK_AVG_LEN,
            self.valley,
        );
        if self.bp_window.len() < 2 * self.period || self.peak_window.len() < PEAK_AVG_LEN {
            return None;
        }
        let n = PEAK_AVG_LEN as f64;
        self.upper = self.fraction * self.peak_sum / n;
        self.lower = self.fraction * self.valley_sum / n;
        let mean = self.bp_sum / (2 * self.period) as f64;
        self.last_value = Some(mean);
        Some(mean)
    }

    fn reset(&mut self) {
        self.prev_in_1 = None;
        self.prev_in_2 = None;
        self.prev_bp_1 = 0.0;
        self.prev_bp_2 = 0.0;
        self.peak = 0.0;
        self.valley = 0.0;
        self.bp_window.clear();
        self.bp_sum = 0.0;
        self.peak_window.clear();
        self.peak_sum = 0.0;
        self.valley_window.clear();
        self.valley_sum = 0.0;
        self.upper = 0.0;
        self.lower = 0.0;
        self.last_value = None;
    }

    #[inline]
    fn warmup_period(&self) -> usize {
        (2 * self.period).max(PEAK_AVG_LEN)
    }

    #[inline]
    fn is_ready(&self) -> bool {
        self.last_value.is_some()
    }

    #[inline]
    fn name(&self) -> &'static str {
        "EmpiricalModeDecomposition"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;

    #[test]
    fn new_rejects_invalid_params() {
        assert!(matches!(
            EmpiricalModeDecomposition::new(0, 0.5),
            Err(Error::PeriodZero)
        ));
        assert!(matches!(
            EmpiricalModeDecomposition::new(20, 0.0),
            Err(Error::InvalidPeriod { .. })
        ));
        assert!(matches!(
            EmpiricalModeDecomposition::new(20, 1.5),
            Err(Error::InvalidPeriod { .. })
        ));
        assert!(matches!(
            EmpiricalModeDecomposition::new(20, f64::NAN),
            Err(Error::InvalidPeriod { .. })
        ));
    }

    #[test]
    fn accessors_and_metadata() {
        let mut emd = EmpiricalModeDecomposition::new(20, 0.5).unwrap();
        assert_eq!(emd.period(), 20);
        assert!((emd.fraction() - 0.5).abs() < 1e-15);
        assert_eq!(emd.name(), "EmpiricalModeDecomposition");
        assert!(emd.warmup_period() >= 1);
        assert!(!emd.is_ready());
        let prices: Vec<f64> = (0..200)
            .map(|i| 100.0 + (f64::from(i) * 0.3).sin() * 5.0)
            .collect();
        emd.batch(&prices);
        assert!(emd.is_ready());
        assert!(emd.value().is_some());
    }

    #[test]
    fn batch_equals_streaming() {
        let prices: Vec<f64> = (0..200)
            .map(|i| 100.0 + (f64::from(i) * 0.2).cos() * 5.0)
            .collect();
        let mut a = EmpiricalModeDecomposition::new(20, 0.5).unwrap();
        let mut b = EmpiricalModeDecomposition::new(20, 0.5).unwrap();
        let batch = a.batch(&prices);
        let streamed: Vec<_> = prices.iter().map(|p| b.update(*p)).collect();
        assert_eq!(batch, streamed);
    }

    #[test]
    fn ignores_non_finite_input() {
        let mut emd = EmpiricalModeDecomposition::new(20, 0.5).unwrap();
        let prices: Vec<f64> = (0..200)
            .map(|i| 100.0 + (f64::from(i) * 0.3).sin() * 5.0)
            .collect();
        emd.batch(&prices);
        let before = emd.value();
        assert!(before.is_some());
        assert_eq!(emd.update(f64::NAN), None);
    }

    #[test]
    fn reset_clears_state() {
        let mut emd = EmpiricalModeDecomposition::new(20, 0.5).unwrap();
        let prices: Vec<f64> = (0..200)
            .map(|i| 100.0 + (f64::from(i) * 0.3).sin() * 5.0)
            .collect();
        emd.batch(&prices);
        assert!(emd.is_ready());
        emd.reset();
        assert!(!emd.is_ready());
    }

    #[test]
    fn rejects_period_above_maximum() {
        assert!(matches!(
            EmpiricalModeDecomposition::new(crate::error::MAX_PERIOD + 1, 0.5),
            Err(Error::InvalidPeriod { .. })
        ));
        assert!(matches!(
            EmpiricalModeDecomposition::new(20, -0.1),
            Err(Error::InvalidPeriod { .. })
        ));
        assert!(matches!(
            EmpiricalModeDecomposition::new(20, f64::INFINITY),
            Err(Error::InvalidPeriod { .. })
        ));
        // fraction == 1 is the inclusive upper bound.
        assert!(EmpiricalModeDecomposition::new(20, 1.0).is_ok());
    }

    #[test]
    fn warmup_is_max_of_two_period_and_fifty() {
        let prices: Vec<f64> = (0..150)
            .map(|i| 100.0 + (f64::from(i) * 0.3).sin() * 5.0)
            .collect();
        for (period, expected) in [(1usize, 50usize), (10, 50), (25, 50), (26, 52), (40, 80)] {
            let mut emd = EmpiricalModeDecomposition::new(period, 0.1).unwrap();
            assert_eq!(emd.warmup_period(), expected);
            let out = emd.batch(&prices);
            assert!(out[..expected - 1].iter().all(Option::is_none));
            assert!(out[expected - 1].is_some());
        }
    }

    #[test]
    fn constant_series_has_zero_mean_and_thresholds() {
        // Price - Price[2] == 0 every bar -> BP == 0 -> Mean, Peak, Valley all 0.
        let mut emd = EmpiricalModeDecomposition::new(10, 0.5).unwrap();
        let out = emd.batch(&[42.0; 80]);
        assert!(out
            .iter()
            .flatten()
            .all(|v| v.to_bits() == 0.0f64.to_bits()));
        assert_eq!(emd.upper().to_bits(), 0.0f64.to_bits());
        assert_eq!(emd.lower().to_bits(), 0.0f64.to_bits());
    }

    #[test]
    fn hand_computed_step_peak_valley_and_thresholds() {
        // period 5: beta = cos(2*pi/5) = 0.309_017; gamma = 1 / cos(4*pi*0.1/5) = 1.032_436;
        // alpha = gamma - sqrt(gamma^2 - 1) = 0.775_680.
        let mut emd = EmpiricalModeDecomposition::new(5, 0.5).unwrap();
        let (alpha, beta) = (emd.alpha, emd.beta);
        assert!((beta - 0.309_016_994_374_947_45).abs() < 1e-15);
        assert!((alpha - 0.775_679_511_049_613_4).abs() < 1e-12);
        // 60 flat bars at 0 -> BP == 0, warmup (50) complete, thresholds 0.
        for _ in 0..60 {
            emd.update(0.0);
        }
        assert_eq!(emd.upper().to_bits(), 0.0f64.to_bits());
        assert_eq!(emd.lower().to_bits(), 0.0f64.to_bits());
        // Step to 1 at bar 60. With c = 0.5 * (1 - alpha) and k = beta * (1 + alpha):
        // bp60 = c * (1 - 0)                         = 0.112_160
        // bp61 = c * (1 - 0) + k * bp60              = 0.173_704
        // bp62 = c * (1 - 1) + k * bp61 - alpha*bp60 = 0.008_314
        // bp63 = k * bp62 - alpha * bp61             = -0.130_177
        // bp64 = k * bp63 - alpha * bp62             = -0.077_879
        let c = 0.5 * (1.0 - alpha);
        let k = beta * (1.0 + alpha);
        let bp60 = c;
        let bp61 = c + k * bp60;
        let bp62 = k * bp61 - alpha * bp60;
        let bp63 = k * bp62 - alpha * bp61;
        let bp64 = k * bp63 - alpha * bp62;
        assert!((bp61 - 0.173_704_269_339_216_5).abs() < 1e-12);
        assert!((bp63 + 0.130_176_956_775_418_27).abs() < 1e-12);
        // Bar 60: BP rises from a flat 0 -> neither a peak nor a valley.
        let m60 = emd.update(1.0).unwrap();
        assert!((m60 - bp60 / 10.0).abs() < 1e-15);
        assert_eq!(emd.upper().to_bits(), 0.0f64.to_bits());
        assert_eq!(emd.lower().to_bits(), 0.0f64.to_bits());
        emd.update(1.0);
        // Bar 62: bp61 > bp62 and bp61 > bp60 -> Peak = bp61. One of the 50 peak
        // slots holds it -> Upper = 0.5 * bp61 / 50 = 0.001_737; no valley yet.
        let m62 = emd.update(1.0).unwrap();
        assert!((emd.upper() - 0.5 * bp61 / 50.0).abs() < 1e-15);
        assert!((emd.upper() - 0.001_737_042_693_392_165).abs() < 1e-12);
        assert_eq!(emd.lower().to_bits(), 0.0f64.to_bits());
        // Mean = SMA(BP, 10) = (bp60 + bp61 + bp62) / 10.
        assert!((m62 - (bp60 + bp61 + bp62) / 10.0).abs() < 1e-15);
        // Bar 63: BP still falling -> no new peak/valley; the peak is held.
        emd.update(1.0);
        assert!((emd.upper() - 0.5 * 2.0 * bp61 / 50.0).abs() < 1e-15);
        assert_eq!(emd.lower().to_bits(), 0.0f64.to_bits());
        // Bar 64: bp63 < bp64 and bp63 < bp62 -> Valley = bp63.
        // Upper = 0.5 * 3 * bp61 / 50 = 0.005_211; Lower = 0.5 * bp63 / 50 = -0.001_302.
        let m64 = emd.update(1.0).unwrap();
        assert!((emd.upper() - 0.5 * 3.0 * bp61 / 50.0).abs() < 1e-15);
        assert!((emd.lower() - 0.5 * bp63 / 50.0).abs() < 1e-15);
        assert!((emd.lower() + 0.001_301_769_567_754_182_7).abs() < 1e-12);
        assert!((m64 - (bp60 + bp61 + bp62 + bp63 + bp64) / 10.0).abs() < 1e-15);
        assert_eq!(emd.value(), Some(m64));
    }

    #[test]
    fn thresholds_bracket_zero_on_an_oscillation() {
        // A sine at the centre period produces repeated peaks (> 0) and valleys (< 0).
        let mut emd = EmpiricalModeDecomposition::new(20, 0.3).unwrap();
        for i in 0..400 {
            emd.update((f64::from(i) * 2.0 * PI / 20.0).sin() * 10.0 + 100.0);
        }
        assert!(emd.upper() > 0.0);
        assert!(emd.lower() < 0.0);
        let mean = emd.value().unwrap();
        assert!(mean.abs() < emd.upper());
    }

    #[test]
    fn reset_replays_identically_and_batch_nan_into_matches() {
        let prices: Vec<f64> = (0..200)
            .map(|i| 100.0 + (f64::from(i) * 0.17).sin() * 3.0 + f64::from(i) * 0.02)
            .collect();
        let mut emd = EmpiricalModeDecomposition::new(12, 0.2).unwrap();
        let first = emd.batch(&prices);
        let (up, lo) = (emd.upper(), emd.lower());
        emd.reset();
        assert_eq!(emd.upper().to_bits(), 0.0f64.to_bits());
        assert_eq!(emd.lower().to_bits(), 0.0f64.to_bits());
        assert_eq!(emd.value(), None);
        let second = emd.batch(&prices);
        assert_eq!(first, second);
        assert_eq!(emd.upper().to_bits(), up.to_bits());
        assert_eq!(emd.lower().to_bits(), lo.to_bits());
        let mut fresh = EmpiricalModeDecomposition::new(12, 0.2).unwrap();
        let mut out = vec![0.0; prices.len()];
        fresh.batch_nan_into(&prices, &mut out);
        assert!(out
            .iter()
            .zip(&first)
            .all(|(a, b)| a.to_bits() == b.unwrap_or(f64::NAN).to_bits()));
    }
}
