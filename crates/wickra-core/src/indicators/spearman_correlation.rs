//! Rolling Spearman rank correlation between two synchronised series.

use std::collections::VecDeque;

use crate::error::{Error, Result};
use crate::traits::Indicator;

/// Rolling Spearman rank correlation between two synchronised series.
///
/// Each `update` receives one `(x, y)` pair. Over the trailing window of
/// `period` pairs, the values in each channel are replaced by their ranks
/// (mid-ranks for ties), and the Pearson correlation of those ranks is
/// reported:
///
/// ```text
/// rx = rank(x_i)  with mid-rank tie handling
/// ry = rank(y_i)  with mid-rank tie handling
/// Spearman = Pearson( rx, ry )
/// ```
///
/// Spearman is the non-linear, **monotone** analogue of
/// [`crate::PearsonCorrelation`]: `+1` means the two series move in the
/// same direction (any monotone relationship, not just linear); `−1`
/// means they move in opposite directions; `0` means no monotone
/// relationship. Because ranks throw away magnitude, Spearman is robust
/// to outliers and to non-linear (but monotone) transformations — the
/// canonical example is two assets that move together but with very
/// different volatility profiles.
///
/// Each `update` is O(period²) in the naïve implementation; Wickra uses
/// an O(period log period) sort-and-pair approach: the window is copied
/// into a scratch buffer, sorted twice (once per channel) to derive the
/// ranks, then Pearson is computed on the rank arrays via the same O(n)
/// rolling sums as [`crate::PearsonCorrelation`].
///
/// A window in which one channel is constant has no rank dispersion and
/// the correlation is undefined; the indicator returns `0` rather than
/// `NaN`. The output is clamped to `[−1, +1]` to absorb tiny
/// floating-point overshoots.
///
/// # Example
///
/// ```
/// use wickra_core::{Indicator, SpearmanCorrelation};
///
/// let mut indicator = SpearmanCorrelation::new(10).unwrap();
/// let mut last = None;
/// for i in 1..20 {
///     // Strictly monotone — Spearman should be +1.
///     last = indicator.update((f64::from(i), (f64::from(i)).powi(3)));
/// }
/// assert!((last.unwrap() - 1.0).abs() < 1e-9);
/// ```
#[derive(Debug, Clone)]
pub struct SpearmanCorrelation {
    period: usize,
    window: VecDeque<(f64, f64)>,
    /// Reusable scratch buffer for ranking; pairs of `(value, original_index)`.
    scratch: Vec<(f64, usize)>,
    /// Reusable rank buffers, indexed by original position in the window.
    rx: Vec<f64>,
    ry: Vec<f64>,
}

impl SpearmanCorrelation {
    /// Construct a new rolling Spearman correlation.
    ///
    /// # Errors
    /// Returns [`Error::InvalidPeriod`] if `period < 2`.
    pub fn new(period: usize) -> Result<Self> {
        if period < 2 {
            return Err(Error::InvalidPeriod {
                message: "spearman correlation needs period >= 2",
            });
        }
        if period > crate::error::MAX_PERIOD {
            return Err(Error::InvalidPeriod {
                message: crate::error::PERIOD_ABOVE_MAX,
            });
        }
        Ok(Self {
            period,
            window: VecDeque::with_capacity(period),
            scratch: Vec::with_capacity(period),
            rx: vec![0.0; period],
            ry: vec![0.0; period],
        })
    }

    /// Configured period.
    pub const fn period(&self) -> usize {
        self.period
    }
}

/// Fill `ranks_out[original_index] = rank` for the supplied `values`,
/// using mid-ranks for ties. `scratch` is reused so no allocation
/// happens per call after the first.
fn rank_into(
    values: impl Iterator<Item = f64>,
    ranks_out: &mut [f64],
    scratch: &mut Vec<(f64, usize)>,
) {
    scratch.clear();
    for (i, v) in values.enumerate() {
        scratch.push((v, i));
    }
    scratch.sort_by(|a, b| a.0.total_cmp(&b.0));
    let n = scratch.len();
    let mut i = 0;
    while i < n {
        let mut j = i + 1;
        while j < n && scratch[j].0 == scratch[i].0 {
            j += 1;
        }
        // Mid-rank of positions [i, j-1] in 1-indexed terms:
        // (i + 1 + j) / 2.
        let mid = (i as f64 + 1.0 + j as f64) / 2.0;
        for k in i..j {
            ranks_out[scratch[k].1] = mid;
        }
        i = j;
    }
}

impl Indicator for SpearmanCorrelation {
    type Input = (f64, f64);
    type Output = f64;

    fn update(&mut self, input: (f64, f64)) -> Option<f64> {
        if !input.0.is_finite() || !input.1.is_finite() {
            return None;
        }
        if self.window.len() == self.period {
            self.window.pop_front();
        }
        self.window.push_back(input);
        if self.window.len() < self.period {
            return None;
        }
        // Rank each channel.
        rank_into(
            self.window.iter().map(|p| p.0),
            &mut self.rx,
            &mut self.scratch,
        );
        rank_into(
            self.window.iter().map(|p| p.1),
            &mut self.ry,
            &mut self.scratch,
        );
        // Pearson over the rank arrays. Closed forms are not used here
        // because tie handling produces mid-ranks; the generic Pearson keeps
        // the code uniform.
        let n = self.period as f64;
        let mut sum_x = 0.0;
        let mut sum_y = 0.0;
        let mut sum_xx = 0.0;
        let mut sum_yy = 0.0;
        let mut sum_xy = 0.0;
        for i in 0..self.period {
            let x = self.rx[i];
            let y = self.ry[i];
            sum_x += x;
            sum_y += y;
            sum_xx += x * x;
            sum_yy += y * y;
            sum_xy += x * y;
        }
        let mean_x = sum_x / n;
        let mean_y = sum_y / n;
        let var_x = (sum_xx / n - mean_x * mean_x).max(0.0);
        let var_y = (sum_yy / n - mean_y * mean_y).max(0.0);
        let cov = sum_xy / n - mean_x * mean_y;
        let denom = (var_x * var_y).sqrt();
        if denom == 0.0 {
            return Some(0.0);
        }
        Some((cov / denom).clamp(-1.0, 1.0))
    }

    fn reset(&mut self) {
        self.window.clear();
        self.scratch.clear();
        self.rx.iter_mut().for_each(|r| *r = 0.0);
        self.ry.iter_mut().for_each(|r| *r = 0.0);
    }

    #[inline]
    fn warmup_period(&self) -> usize {
        self.period
    }

    #[inline]
    fn is_ready(&self) -> bool {
        self.window.len() == self.period
    }

    #[inline]
    fn name(&self) -> &'static str {
        "SpearmanCorrelation"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;
    use approx::assert_relative_eq;

    #[test]
    fn rejects_period_below_two() {
        assert!(SpearmanCorrelation::new(0).is_err());
        assert!(SpearmanCorrelation::new(1).is_err());
        assert!(SpearmanCorrelation::new(2).is_ok());
    }

    #[test]
    fn accessors_and_metadata() {
        let s = SpearmanCorrelation::new(14).unwrap();
        assert_eq!(s.period(), 14);
        assert_eq!(s.warmup_period(), 14);
        assert_eq!(s.name(), "SpearmanCorrelation");
    }

    #[test]
    fn perfect_monotone_relationship_is_one() {
        // y = x³ is strictly monotone but very non-linear; Pearson would
        // not return exactly 1 but Spearman must.
        let pairs: Vec<(f64, f64)> = (1..=10)
            .map(|i| (f64::from(i), (f64::from(i)).powi(3)))
            .collect();
        let last = SpearmanCorrelation::new(5)
            .unwrap()
            .batch(&pairs)
            .into_iter()
            .flatten()
            .last()
            .unwrap();
        assert_relative_eq!(last, 1.0, epsilon = 1e-9);
    }

    #[test]
    fn perfect_inverse_is_minus_one() {
        let pairs: Vec<(f64, f64)> = (1..=10)
            .map(|i| (f64::from(i), 1.0 / (f64::from(i))))
            .collect();
        let last = SpearmanCorrelation::new(5)
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
        let last = SpearmanCorrelation::new(5)
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
                (100.0 + t.sin() * 5.0, 50.0 + (t * 0.7).cos() * 3.0)
            })
            .collect();
        let mut s = SpearmanCorrelation::new(20).unwrap();
        for v in s.batch(&pairs).into_iter().flatten() {
            assert!((-1.0..=1.0).contains(&v));
        }
    }

    #[test]
    fn handles_ties_via_mid_ranks() {
        // x has a tie at the top; Spearman must still produce a sensible
        // value (it equals Pearson of the rank arrays).
        let pairs = [(1.0, 1.0), (2.0, 2.0), (3.0, 3.0), (3.0, 4.0)];
        let last = SpearmanCorrelation::new(4)
            .unwrap()
            .batch(&pairs)
            .into_iter()
            .flatten()
            .last()
            .unwrap();
        // Ranks: rx = [1, 2, 3.5, 3.5]; ry = [1, 2, 3, 4]. Pearson of those
        // is a positive number less than 1 because of the tie in rx.
        assert!(last > 0.0 && last < 1.0);
    }

    #[test]
    fn reset_clears_state() {
        let mut s = SpearmanCorrelation::new(5).unwrap();
        s.batch(&[(1.0, 2.0), (2.0, 4.0), (3.0, 6.0), (4.0, 8.0), (5.0, 10.0)]);
        assert!(s.is_ready());
        s.reset();
        assert!(!s.is_ready());
        assert_eq!(s.update((1.0, 1.0)), None);
    }

    #[test]
    fn batch_equals_streaming() {
        let pairs: Vec<(f64, f64)> = (0..60)
            .map(|i| {
                let t = f64::from(i);
                (t.sin() + (t * 0.1).cos(), (t * 0.3).cos())
            })
            .collect();
        let batch = SpearmanCorrelation::new(14).unwrap().batch(&pairs);
        let mut b = SpearmanCorrelation::new(14).unwrap();
        let streamed: Vec<_> = pairs.iter().map(|p| b.update(*p)).collect();
        assert_eq!(batch, streamed);
    }

    #[test]
    fn non_finite_input_returns_none() {
        let mut s = SpearmanCorrelation::new(2).unwrap();
        assert_eq!(s.update((f64::NAN, 1.0)), None);
        assert_eq!(s.update((1.0, f64::INFINITY)), None);
        // The rejected ticks leave no trace: a fresh window still warms up.
        assert_eq!(s.update((1.0, 2.0)), None);
        assert!(s.update((2.0, 5.0)).is_some());
    }

    #[test]
    fn rejects_period_above_maximum() {
        assert!(matches!(
            SpearmanCorrelation::new(crate::error::MAX_PERIOD + 1),
            Err(Error::InvalidPeriod { .. })
        ));
    }
}
