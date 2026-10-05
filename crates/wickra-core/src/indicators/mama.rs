//! Ehlers MESA Adaptive Moving Average (MAMA) and its follower (FAMA).
#![allow(
    clippy::doc_markdown,
    clippy::doc_lazy_continuation,
    clippy::struct_field_names,
    clippy::manual_clamp
)]

use std::f64::consts::PI;

use crate::error::{Error, Result};
use crate::traits::Indicator;

/// MAMA + FAMA output pair.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MamaOutput {
    /// MESA Adaptive Moving Average.
    pub mama: f64,
    /// Following Adaptive Moving Average (slower companion).
    pub fama: f64,
}

/// Ehlers' MESA Adaptive Moving Average (MAMA).
///
/// MAMA adapts its smoothing constant from the rate-of-change of price phase,
/// derived via a truncated Hilbert transform — full math in "Cycle Analytics
/// for Traders" (Ehlers 2013, ch. 8) and the original 2001 MESA paper.
///
/// The two-parameter `(fast_limit, slow_limit)` is the range over which the
/// adaptive alpha can vary; defaults `(0.5, 0.05)` match the canonical
/// EasyLanguage implementation. The companion FAMA is `mama * 0.5 * fast_limit
/// + fama_prev * (1 - 0.5 * fast_limit)`, lagging MAMA so crossovers signal
/// trend reversals.
///
/// The indicator emits both lines as a [`MamaOutput`]. Use the [`crate::Fama`] wrapper
/// in this module to expose just the slow line if needed (e.g. for chaining).
///
/// # Example
///
/// ```
/// use wickra_core::{Indicator, Mama};
///
/// let mut mama = Mama::new(0.5, 0.05).unwrap();
/// let mut last = None;
/// for i in 0..100 {
///     last = mama.update(100.0 + (f64::from(i) * 0.2).sin() * 5.0);
/// }
/// assert!(last.is_some());
/// ```
#[derive(Debug, Clone)]
pub struct Mama {
    fast_limit: f64,
    slow_limit: f64,
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
    prev_phase: f64,
    prev_mama: f64,
    prev_fama: f64,
    count: usize,
    last_value: Option<MamaOutput>,
}

impl Mama {
    /// Construct with custom `(fast_limit, slow_limit)` adaptive alpha bounds.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidPeriod`] if either limit is outside `(0, 1]`
    /// or if `slow_limit > fast_limit`.
    pub fn new(fast_limit: f64, slow_limit: f64) -> Result<Self> {
        if !fast_limit.is_finite()
            || !slow_limit.is_finite()
            || fast_limit <= 0.0
            || fast_limit > 1.0
            || slow_limit <= 0.0
            || slow_limit > 1.0
            || slow_limit > fast_limit
        {
            return Err(Error::InvalidPeriod {
                message: "fast_limit, slow_limit must satisfy 0 < slow_limit <= fast_limit <= 1",
            });
        }
        Ok(Self {
            fast_limit,
            slow_limit,
            price_buf: Vec::with_capacity(4),
            smooth_buf: Vec::with_capacity(7),
            detrender_buf: Vec::with_capacity(7),
            q1_buf: Vec::with_capacity(7),
            i1_buf: Vec::with_capacity(7),
            prev_i2: 0.0,
            prev_q2: 0.0,
            prev_re: 0.0,
            prev_im: 0.0,
            prev_period: 0.0,
            prev_phase: 0.0,
            prev_mama: 0.0,
            prev_fama: 0.0,
            count: 0,
            last_value: None,
        })
    }

    /// Default `(0.5, 0.05)` parameters from Ehlers' original publication.
    pub fn classic() -> Self {
        Self::new(0.5, 0.05).expect("classic MAMA limits are valid")
    }

    /// Configured `(fast_limit, slow_limit)`.
    pub const fn limits(&self) -> (f64, f64) {
        (self.fast_limit, self.slow_limit)
    }

    /// Current `(mama, fama)` pair if available.
    pub const fn value(&self) -> Option<MamaOutput> {
        self.last_value
    }

    fn push_front(buf: &mut Vec<f64>, v: f64, cap: usize) {
        buf.insert(0, v);
        if buf.len() > cap {
            buf.truncate(cap);
        }
    }
}

impl Indicator for Mama {
    type Input = f64;
    type Output = MamaOutput;

    fn update(&mut self, input: f64) -> Option<MamaOutput> {
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

        let period = self.prev_period.max(6.0).min(50.0);
        let adj = 0.075 * period + 0.54;

        if self.smooth_buf.len() < 7 {
            // Seed the EMA outputs with the smoothed price so early bars are
            // well-behaved without producing a public value.
            self.prev_mama = smooth;
            self.prev_fama = smooth;
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

        // Adaptive alpha derived from phase rate-of-change.
        let phase = if i1.abs() > f64::EPSILON {
            (q1 / i1).atan().to_degrees()
        } else {
            self.prev_phase
        };
        let mut delta_phase = self.prev_phase - phase;
        self.prev_phase = phase;
        if delta_phase < 1.0 {
            delta_phase = 1.0;
        }
        // `delta_phase` is clamped to >= 1.0 above, so `fast_limit / delta_phase`
        // never exceeds `fast_limit`; only the lower bound can bind.
        let mut alpha = self.fast_limit / delta_phase;
        if alpha < self.slow_limit {
            alpha = self.slow_limit;
        }

        self.prev_mama = alpha * input + (1.0 - alpha) * self.prev_mama;
        let fama_alpha = 0.5 * alpha;
        self.prev_fama = fama_alpha * self.prev_mama + (1.0 - fama_alpha) * self.prev_fama;

        if self.count < 33 {
            return None;
        }
        let out = MamaOutput {
            mama: self.prev_mama,
            fama: self.prev_fama,
        };
        self.last_value = Some(out);
        Some(out)
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
        self.prev_phase = 0.0;
        self.prev_mama = 0.0;
        self.prev_fama = 0.0;
        self.count = 0;
        self.last_value = None;
    }

    #[inline]
    fn warmup_period(&self) -> usize {
        33
    }

    #[inline]
    fn is_ready(&self) -> bool {
        self.last_value.is_some()
    }

    #[inline]
    fn name(&self) -> &'static str {
        "MAMA"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;

    #[test]
    fn rejects_invalid_limits() {
        assert!(matches!(
            Mama::new(0.0, 0.05),
            Err(Error::InvalidPeriod { .. })
        ));
        assert!(matches!(
            Mama::new(0.5, 0.0),
            Err(Error::InvalidPeriod { .. })
        ));
        assert!(matches!(
            Mama::new(0.05, 0.5),
            Err(Error::InvalidPeriod { .. })
        ));
        assert!(matches!(
            Mama::new(1.5, 0.05),
            Err(Error::InvalidPeriod { .. })
        ));
        assert!(matches!(
            Mama::new(f64::NAN, 0.05),
            Err(Error::InvalidPeriod { .. })
        ));
    }

    #[test]
    fn accessors_and_metadata() {
        let mut mama = Mama::classic();
        assert_eq!(mama.limits(), (0.5, 0.05));
        assert_eq!(mama.warmup_period(), 33);
        assert_eq!(mama.name(), "MAMA");
        assert!(!mama.is_ready());
        for i in 0..60 {
            mama.update(100.0 + (f64::from(i) * 0.3).sin() * 5.0);
        }
        assert!(mama.is_ready());
        assert!(mama.value().is_some());
    }

    #[test]
    fn fama_lags_or_equals_mama_on_constant_series() {
        let mut mama = Mama::classic();
        let out = mama.batch(&[100.0_f64; 200]);
        let last = out.iter().flatten().last().unwrap();
        // On a flat series both lines converge to the price.
        assert!((last.mama - 100.0).abs() < 1.0);
        assert!((last.fama - 100.0).abs() < 1.0);
    }

    #[test]
    fn batch_equals_streaming() {
        let prices: Vec<f64> = (0..120)
            .map(|i| 100.0 + (f64::from(i) * 0.25).sin() * 5.0)
            .collect();
        let mut a = Mama::classic();
        let mut b = Mama::classic();
        let batch = a.batch(&prices);
        let streamed: Vec<_> = prices.iter().map(|p| b.update(*p)).collect();
        assert_eq!(batch, streamed);
    }

    #[test]
    fn ignores_non_finite_input() {
        let mut mama = Mama::classic();
        let prices: Vec<f64> = (0..100)
            .map(|i| 100.0 + (f64::from(i) * 0.3).sin() * 5.0)
            .collect();
        mama.batch(&prices);
        let before = mama.value();
        assert!(before.is_some());
        assert_eq!(mama.update(f64::NAN), None);
    }

    #[test]
    fn reset_clears_state() {
        let mut mama = Mama::classic();
        let prices: Vec<f64> = (0..100)
            .map(|i| 100.0 + (f64::from(i) * 0.3).sin() * 5.0)
            .collect();
        mama.batch(&prices);
        assert!(mama.is_ready());
        mama.reset();
        assert!(!mama.is_ready());
    }

    #[test]
    fn flat_input_uses_phase_fallback() {
        // Zero inputs make every smooth/detrender term arithmetically exact
        // zero, so `i1 == 0.0` and the phase calculation takes the
        // `self.prev_phase` fallback rather than `atan(q1/i1)`. A non-zero
        // constant like `50.0` leaves a sub-EPSILON cancellation residue
        // that flips the branch back to the `atan` path on real hardware.
        let mut mama = Mama::classic();
        let out = mama.batch(&[0.0_f64; 200]);
        assert!(out.iter().flatten().count() > 100);
    }

    use approx::assert_relative_eq;

    fn sine_prices(n: u32) -> Vec<f64> {
        (0..n)
            .map(|i| 100.0 + (f64::from(i) * 0.3).sin() * 5.0)
            .collect()
    }

    #[test]
    fn rejects_non_finite_and_out_of_range_limits() {
        assert!(matches!(
            Mama::new(f64::INFINITY, 0.05),
            Err(Error::InvalidPeriod { .. })
        ));
        assert!(matches!(
            Mama::new(0.5, f64::NAN),
            Err(Error::InvalidPeriod { .. })
        ));
        assert!(matches!(
            Mama::new(1.0, 1.5),
            Err(Error::InvalidPeriod { .. })
        ));
        assert!(matches!(
            Mama::new(-0.5, -0.6),
            Err(Error::InvalidPeriod { .. })
        ));
        // Boundary values are accepted: slow == fast == 1.
        assert_eq!(Mama::new(1.0, 1.0).unwrap().limits(), (1.0, 1.0));
    }

    #[test]
    fn first_value_lands_exactly_at_warmup() {
        let mut mama = Mama::classic();
        let out = mama.batch(&sine_prices(80));
        let warmup = mama.warmup_period();
        assert!(out[..warmup - 1].iter().all(Option::is_none));
        assert!(out[warmup - 1].is_some());
    }

    #[test]
    fn reset_replays_identically() {
        let prices = sine_prices(150);
        let fresh = Mama::classic().batch(&prices);
        let mut mama = Mama::classic();
        let first = mama.batch(&prices);
        mama.reset();
        let second = mama.batch(&prices);
        assert_eq!(first, fresh);
        assert_eq!(second, fresh);
    }

    #[test]
    fn wma_of_raw_inputs_seeds_lines_and_feeds_detrender_taps() {
        let mut mama = Mama::classic();
        // After exactly 4 inputs the WMA is (4*40 + 3*30 + 2*20 + 10) / 10 = 30,
        // and while the taps fill both lines are seeded with it.
        for p in [10.0, 20.0, 30.0, 40.0] {
            assert_eq!(mama.update(p), None);
        }
        assert_eq!(mama.smooth_buf, vec![30.0]);
        assert_eq!(mama.prev_mama, 30.0);
        assert_eq!(mama.prev_fama, 30.0);

        // Spike of 10 at index 7 in a zero series: smoothed values 4, 3, 2 at
        // indices 7, 8, 9, so the smooth history is [2, 3, 4, 0, 0, 0, 0].
        // adj = 0.075*6 + 0.54 = 0.99 and the detrender reads the smoothed taps:
        //   (0.0962*2 + 0.5769*4 - 0.5769*0 - 0.0962*0) * 0.99 = 2.475.
        let mut mama = Mama::classic();
        let mut series = [0.0; 10];
        series[7] = 10.0;
        let _ = mama.batch(&series);
        assert_eq!(mama.smooth_buf, vec![2.0, 3.0, 4.0, 0.0, 0.0, 0.0, 0.0]);
        assert_eq!(mama.detrender_buf.len(), 1);
        assert_relative_eq!(mama.detrender_buf[0], 2.475, epsilon = 1e-12);
    }

    #[test]
    fn alpha_bounds_hand_computed() {
        // fast_limit == slow_limit == 1 pins alpha to 1 whatever the phase rate,
        // so MAMA equals the input and FAMA = 0.5*MAMA + 0.5*FAMA[-1].
        let mut mama = Mama::new(1.0, 1.0).unwrap();
        let prices = sine_prices(60);
        let out = mama.batch(&prices);
        let first = out[32].unwrap();
        let second = out[33].unwrap();
        assert_eq!(first.mama, prices[32]);
        assert_eq!(second.mama, prices[33]);
        assert_relative_eq!(
            second.fama,
            0.5 * prices[33] + 0.5 * first.fama,
            epsilon = 1e-12
        );
    }

    #[test]
    fn slow_limit_floor_binds_on_fast_phase_rotation() {
        // With fast_limit = 0.1 and slow_limit = 0.09 any phase step above
        // 0.1 / 0.09 = 1.11 degrees makes fast_limit / delta_phase drop below the
        // floor, which a fast cycle does on most bars. Both lines are convex
        // blends of the inputs, so they stay inside the input range.
        let mut mama = Mama::new(0.1, 0.09).unwrap();
        let prices: Vec<f64> = (0..200)
            .map(|i| 100.0 + (f64::from(i) * 2.5).sin() * 5.0)
            .collect();
        let out = mama.batch(&prices);
        assert!(out
            .iter()
            .flatten()
            .all(|o| (90.0..=110.0).contains(&o.mama) && (90.0..=110.0).contains(&o.fama)));
    }
}
