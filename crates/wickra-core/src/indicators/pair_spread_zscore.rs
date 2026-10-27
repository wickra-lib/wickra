//! Pair Spread Z-Score — the standardised log-spread of two cointegrated assets.

use std::collections::VecDeque;

use crate::error::{Error, Result};
use crate::indicators::rolling_moments::{ShiftedMoments, ShiftedPairMoments};
use crate::traits::Indicator;

/// Z-score of the log-spread `ln(a) − β·ln(b)` between two assets.
///
/// This is the canonical mean-reversion / statistical-arbitrage signal for a
/// pair. Each `update` receives one `(a, b)` pair of raw **prices** and the
/// indicator does two things:
///
/// 1. **Hedge ratio.** A rolling ordinary-least-squares regression of
///    `ln(a)` on `ln(b)` over the trailing `beta_period` samples gives the
///    slope `β = cov(ln a, ln b) / var(ln b)`. The instantaneous spread is the
///    residual against the origin, `s = ln(a) − β·ln(b)`.
/// 2. **Standardisation.** The spread is then z-scored over the trailing
///    `z_period` spreads: `z = (s − mean_s) / std_s`.
///
/// A large positive `z` means `a` is rich relative to `b` (sell the spread); a
/// large negative `z` means `a` is cheap (buy the spread); `z` near zero means
/// the pair is at its typical relationship. The two windows are independent:
/// `beta_period` controls how much history the hedge ratio adapts over, and
/// `z_period` controls the look-back for the mean and dispersion of the spread.
///
/// Each `update` is O(1): one shifted pair accumulator maintains the rolling
/// OLS and one shifted scalar accumulator the rolling spread mean/variance.
/// Both are centred on a reference point drawn from inside their own window, so
/// the moments stay on the scale of the spread rather than of the log-level.
/// A flat `ln(b)` window has
/// zero variance and the hedge ratio is undefined; `β` is then taken as `0`,
/// reducing the spread to `ln(a)`. A flat spread window (zero dispersion)
/// yields a z-score of `0` rather than `NaN`.
///
/// Prices must be strictly positive and finite for the logarithm to be
/// defined; a non-positive or non-finite price is skipped (it does not enter
/// either window), exactly as a real feed would discard a bad tick.
///
/// # Example
///
/// ```
/// use wickra_core::{Indicator, PairSpreadZScore};
///
/// let mut zs = PairSpreadZScore::new(2, 2).unwrap();
/// // A flat benchmark gives hedge ratio 0, so the spread is just ln(a); with
/// // a 2-sample z-window the z-score collapses to the sign of the last move.
/// let mut last = None;
/// for a in [100.0, 100.0, 110.0, 120.0] {
///     last = zs.update((a, 100.0));
/// }
/// assert!((last.unwrap() - 1.0).abs() < 1e-9);
/// ```
#[derive(Debug, Clone)]
pub struct PairSpreadZScore {
    beta_period: usize,
    z_period: usize,
    // Rolling OLS of y = ln(a) on x = ln(b). Channel `a` of the accumulator is
    // the regressor x, channel `b` the regressand y.
    reg: VecDeque<(f64, f64)>,
    reg_moments: ShiftedPairMoments,
    // Rolling mean/variance of the spread.
    spreads: VecDeque<f64>,
    spread_moments: ShiftedMoments,
}

impl PairSpreadZScore {
    /// Construct a new pair spread z-score.
    ///
    /// `beta_period` is the look-back for the rolling hedge ratio; `z_period`
    /// is the look-back for standardising the spread.
    ///
    /// # Errors
    /// Returns [`Error::InvalidPeriod`] if either period is below `2`
    /// (variance needs at least two points).
    pub fn new(beta_period: usize, z_period: usize) -> Result<Self> {
        if beta_period < 2 {
            return Err(Error::InvalidPeriod {
                message: "pair spread z-score needs beta_period >= 2",
            });
        }
        if beta_period > crate::error::MAX_PERIOD {
            return Err(Error::InvalidPeriod {
                message: crate::error::PERIOD_ABOVE_MAX,
            });
        }
        if z_period < 2 {
            return Err(Error::InvalidPeriod {
                message: "pair spread z-score needs z_period >= 2",
            });
        }
        if z_period > crate::error::MAX_PERIOD {
            return Err(Error::InvalidPeriod {
                message: crate::error::PERIOD_ABOVE_MAX,
            });
        }
        Ok(Self {
            beta_period,
            z_period,
            reg: VecDeque::with_capacity(beta_period),
            reg_moments: ShiftedPairMoments::new(),
            spreads: VecDeque::with_capacity(z_period),
            spread_moments: ShiftedMoments::new(),
        })
    }

    /// Look-back of the rolling hedge-ratio regression.
    pub const fn beta_period(&self) -> usize {
        self.beta_period
    }

    /// Look-back of the rolling spread standardisation.
    pub const fn z_period(&self) -> usize {
        self.z_period
    }

    /// The current hedge ratio `β`, or `None` while the regression is warming
    /// up. A flat `ln(b)` window reports `0`.
    fn hedge_ratio(&self) -> Option<f64> {
        if self.reg.len() < self.beta_period {
            return None;
        }
        let n = self.beta_period;
        let var_x = self.reg_moments.var_a(n);
        if var_x == 0.0 {
            return Some(0.0);
        }
        Some(self.reg_moments.cov(n) / var_x)
    }

    fn push_spread(&mut self, s: f64) -> Option<f64> {
        if self.spreads.len() == self.z_period {
            let old = self.spreads.pop_front().expect("non-empty");
            self.spread_moments.evict(old);
        }
        self.spreads.push_back(s);
        self.spread_moments.push(s);
        if self.spread_moments.needs_reseed(self.z_period) {
            self.spread_moments.reseed(self.spreads.iter().copied());
        }
        if self.spreads.len() < self.z_period {
            return None;
        }
        let mean_s = self.spread_moments.mean(self.z_period);
        let std_s = self.spread_moments.std_dev(self.z_period);
        if std_s == 0.0 {
            // A flat spread window has no dispersion to standardise against.
            return Some(0.0);
        }
        Some((s - mean_s) / std_s)
    }
}

impl Indicator for PairSpreadZScore {
    /// `(a, b)` price pair.
    type Input = (f64, f64);
    type Output = f64;

    #[inline]
    fn update(&mut self, input: (f64, f64)) -> Option<f64> {
        let (a, b) = input;
        if !(a > 0.0 && b > 0.0 && a.is_finite() && b.is_finite()) {
            // Bad tick: skip it without disturbing either window.
            return None;
        }
        let x = b.ln();
        let y = a.ln();
        if self.reg.len() == self.beta_period {
            let (ox, oy) = self.reg.pop_front().expect("non-empty");
            self.reg_moments.evict(ox, oy);
        }
        self.reg.push_back((x, y));
        self.reg_moments.push(x, y);
        if self.reg_moments.needs_reseed(self.beta_period) {
            self.reg_moments.reseed(self.reg.iter().copied());
        }
        let beta = self.hedge_ratio()?;
        let spread = y - beta * x;
        self.push_spread(spread)
    }

    fn reset(&mut self) {
        self.reg.clear();
        self.reg_moments.reset();
        self.spreads.clear();
        self.spread_moments.reset();
    }

    #[inline]
    fn warmup_period(&self) -> usize {
        // `beta_period` samples to define the hedge ratio (and the first
        // spread), then `z_period − 1` more to fill the spread window.
        self.beta_period + self.z_period - 1
    }

    #[inline]
    fn is_ready(&self) -> bool {
        self.spreads.len() == self.z_period
    }

    #[inline]
    fn name(&self) -> &'static str {
        "PairSpreadZScore"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;
    use approx::assert_relative_eq;

    #[test]
    fn rejects_periods_below_two() {
        assert!(PairSpreadZScore::new(1, 5).is_err());
        assert!(PairSpreadZScore::new(5, 1).is_err());
        assert!(PairSpreadZScore::new(2, 2).is_ok());
    }

    #[test]
    fn accessors_and_metadata() {
        let z = PairSpreadZScore::new(10, 20).unwrap();
        assert_eq!(z.beta_period(), 10);
        assert_eq!(z.z_period(), 20);
        assert_eq!(z.warmup_period(), 29);
        assert_eq!(z.name(), "PairSpreadZScore");
    }

    #[test]
    fn flat_benchmark_two_sample_window_is_sign_of_move() {
        // Flat b ⇒ β = 0 ⇒ spread = ln(a); z_period = 2 ⇒ z = sign of last move.
        let mut z = PairSpreadZScore::new(2, 2).unwrap();
        assert_eq!(z.update((100.0, 100.0)), None);
        assert_eq!(z.update((100.0, 100.0)), None);
        // The ±1 result is exact in real arithmetic; a few ulps of rounding
        // remain in the centred variance.
        assert_relative_eq!(z.update((110.0, 100.0)).unwrap(), 1.0, epsilon = 1e-9);
        assert_relative_eq!(z.update((105.0, 100.0)).unwrap(), -1.0, epsilon = 1e-9);
        assert_relative_eq!(z.update((130.0, 100.0)).unwrap(), 1.0, epsilon = 1e-9);
    }

    #[test]
    fn constant_spread_yields_zero() {
        // Both legs flat ⇒ spread constant ⇒ zero dispersion ⇒ z = 0.
        let pairs: Vec<(f64, f64)> = (0..10).map(|_| (50.0, 100.0)).collect();
        let last = PairSpreadZScore::new(3, 4)
            .unwrap()
            .batch(&pairs)
            .into_iter()
            .flatten()
            .last()
            .unwrap();
        assert_relative_eq!(last, 0.0, epsilon = 1e-12);
    }

    #[test]
    fn bad_tick_is_skipped() {
        let mut z = PairSpreadZScore::new(2, 2).unwrap();
        // A non-positive or non-finite price never enters the windows.
        assert_eq!(z.update((0.0, 100.0)), None);
        assert_eq!(z.update((100.0, f64::NAN)), None);
        assert!(!z.is_ready());
        // Valid ticks then warm the indicator normally.
        z.update((100.0, 100.0));
        z.update((100.0, 100.0));
        z.update((110.0, 100.0));
        assert!(z.is_ready());
    }

    #[test]
    fn reset_clears_state() {
        let mut z = PairSpreadZScore::new(3, 3).unwrap();
        for i in 0..10 {
            let b = 100.0 + 5.0 * f64::from(i).sin();
            z.update((b * 1.5, b));
        }
        assert!(z.is_ready());
        z.reset();
        assert!(!z.is_ready());
        assert_eq!(z.update((100.0, 100.0)), None);
    }

    /// Both windows used to accumulate raw power sums of the log-prices, which
    /// cancel catastrophically once the log-level is large relative to the
    /// log-spread — and a tight spread between two high-priced legs is exactly
    /// the regime this indicator exists for. Measured against the two-pass
    /// reference below, a 0.01% spread at a price level of 1e5 came out with a
    /// relative error of 66 — the sign of the signal was not even reliable.
    /// Centring both accumulators takes the same case to 3.7e-11.
    #[test]
    fn tight_spread_at_a_high_price_level_matches_a_two_pass_reference() {
        const N: usize = 600;
        const BETA_P: usize = 20;
        const Z_P: usize = 30;
        const LEVEL: f64 = 1e5;
        const SPREAD: f64 = 1e-4;

        let pairs: Vec<(f64, f64)> = (0..N)
            .map(|i| {
                let t = i as f64;
                let a = LEVEL * (1.0 + SPREAD * (t * 0.11).sin());
                let b = LEVEL * (1.0 + SPREAD * 0.8 * (t * 0.07).cos());
                (a, b)
            })
            .collect();

        // Two-pass reference: every window statistic is computed from the live
        // window about its own mean, which has no cancellation to speak of.
        let xs: Vec<f64> = pairs.iter().map(|&(_, b)| b.ln()).collect();
        let ys: Vec<f64> = pairs.iter().map(|&(a, _)| a.ln()).collect();
        let mut spreads: Vec<f64> = Vec::new();
        let mut expected: Vec<Option<f64>> = vec![None; N];
        for (i, slot) in expected.iter_mut().enumerate().skip(BETA_P - 1) {
            let window = i + 1 - BETA_P..=i;
            let (xw, yw) = (&xs[window.clone()], &ys[window]);
            let count = BETA_P as f64;
            let mean_x = xw.iter().sum::<f64>() / count;
            let mean_y = yw.iter().sum::<f64>() / count;
            let var_x = xw.iter().map(|v| (v - mean_x) * (v - mean_x)).sum::<f64>() / count;
            let cov = xw
                .iter()
                .zip(yw)
                .map(|(u, v)| (u - mean_x) * (v - mean_y))
                .sum::<f64>()
                / count;
            let spread = yw[BETA_P - 1] - (cov / var_x) * xw[BETA_P - 1];
            spreads.push(spread);
            if spreads.len() < Z_P {
                continue;
            }
            let recent = &spreads[spreads.len() - Z_P..];
            let count = Z_P as f64;
            let mean_s = recent.iter().sum::<f64>() / count;
            let dispersion = (recent
                .iter()
                .map(|v| (v - mean_s) * (v - mean_s))
                .sum::<f64>()
                / count)
                .sqrt();
            *slot = Some((spread - mean_s) / dispersion);
        }

        let mut z = PairSpreadZScore::new(BETA_P, Z_P).unwrap();
        let mut compared = 0_usize;
        for (got, want) in pairs.iter().map(|p| z.update(*p)).zip(&expected) {
            assert_eq!(got.is_some(), want.is_some());
            if let (Some(got), Some(want)) = (got, *want) {
                compared += 1;
                assert_relative_eq!(got, want, max_relative = 1e-9);
            }
        }
        // `warmup_period` samples are consumed before the first emission, so
        // every later sample yields one value.
        assert_eq!(compared, N - z.warmup_period() + 1);
    }

    #[test]
    fn batch_equals_streaming() {
        let pairs: Vec<(f64, f64)> = (0..80)
            .map(|i| {
                let t = f64::from(i);
                let b = 100.0 + 10.0 * (t * 0.2).sin();
                let a = b * (1.0 + 0.05 * (t * 0.5).cos());
                (a, b)
            })
            .collect();
        let batch = PairSpreadZScore::new(14, 10).unwrap().batch(&pairs);
        let mut z = PairSpreadZScore::new(14, 10).unwrap();
        let streamed: Vec<_> = pairs.iter().map(|p| z.update(*p)).collect();
        assert_eq!(batch, streamed);
    }

    #[test]
    fn rejects_beta_period_above_maximum() {
        assert!(matches!(
            PairSpreadZScore::new(crate::error::MAX_PERIOD + 1, 2),
            Err(Error::InvalidPeriod { .. })
        ));
    }

    #[test]
    fn rejects_z_period_above_maximum() {
        assert!(matches!(
            PairSpreadZScore::new(2, crate::error::MAX_PERIOD + 1),
            Err(Error::InvalidPeriod { .. })
        ));
    }
}
