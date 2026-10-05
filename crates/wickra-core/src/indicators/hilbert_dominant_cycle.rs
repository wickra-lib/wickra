//! Ehlers Hilbert Transform Dominant Cycle period estimator.
#![allow(clippy::manual_clamp)]

use std::f64::consts::PI;

use crate::traits::Indicator;

/// Ehlers' Hilbert Transform–based Dominant Cycle period estimator.
///
/// Decomposes price into in-phase and quadrature components via Ehlers'
/// truncated Hilbert transform, then derives the instantaneous phase. The
/// dominant cycle period is recovered from the phase rate of change,
/// rate-limited, clamped and EMA-smoothed (0.2/0.8, then 0.33/0.67). From *Rocket Science for Traders* (Ehlers 2001, ch. 7),
/// implementation aligned with the formulation used in TA-Lib's `HT_DCPERIOD`.
///
/// The output is clamped to the band `[6, 50]` bars, which Ehlers identifies
/// as the meaningful tradable cycle range. The estimator emits its first
/// value after ~50 inputs as the moving-average chain fills.
///
/// # Example
///
/// ```
/// use wickra_core::{Indicator, HilbertDominantCycle};
///
/// let mut ht = HilbertDominantCycle::new();
/// let mut last = None;
/// for i in 0..200 {
///     last = ht.update(100.0 + (f64::from(i) * 0.4).sin() * 5.0);
/// }
/// assert!(last.is_some());
/// ```
#[derive(Debug, Clone, Default)]
pub struct HilbertDominantCycle {
    // Raw input window for the 4-bar WMA.
    price_buf: Vec<f64>,
    // WMA-smoothed price history feeding the Hilbert detrender taps.
    smooth_buf: Vec<f64>,
    // Detrender / Q1 / I1 ring history (need 6 prior).
    detrender_buf: Vec<f64>,
    q1_buf: Vec<f64>,
    i1_buf: Vec<f64>,
    // Smoothed I/Q lines for phase computation.
    prev_i2: f64,
    prev_q2: f64,
    prev_re: f64,
    prev_im: f64,
    prev_period: f64,
    prev_smooth_period: f64,
    count: usize,
    last_value: Option<f64>,
}

impl HilbertDominantCycle {
    /// Construct a new dominant cycle estimator.
    pub fn new() -> Self {
        Self::default()
    }

    /// Current period estimate if available.
    pub const fn value(&self) -> Option<f64> {
        self.last_value
    }
}

impl Indicator for HilbertDominantCycle {
    type Input = f64;
    type Output = f64;

    fn update(&mut self, input: f64) -> Option<f64> {
        if !input.is_finite() {
            return None;
        }
        self.count += 1;

        // 4-bar weighted moving average of the input (smoothed price).
        // Ehlers: (4*x[0] + 3*x[1] + 2*x[2] + x[3]) / 10.
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

        // Adaptive coefficient based on the previous period estimate.
        let period = self.prev_period.max(6.0).min(50.0);
        let adj = 0.075 * period + 0.54;

        // We need the smooth buffer to hold ≥ 7 samples for the Hilbert taps.
        if self.smooth_buf.len() < 7 {
            return None;
        }

        // Ehlers' Hilbert transform of `smooth` (using current + 2/4/6 lags).
        let s0 = smooth;
        let s2 = self.smooth_buf[2];
        let s4 = self.smooth_buf[4];
        let s6 = self.smooth_buf[6];
        let detrender = (0.0962 * s0 + 0.5769 * s2 - 0.5769 * s4 - 0.0962 * s6) * adj;
        Self::push_front(&mut self.detrender_buf, detrender, 7);

        if self.detrender_buf.len() < 7 {
            return None;
        }
        // In-phase and quadrature components.
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

        // Advance the phase 90 deg via a second Hilbert pass.
        let ji = (0.0962 * self.i1_buf[0] + 0.5769 * self.i1_buf[2]
            - 0.5769 * self.i1_buf[4]
            - 0.0962 * self.i1_buf[6])
            * adj;
        let jq = (0.0962 * self.q1_buf[0] + 0.5769 * self.q1_buf[2]
            - 0.5769 * self.q1_buf[4]
            - 0.0962 * self.q1_buf[6])
            * adj;

        // Phasor smoothing.
        let mut i2 = i1 - jq;
        let mut q2 = q1 + ji;
        i2 = 0.2 * i2 + 0.8 * self.prev_i2;
        q2 = 0.2 * q2 + 0.8 * self.prev_q2;

        // Homodyne discriminator.
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
        // Rate-of-change clamp per Ehlers.
        new_period = new_period.min(1.5 * self.prev_period);
        new_period = new_period.max(0.67 * self.prev_period);
        new_period = new_period.clamp(6.0, 50.0);

        // EMA smoothing of the period.
        self.prev_period = 0.2 * new_period + 0.8 * self.prev_period;
        // Second smoothing step (TA-Lib uses 0.33/0.67).
        self.prev_smooth_period = 0.33 * self.prev_period + 0.67 * self.prev_smooth_period;

        if self.count < 50 {
            return None;
        }
        self.last_value = Some(self.prev_smooth_period);
        Some(self.prev_smooth_period)
    }

    fn reset(&mut self) {
        self.price_buf.clear();
        self.smooth_buf.clear();
        self.detrender_buf.clear();
        self.q1_buf.clear();
        self.i1_buf.clear();
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
        "HilbertDominantCycle"
    }
}

impl HilbertDominantCycle {
    /// Push `v` at the front of `buf`, capping the length at `cap`.
    fn push_front(buf: &mut Vec<f64>, v: f64, cap: usize) {
        buf.insert(0, v);
        if buf.len() > cap {
            buf.truncate(cap);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;

    #[test]
    fn accessors_and_metadata() {
        let mut ht = HilbertDominantCycle::new();
        assert_eq!(ht.warmup_period(), 50);
        assert_eq!(ht.name(), "HilbertDominantCycle");
        assert!(!ht.is_ready());
        assert!(ht.value().is_none());
        for i in 0..120 {
            ht.update(100.0 + (f64::from(i) * 0.3).sin() * 5.0);
        }
        assert!(ht.is_ready());
        assert!(ht.value().is_some());
    }

    #[test]
    fn output_within_clamp_band() {
        let mut ht = HilbertDominantCycle::new();
        let prices: Vec<f64> = (0..200)
            .map(|i| 100.0 + (f64::from(i) * 0.4).sin() * 5.0)
            .collect();
        let out = ht.batch(&prices);
        for v in out.iter().flatten() {
            assert!((6.0..=50.0).contains(v), "period {v} outside [6, 50]");
        }
    }

    #[test]
    fn batch_equals_streaming() {
        let prices: Vec<f64> = (0..200)
            .map(|i| 100.0 + (f64::from(i) * 0.3).sin() * 5.0)
            .collect();
        let mut a = HilbertDominantCycle::new();
        let mut b = HilbertDominantCycle::new();
        let batch = a.batch(&prices);
        let streamed: Vec<_> = prices.iter().map(|p| b.update(*p)).collect();
        assert_eq!(batch, streamed);
    }

    #[test]
    fn ignores_non_finite_input() {
        let mut ht = HilbertDominantCycle::new();
        let prices: Vec<f64> = (0..120)
            .map(|i| 100.0 + (f64::from(i) * 0.4).sin() * 5.0)
            .collect();
        ht.batch(&prices);
        let before = ht.value();
        assert!(before.is_some());
        assert_eq!(ht.update(f64::NAN), None);
    }

    #[test]
    fn reset_clears_state() {
        let mut ht = HilbertDominantCycle::new();
        let prices: Vec<f64> = (0..120)
            .map(|i| 100.0 + (f64::from(i) * 0.4).sin() * 5.0)
            .collect();
        ht.batch(&prices);
        assert!(ht.is_ready());
        ht.reset();
        assert!(!ht.is_ready());
        assert!(ht.value().is_none());
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
        let mut ht = HilbertDominantCycle::new();
        let out = ht.batch(&sine_prices(120));
        let warmup = ht.warmup_period();
        assert!(out[..warmup - 1].iter().all(Option::is_none));
        assert!(out[warmup - 1].is_some());
    }

    #[test]
    fn reset_replays_identically() {
        let prices = sine_prices(150);
        let fresh = HilbertDominantCycle::new().batch(&prices);
        let mut ht = HilbertDominantCycle::new();
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
        HilbertDominantCycle::new().batch_nan_into(&prices, &mut out);
        let nan = HilbertDominantCycle::new().batch_nan(&prices);
        let fast = HilbertDominantCycle::new().batch_fast(&prices);
        let mut stream = HilbertDominantCycle::new();
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
        let mut ht = HilbertDominantCycle::new();
        // After exactly 4 inputs the WMA is (4*40 + 3*30 + 2*20 + 10) / 10 = 30.
        for p in [10.0, 20.0, 30.0, 40.0] {
            assert_eq!(ht.update(p), None);
        }
        assert_eq!(ht.smooth_buf, vec![30.0]);
        assert_eq!(ht.detrender_buf.len(), 0);

        // A single spike of 10 at index 7 in an all-zero series gives smoothed
        // values 4, 3, 2 at indices 7, 8, 9, so at index 9 the smooth history is
        // [2, 3, 4, 0, 0, 0, 0]. With the period seed of 6, adj = 0.075*6 + 0.54
        // = 0.99, and the detrender reads the SMOOTHED taps s0, s2, s4, s6:
        //   (0.0962*2 + 0.5769*4 - 0.5769*0 - 0.0962*0) * 0.99 = 2.5 * 0.99 = 2.475.
        // Raw-price taps would instead give 0.5769*10*0.99 = 5.711_31.
        let mut ht = HilbertDominantCycle::new();
        let mut series = [0.0; 10];
        series[7] = 10.0;
        let _ = ht.batch(&series);
        assert_eq!(ht.smooth_buf, vec![2.0, 3.0, 4.0, 0.0, 0.0, 0.0, 0.0]);
        assert_eq!(ht.detrender_buf.len(), 1);
        assert_relative_eq!(ht.detrender_buf[0], 2.475, epsilon = 1e-12);
    }

    #[test]
    fn constant_input_period_settles_at_lower_clamp() {
        // A flat zero series makes re == im == 0, so the period falls back to
        // its previous value, is clamped up to 6, and the EMA chain converges
        // on 6 from below.
        let mut ht = HilbertDominantCycle::new();
        let out = ht.batch(&[0.0; 400]);
        assert!(out
            .iter()
            .flatten()
            .all(|v| v.is_finite() && *v <= 6.0 + 1e-9));
        assert_relative_eq!(ht.value().unwrap(), 6.0, epsilon = 1e-9);

        // A non-zero flat series stays finite and inside the clamp band.
        let mut ht = HilbertDominantCycle::new();
        let out = ht.batch(&[100.0; 400]);
        assert!(out.iter().flatten().all(|v| v.is_finite() && *v <= 50.0));
        assert_eq!(out.iter().flatten().count(), 400 - 49);
    }

    #[test]
    fn non_finite_input_during_warmup_does_not_advance() {
        let prices = sine_prices(120);
        let mut ht = HilbertDominantCycle::new();
        let mut out = Vec::new();
        for (i, &p) in prices.iter().enumerate() {
            if i == 10 {
                assert_eq!(ht.update(f64::INFINITY), None);
            }
            out.push(ht.update(p));
        }
        assert_eq!(out, HilbertDominantCycle::new().batch(&prices));
    }
}
