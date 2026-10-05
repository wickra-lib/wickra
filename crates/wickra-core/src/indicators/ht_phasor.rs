//! Ehlers Hilbert Transform Phasor components (`HT_PHASOR`).
#![allow(clippy::manual_clamp)]

use std::f64::consts::PI;

use crate::traits::Indicator;

/// In-phase and quadrature components of the Hilbert transform phasor.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HtPhasorOutput {
    /// In-phase component (`I1`).
    pub inphase: f64,
    /// Quadrature component (`Q1`).
    pub quadrature: f64,
}

/// Ehlers' Hilbert Transform Phasor (`HT_PHASOR`).
///
/// Runs the same adaptive Hilbert-transform engine as
/// [`HilbertDominantCycle`](crate::HilbertDominantCycle) but reports the raw
/// in-phase (`I1`) and quadrature (`Q1`) components of the analytic signal rather
/// than the recovered cycle period. The two components are 90° out of phase, so
/// their ratio tracks the instantaneous phase of the dominant cycle.
///
/// From *Rocket Science for Traders* (Ehlers 2001), aligned with TA-Lib's
/// `HT_PHASOR`. The first value is emitted once the transform's tap buffers fill.
///
/// # Example
///
/// ```
/// use wickra_core::{Indicator, HtPhasor};
///
/// let mut ht = HtPhasor::new();
/// let mut last = None;
/// for i in 0..120 {
///     last = ht.update(100.0 + (f64::from(i) * 0.4).sin() * 5.0);
/// }
/// assert!(last.is_some());
/// ```
#[derive(Debug, Clone, Default)]
pub struct HtPhasor {
    // Raw input window for the 4-bar WMA.
    price_buf: Vec<f64>,
    // WMA-smoothed price history feeding the Hilbert detrender taps.
    smooth_buf: Vec<f64>,
    detrender_buf: Vec<f64>,
    q1_buf: Vec<f64>,
    i1_buf: Vec<f64>,
    prev_i2: f64,
    prev_q2: f64,
    prev_re: f64,
    prev_im: f64,
    prev_period: f64,
    ready: bool,
}

impl HtPhasor {
    /// Construct a new Hilbert transform phasor.
    pub fn new() -> Self {
        Self::default()
    }

    fn push_front(buf: &mut Vec<f64>, v: f64, cap: usize) {
        buf.insert(0, v);
        if buf.len() > cap {
            buf.truncate(cap);
        }
    }
}

impl Indicator for HtPhasor {
    type Input = f64;
    type Output = HtPhasorOutput;

    fn update(&mut self, input: f64) -> Option<HtPhasorOutput> {
        if !input.is_finite() {
            return None;
        }

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

        // Continue the dominant-cycle period adaptation so the next bar's `adj`
        // coefficient tracks the cycle, exactly as TA-Lib's HT_PHASOR does.
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

        self.ready = true;
        Some(HtPhasorOutput {
            inphase: i1,
            quadrature: q1,
        })
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
        self.ready = false;
    }

    #[inline]
    fn warmup_period(&self) -> usize {
        22
    }

    #[inline]
    fn is_ready(&self) -> bool {
        self.ready
    }

    #[inline]
    fn name(&self) -> &'static str {
        "HT_PHASOR"
    }
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
        let ht = HtPhasor::new();
        assert_eq!(ht.warmup_period(), 22);
        assert_eq!(ht.name(), "HT_PHASOR");
        assert!(!ht.is_ready());
    }

    #[test]
    fn emits_after_warmup_and_stays_finite() {
        let mut ht = HtPhasor::new();
        let out: Vec<Option<HtPhasorOutput>> = ht.batch(&sine_prices(120));
        assert_eq!(out[0], None);
        let first = out.iter().position(Option::is_some).expect("emits");
        assert!(first <= 21, "first phasor at index {first}");
        for o in out.into_iter().flatten() {
            assert!(o.inphase.is_finite() && o.quadrature.is_finite());
        }
        assert!(ht.is_ready());
    }

    #[test]
    fn ignores_non_finite_input() {
        let mut ht = HtPhasor::new();
        let _ = ht.batch(&sine_prices(120));
        // A non-finite input is skipped and produces no value.
        assert_eq!(ht.update(f64::NAN), None);
    }

    #[test]
    fn batch_equals_streaming() {
        let prices = sine_prices(150);
        let mut a = HtPhasor::new();
        let mut b = HtPhasor::new();
        let batch = a.batch(&prices);
        let streamed: Vec<_> = prices.iter().map(|p| b.update(*p)).collect();
        assert_eq!(batch, streamed);
    }

    #[test]
    fn reset_clears_state() {
        let mut ht = HtPhasor::new();
        let _ = ht.batch(&sine_prices(120));
        assert!(ht.is_ready());
        ht.reset();
        assert!(!ht.is_ready());
        assert_eq!(ht.update(100.0), None);
    }

    use approx::assert_relative_eq;

    #[test]
    fn first_value_lands_exactly_at_warmup() {
        // 4 inputs fill the WMA, 7 smoothed values the detrender taps, 7
        // detrender values the I/Q taps and 7 more of those the second Hilbert
        // pass: 3 + 6 + 6 + 6 = 21 -> first value at index 21.
        let mut ht = HtPhasor::new();
        let out = ht.batch(&sine_prices(60));
        let warmup = ht.warmup_period();
        assert_eq!(warmup, 22);
        assert!(out[..warmup - 1].iter().all(Option::is_none));
        assert!(out[warmup - 1].is_some());
    }

    #[test]
    fn reset_replays_identically() {
        let prices = sine_prices(150);
        let fresh = HtPhasor::new().batch(&prices);
        let mut ht = HtPhasor::new();
        let first = ht.batch(&prices);
        ht.reset();
        let second = ht.batch(&prices);
        assert_eq!(first, fresh);
        assert_eq!(second, fresh);
    }

    #[test]
    fn wma_of_raw_inputs_feeds_detrender_taps() {
        let mut ht = HtPhasor::new();
        // After exactly 4 inputs the WMA is (4*40 + 3*30 + 2*20 + 10) / 10 = 30.
        for p in [10.0, 20.0, 30.0, 40.0] {
            assert_eq!(ht.update(p), None);
        }
        assert_eq!(ht.smooth_buf, vec![30.0]);

        // Spike of 10 at index 7 in a zero series: smoothed values 4, 3, 2 at
        // indices 7, 8, 9, so the smooth history is [2, 3, 4, 0, 0, 0, 0].
        // adj = 0.075*6 + 0.54 = 0.99 and the detrender reads the smoothed taps:
        //   (0.0962*2 + 0.5769*4 - 0.5769*0 - 0.0962*0) * 0.99 = 2.475.
        let mut ht = HtPhasor::new();
        let mut series = [0.0; 10];
        series[7] = 10.0;
        let _ = ht.batch(&series);
        assert_eq!(ht.smooth_buf, vec![2.0, 3.0, 4.0, 0.0, 0.0, 0.0, 0.0]);
        assert_eq!(ht.detrender_buf.len(), 1);
        assert_relative_eq!(ht.detrender_buf[0], 2.475, epsilon = 1e-12);
    }

    #[test]
    fn inphase_is_detrender_three_bars_back() {
        // I1 is the detrender delayed by 3 bars; Q1 is the Hilbert transform of
        // the detrender history scaled by the same adj.
        let mut ht = HtPhasor::new();
        let prices = sine_prices(60);
        let _ = ht.batch(&prices[..59]);
        let out = ht.update(prices[59]).unwrap();
        assert_eq!(out.inphase.to_bits(), ht.detrender_buf[3].to_bits());
        assert_eq!(out.quadrature.to_bits(), ht.q1_buf[0].to_bits());
    }

    #[test]
    fn constant_input_stays_finite_and_period_clamps_to_six() {
        // Zero input: every I/Q term is exactly 0, re == im == 0, and the period
        // falls back and is clamped up to 6 by the EMA chain.
        let mut ht = HtPhasor::new();
        let out = ht.batch(&[0.0; 200]);
        assert!(out
            .iter()
            .flatten()
            .all(|o| o.inphase.abs().to_bits() == 0 && o.quadrature.abs().to_bits() == 0));
        assert_relative_eq!(ht.prev_period, 6.0, epsilon = 1e-9);
        let mut ht = HtPhasor::new();
        let out = ht.batch(&[100.0; 200]);
        assert!(out
            .iter()
            .flatten()
            .all(|o| o.inphase.is_finite() && o.quadrature.is_finite()));
        assert_eq!(out.iter().flatten().count(), 200 - 21);
    }
}
