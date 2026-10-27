//! Rolling Hurst Exponent via simplified R/S analysis.

use std::collections::VecDeque;

use crate::error::{Error, Result};
use crate::traits::Indicator;

/// Hurst Exponent of the last `period` values, estimated by rescaled-range
/// (R/S) analysis.
///
/// The classic Hurst-Mandelbrot estimator forms log-log pairs of `(n,
/// R(n)/S(n))` for several window lengths `n` and reports the slope of the
/// least-squares fit. Wickra uses a streaming-friendly variant that
/// partitions the trailing window into `chunks` of equal size,
/// computes `(R/S)` for each chunk length, and fits a log-log line to the
/// resulting points:
///
/// ```text
/// for each chunk size m ∈ {n/2, n/3, …, n/chunks}:
///     mean_m   = (1/m) · Σ x_i               over the chunk
///     dev_m_i  = (Σ_{j ≤ i} (x_j − mean_m))  // cumulative deviation
///     R_m      = max(dev_m) − min(dev_m)
///     S_m      = population_stddev(chunk)
///     pair     = (log m, log(R_m / S_m))
/// H = slope of OLS line through the (log m, log(R/S)) points
/// ```
///
/// The interpretation is unchanged from the textbook:
///
/// - `H ≈ 0.5` → random walk; recent moves carry no information about
///   future direction (the efficient-markets baseline).
/// - `H > 0.5` → persistent / trending; up moves are likelier to be
///   followed by more up moves.
/// - `H < 0.5` → anti-persistent / mean-reverting; up moves tend to
///   reverse.
///
/// Use it as a regime filter: trend-following strategies prefer
/// `H > 0.55`; mean-reversion prefers `H < 0.45`. The output is clamped
/// to `[0, 1]` to absorb degenerate fits on very small windows.
///
/// `period` must be at least `2 · chunks` so every chunk has at least two
/// points (otherwise its stddev is zero). A perfectly flat window has all
/// `R/S = 0` and the indicator returns `0.5` (random-walk baseline) to
/// avoid divide-by-zero / log-zero failures.
///
/// Each `update` is O(period); the window is stored in a deque and the
/// chunked R/S computation runs once per emission, not per input.
///
/// # Example
///
/// ```
/// use wickra_core::{HurstExponent, Indicator};
///
/// let mut indicator = HurstExponent::new(100, 4).unwrap();
/// let mut last = None;
/// for i in 0..200 {
///     last = indicator.update(f64::from(i));
/// }
/// assert!(last.is_some());
/// ```
#[derive(Debug, Clone)]
pub struct HurstExponent {
    period: usize,
    chunks: usize,
    window: VecDeque<f64>,
    /// Reusable scratch buffer to avoid allocating per `update`.
    scratch: Vec<f64>,
}

impl HurstExponent {
    /// Construct a new Hurst Exponent over a window of `period` inputs,
    /// fitted across `chunks` log-log points.
    ///
    /// `chunks` controls the number of R/S pairs that go into the slope
    /// fit; the typical value is `4` (the original Hurst paper used 5 — 9
    /// points; smaller windows constrain the choice).
    ///
    /// # Errors
    /// Returns [`Error::InvalidPeriod`] if `chunks < 2` or
    /// `period < 2 · chunks`.
    pub fn new(period: usize, chunks: usize) -> Result<Self> {
        if chunks < 2 {
            return Err(Error::InvalidPeriod {
                message: "Hurst chunks must be >= 2",
            });
        }
        if chunks > crate::error::MAX_PERIOD {
            return Err(Error::InvalidPeriod {
                message: crate::error::PERIOD_ABOVE_MAX,
            });
        }
        if period < 2 * chunks {
            return Err(Error::InvalidPeriod {
                message: "Hurst period must be >= 2 * chunks",
            });
        }
        Ok(Self {
            period,
            chunks,
            window: VecDeque::with_capacity(period),
            scratch: Vec::with_capacity(period),
        })
    }

    /// Configured window period.
    pub const fn period(&self) -> usize {
        self.period
    }

    /// Configured chunk count.
    pub const fn chunks(&self) -> usize {
        self.chunks
    }
}

/// R/S over a single chunk; returns `None` if the chunk has zero dispersion
/// (its stddev is zero, so the ratio is undefined).
fn rescaled_range(chunk: &[f64]) -> Option<f64> {
    let n = chunk.len() as f64;
    let mean = chunk.iter().sum::<f64>() / n;
    let mut cum = 0.0;
    let mut hi = f64::NEG_INFINITY;
    let mut lo = f64::INFINITY;
    let mut sum_sq = 0.0;
    for &x in chunk {
        let d = x - mean;
        cum += d;
        if cum > hi {
            hi = cum;
        }
        if cum < lo {
            lo = cum;
        }
        sum_sq += d * d;
    }
    let r = hi - lo;
    let s = (sum_sq / n).sqrt();
    if s == 0.0 || r == 0.0 {
        return None;
    }
    Some(r / s)
}

impl Indicator for HurstExponent {
    type Input = f64;
    type Output = f64;

    fn update(&mut self, value: f64) -> Option<f64> {
        if !value.is_finite() {
            return None;
        }
        if self.window.len() == self.period {
            self.window.pop_front();
        }
        self.window.push_back(value);
        if self.window.len() < self.period {
            return None;
        }

        // Materialise the window contiguously so chunk slicing is trivial.
        self.scratch.clear();
        self.scratch.extend(self.window.iter().copied());
        let buf = &self.scratch;
        // Build (log m, log(R/S)) points. The chunk size sweeps from period
        // (one big chunk) down to period / chunks (chunks small chunks).
        let mut sum_x = 0.0;
        let mut sum_y = 0.0;
        let mut sum_xy = 0.0;
        let mut sum_xx = 0.0;
        let mut count = 0usize;
        for k in 1..=self.chunks {
            // k chunks each of size m; ignore the integer-division leftover
            // bars at the end of the window. The `period >= 2 * chunks`
            // constructor invariant guarantees m >= 2 for every k in range.
            let m = self.period / k;
            // Average R/S across the k chunks of size m to reduce noise.
            let mut acc = 0.0;
            let mut chunks_used = 0;
            for c in 0..k {
                let start = c * m;
                let end = start + m;
                if let Some(rs) = rescaled_range(&buf[start..end]) {
                    acc += rs;
                    chunks_used += 1;
                }
            }
            if chunks_used == 0 {
                continue;
            }
            let avg_rs = acc / f64::from(chunks_used);
            let x = (m as f64).ln();
            let y = avg_rs.ln();
            sum_x += x;
            sum_y += y;
            sum_xy += x * y;
            sum_xx += x * x;
            count += 1;
        }
        if count < 2 {
            // A perfectly flat window yields no usable R/S point; the
            // canonical fallback for R/S on white noise is H = 0.5.
            return Some(0.5);
        }
        // With chunks >= 2 and period >= 2 * chunks, m_1 = period and
        // m_2 = period / 2 are always distinct, so the variance of the
        // log-m values is strictly positive and `denom > 0`.
        let n = count as f64;
        let denom = n * sum_xx - sum_x * sum_x;
        let slope = (n * sum_xy - sum_x * sum_y) / denom;
        Some(slope.clamp(0.0, 1.0))
    }

    fn reset(&mut self) {
        self.window.clear();
        self.scratch.clear();
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
        "HurstExponent"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;
    use approx::assert_relative_eq;

    #[test]
    fn rejects_invalid_parameters() {
        assert!(HurstExponent::new(10, 0).is_err());
        assert!(HurstExponent::new(10, 1).is_err());
        assert!(HurstExponent::new(3, 2).is_err());
        assert!(HurstExponent::new(4, 2).is_ok());
    }

    #[test]
    fn accessors_and_metadata() {
        let h = HurstExponent::new(100, 4).unwrap();
        assert_eq!(h.period(), 100);
        assert_eq!(h.chunks(), 4);
        assert_eq!(h.warmup_period(), 100);
        assert_eq!(h.name(), "HurstExponent");
    }

    #[test]
    fn constant_series_is_one_half() {
        let mut h = HurstExponent::new(40, 4).unwrap();
        for v in h.batch(&[42.0; 80]).into_iter().flatten() {
            assert_relative_eq!(v, 0.5, epsilon = 1e-12);
        }
    }

    #[test]
    fn output_stays_in_zero_one_range() {
        let prices: Vec<f64> = (0..400)
            .map(|i| {
                100.0
                    + (f64::from(i) * 0.05).sin() * 8.0
                    + (f64::from(i) * 0.21).cos() * 3.0
                    + f64::from(i) * 0.1
            })
            .collect();
        let mut h = HurstExponent::new(100, 4).unwrap();
        for v in h.batch(&prices).into_iter().flatten() {
            assert!((0.0..=1.0).contains(&v), "Hurst out of range: {v}");
        }
    }

    #[test]
    fn trending_series_above_half() {
        // A clean monotonic ramp is the textbook persistent series; the R/S
        // pairs must lie above the random-walk baseline.
        let prices: Vec<f64> = (0..200).map(f64::from).collect();
        let mut h = HurstExponent::new(100, 4).unwrap();
        let last = h.batch(&prices).into_iter().flatten().last().unwrap();
        assert!(
            last > 0.5,
            "trending series should have H > 0.5, got {last}"
        );
    }

    #[test]
    fn reset_clears_state() {
        let mut h = HurstExponent::new(20, 4).unwrap();
        for i in 0..20 {
            h.update(f64::from(i));
        }
        assert!(h.is_ready());
        h.reset();
        assert!(!h.is_ready());
        assert_eq!(h.update(1.0), None);
    }

    #[test]
    fn batch_equals_streaming() {
        let prices: Vec<f64> = (0..200)
            .map(|i| 100.0 + (f64::from(i) * 0.1).sin() * 5.0)
            .collect();
        let batch = HurstExponent::new(50, 4).unwrap().batch(&prices);
        let mut b = HurstExponent::new(50, 4).unwrap();
        let streamed: Vec<_> = prices.iter().map(|p| b.update(*p)).collect();
        assert_eq!(batch, streamed);
    }

    #[test]
    fn rejects_chunks_above_maximum() {
        assert!(matches!(
            HurstExponent::new(4, crate::error::MAX_PERIOD + 1),
            Err(Error::InvalidPeriod { .. })
        ));
    }
}
