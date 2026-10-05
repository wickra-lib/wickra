//! Burke Ratio — mean return over the root of the summed squared drawdown episodes.

use std::collections::VecDeque;

use crate::error::{Error, Result};
use crate::traits::Indicator;

/// Burke Ratio over a trailing window of `period` returns.
///
/// ```text
/// equity_t = Π_{i<=t} (1 + return_i)          (compounded curve)
/// episode  = a stretch below the running peak, closed by a full recovery
/// D_j      = (peak_j − trough_j) / peak_j      (depth of episode j)
/// Burke    = mean(returns) / sqrt( Σ_j D_j² )
/// ```
///
/// The Burke Ratio (Gibbons Burke, 1994) divides the average per-period return by
/// the **Euclidean norm of the drawdown episodes** — the square root of the sum
/// of each episode's squared depth. Squaring penalises deep drawdowns far more
/// than shallow ones, and summing means the denominator grows with both the depth
/// and the *number* of drawdowns, but not with how many bars an episode lasts.
/// An episode still open at the window's right edge is booked at its current
/// trough. Where the [`SterlingRatio`](crate::SterlingRatio) averages the episode
/// depths and shrugs off a single crater, Burke makes that crater dominate. A
/// window that never draws down has a zero denominator and reports `0.0`.
///
/// The first value lands after `period` returns; each `update` rebuilds the equity
/// curve over the window (O(period)), which is O(1) in the length of the overall
/// series.
///
/// # Example
///
/// ```
/// use wickra_core::{Indicator, BurkeRatio};
///
/// let mut indicator = BurkeRatio::new(12).unwrap();
/// let mut last = None;
/// for i in 0..24 {
///     last = indicator.update((f64::from(i) * 0.5).sin() * 0.05);
/// }
/// assert!(last.is_some());
/// ```
#[derive(Debug, Clone)]
pub struct BurkeRatio {
    period: usize,
    window: VecDeque<f64>,
}

impl BurkeRatio {
    /// Construct a Burke Ratio over `period` returns.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidPeriod`] if `period < 2`.
    pub fn new(period: usize) -> Result<Self> {
        if period < 2 {
            return Err(Error::InvalidPeriod {
                message: "burke ratio needs period >= 2",
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
        })
    }

    /// Configured window of returns.
    pub const fn period(&self) -> usize {
        self.period
    }

    fn compute(&self) -> f64 {
        #[allow(clippy::cast_precision_loss)]
        let length = self.window.len() as f64;
        let mut sum_return = 0.0;
        let mut equity = 1.0;
        let mut peak: f64 = 1.0;
        let mut trough = f64::INFINITY;
        let mut underwater = false;
        let mut depths = 0.0;
        for ret in &self.window {
            sum_return += *ret;
            equity *= 1.0 + *ret;
            if equity >= peak {
                if underwater {
                    // Full recovery closes the episode at its trough depth.
                    let depth = (peak - trough) / peak;
                    depths += depth * depth;
                    underwater = false;
                }
                peak = equity;
            } else if underwater {
                trough = trough.min(equity);
            } else {
                underwater = true;
                trough = equity;
            }
        }
        if underwater {
            // An episode still open at the window's edge is booked at its trough.
            let depth = (peak - trough) / peak;
            depths += depth * depth;
        }
        let denom = depths.sqrt();
        if denom > 0.0 {
            (sum_return / length) / denom
        } else {
            0.0
        }
    }
}

impl Indicator for BurkeRatio {
    type Input = f64;
    type Output = f64;

    #[inline]
    fn update(&mut self, ret: f64) -> Option<f64> {
        if !ret.is_finite() {
            return None;
        }
        if self.window.len() == self.period {
            self.window.pop_front();
        }
        self.window.push_back(ret);
        if self.window.len() < self.period {
            return None;
        }
        Some(self.compute())
    }

    fn reset(&mut self) {
        self.window.clear();
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
        "BurkeRatio"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;
    use approx::assert_relative_eq;

    #[test]
    fn rejects_period_less_than_two() {
        assert!(matches!(
            BurkeRatio::new(1),
            Err(Error::InvalidPeriod { .. })
        ));
    }

    #[test]
    fn accessors_and_metadata() {
        let br = BurkeRatio::new(12).unwrap();
        assert_eq!(br.period(), 12);
        assert_eq!(br.warmup_period(), 12);
        assert_eq!(br.name(), "BurkeRatio");
        assert!(!br.is_ready());
    }

    #[test]
    fn reference_value() {
        // returns [0.1, -0.1, 0.1]: equity 1.1, 0.99, 1.089 — one episode from
        // the 1.1 peak down to 0.99, still open: depth 0.1.
        // Burke = (0.1/3) / sqrt(0.1²) = (0.1/3) / 0.1.
        let mut br = BurkeRatio::new(3).unwrap();
        let out = br.batch(&[0.1, -0.1, 0.1]);
        let expected = (0.1_f64 / 3.0) / 0.1;
        assert_relative_eq!(out[2].unwrap(), expected, epsilon = 1e-9);
    }

    #[test]
    fn no_drawdown_is_zero() {
        let mut br = BurkeRatio::new(3).unwrap();
        let last = br
            .batch(&[0.01, 0.02, 0.03])
            .into_iter()
            .flatten()
            .last()
            .unwrap();
        assert_relative_eq!(last, 0.0, epsilon = 1e-12);
    }

    #[test]
    fn losing_window_is_negative() {
        let mut br = BurkeRatio::new(3).unwrap();
        let last = br
            .batch(&[-0.05, -0.02, -0.03])
            .into_iter()
            .flatten()
            .last()
            .unwrap();
        assert!(last < 0.0);
    }

    #[test]
    fn ignores_non_finite_input() {
        let mut br = BurkeRatio::new(3).unwrap();
        assert_eq!(br.update(0.1), None);
        assert_eq!(br.update(f64::NAN), None);
        assert_eq!(br.update(-0.1), None);
        assert!(br.update(0.1).is_some());
    }

    #[test]
    fn reset_clears_state() {
        let mut br = BurkeRatio::new(3).unwrap();
        br.batch(&[0.1, -0.1, 0.1]);
        assert!(br.is_ready());
        br.reset();
        assert!(!br.is_ready());
        assert_eq!(br.update(0.1), None);
    }

    #[test]
    fn batch_equals_streaming() {
        let rets: Vec<f64> = (0..60)
            .map(|i| (f64::from(i) * 0.25).sin() * 0.05)
            .collect();
        let batch = BurkeRatio::new(12).unwrap().batch(&rets);
        let mut streamer = BurkeRatio::new(12).unwrap();
        let streamed: Vec<_> = rets.iter().map(|r| streamer.update(*r)).collect();
        assert_eq!(batch, streamed);
    }

    #[test]
    fn rejects_zero_and_oversized_period() {
        assert!(matches!(
            BurkeRatio::new(0),
            Err(Error::InvalidPeriod { .. })
        ));
        let too_long = crate::error::MAX_PERIOD + 1;
        assert!(matches!(
            BurkeRatio::new(too_long),
            Err(Error::InvalidPeriod { .. })
        ));
        assert!(BurkeRatio::new(2).is_ok());
    }

    fn wavy(len: i32) -> Vec<f64> {
        (0..len)
            .map(|i| (f64::from(i) * 0.6).sin() * 0.04 + (f64::from(i) * 1.7).cos() * 0.02)
            .collect()
    }

    #[test]
    fn first_value_lands_exactly_at_warmup_index() {
        let rets = wavy(30);
        let mut ratio = BurkeRatio::new(8).unwrap();
        let warmup = ratio.warmup_period();
        let out = ratio.batch(&rets);
        assert!(out.iter().take(warmup - 1).all(Option::is_none));
        assert!(out.iter().skip(warmup - 1).all(Option::is_some));
    }

    #[test]
    fn reset_replays_identically_to_fresh_instance() {
        let rets = wavy(50);
        let mut used = BurkeRatio::new(10).unwrap();
        used.batch(&rets);
        used.reset();
        let replay = used.batch(&rets);
        assert_eq!(replay, BurkeRatio::new(10).unwrap().batch(&rets));
    }

    #[test]
    fn batch_nan_into_matches_streaming_bits() {
        let rets = wavy(70);
        let mut nan_out = vec![0.0; rets.len()];
        BurkeRatio::new(9)
            .unwrap()
            .batch_nan_into(&rets, &mut nan_out);
        let mut streamer = BurkeRatio::new(9).unwrap();
        let identical = rets
            .iter()
            .zip(&nan_out)
            .all(|(r, v)| streamer.update(*r).unwrap_or(f64::NAN).to_bits() == v.to_bits());
        assert!(identical);
    }

    /// An overflowing equity curve (`inf · 0 = NaN`) poisons the episode depth;
    /// the non-positive / NaN denominator guard must report `0.0`, not NaN.
    #[test]
    fn nan_depth_from_overflowing_equity_reports_zero() {
        let mut ratio = BurkeRatio::new(3).unwrap();
        let out = ratio.batch(&[1e300, 1e300, -1.0]);
        assert_eq!(out[2], Some(0.0));
    }

    /// Two closed episodes. Returns `[-0.5, 1.0, -0.5, 1.0]`:
    /// equity `0.5, 1.0, 0.5, 1.0`. Each dip from the `1.0` peak to `0.5` is fully
    /// recovered (equity == peak closes it): `D_1 = D_2 = 0.5`.
    /// `mean = (−0.5 + 1 − 0.5 + 1) / 4 = 0.25`, `sqrt(0.25 + 0.25) = sqrt(0.5)`,
    /// `Burke = 0.25 / sqrt(0.5)`.
    #[test]
    fn reference_two_closed_episodes() {
        let mut br = BurkeRatio::new(4).unwrap();
        let out = br.batch(&[-0.5, 1.0, -0.5, 1.0]);
        assert_relative_eq!(out[3].unwrap(), 0.25 / 0.5_f64.sqrt(), epsilon = 1e-12);
    }

    /// Several troughs inside one episode plus an episode open at the window edge.
    /// Returns `[0.25, −0.2, 0.5, −0.5, 0.5, −0.5, 0.6]`:
    /// equity `1.25` (peak), `1.0` (episode 1 opens, trough 1.0),
    /// `1.5` (recovers: `D_1 = (1.25 − 1.0) / 1.25 = 0.2`, new peak 1.5),
    /// `0.75` (episode 2 opens), `1.125` (still under water, trough stays 0.75),
    /// `0.5625` (deeper trough), `0.9` (still under water at the edge):
    /// `D_2 = (1.5 − 0.5625) / 1.5 = 0.625`.
    /// `mean = 0.65 / 7`; `Burke = (0.65 / 7) / sqrt(0.2² + 0.625²)
    /// = (0.65 / 7) / sqrt(0.430625)`.
    #[test]
    fn reference_multi_trough_and_open_episode() {
        let mut br = BurkeRatio::new(7).unwrap();
        let out = br.batch(&[0.25, -0.2, 0.5, -0.5, 0.5, -0.5, 0.6]);
        let expected = (0.65 / 7.0) / 0.430_625_f64.sqrt();
        assert_relative_eq!(out[6].unwrap(), expected, epsilon = 1e-12);
    }

    #[test]
    fn bar_count_of_an_episode_does_not_change_its_weight() {
        // Same single 0.5-deep episode, held for one bar versus three bars
        // (mean return kept equal): the denominator is the same.
        let short = BurkeRatio::new(4).unwrap().batch(&[-0.5, 1.0, 0.0, 0.0]);
        let long = BurkeRatio::new(4).unwrap().batch(&[-0.5, 0.0, 0.0, 1.0]);
        assert_relative_eq!(short[3].unwrap(), long[3].unwrap(), epsilon = 1e-12);
        assert_relative_eq!(short[3].unwrap(), 0.125 / 0.5, epsilon = 1e-12);
    }
}
