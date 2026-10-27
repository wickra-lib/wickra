//! Polarized Fractal Efficiency (PFE).

use std::collections::VecDeque;

use crate::error::{Error, Result};
use crate::indicators::ema::Ema;
use crate::traits::Indicator;

/// Polarized Fractal Efficiency: how efficiently price travelled over the last
/// `period` bars, signed by direction and smoothed by an EMA.
///
/// ```text
/// straight  = sqrt((C_t - C_{t-n})^2 + n^2)            (direct distance over n bars)
/// path      = Σ_{i=1..n} sqrt((C_{t-i+1} - C_{t-i})^2 + 1)   (sum of single-bar steps)
/// raw       = 100 * sign(C_t - C_{t-n}) * straight / path
/// PFE       = EMA(raw, smoothing)
/// ```
///
/// The ratio `straight / path` is the fractal efficiency: it is `1` when price
/// moved in a perfectly straight line and falls toward `0` as the path becomes
/// jagged. Polarizing it by the sign of the net move pushes the reading to
/// `+100` for an efficient up-move and `-100` for an efficient down-move, with
/// choppy markets oscillating near zero. Because each single-bar step and the
/// `n`-bar diagonal both carry the bar count on the x-axis (`+1` and `+n^2`),
/// the path length is always `>= n`, so the denominator can never be zero.
///
/// Reference: Hans Hannula, *Stocks & Commodities*, 1994.
///
/// # Example
///
/// ```
/// use wickra_core::{Indicator, PolarizedFractalEfficiency};
///
/// let mut indicator = PolarizedFractalEfficiency::new(10, 5).unwrap();
/// let mut last = None;
/// for i in 0..40 {
///     last = indicator.update(100.0 + f64::from(i));
/// }
/// assert!(last.is_some());
/// ```
#[derive(Debug, Clone)]
pub struct PolarizedFractalEfficiency {
    period: usize,
    smoothing: usize,
    closes: VecDeque<f64>,
    prev_close: Option<f64>,
    segments: VecDeque<f64>,
    segment_sum: f64,
    ema: Ema,
}

impl PolarizedFractalEfficiency {
    /// Construct a PFE with the fractal lookback `period` and the EMA
    /// `smoothing` period.
    ///
    /// # Errors
    ///
    /// Returns [`Error::PeriodZero`] if `period == 0` or `smoothing == 0`.
    pub fn new(period: usize, smoothing: usize) -> Result<Self> {
        if period == 0 {
            return Err(Error::PeriodZero);
        }
        if period > crate::error::MAX_PERIOD {
            return Err(Error::InvalidPeriod {
                message: crate::error::PERIOD_ABOVE_MAX,
            });
        }
        Ok(Self {
            period,
            smoothing,
            closes: VecDeque::with_capacity(period + 1),
            prev_close: None,
            segments: VecDeque::with_capacity(period),
            segment_sum: 0.0,
            ema: Ema::new(smoothing)?,
        })
    }

    /// Configured `(period, smoothing)`.
    pub const fn periods(&self) -> (usize, usize) {
        (self.period, self.smoothing)
    }
}

impl Indicator for PolarizedFractalEfficiency {
    type Input = f64;
    type Output = f64;

    #[inline]
    fn update(&mut self, close: f64) -> Option<f64> {
        if !close.is_finite() {
            return None;
        }
        if let Some(prev) = self.prev_close {
            let diff = close - prev;
            let segment = diff.mul_add(diff, 1.0).sqrt();
            self.segment_sum += segment;
            self.segments.push_back(segment);
            if self.segments.len() > self.period {
                self.segment_sum -= self.segments.pop_front().unwrap_or(0.0);
            }
        }
        self.prev_close = Some(close);

        self.closes.push_back(close);
        if self.closes.len() > self.period + 1 {
            self.closes.pop_front();
        }
        if self.closes.len() <= self.period {
            return None;
        }

        let oldest = *self.closes.front().unwrap_or(&close);
        let net = close - oldest;
        let direction = if net > 0.0 {
            1.0
        } else if net < 0.0 {
            -1.0
        } else {
            0.0
        };
        let span = self.period as f64;
        let straight = net.mul_add(net, span * span).sqrt();
        let raw = 100.0 * direction * straight / self.segment_sum;
        self.ema.update(raw)
    }

    fn reset(&mut self) {
        self.closes.clear();
        self.prev_close = None;
        self.segments.clear();
        self.segment_sum = 0.0;
        self.ema.reset();
    }

    #[inline]
    fn warmup_period(&self) -> usize {
        self.period + self.smoothing
    }

    #[inline]
    fn is_ready(&self) -> bool {
        self.ema.is_ready()
    }

    #[inline]
    fn name(&self) -> &'static str {
        "PolarizedFractalEfficiency"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;
    use approx::assert_relative_eq;

    #[test]
    fn rejects_zero_period() {
        assert!(matches!(
            PolarizedFractalEfficiency::new(0, 5),
            Err(Error::PeriodZero)
        ));
        assert!(matches!(
            PolarizedFractalEfficiency::new(10, 0),
            Err(Error::PeriodZero)
        ));
    }

    #[test]
    fn accessors_and_metadata() {
        let pfe = PolarizedFractalEfficiency::new(10, 5).unwrap();
        assert_eq!(pfe.periods(), (10, 5));
        assert_eq!(pfe.warmup_period(), 15);
        assert_eq!(pfe.name(), "PolarizedFractalEfficiency");
        assert!(!pfe.is_ready());
    }

    #[test]
    fn warmup_emits_after_period_plus_smoothing() {
        let mut pfe = PolarizedFractalEfficiency::new(4, 2).unwrap();
        // raw needs period+1 = 5 closes; EMA(2) needs 2 raws -> first value at
        // input 6 (index 5).
        let inputs: Vec<f64> = (0..10).map(f64::from).collect();
        let out = pfe.batch(&inputs);
        assert!(out[4].is_none());
        assert!(out[5].is_some());
    }

    #[test]
    fn perfect_uptrend_is_strongly_positive() {
        // A straight ramp: every step is +1, the diagonal is maximally
        // efficient, so PFE saturates near +100.
        let mut pfe = PolarizedFractalEfficiency::new(5, 3).unwrap();
        let inputs: Vec<f64> = (0..30).map(f64::from).collect();
        let last = pfe.batch(&inputs).last().unwrap().unwrap();
        assert!(last > 99.0, "pfe {last} should be near +100");
    }

    #[test]
    fn perfect_downtrend_is_strongly_negative() {
        let mut pfe = PolarizedFractalEfficiency::new(5, 3).unwrap();
        let inputs: Vec<f64> = (0..30).map(|i| -f64::from(i)).collect();
        let last = pfe.batch(&inputs).last().unwrap().unwrap();
        assert!(last < -99.0, "pfe {last} should be near -100");
    }

    #[test]
    fn flat_market_returns_zero() {
        // No net move over the window -> direction 0 -> raw 0 -> PFE 0.
        let mut pfe = PolarizedFractalEfficiency::new(5, 3).unwrap();
        let inputs = [10.0; 20];
        let last = pfe.batch(&inputs).last().unwrap().unwrap();
        assert_relative_eq!(last, 0.0, epsilon = 1e-12);
    }

    #[test]
    fn choppy_market_is_inefficient() {
        // A sawtooth whip: the net move is tiny relative to the jagged path, so
        // efficiency stays well below the +-100 saturation of a clean trend.
        let mut pfe = PolarizedFractalEfficiency::new(5, 3).unwrap();
        let inputs: Vec<f64> = (0..40)
            .map(|i| if i % 2 == 0 { 100.0 } else { 102.0 })
            .collect();
        let last = pfe.batch(&inputs).last().unwrap().unwrap();
        assert!(
            last.abs() < 60.0,
            "choppy pfe {last} should be far from +-100"
        );
    }

    #[test]
    fn reset_clears_state() {
        let mut pfe = PolarizedFractalEfficiency::new(5, 3).unwrap();
        let inputs: Vec<f64> = (0..30).map(f64::from).collect();
        pfe.batch(&inputs);
        assert!(pfe.is_ready());
        pfe.reset();
        assert!(!pfe.is_ready());
        assert_eq!(pfe.periods(), (5, 3));
    }

    #[test]
    fn batch_equals_streaming() {
        let inputs: Vec<f64> = (0..80)
            .map(|i| 100.0 + (f64::from(i) * 0.3).sin() * 5.0)
            .collect();
        let mut a = PolarizedFractalEfficiency::new(10, 5).unwrap();
        let mut b = PolarizedFractalEfficiency::new(10, 5).unwrap();
        assert_eq!(
            a.batch(&inputs),
            inputs.iter().map(|x| b.update(*x)).collect::<Vec<_>>()
        );
    }

    #[test]
    fn rejects_period_above_maximum() {
        assert!(matches!(
            PolarizedFractalEfficiency::new(crate::error::MAX_PERIOD + 1, 5),
            Err(Error::InvalidPeriod { .. })
        ));
    }
}
