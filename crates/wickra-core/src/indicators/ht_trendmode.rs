//! Ehlers Hilbert Transform Trend vs Cycle Mode (`HT_TRENDMODE`).
#![allow(clippy::manual_clamp)]

use std::f64::consts::PI;

use crate::indicators::dc_phasor::{self, MAX_DC_PERIOD};
use crate::traits::Indicator;

/// Ehlers' Hilbert Transform Trend Mode (`HT_TRENDMODE`).
///
/// Runs the same adaptive Hilbert-transform engine as
/// [`HilbertDominantCycle`](crate::HilbertDominantCycle), derives the dominant
/// cycle phase, its sine / lead-sine, and an instantaneous trendline, then
/// classifies the market into **trend mode (`1`)** or **cycle mode (`0`)**:
///
/// - it is a *cycle* shortly after the sine and lead-sine cross, while the phase
///   advances at roughly the dominant-cycle rate;
/// - it is a *trend* otherwise, and is forced to trend whenever price separates
///   from the trendline by more than 1.5%.
///
/// From *Rocket Science for Traders* (Ehlers 2001), aligned with TA-Lib's
/// `HT_TRENDMODE`. The output is `1.0` or `0.0`; the first value is emitted after
/// ~50 inputs once the engine's moving-average chain has filled.
///
/// # Example
///
/// ```
/// use wickra_core::{Indicator, HtTrendMode};
///
/// let mut ht = HtTrendMode::new();
/// let mut last = None;
/// for i in 0..120 {
///     last = ht.update(100.0 + f64::from(i));
/// }
/// assert!(last.is_some());
/// ```
#[derive(Debug, Clone, Default)]
pub struct HtTrendMode {
    // Raw input window for the 4-bar WMA.
    price_buf: Vec<f64>,
    // WMA-smoothed price history feeding the Hilbert detrender taps.
    smooth_buf: Vec<f64>,
    detrender_buf: Vec<f64>,
    q1_buf: Vec<f64>,
    i1_buf: Vec<f64>,
    smooth_price: Vec<f64>,
    // Raw input history the instantaneous trendline averages (TA-Lib sums
    // `inReal`, not the smoothed price).
    raw_price: Vec<f64>,
    prev_i2: f64,
    prev_q2: f64,
    prev_re: f64,
    prev_im: f64,
    prev_period: f64,
    prev_smooth_period: f64,
    // Trend-mode state.
    prev_dc_phase: f64,
    prev_sine: f64,
    prev_lead_sine: f64,
    days_in_trend: f64,
    it1: f64,
    it2: f64,
    it3: f64,
    count: usize,
    last_value: Option<f64>,
}

impl HtTrendMode {
    /// Construct a new Hilbert transform trend-mode classifier.
    pub fn new() -> Self {
        Self::default()
    }

    /// Current trend-mode flag (`1.0` trend, `0.0` cycle) if available.
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

impl Indicator for HtTrendMode {
    type Input = f64;
    type Output = f64;

    #[allow(clippy::too_many_lines)]
    fn update(&mut self, input: f64) -> Option<f64> {
        if !input.is_finite() {
            return None;
        }
        self.count += 1;

        Self::push_front(&mut self.raw_price, input, MAX_DC_PERIOD);
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

        let smooth_period = self.prev_smooth_period;
        let dc_period = ((smooth_period + 0.5) as usize).clamp(1, self.smooth_price.len());

        // Dominant-cycle phase over one cycle window.
        let mut real_part = 0.0;
        let mut imag_part = 0.0;
        for (&(sin, cos), &sp) in dc_phasor::phasor(dc_period).iter().zip(&self.smooth_price) {
            real_part += sin * sp;
            imag_part += cos * sp;
        }
        let dc_phase = compute_dc_phase(real_part, imag_part, smooth_period);

        let sine = (dc_phase * PI / 180.0).sin();
        let lead_sine = ((dc_phase + 45.0) * PI / 180.0).sin();

        // Instantaneous trendline: average raw price over the cycle window,
        // then a 4-3-2-1 weighted smoothing of that running average.
        let mut trend_sum = 0.0;
        for &price in &self.raw_price[..dc_period] {
            trend_sum += price;
        }
        trend_sum /= dc_period as f64;
        let trendline = (4.0 * trend_sum + 3.0 * self.it1 + 2.0 * self.it2 + self.it3) / 10.0;
        self.it3 = self.it2;
        self.it2 = self.it1;
        self.it1 = trend_sum;

        // Trend / cycle decision (assume trend, override to cycle).
        let mut trend = 1.0_f64;

        // A crossing of sine and lead-sine restarts the cycle clock.
        if (sine > lead_sine && self.prev_sine <= self.prev_lead_sine)
            || (sine < lead_sine && self.prev_sine >= self.prev_lead_sine)
        {
            self.days_in_trend = 0.0;
            trend = 0.0;
        }
        self.days_in_trend += 1.0;
        if self.days_in_trend < 0.5 * smooth_period {
            trend = 0.0;
        }

        // Cycle mode while the phase advances at roughly the dominant-cycle rate.
        let delta_phase = dc_phase - self.prev_dc_phase;
        if smooth_period != 0.0
            && delta_phase > 0.67 * 360.0 / smooth_period
            && delta_phase < 1.5 * 360.0 / smooth_period
        {
            trend = 0.0;
        }

        // Force trend mode when price separates from the trendline.
        if trendline != 0.0 && ((smooth - trendline) / trendline).abs() >= 0.015 {
            trend = 1.0;
        }

        self.prev_dc_phase = dc_phase;
        self.prev_sine = sine;
        self.prev_lead_sine = lead_sine;

        if self.count < 50 {
            return None;
        }
        self.last_value = Some(trend);
        Some(trend)
    }

    fn reset(&mut self) {
        self.price_buf.clear();
        self.smooth_buf.clear();
        self.detrender_buf.clear();
        self.q1_buf.clear();
        self.i1_buf.clear();
        self.smooth_price.clear();
        self.raw_price.clear();
        self.prev_i2 = 0.0;
        self.prev_q2 = 0.0;
        self.prev_re = 0.0;
        self.prev_im = 0.0;
        self.prev_period = 0.0;
        self.prev_smooth_period = 0.0;
        self.prev_dc_phase = 0.0;
        self.prev_sine = 0.0;
        self.prev_lead_sine = 0.0;
        self.days_in_trend = 0.0;
        self.it1 = 0.0;
        self.it2 = 0.0;
        self.it3 = 0.0;
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
        "HT_TRENDMODE"
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

    /// A trending ramp followed by a clean cycle, so both modes are exercised.
    fn mixed_prices() -> Vec<f64> {
        let mut v = Vec::new();
        for i in 0..150 {
            v.push(100.0 + f64::from(i) * 0.8);
        }
        for i in 0..200 {
            v.push(220.0 + (f64::from(i) * 0.45).sin() * 12.0);
        }
        v
    }

    #[test]
    fn accessors_and_metadata() {
        let ht = HtTrendMode::new();
        assert_eq!(ht.warmup_period(), 50);
        assert_eq!(ht.name(), "HT_TRENDMODE");
        assert!(!ht.is_ready());
        assert!(ht.value().is_none());
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
    fn emits_binary_flag_and_visits_both_modes() {
        let mut ht = HtTrendMode::new();
        let out: Vec<Option<f64>> = ht.batch(&mixed_prices());
        assert_eq!(out[0], None);
        assert!(ht.is_ready());
        let mut saw_trend = false;
        let mut saw_cycle = false;
        for v in out.into_iter().flatten() {
            assert!(v == 0.0 || v == 1.0, "trend mode must be binary, got {v}");
            if v == 1.0 {
                saw_trend = true;
            } else {
                saw_cycle = true;
            }
        }
        assert!(saw_trend, "ramp segment should report trend mode");
        assert!(saw_cycle, "cycle segment should report cycle mode");
    }

    #[test]
    fn ignores_non_finite_input() {
        let mut ht = HtTrendMode::new();
        let _ = ht.batch(&mixed_prices());
        let before = ht.value();
        assert_eq!(ht.update(f64::NAN), None);
        // The rejected input must not have disturbed the state.
        assert_eq!(ht.value(), before);
    }

    #[test]
    fn batch_equals_streaming() {
        let prices = mixed_prices();
        let mut a = HtTrendMode::new();
        let mut b = HtTrendMode::new();
        let batch = a.batch(&prices);
        let streamed: Vec<_> = prices.iter().map(|p| b.update(*p)).collect();
        assert_eq!(batch, streamed);
    }

    #[test]
    fn reset_clears_state() {
        let mut ht = HtTrendMode::new();
        let _ = ht.batch(&mixed_prices());
        assert!(ht.is_ready());
        ht.reset();
        assert!(!ht.is_ready());
        assert_eq!(ht.update(100.0), None);
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
        let mut ht = HtTrendMode::new();
        let out = ht.batch(&mixed_prices());
        let warmup = ht.warmup_period();
        assert!(out[..warmup - 1].iter().all(Option::is_none));
        assert!(out[warmup - 1].is_some());
    }

    #[test]
    fn reset_replays_identically() {
        let prices = mixed_prices();
        let fresh = HtTrendMode::new().batch(&prices);
        let mut ht = HtTrendMode::new();
        let first = ht.batch(&prices);
        ht.reset();
        let second = ht.batch(&prices);
        assert_eq!(first, fresh);
        assert_eq!(second, fresh);
    }

    #[test]
    fn batch_nan_paths_match_streaming_bitwise() {
        let prices = mixed_prices();
        let mut out = vec![0.0; prices.len()];
        HtTrendMode::new().batch_nan_into(&prices, &mut out);
        let nan = HtTrendMode::new().batch_nan(&prices);
        let fast = HtTrendMode::new().batch_fast(&prices);
        let mut stream = HtTrendMode::new();
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
        let mut ht = HtTrendMode::new();
        // After exactly 4 inputs the WMA is (4*40 + 3*30 + 2*20 + 10) / 10 = 30,
        // while the raw history keeps the inputs newest first.
        for p in [10.0, 20.0, 30.0, 40.0] {
            assert_eq!(ht.update(p), None);
        }
        assert_eq!(ht.smooth_buf, vec![30.0]);
        assert_eq!(ht.raw_price, vec![40.0, 30.0, 20.0, 10.0]);

        // Spike of 10 at index 7 in a zero series: smoothed values 4, 3, 2 at
        // indices 7, 8, 9, so the smooth history is [2, 3, 4, 0, 0, 0, 0].
        // adj = 0.075*6 + 0.54 = 0.99 and the detrender reads the smoothed taps:
        //   (0.0962*2 + 0.5769*4 - 0.5769*0 - 0.0962*0) * 0.99 = 2.475.
        let mut ht = HtTrendMode::new();
        let mut series = [0.0; 10];
        series[7] = 10.0;
        let _ = ht.batch(&series);
        assert_eq!(ht.smooth_buf, vec![2.0, 3.0, 4.0, 0.0, 0.0, 0.0, 0.0]);
        assert_eq!(ht.detrender_buf.len(), 1);
        assert_relative_eq!(ht.detrender_buf[0], 2.475, epsilon = 1e-12);
    }

    #[test]
    fn trendline_averages_raw_not_smoothed_price() {
        // On a unit-slope ramp p[k] = 100 + k the 4-bar WMA lags by exactly one
        // bar: (4k + 3(k-1) + 2(k-2) + (k-3)) / 10 = k - 1. So the mean of the
        // raw prices over the dominant-cycle window exceeds the mean of the
        // smoothed prices by exactly 1.0, which tells the two averages apart.
        let prices: Vec<f64> = (0..120).map(|k| 100.0 + f64::from(k)).collect();
        let mut ht = HtTrendMode::new();
        let _ = ht.batch(&prices);
        let dc_period = ((ht.prev_smooth_period + 0.5) as usize).clamp(1, ht.smooth_price.len());
        let n = prices.len();
        let window = &prices[n - dc_period..];
        let dc = dc_period as f64;
        let raw_mean = window.iter().sum::<f64>() / dc;
        let smooth_mean = ht.smooth_price[..dc_period].iter().sum::<f64>() / dc;
        // `it1` stores the latest trendline average.
        assert_relative_eq!(ht.it1, raw_mean, epsilon = 1e-9);
        assert_relative_eq!(raw_mean - smooth_mean, 1.0, epsilon = 1e-9);
        // The raw window is exactly the newest `dc_period` inputs, newest first.
        assert!(ht
            .raw_price
            .iter()
            .zip(prices.iter().rev())
            .all(|(a, b)| a.to_bits() == b.to_bits()));
        assert_eq!(ht.raw_price.len(), MAX_DC_PERIOD);
    }

    #[test]
    fn steady_ramp_reports_trend_and_clean_cycle_reports_cycle() {
        // Strong ramp: price separates from the trendline by more than 1.5%,
        // forcing trend mode on every emitted bar once the engine settles.
        let ramp: Vec<f64> = (0..200).map(|k| 100.0 * 1.02_f64.powi(k)).collect();
        let mut ht = HtTrendMode::new();
        let out = ht.batch(&ramp);
        assert!(out[100..]
            .iter()
            .flatten()
            .all(|v| v.to_bits() == 1.0_f64.to_bits()));
        // Clean cycle with a small amplitude: cycle mode must appear.
        let mut ht = HtTrendMode::new();
        let out = ht.batch(&sine_prices(300));
        assert!(out
            .iter()
            .flatten()
            .any(|v| v.to_bits() == 0.0_f64.to_bits()));
    }

    #[test]
    fn zero_series_skips_trendline_separation_and_settles_in_trend() {
        // All zero: the trendline is exactly 0, so the separation test is
        // skipped; the phase is constant (no crossing, no phase advance), so
        // after days_in_trend passes half the period the flag is trend (1).
        let mut ht = HtTrendMode::new();
        let out = ht.batch(&[0.0; 200]);
        assert_eq!(out.iter().flatten().count(), 200 - 49);
        assert_eq!(ht.value(), Some(1.0));
    }

    #[test]
    fn dc_phase_quadrant_and_wrap_hand_computed() {
        // real = 1, imag = -1, period 20: atan(-1) = -45; -45 + 90 + 18 = 63;
        // imag < 0 adds 180 -> 243.
        assert_relative_eq!(compute_dc_phase(1.0, -1.0, 20.0), 243.0, epsilon = 1e-9);
        // real = -1, imag = -1: 45 + 90 + 18 + 180 = 333 > 315 -> -27.
        assert_relative_eq!(compute_dc_phase(-1.0, -1.0, 20.0), -27.0, epsilon = 1e-9);
    }
}
