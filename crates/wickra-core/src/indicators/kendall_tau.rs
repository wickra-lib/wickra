//! Kendall's tau-b — rank correlation by concordant vs. discordant pairs.

use std::collections::VecDeque;

use crate::error::{Error, Result};
use crate::traits::Indicator;

/// `+1` / `0` / `-1` sign of `a − b`.
fn sign(a: f64, b: f64) -> i32 {
    if a > b {
        1
    } else if a < b {
        -1
    } else {
        0
    }
}

/// Kendall's tau-b — a rank correlation between two synchronised series based on
/// the balance of **concordant** and **discordant** pairs, with a tie correction.
///
/// ```text
/// over all pairs (i < j) in the window:
///   concordant if (x_j − x_i) and (y_j − y_i) share a sign
///   discordant if they have opposite signs
///   tie_x / tie_y if the respective difference is zero
/// n0  = N(N−1)/2
/// tau_b = (n_concordant − n_discordant) / sqrt((n0 − tie_x)(n0 − tie_y))
/// ```
///
/// Where [`PearsonCorrelation`](crate::PearsonCorrelation) measures *linear*
/// co-movement and [`SpearmanCorrelation`](crate::SpearmanCorrelation) correlates
/// ranks via their differences, Kendall's tau counts how often the two series move
/// the **same direction** between every pair of observations. It is the most
/// robust of the three to outliers and to non-linear-but-monotonic
/// relationships, and the tau-b form corrects for ties so repeated values do not
/// bias it. The output is in `[−1, +1]`: `+1` perfectly concordant, `−1`
/// perfectly discordant, `0` no monotonic association.
///
/// The window holds the last `period` pairs and is recomputed each bar in
/// O(`period²`). A window with no untied pairs on one side returns `0`. The first
/// value lands after `period` inputs.
///
/// # Example
///
/// ```
/// use wickra_core::{Indicator, KendallTau};
///
/// let mut indicator = KendallTau::new(20).unwrap();
/// let mut last = None;
/// for i in 0..40 {
///     let x = f64::from(i);
///     last = indicator.update((x, 2.0 * x)); // perfectly concordant
/// }
/// assert!((last.unwrap() - 1.0).abs() < 1e-9);
/// ```
#[derive(Debug, Clone)]
pub struct KendallTau {
    period: usize,
    window: VecDeque<(f64, f64)>,
    /// Pair counts over the live window, kept as pairs enter and leave: each
    /// entering or leaving pair meets every other once, so a step costs
    /// `O(period)` instead of recounting all `period²/2` pairs. Whole numbers,
    /// so the result is exactly the recount's.
    counts: PairCounts,
    last: Option<f64>,
}

/// Concordant, discordant and tied pairs among a window's pairs.
#[derive(Debug, Clone, Copy, Default)]
struct PairCounts {
    concordant: i64,
    discordant: i64,
    tie_x: i64,
    tie_y: i64,
}

impl PairCounts {
    /// Add (`step = 1`) or remove (`step = -1`) the pair `(a, b)`.
    fn apply(&mut self, a: (f64, f64), b: (f64, f64), step: i64) {
        let sx = sign(b.0, a.0);
        let sy = sign(b.1, a.1);
        if sx == 0 {
            self.tie_x += step;
        }
        if sy == 0 {
            self.tie_y += step;
        }
        let prod = sx * sy;
        if prod > 0 {
            self.concordant += step;
        } else if prod < 0 {
            self.discordant += step;
        }
    }
}

impl KendallTau {
    /// Construct a rolling Kendall's tau-b over `period` pairs.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidPeriod`] if `period < 2` (a correlation needs at
    /// least two pairs).
    pub fn new(period: usize) -> Result<Self> {
        if period < 2 {
            return Err(Error::InvalidPeriod {
                message: "Kendall tau needs period >= 2",
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
            counts: PairCounts::default(),
            last: None,
        })
    }

    /// Configured window of pairs.
    pub const fn period(&self) -> usize {
        self.period
    }

    /// Current value if available.
    pub const fn value(&self) -> Option<f64> {
        self.last
    }

    fn compute(&self) -> f64 {
        let len = self.window.len();
        let PairCounts {
            concordant,
            discordant,
            tie_x,
            tie_y,
        } = self.counts;
        let n0 = (len * (len - 1) / 2) as f64;
        let denom = ((n0 - tie_x as f64) * (n0 - tie_y as f64)).sqrt();
        if denom == 0.0 {
            return 0.0;
        }
        ((concordant - discordant) as f64 / denom).clamp(-1.0, 1.0)
    }
}

impl Indicator for KendallTau {
    type Input = (f64, f64);
    type Output = f64;

    #[inline]
    fn update(&mut self, input: (f64, f64)) -> Option<f64> {
        if !input.0.is_finite() || !input.1.is_finite() {
            return None;
        }
        if self.window.len() == self.period {
            let oldest = self.window.pop_front().expect("window is full");
            for &other in &self.window {
                self.counts.apply(oldest, other, -1);
            }
        }
        for &other in &self.window {
            self.counts.apply(other, input, 1);
        }
        self.window.push_back(input);
        if self.window.len() < self.period {
            return None;
        }
        let out = self.compute();
        self.last = Some(out);
        Some(out)
    }

    fn reset(&mut self) {
        self.window.clear();
        self.counts = PairCounts::default();
        self.last = None;
    }

    #[inline]
    fn warmup_period(&self) -> usize {
        self.period
    }

    #[inline]
    fn is_ready(&self) -> bool {
        self.last.is_some()
    }

    #[inline]
    fn name(&self) -> &'static str {
        "KendallTau"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;
    use approx::assert_relative_eq;

    #[test]
    fn rejects_period_below_two() {
        assert!(matches!(
            KendallTau::new(1),
            Err(Error::InvalidPeriod { .. })
        ));
        assert!(KendallTau::new(2).is_ok());
    }

    #[test]
    fn accessors_and_metadata() {
        let k = KendallTau::new(20).unwrap();
        assert_eq!(k.period(), 20);
        assert_eq!(k.warmup_period(), 20);
        assert_eq!(k.name(), "KendallTau");
        assert!(!k.is_ready());
        assert_eq!(k.value(), None);
    }

    #[test]
    fn first_emission_at_warmup_period() {
        let mut k = KendallTau::new(4).unwrap();
        let out = k.batch(&[(1.0, 1.0), (2.0, 2.0), (3.0, 3.0), (4.0, 4.0), (5.0, 5.0)]);
        for v in out.iter().take(3) {
            assert!(v.is_none());
        }
        assert!(out[3].is_some());
    }

    #[test]
    fn monotone_increasing_is_one() {
        let pairs: Vec<(f64, f64)> = (0..20)
            .map(|i| (f64::from(i), 2.0 * f64::from(i) + 1.0))
            .collect();
        let last = KendallTau::new(10)
            .unwrap()
            .batch(&pairs)
            .into_iter()
            .flatten()
            .last()
            .unwrap();
        assert_relative_eq!(last, 1.0, epsilon = 1e-9);
    }

    #[test]
    fn monotone_decreasing_is_minus_one() {
        let pairs: Vec<(f64, f64)> = (0..20)
            .map(|i| (f64::from(i), -3.0 * f64::from(i)))
            .collect();
        let last = KendallTau::new(10)
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
        // y constant -> every y-difference is a tie -> denom 0 -> 0.
        let pairs: Vec<(f64, f64)> = (0..20).map(|i| (f64::from(i), 7.0)).collect();
        let last = KendallTau::new(8)
            .unwrap()
            .batch(&pairs)
            .into_iter()
            .flatten()
            .last()
            .unwrap();
        assert_relative_eq!(last, 0.0, epsilon = 1e-12);
    }

    #[test]
    fn output_in_range() {
        let pairs: Vec<(f64, f64)> = (0..80)
            .map(|i| {
                let t = f64::from(i);
                (100.0 + t.sin() * 5.0, 50.0 + (t * 0.3).cos() * 3.0)
            })
            .collect();
        for v in KendallTau::new(20)
            .unwrap()
            .batch(&pairs)
            .into_iter()
            .flatten()
        {
            assert!((-1.0..=1.0).contains(&v));
        }
    }

    #[test]
    fn reset_clears_state() {
        let mut k = KendallTau::new(4).unwrap();
        k.batch(&[(1.0, 1.0), (2.0, 2.0), (3.0, 3.0), (4.0, 4.0)]);
        assert!(k.is_ready());
        k.reset();
        assert!(!k.is_ready());
        assert_eq!(k.value(), None);
        assert_eq!(k.update((1.0, 1.0)), None);
    }

    #[test]
    fn batch_equals_streaming() {
        let pairs: Vec<(f64, f64)> = (0..60)
            .map(|i| {
                let t = f64::from(i);
                (t.sin(), (t * 0.5).cos())
            })
            .collect();
        let batch = KendallTau::new(14).unwrap().batch(&pairs);
        let mut b = KendallTau::new(14).unwrap();
        let streamed: Vec<_> = pairs.iter().map(|p| b.update(*p)).collect();
        assert_eq!(batch, streamed);
    }

    #[test]
    fn ties_are_corrected() {
        // Tied x values (points 0 and 1) and tied y values (points 1 and 2)
        // exercise the tie_x / tie_y correction counters.
        let mut k = KendallTau::new(4).unwrap();
        assert_eq!(k.update((1.0, 1.0)), None);
        assert_eq!(k.update((1.0, 2.0)), None);
        assert_eq!(k.update((2.0, 2.0)), None);
        let v = k.update((3.0, 3.0)).unwrap();
        assert!((-1.0..=1.0).contains(&v), "got {v}");
    }

    #[test]
    fn non_finite_input_returns_none() {
        let mut k = KendallTau::new(2).unwrap();
        assert_eq!(k.update((f64::NAN, 1.0)), None);
        assert_eq!(k.update((1.0, f64::INFINITY)), None);
        // The rejected ticks leave no trace: a fresh window still warms up.
        assert_eq!(k.update((1.0, 2.0)), None);
        assert!(k.update((2.0, 5.0)).is_some());
    }
}
