//! Ehlers Sine Wave indicator.

use crate::indicators::ht_dcphase::HtDcPhase;
use crate::traits::Indicator;

/// Ehlers' Sine Wave indicator (sine + leadsine), TA-Lib `HT_SINE`.
///
/// Implementation from *Rocket Science for Traders* (Ehlers 2001, ch. 9). The
/// phase is the dominant-cycle phase of [`HtDcPhase`]: the smoothed price is
/// correlated with one cycle of a sine and a cosine of the measured dominant
/// period, `DCPhase = atan(Real / Imag)`, then corrected (`+90°`, the smoother's
/// lag, the quadrant). The indicator returns
///
/// ```text
/// Sine     = sin(DCPhase)
/// LeadSine = sin(DCPhase + 45°)
/// ```
///
/// The two lines cross deep in trends but oscillate rapidly during cycles,
/// providing a visual lead/lag signal.
///
/// Only the primary `sine` line is exposed as the scalar output to match the
/// crate's standard scalar-indicator surface; the lead is accessible via the
/// [`SineWave::lead`] accessor after each update.
///
/// # Example
///
/// ```
/// use wickra_core::{Indicator, SineWave};
///
/// let mut sw = SineWave::new();
/// let mut last = None;
/// for i in 0..200 {
///     last = sw.update(100.0 + (f64::from(i) * 0.4).sin() * 5.0);
/// }
/// assert!(last.is_some());
/// ```
#[derive(Debug, Clone, Default)]
pub struct SineWave {
    phase: HtDcPhase,
    last_sine: Option<f64>,
    last_lead: f64,
}

impl SineWave {
    /// Construct a new Sine Wave indicator.
    pub fn new() -> Self {
        Self::default()
    }

    /// Most recent lead (45°-ahead) value. `0.0` until the indicator is ready.
    pub const fn lead(&self) -> f64 {
        self.last_lead
    }

    /// Current sine value if available.
    pub const fn value(&self) -> Option<f64> {
        self.last_sine
    }
}

impl Indicator for SineWave {
    type Input = f64;
    type Output = f64;

    fn update(&mut self, input: f64) -> Option<f64> {
        if !input.is_finite() {
            return None;
        }
        let phase = self.phase.update(input)?.to_radians();
        let sine = phase.sin();
        self.last_lead = (phase + 45f64.to_radians()).sin();
        self.last_sine = Some(sine);
        Some(sine)
    }

    fn reset(&mut self) {
        self.phase.reset();
        self.last_sine = None;
        self.last_lead = 0.0;
    }

    #[inline]
    fn warmup_period(&self) -> usize {
        self.phase.warmup_period()
    }

    #[inline]
    fn is_ready(&self) -> bool {
        self.last_sine.is_some()
    }

    #[inline]
    fn name(&self) -> &'static str {
        "SineWave"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;

    #[test]
    fn accessors_and_metadata() {
        let mut sw = SineWave::new();
        assert_eq!(sw.warmup_period(), 50);
        assert_eq!(sw.name(), "SineWave");
        assert!(!sw.is_ready());
        assert!(sw.value().is_none());
        let prices: Vec<f64> = (0..120)
            .map(|i| 100.0 + (f64::from(i) * 0.4).sin() * 5.0)
            .collect();
        sw.batch(&prices);
        assert!(sw.is_ready());
        assert!(sw.value().is_some());
    }

    #[test]
    fn output_bounded() {
        let prices: Vec<f64> = (0..200)
            .map(|i| 100.0 + (f64::from(i) * 0.3).cos() * 5.0)
            .collect();
        let mut sw = SineWave::new();
        for v in sw.batch(&prices).into_iter().flatten() {
            assert!((-1.0..=1.0).contains(&v), "sine out of bounds: {v}");
        }
        // Lead value also bounded after warmup.
        assert!(sw.lead() >= -1.0 && sw.lead() <= 1.0);
    }

    #[test]
    fn batch_equals_streaming() {
        let prices: Vec<f64> = (0..200)
            .map(|i| 100.0 + (f64::from(i) * 0.3).sin() * 5.0)
            .collect();
        let mut a = SineWave::new();
        let mut b = SineWave::new();
        let batch = a.batch(&prices);
        let streamed: Vec<_> = prices.iter().map(|p| b.update(*p)).collect();
        assert_eq!(batch, streamed);
    }

    #[test]
    fn ignores_non_finite_input() {
        let mut sw = SineWave::new();
        let prices: Vec<f64> = (0..120)
            .map(|i| 100.0 + (f64::from(i) * 0.4).sin() * 5.0)
            .collect();
        sw.batch(&prices);
        let before = sw.value();
        assert!(before.is_some());
        assert_eq!(sw.update(f64::NAN), None);
    }

    #[test]
    fn reset_clears_state() {
        let mut sw = SineWave::new();
        let prices: Vec<f64> = (0..120)
            .map(|i| 100.0 + (f64::from(i) * 0.4).sin() * 5.0)
            .collect();
        sw.batch(&prices);
        assert!(sw.is_ready());
        sw.reset();
        assert!(!sw.is_ready());
        assert!(sw.value().is_none());
    }

    #[test]
    fn flat_input_uses_phase_fallback() {
        // Zero inputs make the DC phase's real and imaginary parts exactly
        // zero, so the phase takes its degenerate-imaginary guard and the
        // sine is still defined.
        let mut sw = SineWave::new();
        let _ = sw.batch(&[0.0_f64; 120]);
        assert!(sw.value().is_some());
    }

    use crate::traits::BatchNanExt;
    use approx::assert_relative_eq;

    fn sine_prices(n: u32) -> Vec<f64> {
        (0..n)
            .map(|i| 100.0 + (f64::from(i) * 0.4).sin() * 5.0)
            .collect()
    }

    #[test]
    fn first_value_lands_exactly_at_warmup() {
        let mut sw = SineWave::new();
        let out = sw.batch(&sine_prices(120));
        let warmup = sw.warmup_period();
        assert!(out[..warmup - 1].iter().all(Option::is_none));
        assert!(out[warmup - 1].is_some());
    }

    #[test]
    fn reset_replays_identically() {
        let prices = sine_prices(150);
        let fresh = SineWave::new().batch(&prices);
        let mut sw = SineWave::new();
        let first = sw.batch(&prices);
        sw.reset();
        assert_eq!(sw.lead().to_bits(), 0.0_f64.to_bits());
        let second = sw.batch(&prices);
        assert_eq!(first, fresh);
        assert_eq!(second, fresh);
    }

    #[test]
    fn batch_nan_paths_match_streaming_bitwise() {
        let prices = sine_prices(150);
        let mut out = vec![0.0; prices.len()];
        SineWave::new().batch_nan_into(&prices, &mut out);
        let nan = SineWave::new().batch_nan(&prices);
        let fast = SineWave::new().batch_fast(&prices);
        let mut stream = SineWave::new();
        let expected: Vec<u64> = prices
            .iter()
            .map(|&p| stream.update(p).unwrap_or(f64::NAN).to_bits())
            .collect();
        assert!(out.iter().zip(&expected).all(|(v, e)| v.to_bits() == *e));
        assert!(nan.iter().zip(&expected).all(|(v, e)| v.to_bits() == *e));
        assert!(fast.iter().zip(&expected).all(|(v, e)| v.to_bits() == *e));
    }

    #[test]
    fn sine_and_lead_are_functions_of_dc_phase() {
        // sine = sin(phase), lead = sin(phase + 45 deg), where phase is HT_DCPHASE.
        let prices = sine_prices(150);
        let mut sw = SineWave::new();
        let mut phase = HtDcPhase::new();
        for &p in &prices {
            let sine = sw.update(p);
            let ph = phase.update(p);
            assert_eq!(sine.is_some(), ph.is_some());
            if let (Some(s), Some(deg)) = (sine, ph) {
                let rad = deg.to_radians();
                assert_eq!(s.to_bits(), rad.sin().to_bits());
                assert_eq!(
                    sw.lead().to_bits(),
                    (rad + 45f64.to_radians()).sin().to_bits()
                );
            }
        }
    }

    #[test]
    fn zero_series_hand_computed() {
        // On a zero series HT_DCPHASE converges to 90 + 90 + 360/6 = 240 degrees,
        // so sine = sin(240 deg) = -sqrt(3)/2 = -0.866_025_4 and
        // lead = sin(285 deg) = -(sqrt(6) + sqrt(2))/4 = -0.965_925_8.
        let mut sw = SineWave::new();
        let _ = sw.batch(&[0.0; 400]);
        assert_relative_eq!(sw.value().unwrap(), -(3.0_f64.sqrt()) / 2.0, epsilon = 1e-6);
        assert_relative_eq!(
            sw.lead(),
            -(6.0_f64.sqrt() + 2.0_f64.sqrt()) / 4.0,
            epsilon = 1e-6
        );
    }
}
