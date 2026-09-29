//! Weighted Moving Average (linear weights).

use crate::error::{Error, Result};
use crate::traits::Indicator;

/// Weighted Moving Average with linear weights `1, 2, ..., period`.
///
/// Output is `sum(weight_i * price_i) / sum(weights)`. Maintained incrementally in
/// O(1) by keeping the rolling sum of values and the rolling weighted sum.
///
/// Both running sums are recomputed exactly from the live window every
/// `16 · period` steady-state updates, the same bound the SMA puts on drift.
/// Without it the weighted sum — updated as `W − S + period · x`, a difference of
/// quantities `period` times larger than the result — lost about 6e-10 of
/// relative accuracy over 500 000 updates of a price series.
///
/// # Example
///
/// ```
/// use wickra_core::{Indicator, Wma};
///
/// let mut indicator = Wma::new(3).unwrap();
/// let mut last = None;
/// for i in 0..80 {
///     last = indicator.update(100.0 + f64::from(i));
/// }
/// assert!(last.is_some());
/// ```
#[derive(Debug, Clone)]
pub struct Wma {
    period: usize,
    /// Ring buffer of the last `period` finite inputs; `head` is the next slot
    /// to write and, once full, the oldest value. A flat buffer with a write
    /// cursor instead of a `VecDeque`: no per-update bookkeeping on the hot path.
    buf: Box<[f64]>,
    head: usize,
    /// Slots filled, saturating at `period`.
    count: usize,
    weight_sum: f64, // sum_i (weight_i * value_i)
    value_sum: f64,  // sum_i (value_i)
    weights_total: f64,
    /// Steady-state updates since the running sums were last recomputed.
    updates_since_reseed: usize,
}

/// Recompute the running sums every `RESEED_EVERY * period` steady-state
/// updates (the SMA's cadence).
const RESEED_EVERY: usize = 16;

impl Wma {
    /// Construct a new WMA with the given window length.
    ///
    /// # Errors
    ///
    /// Returns [`Error::PeriodZero`] if `period == 0`.
    pub fn new(period: usize) -> Result<Self> {
        if period == 0 {
            return Err(Error::PeriodZero);
        }
        if period > crate::error::MAX_PERIOD {
            return Err(Error::InvalidPeriod {
                message: crate::error::PERIOD_ABOVE_MAX,
            });
        }
        let n = period as f64;
        let weights_total = n * (n + 1.0) / 2.0;
        Ok(Self {
            period,
            buf: vec![0.0; period].into_boxed_slice(),
            head: 0,
            count: 0,
            weight_sum: 0.0,
            value_sum: 0.0,
            weights_total,
            updates_since_reseed: 0,
        })
    }

    /// Configured period.
    pub const fn period(&self) -> usize {
        self.period
    }


    /// Current value if available.
    pub fn value(&self) -> Option<f64> {
        if self.count == self.period {
            Some(self.weight_sum / self.weights_total)
        } else {
            None
        }
    }

    /// The weighted sum `Σ (k + 1) · x_k` over the window in chronological
    /// order (oldest first, weight 1).
    fn weighted_window_sum(&self) -> f64 {
        self.buf[self.head..]
            .iter()
            .chain(&self.buf[..self.head])
            .enumerate()
            .map(|(i, v)| (i as f64 + 1.0) * v)
            .sum()
    }
}

impl Indicator for Wma {
    type Input = f64;
    type Output = f64;

    #[inline]
    fn update(&mut self, input: f64) -> Option<f64> {
        if !input.is_finite() {
            return None;
        }
        if self.count < self.period {
            // Warmup. Just accumulate; compute weight_sum once when the window first
            // becomes full to avoid having to track changing weights during warmup.
            self.buf[self.head] = input;
            self.head += 1;
            if self.head == self.period {
                self.head = 0;
            }
            self.value_sum += input;
            self.count += 1;
            if self.count == self.period {
                self.weight_sum = self.weighted_window_sum();
            }
            return self.value();
        }
        // Steady state: slide the window. With weights [1, 2, ..., period],
        //   new_weight_sum = old_weight_sum - old_value_sum + period * new_input
        // because every retained element's weight drops by one and the newcomer
        // enters at weight = period. Order matters: subtract `value_sum` BEFORE
        // updating it.
        let oldest = self.buf[self.head];
        self.weight_sum = self.weight_sum - self.value_sum + self.period as f64 * input;
        self.value_sum = self.value_sum - oldest + input;
        self.buf[self.head] = input;
        self.head += 1;
        if self.head == self.period {
            self.head = 0;
        }
        self.updates_since_reseed += 1;
        if self.updates_since_reseed >= RESEED_EVERY * self.period {
            self.value_sum = self.buf[self.head..]
                .iter()
                .chain(&self.buf[..self.head])
                .sum();
            self.weight_sum = self.weighted_window_sum();
            self.updates_since_reseed = 0;
        }
        self.value()
    }

    fn reset(&mut self) {
        self.head = 0;
        self.count = 0;
        self.weight_sum = 0.0;
        self.value_sum = 0.0;
        self.updates_since_reseed = 0;
    }

    #[inline]
    fn warmup_period(&self) -> usize {
        self.period
    }

    #[inline]
    fn is_ready(&self) -> bool {
        self.count == self.period
    }

    #[inline]
    fn name(&self) -> &'static str {
        "WMA"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;
    use approx::assert_relative_eq;

    /// Over many reseed intervals the WMA must stay at its definition
    /// (`Σ (k + 1) · x_k / Σ weights` over the live window) to a few ulps; the
    /// incremental weighted sum alone drifted to ~1e-10 on such a series.
    #[test]
    fn long_stream_drift_stays_bounded() {
        let period = 14;
        let mut wma = Wma::new(period).unwrap();
        let xs: Vec<f64> = (0..40_000)
            .map(|i| {
                let t = f64::from(i);
                100.0 + (t * 0.0137).sin() * 5.0 + (t * 0.37).cos() + (t * 0.0011).sin() * 20.0
            })
            .collect();
        let total = (period * (period + 1) / 2) as f64;
        for (i, &x) in xs.iter().enumerate() {
            let got = wma.update(x);
            if i + 1 >= period && i % 509 == 0 {
                let def: f64 = xs[i + 1 - period..=i]
                    .iter()
                    .enumerate()
                    .map(|(k, v)| (k as f64 + 1.0) * v)
                    .sum::<f64>()
                    / total;
                let got = got.unwrap();
                assert!(((got - def) / def).abs() < 1e-13, "at {i}: {got} vs {def}");
            }
        }
    }

    /// Until the first reseed the ring buffer computes exactly what the old
    /// sliding sums did: warmup accumulation, then `W − S + p·x`.
    #[test]
    fn matches_the_incremental_form_before_the_first_reseed() {
        let period = 5;
        let xs: Vec<f64> = (0..70).map(|i| f64::from(i % 7) * 1.5 + 3.25).collect();
        let mut wma = Wma::new(period).unwrap();
        let (mut value_sum, mut weight_sum) = (0.0_f64, 0.0_f64);
        for (i, &x) in xs.iter().enumerate() {
            let got = wma.update(x);
            if i < period {
                value_sum += x;
                if i + 1 == period {
                    weight_sum = xs[..period]
                        .iter()
                        .enumerate()
                        .map(|(k, v)| (k as f64 + 1.0) * v)
                        .sum();
                }
            } else {
                weight_sum = weight_sum - value_sum + period as f64 * x;
                value_sum = value_sum - xs[i - period] + x;
            }
            if i + 1 >= period {
                assert_eq!(
                    got.unwrap().to_bits(),
                    (weight_sum / 15.0).to_bits(),
                    "at {i}"
                );
            }
        }
    }

    /// Reference implementation: explicit weighted average over a window.
    fn wma_naive(prices: &[f64], period: usize) -> Vec<Option<f64>> {
        let weights_total = (period as f64) * (period as f64 + 1.0) / 2.0;
        prices
            .iter()
            .enumerate()
            .map(|(i, _)| {
                if i + 1 < period {
                    None
                } else {
                    let window = &prices[i + 1 - period..=i];
                    let s: f64 = window
                        .iter()
                        .enumerate()
                        .map(|(j, p)| (j as f64 + 1.0) * p)
                        .sum();
                    Some(s / weights_total)
                }
            })
            .collect()
    }

    #[test]
    fn new_rejects_zero_period() {
        assert!(matches!(Wma::new(0), Err(Error::PeriodZero)));
    }

    /// Cover the const accessor `period` (56-58) and the Indicator-impl
    /// `warmup_period` (111-113) + `name` (119-121). Existing tests never
    /// inspect these metadata methods.
    #[test]
    fn accessors_and_metadata() {
        let wma = Wma::new(7).unwrap();
        assert_eq!(wma.period(), 7);
        assert_eq!(wma.warmup_period(), 7);
        assert_eq!(wma.name(), "WMA");
    }

    #[test]
    fn warmup_returns_none() {
        let mut wma = Wma::new(3).unwrap();
        assert_eq!(wma.update(1.0), None);
        assert_eq!(wma.update(2.0), None);
        // WMA(3) of [1,2,3]: oldest = 1 (weight 1), middle = 2 (weight 2), newest = 3 (weight 3)
        // -> (1*1 + 2*2 + 3*3) / (1+2+3) = 14/6
        assert_relative_eq!(wma.update(3.0).unwrap(), 14.0 / 6.0, epsilon = 1e-12);
    }

    #[test]
    fn known_values_period_4() {
        // WMA(4) weights 1,2,3,4 (total 10); inputs [1,2,3,4]:
        // (1*1 + 2*2 + 3*3 + 4*4) / 10 = (1+4+9+16)/10 = 30/10 = 3.0
        let mut wma = Wma::new(4).unwrap();
        let v = wma.batch(&[1.0, 2.0, 3.0, 4.0]);
        assert_relative_eq!(v[3].unwrap(), 3.0, epsilon = 1e-12);
    }

    #[test]
    fn matches_naive_over_random_inputs() {
        let prices: Vec<f64> = (1..=30).map(|i| f64::from(i) * 1.7 - 5.0).collect();
        let mut wma = Wma::new(7).unwrap();
        let got = wma.batch(&prices);
        let want = wma_naive(&prices, 7);
        for (i, (g, w)) in got.iter().zip(want.iter()).enumerate() {
            // Same warmup — emission shape must agree at every index.
            assert_eq!(g.is_some(), w.is_some(), "warmup mismatch at index {i}");
            if let (Some(a), Some(b)) = (g, w) {
                assert_relative_eq!(*a, *b, epsilon = 1e-9);
            }
        }
    }

    #[test]
    fn period_one_is_pass_through() {
        let mut wma = Wma::new(1).unwrap();
        assert_relative_eq!(wma.update(5.5).unwrap(), 5.5, epsilon = 1e-12);
        assert_relative_eq!(wma.update(7.5).unwrap(), 7.5, epsilon = 1e-12);
    }

    #[test]
    fn reset_clears_state() {
        let mut wma = Wma::new(4).unwrap();
        wma.batch(&[1.0, 2.0, 3.0, 4.0, 5.0]);
        assert!(wma.is_ready());
        wma.reset();
        assert!(!wma.is_ready());
        assert_eq!(wma.update(10.0), None);
    }

    #[test]
    fn batch_equals_streaming() {
        let prices: Vec<f64> = (1..=20).map(|i| f64::from(i) * 0.5).collect();
        let mut a = Wma::new(5).unwrap();
        let mut b = Wma::new(5).unwrap();
        assert_eq!(
            a.batch(&prices),
            prices.iter().map(|p| b.update(*p)).collect::<Vec<_>>()
        );
    }

    #[test]
    fn ignores_non_finite_input_but_keeps_state() {
        let mut wma = Wma::new(3).unwrap();
        wma.update(1.0);
        wma.update(2.0);
        wma.update(3.0).expect("WMA(3) ready after three inputs");
        // Non-finite inputs return the last value without mutating the window.
        assert_eq!(wma.update(f64::NAN), None);
        assert_eq!(wma.update(f64::INFINITY), None);
        // The window still holds 1, 2, 3 -> next real input slides it to 2, 3, 4.
        assert_relative_eq!(
            wma.update(4.0).unwrap(),
            (2.0 * 1.0 + 3.0 * 2.0 + 4.0 * 3.0) / 6.0,
            epsilon = 1e-12
        );
    }

    proptest::proptest! {
        #![proptest_config(proptest::test_runner::Config::with_cases(48))]
        #[test]
        fn proptest_matches_naive(
            period in 1usize..15,
            prices in proptest::collection::vec(-500.0_f64..500.0, 0..120),
        ) {
            let mut wma = Wma::new(period).unwrap();
            let got = wma.batch(&prices);
            let want = wma_naive(&prices, period);
            proptest::prop_assert_eq!(got.len(), want.len());
            for (g, w) in got.iter().zip(want.iter()) {
                match (g, w) {
                    (None, None) => {}
                    (Some(a), Some(b)) => proptest::prop_assert!(
                        (a - b).abs() < 1e-7,
                        "got={a} want={b}"
                    ),
                    _ => proptest::prop_assert!(false, "warmup mismatch"),
                }
            }
        }
    }
}
