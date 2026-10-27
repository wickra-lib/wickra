//! RSX — Jurik-style smoothed RSI.

use crate::error::{Error, Result};
use crate::traits::Indicator;

/// RSX — a noise-free RSI built from Jurik's three-stage smoothing cascade.
///
/// Where Wilder's [`Rsi`](crate::Rsi) smooths the up/down moves with a single
/// EMA, the RSX runs the signed price change *and* its absolute value through
/// three cascaded "double-EMA with overshoot" stages (each stage is
/// `x = 1.5·a − 0.5·b`, the same lag-cancelling trick as a DEMA), then forms the
/// RSI-style ratio from the two smoothed streams:
///
/// ```text
/// f18 = 3 / (length + 2),  f20 = 1 - f18
/// each stage: a = f20·a + f18·in;  b = f18·a + f20·b;  out = 1.5·a − 0.5·b
/// v14 = stage3(signed change),  v1C = stage3(|change|)
/// RSX = clamp((v14 / v1C + 1) · 50, 0, 100)        (50 when v1C == 0)
/// ```
///
/// The result is an oscillator in `[0, 100]` that tracks the RSI but is far
/// smoother for the same responsiveness — it has very little of the RSI's
/// bar-to-bar jitter, so threshold crosses and divergences are cleaner. A flat
/// market returns the neutral `50`.
///
/// # Example
///
/// ```
/// use wickra_core::{Indicator, Rsx};
///
/// let mut indicator = Rsx::new(14).unwrap();
/// let mut last = None;
/// for i in 0..80 {
///     last = indicator.update(100.0 + (f64::from(i) * 0.2).sin() * 5.0);
/// }
/// assert!(last.is_some());
/// ```
#[derive(Debug, Clone)]
pub struct Rsx {
    length: usize,
    f18: f64,
    f20: f64,
    prev: Option<f64>,
    count: usize,
    // Signed-change cascade (three stages: a/b pairs).
    s_a0: f64,
    s_b0: f64,
    s_a1: f64,
    s_b1: f64,
    s_a2: f64,
    s_b2: f64,
    // Absolute-change cascade.
    a_a0: f64,
    a_b0: f64,
    a_a1: f64,
    a_b1: f64,
    a_a2: f64,
    a_b2: f64,
    last_value: Option<f64>,
}

impl Rsx {
    /// Construct an RSX with the given smoothing length.
    ///
    /// # Errors
    ///
    /// Returns [`Error::PeriodZero`] if `length == 0`.
    pub fn new(length: usize) -> Result<Self> {
        if length == 0 {
            return Err(Error::PeriodZero);
        }
        if length > crate::error::MAX_PERIOD {
            return Err(Error::InvalidPeriod {
                message: crate::error::PERIOD_ABOVE_MAX,
            });
        }
        let f18 = 3.0 / (length as f64 + 2.0);
        Ok(Self {
            length,
            f18,
            f20: 1.0 - f18,
            prev: None,
            count: 0,
            s_a0: 0.0,
            s_b0: 0.0,
            s_a1: 0.0,
            s_b1: 0.0,
            s_a2: 0.0,
            s_b2: 0.0,
            a_a0: 0.0,
            a_b0: 0.0,
            a_a1: 0.0,
            a_b1: 0.0,
            a_a2: 0.0,
            a_b2: 0.0,
            last_value: None,
        })
    }

    /// Configured length.
    pub const fn length(&self) -> usize {
        self.length
    }

    /// Current value if available.
    pub const fn value(&self) -> Option<f64> {
        self.last_value
    }

    /// One double-EMA-with-overshoot stage: updates the `(a, b)` pair in place
    /// and returns `1.5·a − 0.5·b`.
    fn stage(&self, a: &mut f64, b: &mut f64, input: f64) -> f64 {
        *a = self.f20 * *a + self.f18 * input;
        *b = self.f18 * *a + self.f20 * *b;
        1.5 * *a - 0.5 * *b
    }
}

impl Indicator for Rsx {
    type Input = f64;
    type Output = f64;

    fn update(&mut self, price: f64) -> Option<f64> {
        if !price.is_finite() {
            return None;
        }
        let Some(prev) = self.prev else {
            self.prev = Some(price);
            return None;
        };
        self.prev = Some(price);

        let change = price - prev;

        // Signed-change cascade.
        let (mut sa0, mut sb0) = (self.s_a0, self.s_b0);
        let v_c = self.stage(&mut sa0, &mut sb0, change);
        self.s_a0 = sa0;
        self.s_b0 = sb0;
        let (mut sa1, mut sb1) = (self.s_a1, self.s_b1);
        let v_10 = self.stage(&mut sa1, &mut sb1, v_c);
        self.s_a1 = sa1;
        self.s_b1 = sb1;
        let (mut sa2, mut sb2) = (self.s_a2, self.s_b2);
        let v_14 = self.stage(&mut sa2, &mut sb2, v_10);
        self.s_a2 = sa2;
        self.s_b2 = sb2;

        // Absolute-change cascade.
        let abs = change.abs();
        let (mut aa0, mut ab0) = (self.a_a0, self.a_b0);
        let v_c1 = self.stage(&mut aa0, &mut ab0, abs);
        self.a_a0 = aa0;
        self.a_b0 = ab0;
        let (mut aa1, mut ab1) = (self.a_a1, self.a_b1);
        let v_18 = self.stage(&mut aa1, &mut ab1, v_c1);
        self.a_a1 = aa1;
        self.a_b1 = ab1;
        let (mut aa2, mut ab2) = (self.a_a2, self.a_b2);
        let v_1c = self.stage(&mut aa2, &mut ab2, v_18);
        self.a_a2 = aa2;
        self.a_b2 = ab2;

        let v4 = if v_1c > 0.0 {
            (v_14 / v_1c + 1.0) * 50.0
        } else {
            50.0
        };
        let rsx = v4.clamp(0.0, 100.0);

        self.count += 1;
        self.last_value = Some(rsx);
        if self.count >= self.length {
            Some(rsx)
        } else {
            None
        }
    }

    fn reset(&mut self) {
        *self = Self::new(self.length).expect("length already validated");
    }

    #[inline]
    fn warmup_period(&self) -> usize {
        // One input to seed `prev`, then `length` changes to settle the cascade.
        self.length + 1
    }

    #[inline]
    fn is_ready(&self) -> bool {
        self.count >= self.length
    }

    #[inline]
    fn name(&self) -> &'static str {
        "RSX"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;
    use approx::assert_relative_eq;

    #[test]
    fn rejects_zero_length() {
        assert!(matches!(Rsx::new(0), Err(Error::PeriodZero)));
    }

    /// Cover the const accessors `length` + `value` and the Indicator-impl
    /// `warmup_period` + `name`.
    #[test]
    fn accessors_and_metadata() {
        let rsx = Rsx::new(14).unwrap();
        assert_eq!(rsx.length(), 14);
        assert_eq!(rsx.value(), None);
        assert_eq!(rsx.warmup_period(), 15);
        assert_eq!(rsx.name(), "RSX");
    }

    #[test]
    fn warmup_then_emits() {
        let mut rsx = Rsx::new(3).unwrap();
        // 1 input seeds prev; then 3 changes settle -> first Some on input 4.
        assert_eq!(rsx.update(10.0), None);
        assert_eq!(rsx.update(11.0), None);
        assert_eq!(rsx.update(12.0), None);
        assert!(rsx.update(13.0).is_some());
    }

    #[test]
    fn flat_market_is_neutral() {
        // No movement -> absolute cascade is zero -> neutral 50.
        let mut rsx = Rsx::new(5).unwrap();
        let last = rsx.batch(&[7.0; 40]).into_iter().flatten().last().unwrap();
        assert_relative_eq!(last, 50.0, epsilon = 1e-12);
    }

    #[test]
    fn output_stays_in_range() {
        let prices: Vec<f64> = (0..120)
            .map(|i| 100.0 + (f64::from(i) * 0.35).sin() * 12.0)
            .collect();
        let mut rsx = Rsx::new(14).unwrap();
        for v in rsx.batch(&prices).into_iter().flatten() {
            assert!((0.0..=100.0).contains(&v), "RSX {v} left [0, 100]");
        }
    }

    #[test]
    fn strong_uptrend_is_high() {
        // A sustained rise drives RSX well above the neutral 50.
        let prices: Vec<f64> = (1..=60).map(f64::from).collect();
        let mut rsx = Rsx::new(14).unwrap();
        let last = rsx.batch(&prices).into_iter().flatten().last().unwrap();
        assert!(
            last > 80.0,
            "strong uptrend should push RSX high, got {last}"
        );
    }

    #[test]
    fn ignores_non_finite_input() {
        let mut rsx = Rsx::new(3).unwrap();
        let _ready = rsx
            .batch(&[1.0, 2.0, 3.0, 4.0, 5.0])
            .into_iter()
            .flatten()
            .last()
            .unwrap();
        assert_eq!(rsx.update(f64::NAN), None);
        assert_eq!(rsx.update(f64::INFINITY), None);
    }

    #[test]
    fn reset_clears_state() {
        let mut rsx = Rsx::new(5).unwrap();
        rsx.batch(&(1..=40).map(f64::from).collect::<Vec<_>>());
        assert!(rsx.is_ready());
        rsx.reset();
        assert!(!rsx.is_ready());
        assert_eq!(rsx.update(1.0), None);
    }

    #[test]
    fn batch_equals_streaming() {
        let prices: Vec<f64> = (1..=60)
            .map(|i| 50.0 + (f64::from(i) * 0.5).sin() * 10.0)
            .collect();
        let mut a = Rsx::new(14).unwrap();
        let mut b = Rsx::new(14).unwrap();
        assert_eq!(
            a.batch(&prices),
            prices.iter().map(|p| b.update(*p)).collect::<Vec<_>>()
        );
    }

    #[test]
    fn rejects_length_above_maximum() {
        assert!(matches!(
            Rsx::new(crate::error::MAX_PERIOD + 1),
            Err(Error::InvalidPeriod { .. })
        ));
    }
}
