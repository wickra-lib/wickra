//! Ehlers Hilbert Transform Dominant Cycle Phase (`HT_DCPHASE`).
#![allow(clippy::manual_clamp)]

use std::f64::consts::PI;

use crate::indicators::dc_phasor::{self, MAX_DC_PERIOD};
use crate::traits::Indicator;

/// Ehlers' Hilbert Transform Dominant Cycle Phase (`HT_DCPHASE`).
///
/// Runs the same adaptive Hilbert-transform engine as
/// [`HilbertDominantCycle`](crate::HilbertDominantCycle) to recover the dominant
/// cycle period, then measures the **phase angle** of that cycle (in degrees) by
/// correlating the smoothed price over one dominant-cycle window against a unit
/// phasor. The phase advances roughly linearly through a clean cycle and stalls
/// in a trend, which is the basis of Ehlers' trend-versus-cycle detection.
///
/// From *Rocket Science for Traders* (Ehlers 2001), aligned with TA-Lib's
/// `HT_DCPHASE`. The first value is emitted after ~50 inputs, once the engine's
/// moving-average chain has filled.
///
/// # Example
///
/// ```
/// use wickra_core::{Indicator, HtDcPhase};
///
/// let mut ht = HtDcPhase::new();
/// let mut last = None;
/// for i in 0..120 {
///     last = ht.update(100.0 + (f64::from(i) * 0.4).sin() * 5.0);
/// }
/// assert!(last.is_some());
/// ```
#[derive(Debug, Clone, Default)]
pub struct HtDcPhase {
    // Raw input window for the 4-bar WMA.
    price_buf: Vec<f64>,
    // WMA-smoothed price history feeding the Hilbert detrender taps.
    smooth_buf: Vec<f64>,
    detrender_buf: Vec<f64>,
    q1_buf: Vec<f64>,
    i1_buf: Vec<f64>,
    // Longer history of the 4-bar smoothed price, used to integrate the phase
    // over one dominant-cycle window (up to 50 bars).
    smooth_price: Vec<f64>,
    prev_i2: f64,
    prev_q2: f64,
    prev_re: f64,
    prev_im: f64,
    prev_period: f64,
    prev_smooth_period: f64,
    count: usize,
    last_value: Option<f64>,
}

impl HtDcPhase {
    /// Construct a new Hilbert transform dominant-cycle phase estimator.
    pub fn new() -> Self {
        Self::default()
    }

    /// Current dominant-cycle phase (degrees) if available.
    pub const fn value(&self) -> Option<f64> {
        self.last_value
    }

    fn push_front(buf: &mut Vec<f64>, v: f64, cap: usize) {
        buf.insert(0, v);
        if buf.len() > cap {
            buf.truncate(cap);
        }
    }
}

impl Indicator for HtDcPhase {
    type Input = f64;
    type Output = f64;

    fn update(&mut self, input: f64) -> Option<f64> {
        if !input.is_finite() {
            return None;
        }
        self.count += 1;

        Self::push_front(&mut self.price_buf, input, 4);
        if self.price_buf.len() < 4 {
            return None;
        }
        let smooth = (4.0 * self.price_buf[0]
            + 3.0 * self.price_buf[1]
            + 2.0 * self.price_buf[2]
            + self.price_buf[3])
            / 10.0;
        Self::push_front(&mut self.smooth_buf, smooth, 7);
        Self::push_front(&mut self.smooth_price, smooth, MAX_DC_PERIOD);

        let period = self.prev_period.max(6.0).min(50.0);
        let adj = 0.075 * period + 0.54;

        if self.smooth_buf.len() < 7 {
            return None;
        }
        let s0 = smooth;
        let s2 = self.smooth_buf[2];
        let s4 = self.smooth_buf[4];
        let s6 = self.smooth_buf[6];
        let detrender = (0.0962 * s0 + 0.5769 * s2 - 0.5769 * s4 - 0.0962 * s6) * adj;
        Self::push_front(&mut self.detrender_buf, detrender, 7);
        if self.detrender_buf.len() < 7 {
            return None;
        }

        let q1 = (0.0962 * self.detrender_buf[0] + 0.5769 * self.detrender_buf[2]
            - 0.5769 * self.detrender_buf[4]
            - 0.0962 * self.detrender_buf[6])
            * adj;
        let i1 = self.detrender_buf[3];

        Self::push_front(&mut self.q1_buf, q1, 7);
        Self::push_front(&mut self.i1_buf, i1, 7);
        if self.q1_buf.len() < 7 || self.i1_buf.len() < 7 {
            return None;
        }

        let ji = (0.0962 * self.i1_buf[0] + 0.5769 * self.i1_buf[2]
            - 0.5769 * self.i1_buf[4]
            - 0.0962 * self.i1_buf[6])
            * adj;
        let jq = (0.0962 * self.q1_buf[0] + 0.5769 * self.q1_buf[2]
            - 0.5769 * self.q1_buf[4]
            - 0.0962 * self.q1_buf[6])
            * adj;

        let mut i2 = i1 - jq;
        let mut q2 = q1 + ji;
        i2 = 0.2 * i2 + 0.8 * self.prev_i2;
        q2 = 0.2 * q2 + 0.8 * self.prev_q2;

        let mut re = i2 * self.prev_i2 + q2 * self.prev_q2;
        let mut im = i2 * self.prev_q2 - q2 * self.prev_i2;
        re = 0.2 * re + 0.8 * self.prev_re;
        im = 0.2 * im + 0.8 * self.prev_im;

        self.prev_i2 = i2;
        self.prev_q2 = q2;
        self.prev_re = re;
        self.prev_im = im;

        let mut new_period = if im.abs() > f64::EPSILON && re.abs() > f64::EPSILON {
            2.0 * PI / im.atan2(re)
        } else {
            self.prev_period
        };
        new_period = new_period.min(1.5 * self.prev_period);
        new_period = new_period.max(0.67 * self.prev_period);
        new_period = new_period.clamp(6.0, 50.0);
        self.prev_period = 0.2 * new_period + 0.8 * self.prev_period;
        self.prev_smooth_period = 0.33 * self.prev_period + 0.67 * self.prev_smooth_period;

        if self.count < 50 {
            return None;
        }

        // Integrate the smoothed price over one dominant-cycle window against a
        // unit phasor to recover the instantaneous dominant-cycle phase.
        let smooth_period = self.prev_smooth_period;
        let dc_period = (smooth_period + 0.5) as usize;
        let dc_period = dc_period.clamp(1, self.smooth_price.len());
        let mut real_part = 0.0;
        let mut imag_part = 0.0;
        for (&(sin, cos), &sp) in dc_phasor::phasor(dc_period).iter().zip(&self.smooth_price) {
            real_part += sin * sp;
            imag_part += cos * sp;
        }

        let dc_phase = compute_dc_phase(real_part, imag_part, smooth_period);

        self.last_value = Some(dc_phase);
        Some(dc_phase)
    }

    fn reset(&mut self) {
        self.price_buf.clear();
        self.smooth_buf.clear();
        self.detrender_buf.clear();
        self.q1_buf.clear();
        self.i1_buf.clear();
        self.smooth_price.clear();
        self.prev_i2 = 0.0;
        self.prev_q2 = 0.0;
        self.prev_re = 0.0;
        self.prev_im = 0.0;
        self.prev_period = 0.0;
        self.prev_smooth_period = 0.0;
        self.count = 0;
        self.last_value = None;
    }

    #[inline]
    fn warmup_period(&self) -> usize {
        50
    }

    #[inline]
    fn is_ready(&self) -> bool {
        self.last_value.is_some()
    }

    #[inline]
    fn name(&self) -> &'static str {
        "HT_DCPHASE"
    }
}

/// Recovers the dominant-cycle phase (degrees) from the real/imaginary parts of
/// the one-cycle homodyne integration, then unwraps it into TA-Lib's
/// `[-45, 315)` output range with the 4-bar smoother group-delay correction.
///
/// When `imag_part` is within `±0.001` of zero the `atan` is undefined, so the
/// phase collapses to `±90°` by the sign of `real_part`.
fn compute_dc_phase(real_part: f64, imag_part: f64, smooth_period: f64) -> f64 {
    let mut dc_phase = if imag_part.abs() > 0.001 {
        (real_part / imag_part).atan().to_degrees()
    } else if real_part < 0.0 {
        -90.0
    } else {
        90.0
    };
    dc_phase += 90.0;
    // Compensate the group delay of the 4-bar weighted smoother.
    dc_phase += 360.0 / smooth_period;
    if imag_part < 0.0 {
        dc_phase += 180.0;
    }
    if dc_phase > 315.0 {
        dc_phase -= 360.0;
    }
    dc_phase
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;

    fn sine_prices(n: usize) -> Vec<f64> {
        (0..n)
            .map(|i| 100.0 + (i as f64 * 0.4).sin() * 5.0)
            .collect()
    }

    #[test]
    fn accessors_and_metadata() {
        let ht = HtDcPhase::new();
        assert_eq!(ht.warmup_period(), 50);
        assert_eq!(ht.name(), "HT_DCPHASE");
        assert!(!ht.is_ready());
    }

    #[test]
    fn near_zero_imaginary_collapses_to_signed_ninety() {
        // A near-zero imaginary part makes atan(real/imag) undefined, so the phase
        // collapses to +90 for non-negative real and -90 for negative real before
        // the +90 offset and group-delay correction unwrap it.
        let pos = compute_dc_phase(1.0, 0.0, 20.0);
        let neg = compute_dc_phase(-1.0, 0.0, 20.0);
        assert!((pos - 198.0).abs() < 1e-9);
        assert!((neg - 18.0).abs() < 1e-9);
        // The normal path still flows through atan.
        let mid = compute_dc_phase(1.0, 1.0, 20.0);
        assert!((mid - 153.0).abs() < 1e-9);
    }

    #[test]
    fn emits_after_warmup_within_phase_band() {
        let mut ht = HtDcPhase::new();
        let out: Vec<Option<f64>> = ht.batch(&sine_prices(200));
        assert_eq!(out[0], None);
        assert!(ht.is_ready());
        for v in out.into_iter().flatten() {
            assert!(v.is_finite(), "phase must be finite");
            assert!((-360.0..=360.0).contains(&v), "phase {v} outside band");
        }
    }

    #[test]
    fn ignores_non_finite_input() {
        let mut ht = HtDcPhase::new();
        let _ = ht.batch(&sine_prices(120));
        let before = ht.value();
        assert_eq!(ht.update(f64::NAN), None);
        // The rejected input must not have disturbed the state.
        assert_eq!(ht.value(), before);
    }

    #[test]
    fn batch_equals_streaming() {
        let prices = sine_prices(200);
        let mut a = HtDcPhase::new();
        let mut b = HtDcPhase::new();
        let batch = a.batch(&prices);
        let streamed: Vec<_> = prices.iter().map(|p| b.update(*p)).collect();
        assert_eq!(batch, streamed);
    }

    #[test]
    fn reset_clears_state() {
        let mut ht = HtDcPhase::new();
        let _ = ht.batch(&sine_prices(120));
        assert!(ht.is_ready());
        ht.reset();
        assert!(!ht.is_ready());
        assert_eq!(ht.update(100.0), None);
    }

    use crate::traits::BatchNanExt;
    use approx::assert_relative_eq;

    #[test]
    fn first_value_lands_exactly_at_warmup() {
        let mut ht = HtDcPhase::new();
        let out = ht.batch(&sine_prices(120));
        let warmup = ht.warmup_period();
        assert!(out[..warmup - 1].iter().all(Option::is_none));
        assert!(out[warmup - 1].is_some());
    }

    #[test]
    fn reset_replays_identically() {
        let prices = sine_prices(150);
        let fresh = HtDcPhase::new().batch(&prices);
        let mut ht = HtDcPhase::new();
        let first = ht.batch(&prices);
        ht.reset();
        let second = ht.batch(&prices);
        assert_eq!(first, fresh);
        assert_eq!(second, fresh);
    }

    #[test]
    fn batch_nan_paths_match_streaming_bitwise() {
        let prices = sine_prices(150);
        let mut out = vec![0.0; prices.len()];
        HtDcPhase::new().batch_nan_into(&prices, &mut out);
        let nan = HtDcPhase::new().batch_nan(&prices);
        let fast = HtDcPhase::new().batch_fast(&prices);
        let mut stream = HtDcPhase::new();
        let expected: Vec<u64> = prices
            .iter()
            .map(|&p| stream.update(p).unwrap_or(f64::NAN).to_bits())
            .collect();
        assert!(out.iter().zip(&expected).all(|(v, e)| v.to_bits() == *e));
        assert!(nan.iter().zip(&expected).all(|(v, e)| v.to_bits() == *e));
        assert!(fast.iter().zip(&expected).all(|(v, e)| v.to_bits() == *e));
    }

    #[test]
    fn wma_of_raw_inputs_feeds_detrender_taps() {
        let mut ht = HtDcPhase::new();
        // After exactly 4 inputs the WMA is (4*40 + 3*30 + 2*20 + 10) / 10 = 30.
        for p in [10.0, 20.0, 30.0, 40.0] {
            assert_eq!(ht.update(p), None);
        }
        assert_eq!(ht.smooth_buf, vec![30.0]);
        assert_eq!(ht.smooth_price, vec![30.0]);

        // Spike of 10 at index 7 in a zero series: smoothed values 4, 3, 2 at
        // indices 7, 8, 9, so the smooth history is [2, 3, 4, 0, 0, 0, 0].
        // adj = 0.075*6 + 0.54 = 0.99 and the detrender reads the smoothed taps:
        //   (0.0962*2 + 0.5769*4 - 0.5769*0 - 0.0962*0) * 0.99 = 2.475.
        let mut ht = HtDcPhase::new();
        let mut series = [0.0; 10];
        series[7] = 10.0;
        let _ = ht.batch(&series);
        assert_eq!(ht.smooth_buf, vec![2.0, 3.0, 4.0, 0.0, 0.0, 0.0, 0.0]);
        assert_eq!(ht.detrender_buf.len(), 1);
        assert_relative_eq!(ht.detrender_buf[0], 2.475, epsilon = 1e-12);
    }

    #[test]
    fn dc_phase_quadrant_and_wrap_hand_computed() {
        // real = 1, imag = -1, period 20: atan(-1) = -45; -45 + 90 + 360/20 = 63;
        // imag < 0 adds 180 -> 243 (no wrap, 243 <= 315).
        assert_relative_eq!(compute_dc_phase(1.0, -1.0, 20.0), 243.0, epsilon = 1e-9);
        // real = -1, imag = -1, period 20: atan(1) = 45; 45 + 90 + 18 = 153;
        // imag < 0 adds 180 -> 333 > 315, so it wraps to 333 - 360 = -27.
        assert_relative_eq!(compute_dc_phase(-1.0, -1.0, 20.0), -27.0, epsilon = 1e-9);
        // real = 0, imag = 0, period 6: degenerate -> 90; 90 + 90 + 60 = 240.
        assert_relative_eq!(compute_dc_phase(0.0, 0.0, 6.0), 240.0, epsilon = 1e-12);
    }

    #[test]
    fn zero_series_phase_converges_to_degenerate_value() {
        // Zero input: real = imag = 0 so the phase is 90 + 90 + 360/period, and
        // the smoothed period converges on the lower clamp of 6 -> 240 degrees.
        let mut ht = HtDcPhase::new();
        let out = ht.batch(&[0.0; 400]);
        assert!(out.iter().flatten().all(|v| v.is_finite()));
        assert_relative_eq!(ht.value().unwrap(), 240.0, epsilon = 1e-6);
        // A non-zero flat series stays finite.
        let mut ht = HtDcPhase::new();
        let out = ht.batch(&[100.0; 400]);
        assert_eq!(
            out.iter().flatten().filter(|v| v.is_finite()).count(),
            400 - 49
        );
    }
}
