//! Rolling covariance of the period-over-period *returns* of two series.

use std::collections::VecDeque;

use crate::error::{Error, Result};
use crate::indicators::rolling_moments::ShiftedPairMoments;
use crate::traits::Indicator;

/// Rolling covariance of the **returns** of two synchronised series.
///
/// Each `update` takes one `(x, y)` level pair, differences each channel into a
/// one-step return, and reports the population covariance of those returns over
/// the trailing window of `period` return pairs:
///
/// ```text
/// rxₜ = xₜ − xₜ₋₁          ryₜ = yₜ − yₜ₋₁
/// cov = (1/n) · Σ rx·ry − r̄x · r̄y
/// ```
///
/// Unlike [`crate::RollingCorrelation`] the result is **not** normalised to
/// `[−1, 1]`: it carries the units of the two return streams multiplied
/// together, so it scales with volatility. It is the raw building block behind
/// correlation, beta and portfolio variance — positive when the two return
/// streams tend to move the same way, negative when they offset.
///
/// Each `update` is O(1): three running sums (`Σrx`, `Σry`, `Σrxry`) are
/// maintained as the window slides. The first level in each channel produces no
/// return, so a `period`-pair covariance needs `period + 1` updates of warmup.
///
/// # Example
///
/// ```
/// use wickra_core::{Indicator, RollingCovariance};
///
/// let mut rc = RollingCovariance::new(5).unwrap();
/// let mut last = None;
/// for i in 0..20 {
///     let x = f64::from(i);
///     last = rc.update((x, 3.0 * x)); // y's return is 3× x's return
/// }
/// // cov(rx, ry) = cov(1, 3) over constant unit returns = 3 · var(rx) = 0
/// // for a constant return; use a varying path in practice. Here returns are
/// // constant (1 and 3) ⇒ covariance 0.
/// assert!(last.unwrap().abs() < 1e-9);
/// ```
#[derive(Debug, Clone)]
pub struct RollingCovariance {
    period: usize,
    prev: Option<(f64, f64)>,
    window: VecDeque<(f64, f64)>,
    moments: ShiftedPairMoments,
}

impl RollingCovariance {
    /// Construct a new rolling return-covariance.
    ///
    /// # Errors
    /// Returns [`Error::InvalidPeriod`] if `period < 2` — covariance is
    /// undefined for fewer than two return pairs.
    pub fn new(period: usize) -> Result<Self> {
        if period < 2 {
            return Err(Error::InvalidPeriod {
                message: "rolling covariance needs period >= 2",
            });
        }
        if period > crate::error::MAX_PERIOD {
            return Err(Error::InvalidPeriod {
                message: crate::error::PERIOD_ABOVE_MAX,
            });
        }
        Ok(Self {
            period,
            prev: None,
            window: VecDeque::with_capacity(period),
            moments: ShiftedPairMoments::new(),
        })
    }

    /// Configured window of returns.
    pub const fn period(&self) -> usize {
        self.period
    }
}

impl Indicator for RollingCovariance {
    type Input = (f64, f64);
    type Output = f64;

    #[inline]
    fn update(&mut self, input: (f64, f64)) -> Option<f64> {
        let (x, y) = input;
        if !x.is_finite() || !y.is_finite() {
            return None;
        }
        let Some((px, py)) = self.prev else {
            self.prev = Some((x, y));
            return None;
        };
        self.prev = Some((x, y));
        let (rx, ry) = (x - px, y - py);
        if self.window.len() == self.period {
            let (ox, oy) = self.window.pop_front().expect("non-empty");
            self.moments.evict(ox, oy);
        }
        self.window.push_back((rx, ry));
        self.moments.push(rx, ry);
        if self.moments.needs_reseed(self.period) {
            self.moments.reseed(self.window.iter().copied());
        }
        if self.window.len() < self.period {
            return None;
        }
        Some(self.moments.cov(self.period))
    }

    fn reset(&mut self) {
        self.prev = None;
        self.window.clear();
        self.moments.reset();
    }

    #[inline]
    fn warmup_period(&self) -> usize {
        self.period + 1
    }

    #[inline]
    fn is_ready(&self) -> bool {
        self.window.len() == self.period
    }

    #[inline]
    fn name(&self) -> &'static str {
        "RollingCovariance"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;
    use approx::assert_relative_eq;

    #[test]
    fn rejects_period_below_two() {
        assert!(RollingCovariance::new(0).is_err());
        assert!(RollingCovariance::new(1).is_err());
        assert!(RollingCovariance::new(2).is_ok());
    }

    #[test]
    fn accessors_and_metadata() {
        let rc = RollingCovariance::new(14).unwrap();
        assert_eq!(rc.period(), 14);
        assert_eq!(rc.warmup_period(), 15);
        assert_eq!(rc.name(), "RollingCovariance");
        assert!(!rc.is_ready());
    }

    #[test]
    fn warmup_needs_period_plus_one() {
        let mut rc = RollingCovariance::new(3).unwrap();
        assert_eq!(rc.update((1.0, 1.0)), None);
        assert_eq!(rc.update((2.0, 3.0)), None);
        assert_eq!(rc.update((3.0, 5.0)), None);
        assert!(rc.update((4.0, 7.0)).is_some());
        assert!(rc.is_ready());
    }

    #[test]
    fn hand_computed_value() {
        // Levels x = 0,1,3,6,10 ⇒ returns 1,2,3,4; y = 2x ⇒ returns 2,4,6,8.
        // With period = 3 the final window is rx = [2,3,4], ry = [4,6,8]:
        //   Σrx·ry/3 = 58/3, r̄x·r̄y = 3·6 = 18 ⇒ cov = 58/3 − 18 = 4/3.
        let pairs = [
            (0.0, 0.0),
            (1.0, 2.0),
            (3.0, 6.0),
            (6.0, 12.0),
            (10.0, 20.0),
        ];
        let last = RollingCovariance::new(3)
            .unwrap()
            .batch(&pairs)
            .into_iter()
            .flatten()
            .last()
            .unwrap();
        assert_relative_eq!(last, 4.0 / 3.0, epsilon = 1e-9);
    }

    #[test]
    fn opposing_returns_give_negative_covariance() {
        let pairs: Vec<(f64, f64)> = (0..30)
            .map(|i| {
                let x = (f64::from(i) * 0.4).sin() * 10.0;
                (x, -x)
            })
            .collect();
        let last = RollingCovariance::new(10)
            .unwrap()
            .batch(&pairs)
            .into_iter()
            .flatten()
            .last()
            .unwrap();
        assert!(last < 0.0, "cov {last}");
    }

    #[test]
    fn flat_channel_gives_zero() {
        let pairs: Vec<(f64, f64)> = (0..20).map(|i| (f64::from(i), 7.0)).collect();
        let last = RollingCovariance::new(6)
            .unwrap()
            .batch(&pairs)
            .into_iter()
            .flatten()
            .last()
            .unwrap();
        assert_relative_eq!(last, 0.0, epsilon = 1e-12);
    }

    #[test]
    fn reset_clears_state() {
        let mut rc = RollingCovariance::new(4).unwrap();
        rc.batch(&[(1.0, 2.0), (2.0, 4.0), (3.0, 1.0), (4.0, 9.0), (5.0, 2.0)]);
        assert!(rc.is_ready());
        rc.reset();
        assert!(!rc.is_ready());
        assert_eq!(rc.update((1.0, 1.0)), None);
    }

    #[test]
    fn batch_equals_streaming() {
        let pairs: Vec<(f64, f64)> = (0..60)
            .map(|i| {
                let t = f64::from(i);
                (t.sin() * 4.0, (t * 0.5).cos() * 2.0)
            })
            .collect();
        let batch = RollingCovariance::new(12).unwrap().batch(&pairs);
        let mut rc = RollingCovariance::new(12).unwrap();
        let streamed: Vec<_> = pairs.iter().map(|p| rc.update(*p)).collect();
        assert_eq!(batch, streamed);
    }

    #[test]
    fn non_finite_input_returns_none() {
        let mut rc = RollingCovariance::new(2).unwrap();
        assert_eq!(rc.update((f64::NAN, 1.0)), None);
        assert_eq!(rc.update((1.0, f64::INFINITY)), None);
        // First finite tick seeds prev; two more returns fill the window.
        assert_eq!(rc.update((1.0, 1.0)), None);
        assert_eq!(rc.update((2.0, 3.0)), None);
        assert!(rc.update((3.0, 5.0)).is_some());
    }

    /// The covariance was accumulated as `E[xy] - E[x]E[y]` over running sums
    /// that were never rebuilt, so it carried both the cancellation of the
    /// one-pass form and unbounded drift. A centred accumulator with a periodic
    /// rebuild takes 2000 updates from 1.5e-11 to 3.3e-12 against a two-pass
    /// reference, and bounds where that residual can go.
    #[test]
    fn matches_a_two_pass_reference_over_a_long_stream() {
        const PERIOD: usize = 20;
        let series: Vec<(f64, f64)> = (0..2000)
            .map(|i| {
                let t = f64::from(i);
                (
                    1e5 * (1.0 + 0.05 * (t * 0.11).sin()),
                    1e5 * (1.0 + 0.04 * (t * 0.07).cos()),
                )
            })
            .collect();

        let mut ind = RollingCovariance::new(PERIOD).unwrap();
        let (mut dx, mut dy): (Vec<f64>, Vec<f64>) = (Vec::new(), Vec::new());
        let mut prev: Option<(f64, f64)> = None;
        let mut compared = 0_usize;
        for &(x, y) in &series {
            let got = ind.update((x, y));
            if let Some((px, py)) = prev {
                dx.push(x - px);
                dy.push(y - py);
            }
            prev = Some((x, y));
            let Some(cov) = got else { continue };
            let k = dx.len();
            let (xs, ys) = (&dx[k - PERIOD..], &dy[k - PERIOD..]);
            let n = PERIOD as f64;
            let mean_x = xs.iter().sum::<f64>() / n;
            let mean_y = ys.iter().sum::<f64>() / n;
            let want = xs
                .iter()
                .zip(ys)
                .map(|(u, v)| (u - mean_x) * (v - mean_y))
                .sum::<f64>()
                / n;
            compared += 1;
            assert_relative_eq!(cov, want, max_relative = 1e-10);
        }
        assert_eq!(compared, series.len() - ind.warmup_period() + 1);
    }

    #[test]
    fn rejects_period_above_maximum() {
        assert!(matches!(
            RollingCovariance::new(crate::error::MAX_PERIOD + 1),
            Err(Error::InvalidPeriod { .. })
        ));
    }
}
