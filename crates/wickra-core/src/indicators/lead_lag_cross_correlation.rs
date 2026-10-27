//! Lead–Lag Cross-Correlation — which of two assets leads the other, and by how much.

use std::collections::VecDeque;

use crate::error::{Error, Result};
use crate::indicators::rolling_moments::centred_moments;
use crate::traits::Indicator;

/// Output of [`LeadLagCrossCorrelation`]: the lead/lag offset and its correlation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LeadLagCrossCorrelationOutput {
    /// The offset `k ∈ [−max_lag, max_lag]` that maximises `|corr(a[t], b[t+k])|`.
    ///
    /// A **positive** lag means `a` leads `b` by `lag` samples (a's pattern
    /// shows up in `b` that many steps later); a **negative** lag means `b`
    /// leads `a`; `0` means the two are most correlated contemporaneously.
    pub lag: i64,
    /// The (signed) Pearson correlation at that lag, in `[−1, +1]`.
    pub correlation: f64,
}

/// Rolling lead–lag cross-correlation between two synchronised series.
///
/// Each `update` receives one `(a, b)` pair. The indicator keeps the most
/// recent `window + 2·max_lag` samples of each series and, once full, reports
/// the integer offset `k ∈ [−max_lag, +max_lag]` that maximises the absolute
/// Pearson correlation between `a` and a copy of `b` shifted by `k`:
///
/// ```text
/// lag = argmax_k | corr( a[t], b[t+k] ) |
/// ```
///
/// This answers "does BTC lead ETH on this timescale, and by how many bars?".
/// A positive lag means `a` leads `b`; a negative lag means `b` leads `a`. The
/// reported `correlation` is the signed correlation at that lag, so its sign
/// tells you whether the lead relationship is positive or inverse.
///
/// The comparison is fully causal: `a`'s window is held fixed in the centre of
/// the buffer and `b`'s window slides across it, so every lag — positive and
/// negative — is evaluated only against data already seen. The candidate lags
/// are scanned in order of increasing `|k|`, so ties resolve to the smallest
/// absolute offset (lag `0` wins an exact tie).
///
/// Each `update` is `O(window · max_lag)` — proportional to the fixed
/// parameters, not the series length. A flat window in either channel makes a
/// correlation undefined; it is reported as `0` rather than `NaN`.
///
/// Feed raw prices or returns depending on your convention; lead–lag on
/// returns is the more common choice for relating two assets.
///
/// # Example
///
/// ```
/// use wickra_core::{Indicator, LeadLagCrossCorrelation};
///
/// let mut ll = LeadLagCrossCorrelation::new(12, 5).unwrap();
/// let mut last = None;
/// for t in 0..60 {
///     let a = (f64::from(t) * 0.4).sin() + 0.4 * (f64::from(t) * 1.1).sin();
///     // `b` is `a` delayed by 3 samples, so `a` leads `b` by 3.
///     let b = (f64::from(t - 3) * 0.4).sin() + 0.4 * (f64::from(t - 3) * 1.1).sin();
///     last = ll.update((a, b));
/// }
/// let out = last.unwrap();
/// assert_eq!(out.lag, 3);
/// assert!(out.correlation > 0.99);
/// ```
#[derive(Debug, Clone)]
pub struct LeadLagCrossCorrelation {
    window: usize,
    max_lag: usize,
    len: usize,
    a_buf: VecDeque<f64>,
    b_buf: VecDeque<f64>,
}

impl LeadLagCrossCorrelation {
    /// Construct a new lead–lag cross-correlation.
    ///
    /// `window` is the number of overlapping points each correlation is
    /// computed over; `max_lag` is the largest offset (in either direction)
    /// that is searched.
    ///
    /// # Errors
    /// Returns [`Error::InvalidPeriod`] if `window < 2` or `max_lag == 0`.
    pub fn new(window: usize, max_lag: usize) -> Result<Self> {
        if window < 2 {
            return Err(Error::InvalidPeriod {
                message: "lead-lag cross-correlation needs window >= 2",
            });
        }
        if window > crate::error::MAX_PERIOD {
            return Err(Error::InvalidPeriod {
                message: crate::error::PERIOD_ABOVE_MAX,
            });
        }
        if max_lag == 0 {
            return Err(Error::InvalidPeriod {
                message: "lead-lag cross-correlation needs max_lag >= 1",
            });
        }
        let len = window + 2 * max_lag;
        Ok(Self {
            window,
            max_lag,
            len,
            a_buf: VecDeque::with_capacity(len),
            b_buf: VecDeque::with_capacity(len),
        })
    }

    /// Number of overlapping points per correlation.
    pub const fn window(&self) -> usize {
        self.window
    }

    /// Largest offset searched in either direction.
    pub const fn max_lag(&self) -> usize {
        self.max_lag
    }

    /// Pearson correlation between `a[a_start .. a_start+window]` and
    /// `b[b_start .. b_start+window]`, clamped to `[−1, 1]`. Returns `0` when
    /// either window has zero variance.
    fn corr_at(&self, a_start: usize, b_start: usize) -> f64 {
        let moments = centred_moments(
            (0..self.window).map(|j| (self.a_buf[a_start + j], self.b_buf[b_start + j])),
        );
        let denom = (moments.var_x * moments.var_y).sqrt();
        if denom == 0.0 {
            return 0.0;
        }
        (moments.cov / denom).clamp(-1.0, 1.0)
    }
}

impl Indicator for LeadLagCrossCorrelation {
    /// `(a, b)` pair.
    type Input = (f64, f64);
    type Output = LeadLagCrossCorrelationOutput;

    fn update(&mut self, input: (f64, f64)) -> Option<LeadLagCrossCorrelationOutput> {
        let (a, b) = input;
        if !a.is_finite() || !b.is_finite() {
            return None;
        }
        if self.a_buf.len() == self.len {
            self.a_buf.pop_front();
            self.b_buf.pop_front();
        }
        self.a_buf.push_back(a);
        self.b_buf.push_back(b);
        if self.a_buf.len() < self.len {
            return None;
        }
        // `a`'s window sits in the centre; `b`'s window slides ±max_lag.
        let a_start = self.max_lag;
        // Start at lag 0, then widen outward so ties prefer the smallest |lag|.
        // The lag is tracked as a signed counter incremented by ±1, so no
        // unsigned index is ever cast to a signed type.
        let mut best_lag: i64 = 0;
        let mut best_corr = self.corr_at(a_start, a_start);
        let mut best_abs = best_corr.abs();
        let mut lag_neg: i64 = 0;
        let mut lag_pos: i64 = 0;
        for d in 1..=self.max_lag {
            lag_neg -= 1;
            lag_pos += 1;
            // Negative lag: b shifted earlier (b leads a).
            let c_neg = self.corr_at(a_start, a_start - d);
            if c_neg.abs() > best_abs {
                best_abs = c_neg.abs();
                best_corr = c_neg;
                best_lag = lag_neg;
            }
            // Positive lag: b shifted later (a leads b).
            let c_pos = self.corr_at(a_start, a_start + d);
            if c_pos.abs() > best_abs {
                best_abs = c_pos.abs();
                best_corr = c_pos;
                best_lag = lag_pos;
            }
        }
        Some(LeadLagCrossCorrelationOutput {
            lag: best_lag,
            correlation: best_corr,
        })
    }

    fn reset(&mut self) {
        self.a_buf.clear();
        self.b_buf.clear();
    }

    #[inline]
    fn warmup_period(&self) -> usize {
        self.len
    }

    #[inline]
    fn is_ready(&self) -> bool {
        self.a_buf.len() == self.len
    }

    #[inline]
    fn name(&self) -> &'static str {
        "LeadLagCrossCorrelation"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;
    use approx::assert_relative_eq;

    fn signal(t: i64) -> f64 {
        let t = t as f64;
        (t * 0.4).sin() + 0.4 * (t * 1.1).sin() + 0.2 * (t * 0.27).cos()
    }

    #[test]
    fn rejects_invalid_params() {
        assert!(LeadLagCrossCorrelation::new(1, 5).is_err());
        assert!(LeadLagCrossCorrelation::new(10, 0).is_err());
        assert!(LeadLagCrossCorrelation::new(10, 5).is_ok());
    }

    #[test]
    fn accessors_and_metadata() {
        let ll = LeadLagCrossCorrelation::new(10, 4).unwrap();
        assert_eq!(ll.window(), 10);
        assert_eq!(ll.max_lag(), 4);
        // len = window + 2*max_lag = 10 + 8 = 18.
        assert_eq!(ll.warmup_period(), 18);
        assert_eq!(ll.name(), "LeadLagCrossCorrelation");
    }

    #[test]
    fn detects_positive_lead() {
        // b is a delayed by 3 ⇒ a leads b ⇒ lag = +3, correlation ≈ 1.
        let pairs: Vec<(f64, f64)> = (0..60).map(|t| (signal(t), signal(t - 3))).collect();
        let out = LeadLagCrossCorrelation::new(12, 5)
            .unwrap()
            .batch(&pairs)
            .into_iter()
            .flatten()
            .last()
            .unwrap();
        assert_eq!(out.lag, 3);
        assert!(out.correlation > 0.99, "corr was {}", out.correlation);
    }

    #[test]
    fn detects_negative_lead() {
        // a is a delayed copy of b ⇒ b leads a ⇒ lag = −2.
        let pairs: Vec<(f64, f64)> = (0..60).map(|t| (signal(t - 2), signal(t))).collect();
        let out = LeadLagCrossCorrelation::new(12, 5)
            .unwrap()
            .batch(&pairs)
            .into_iter()
            .flatten()
            .last()
            .unwrap();
        assert_eq!(out.lag, -2);
        assert!(out.correlation > 0.99, "corr was {}", out.correlation);
    }

    #[test]
    fn contemporaneous_is_lag_zero() {
        // Identical streams correlate best at lag 0 with correlation 1.
        let pairs: Vec<(f64, f64)> = (0..60).map(|t| (signal(t), signal(t))).collect();
        let out = LeadLagCrossCorrelation::new(12, 5)
            .unwrap()
            .batch(&pairs)
            .into_iter()
            .flatten()
            .last()
            .unwrap();
        assert_eq!(out.lag, 0);
        assert_relative_eq!(out.correlation, 1.0, epsilon = 1e-9);
    }

    #[test]
    fn flat_channel_yields_zero_correlation() {
        // A constant `a` has no variance ⇒ every correlation is 0 ⇒ lag 0.
        let pairs: Vec<(f64, f64)> = (0..40).map(|t| (5.0, signal(t))).collect();
        let out = LeadLagCrossCorrelation::new(10, 4)
            .unwrap()
            .batch(&pairs)
            .into_iter()
            .flatten()
            .last()
            .unwrap();
        assert_eq!(out.lag, 0);
        assert_relative_eq!(out.correlation, 0.0, epsilon = 1e-12);
    }

    #[test]
    fn reset_clears_state() {
        let mut ll = LeadLagCrossCorrelation::new(10, 4).unwrap();
        for t in 0..40 {
            ll.update((signal(t), signal(t - 2)));
        }
        assert!(ll.is_ready());
        ll.reset();
        assert!(!ll.is_ready());
        assert_eq!(ll.update((1.0, 1.0)), None);
    }

    #[test]
    fn batch_equals_streaming() {
        let pairs: Vec<(f64, f64)> = (0..80).map(|t| (signal(t), signal(t - 1))).collect();
        let batch = LeadLagCrossCorrelation::new(12, 5).unwrap().batch(&pairs);
        let mut ll = LeadLagCrossCorrelation::new(12, 5).unwrap();
        let streamed: Vec<_> = pairs.iter().map(|p| ll.update(*p)).collect();
        assert_eq!(batch, streamed);
    }

    #[test]
    fn non_finite_input_returns_none() {
        // len = window + 2*max_lag = 2 + 2 = 4 finite ticks fill the buffers.
        let mut ll = LeadLagCrossCorrelation::new(2, 1).unwrap();
        assert_eq!(ll.update((f64::NAN, 1.0)), None);
        assert_eq!(ll.update((1.0, f64::INFINITY)), None);
        // The rejected ticks leave no trace: a fresh window still warms up.
        assert_eq!(ll.update((1.0, 2.0)), None);
        assert_eq!(ll.update((2.0, 1.0)), None);
        assert_eq!(ll.update((3.0, 4.0)), None);
        assert!(ll.update((4.0, 2.0)).is_some());
    }

    /// The correlation at each candidate lag was accumulated as raw power sums
    /// and combined with `E[xy] - E[x]E[y]`, which cancels once the series
    /// level is large relative to how it varies. Measured against a two-pass
    /// reference over 2000 bars, two legs at a level of 1e5 varying by 0.01%
    /// gave a relative error of 1.8e-05 on a quantity bounded to [-1, 1]; at
    /// 1e8 it reached 2.2e-05. Centring both channels makes it exact.
    #[test]
    fn high_level_correlation_matches_a_two_pass_reference() {
        const WINDOW: usize = 20;
        const MAX_LAG: usize = 3;
        const LEVEL: f64 = 1e8;

        let series: Vec<(f64, f64)> = (0..400)
            .map(|i| {
                let t = f64::from(i);
                (
                    LEVEL * (1.0 + 1e-4 * (t * 0.11).sin()),
                    LEVEL * (1.0 + 8e-5 * (t * 0.07).cos()),
                )
            })
            .collect();

        let mut ind = LeadLagCrossCorrelation::new(WINDOW, MAX_LAG).unwrap();
        let (mut a_all, mut b_all): (Vec<f64>, Vec<f64>) = (Vec::new(), Vec::new());
        let mut compared = 0_usize;
        for &(a, b) in &series {
            let got = ind.update((a, b));
            a_all.push(a);
            b_all.push(b);
            let Some(out) = got else { continue };
            // The buffers hold the last `window + 2*max_lag` samples, with the
            // window of a at offset max_lag and that of b sliding by the
            // reported lag.
            let a_start = a_all.len() - WINDOW - MAX_LAG;
            let shift = out.lag.unsigned_abs() as usize;
            let b_start = if out.lag >= 0 {
                a_start + shift
            } else {
                a_start - shift
            };
            let want = two_pass_correlation(
                &a_all[a_start..a_start + WINDOW],
                &b_all[b_start..b_start + WINDOW],
            );
            compared += 1;
            assert_relative_eq!(out.correlation, want, max_relative = 1e-12);
        }
        assert_eq!(compared, series.len() - ind.warmup_period() + 1);
    }

    /// Pearson correlation computed about the means of the windows themselves.
    fn two_pass_correlation(xs: &[f64], ys: &[f64]) -> f64 {
        let n = xs.len() as f64;
        let mean_x = xs.iter().sum::<f64>() / n;
        let mean_y = ys.iter().sum::<f64>() / n;
        let var_x = xs.iter().map(|v| (v - mean_x) * (v - mean_x)).sum::<f64>() / n;
        let var_y = ys.iter().map(|v| (v - mean_y) * (v - mean_y)).sum::<f64>() / n;
        let cov = xs
            .iter()
            .zip(ys)
            .map(|(u, v)| (u - mean_x) * (v - mean_y))
            .sum::<f64>()
            / n;
        (cov / (var_x * var_y).sqrt()).clamp(-1.0, 1.0)
    }

    #[test]
    fn rejects_window_above_maximum() {
        assert!(matches!(
            LeadLagCrossCorrelation::new(crate::error::MAX_PERIOD + 1, 5),
            Err(Error::InvalidPeriod { .. })
        ));
    }
}
