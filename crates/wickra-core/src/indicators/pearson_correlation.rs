//! Rolling Pearson correlation between two synchronised series.

use crate::error::{Error, Result};
use crate::indicators::rolling_moments::ShiftedPairMoments;
use crate::traits::Indicator;

/// Rolling Pearson correlation between two synchronised series.
///
/// Each `update` receives one `(x, y)` pair (e.g. the latest close of the
/// asset and of the benchmark). Over the trailing window of `period`
/// pairs:
///
/// ```text
/// cov_xy   = (1/n) · Σ x·y − x̄·ȳ
/// var_x    = (1/n) · Σ x² − x̄²
/// var_y    = (1/n) · Σ y² − ȳ²
/// Pearson  = cov_xy / √(var_x · var_y)
/// ```
///
/// Output is in `[−1, +1]`. `+1` means a perfect positive linear
/// relationship; `−1` is a perfect inverse one; `0` means no linear
/// relationship. It is the same statistic `SciPy` / `NumPy` report as
/// `pearsonr` and the standardised relative of [`crate::Beta`] — Beta
/// scales Pearson by the ratio of standard deviations.
///
/// Each `update` is O(1): five running sums (`Σx`, `Σy`, `Σx²`, `Σy²`,
/// `Σxy`) are maintained as the window slides. A flat series in either
/// channel gives an undefined ratio; the indicator returns `0` in that
/// case rather than producing `NaN`. The output is clamped to `[−1, +1]`
/// to absorb tiny floating-point overshoots near the boundaries.
///
/// # Example
///
/// ```
/// use wickra_core::{Indicator, PearsonCorrelation};
///
/// let mut indicator = PearsonCorrelation::new(20).unwrap();
/// let mut last = None;
/// for i in 0..40 {
///     last = indicator.update((f64::from(i), 2.0 * f64::from(i) + 1.0));
/// }
/// // A perfectly linear pair → +1.
/// assert!((last.unwrap() - 1.0).abs() < 1e-9);
/// ```
#[derive(Debug, Clone)]
pub struct PearsonCorrelation {
    period: usize,
    /// Ring buffer of the last `period` pairs; `head` is the next slot to write
    /// and, once full, the oldest pair.
    buf: Box<[(f64, f64)]>,
    head: usize,
    /// Pairs held, saturating at `period`.
    count: usize,
    moments: ShiftedPairMoments,
}

impl PearsonCorrelation {
    /// Construct a new rolling Pearson correlation.
    ///
    /// # Errors
    /// Returns [`Error::InvalidPeriod`] if `period < 2` — correlation is
    /// undefined for fewer than two pairs.
    pub fn new(period: usize) -> Result<Self> {
        if period < 2 {
            return Err(Error::InvalidPeriod {
                message: "pearson correlation needs period >= 2",
            });
        }
        if period > crate::error::MAX_PERIOD {
            return Err(Error::InvalidPeriod {
                message: crate::error::PERIOD_ABOVE_MAX,
            });
        }
        Ok(Self {
            period,
            buf: vec![(0.0, 0.0); period].into_boxed_slice(),
            head: 0,
            count: 0,
            moments: ShiftedPairMoments::new(),
        })
    }

    /// Configured period.
    pub const fn period(&self) -> usize {
        self.period
    }
}

impl PearsonCorrelation {
    /// Exact batch over two columns: one output per pair (`NaN` during warmup),
    /// bit for bit what replaying `update` gives, written into `out`.
    ///
    /// # Panics
    ///
    /// Panics if `a`, `b` and `out` differ in length.
    pub fn batch_pairs_into(&mut self, a: &[f64], b: &[f64], out: &mut [f64]) {
        assert!(
            a.len() == b.len() && out.len() == a.len(),
            "both series and the output must be equal length"
        );
        for ((slot, &x), &y) in out.iter_mut().zip(a).zip(b) {
            *slot = self.update((x, y)).unwrap_or(f64::NAN);
        }
    }

    /// Opt-in fast variant of [`batch_pairs_into`](Self::batch_pairs_into):
    /// the shifted sums of `a`, `b`, `a²`, `b²` and `a·b` run as SIMD
    /// prefix scans, re-centred every window like the exact accumulator, and
    /// the correlation is finished lane-parallel. Every value
    /// agrees with the exact batch to within a few units in the last place;
    /// warmup `NaN`s and length are identical, and the result is the same on
    /// every platform. Only a fresh indicator over finite values within
    /// `1e100`, at least one window long, takes the kernel; anything else is
    /// the exact batch. The correlation only remembers its last `period` pairs,
    /// so afterwards the state is rebuilt exactly by replaying them.
    ///
    /// # Panics
    ///
    /// Panics if `a`, `b` and `out` differ in length.
    pub fn batch_pairs_fast_into(&mut self, a: &[f64], b: &[f64], out: &mut [f64]) {
        assert!(
            a.len() == b.len() && out.len() == a.len(),
            "both series and the output must be equal length"
        );
        let p = self.period;
        let n = a.len();
        if self.count != 0 || n < p || !crate::fast::in_range(a) || !crate::fast::in_range(b) {
            self.batch_pairs_into(a, b, out);
            return;
        }
        crate::fast::with_scratch(crate::fast::power_scratch_len(5, p), |scratch| {
            wickra_simd::dispatch(crate::fast::PearsonFast {
                a,
                b,
                period: p,
                scratch,
                out,
                _borrow: std::marker::PhantomData,
            });
        });
        self.reset();
        for (&x, &y) in a[n - p..].iter().zip(&b[n - p..]) {
            let _ = self.update((x, y));
        }
    }
}

impl Indicator for PearsonCorrelation {
    type Input = (f64, f64);
    type Output = f64;

    #[inline]
    fn update(&mut self, input: (f64, f64)) -> Option<f64> {
        let (x, y) = input;
        if !x.is_finite() || !y.is_finite() {
            return None;
        }
        // One indexed slot for both the evicted pair and the new one.
        let slot = &mut self.buf[self.head];
        if self.count == self.period {
            let (ox, oy) = std::mem::replace(slot, (x, y));
            self.moments.evict(ox, oy);
        } else {
            *slot = (x, y);
            self.count += 1;
        }
        self.head += 1;
        if self.head == self.period {
            self.head = 0;
        }
        self.moments.push(x, y);
        if self.moments.needs_reseed(self.period) {
            // Chronological order: oldest at `head` once full, `buf[..count]`
            // while still warming up.
            let (older, newer) = if self.count == self.period {
                (&self.buf[self.head..], &self.buf[..self.head])
            } else {
                (&self.buf[..self.count], &self.buf[..0])
            };
            self.moments.reseed(older.iter().chain(newer).copied());
        }
        if self.count < self.period {
            return None;
        }
        let var_x = self.moments.var_a(self.period);
        let var_y = self.moments.var_b(self.period);
        let cov = self.moments.cov(self.period);
        let denom = (var_x * var_y).sqrt();
        if denom == 0.0 {
            // At least one channel is flat: correlation is undefined.
            return Some(0.0);
        }
        Some((cov / denom).clamp(-1.0, 1.0))
    }

    fn reset(&mut self) {
        self.head = 0;
        self.count = 0;
        self.moments.reset();
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
        "PearsonCorrelation"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;
    use approx::assert_relative_eq;

    #[test]
    fn rejects_period_below_two() {
        assert!(PearsonCorrelation::new(0).is_err());
        assert!(PearsonCorrelation::new(1).is_err());
        assert!(PearsonCorrelation::new(2).is_ok());
    }

    #[test]
    fn accessors_and_metadata() {
        let p = PearsonCorrelation::new(14).unwrap();
        assert_eq!(p.period(), 14);
        assert_eq!(p.warmup_period(), 14);
        assert_eq!(p.name(), "PearsonCorrelation");
    }

    #[test]
    fn perfect_positive_is_one() {
        let pairs: Vec<(f64, f64)> = (0..10)
            .map(|i| (f64::from(i), 3.0 * f64::from(i) + 1.0))
            .collect();
        let last = PearsonCorrelation::new(5)
            .unwrap()
            .batch(&pairs)
            .into_iter()
            .flatten()
            .last()
            .unwrap();
        assert_relative_eq!(last, 1.0, epsilon = 1e-9);
    }

    #[test]
    fn perfect_negative_is_minus_one() {
        let pairs: Vec<(f64, f64)> = (0..10)
            .map(|i| (f64::from(i), -2.0 * f64::from(i) + 5.0))
            .collect();
        let last = PearsonCorrelation::new(5)
            .unwrap()
            .batch(&pairs)
            .into_iter()
            .flatten()
            .last()
            .unwrap();
        assert_relative_eq!(last, -1.0, epsilon = 1e-9);
    }

    #[test]
    fn constant_channel_yields_zero() {
        let pairs: Vec<(f64, f64)> = (0..10).map(|i| (f64::from(i), 7.0)).collect();
        let last = PearsonCorrelation::new(5)
            .unwrap()
            .batch(&pairs)
            .into_iter()
            .flatten()
            .last()
            .unwrap();
        assert_relative_eq!(last, 0.0, epsilon = 1e-12);
    }

    #[test]
    fn output_in_minus_one_to_one_range() {
        let pairs: Vec<(f64, f64)> = (0..60)
            .map(|i| {
                let t = f64::from(i);
                (100.0 + t.sin() * 5.0, 50.0 + (t * 0.3).cos() * 3.0)
            })
            .collect();
        let mut p = PearsonCorrelation::new(20).unwrap();
        for v in p.batch(&pairs).into_iter().flatten() {
            assert!((-1.0..=1.0).contains(&v));
        }
    }

    #[test]
    fn reset_clears_state() {
        let mut p = PearsonCorrelation::new(5).unwrap();
        p.batch(&[(1.0, 2.0), (2.0, 4.0), (3.0, 6.0), (4.0, 8.0), (5.0, 10.0)]);
        assert!(p.is_ready());
        p.reset();
        assert!(!p.is_ready());
        assert_eq!(p.update((1.0, 1.0)), None);
    }

    #[test]
    fn batch_equals_streaming() {
        let pairs: Vec<(f64, f64)> = (0..60)
            .map(|i| {
                let t = f64::from(i);
                (t.sin(), (t * 0.5).cos())
            })
            .collect();
        let batch = PearsonCorrelation::new(14).unwrap().batch(&pairs);
        let mut b = PearsonCorrelation::new(14).unwrap();
        let streamed: Vec<_> = pairs.iter().map(|p| b.update(*p)).collect();
        assert_eq!(batch, streamed);
    }

    #[test]
    fn non_finite_input_returns_none() {
        let mut p = PearsonCorrelation::new(3).unwrap();
        assert_eq!(p.update((f64::NAN, 1.0)), None);
        assert_eq!(p.update((1.0, f64::INFINITY)), None);
        // The rejected ticks leave no trace: a fresh window still warms up.
        assert_eq!(p.update((1.0, 2.0)), None);
        assert_eq!(p.update((2.0, 5.0)), None);
        assert!(p.update((3.0, 7.0)).is_some());
    }
}
