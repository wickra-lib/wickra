//! Bollinger bands on the spread of two series, for pairs mean-reversion trading.

use std::collections::VecDeque;

use crate::error::{Error, Result};
use crate::indicators::rolling_moments::ShiftedMoments;
use crate::traits::Indicator;

/// Output of [`SpreadBollingerBands`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SpreadBollingerBandsOutput {
    /// Middle band: the rolling mean of the spread.
    pub middle: f64,
    /// Upper band: `middle + num_std · σ`.
    pub upper: f64,
    /// Lower band: `middle − num_std · σ`.
    pub lower: f64,
    /// `%b`: where the current spread sits across the band, `(s − lower) /
    /// (upper − lower)`. `0` is the lower band, `1` the upper, `0.5` the middle.
    /// Reported as `0.5` when the band has zero width (a flat spread).
    pub percent_b: f64,
}

/// Bollinger bands on the spread `a − b` of two series.
///
/// Each `update` takes one `(a, b)` price pair and forms the spread
/// `sₜ = aₜ − bₜ`. Over the trailing window of `period` spreads it builds a
/// classic Bollinger envelope:
///
/// ```text
/// middle = mean(s)        σ = stddev(s)
/// upper  = middle + num_std · σ
/// lower  = middle − num_std · σ
/// %b     = (s_now − lower) / (upper − lower)
/// ```
///
/// Applied to a spread rather than a price, the bands are a ready-made pairs
/// mean-reversion signal: the spread riding the **upper** band is stretched
/// rich (a short-the-spread setup), the **lower** band stretched cheap, and a
/// return to the **middle** is the exit. `%b` compresses the location into one
/// number for thresholding. The spread is the raw difference `a − b`, so feed
/// already-comparable legs (e.g. a hedged pair, two yields, or log prices); pair
/// this with [`crate::BetaNeutralSpread`] when the legs need a hedge ratio first.
///
/// A flat spread yields a zero-width band; `%b` is then reported as the neutral
/// `0.5`. Each `update` is `O(1)`: the mean and variance come from two running
/// sums maintained as the window slides.
///
/// # Example
///
/// ```
/// use wickra_core::{Indicator, SpreadBollingerBands};
///
/// let mut bb = SpreadBollingerBands::new(20, 2.0).unwrap();
/// let mut last = None;
/// for t in 0..60 {
///     let b = 100.0 + f64::from(t);
///     let a = b + 2.0 * (f64::from(t) * 0.5).sin();
///     last = bb.update((a, b));
/// }
/// let out = last.unwrap();
/// assert!(out.lower <= out.middle && out.middle <= out.upper);
/// ```
#[derive(Debug, Clone)]
pub struct SpreadBollingerBands {
    period: usize,
    num_std: f64,
    window: VecDeque<f64>,
    moments: ShiftedMoments,
}

impl SpreadBollingerBands {
    /// Construct new spread Bollinger bands.
    ///
    /// `period` is the look-back window; `num_std` is the band width in standard
    /// deviations.
    ///
    /// # Errors
    /// Returns [`Error::InvalidPeriod`] if `period < 2`, or
    /// [`Error::InvalidParameter`] if `num_std` is not strictly positive (and
    /// finite).
    pub fn new(period: usize, num_std: f64) -> Result<Self> {
        if period < 2 {
            return Err(Error::InvalidPeriod {
                message: "spread bollinger bands needs period >= 2",
            });
        }
        if period > crate::error::MAX_PERIOD {
            return Err(Error::InvalidPeriod {
                message: crate::error::PERIOD_ABOVE_MAX,
            });
        }
        if !num_std.is_finite() || num_std <= 0.0 {
            return Err(Error::InvalidParameter {
                message: "spread bollinger bands needs num_std > 0",
            });
        }
        Ok(Self {
            period,
            num_std,
            window: VecDeque::with_capacity(period),
            moments: ShiftedMoments::new(),
        })
    }

    /// Configured look-back window.
    pub const fn period(&self) -> usize {
        self.period
    }

    /// Configured band width in standard deviations.
    pub const fn num_std(&self) -> f64 {
        self.num_std
    }
}

impl Indicator for SpreadBollingerBands {
    type Input = (f64, f64);
    type Output = SpreadBollingerBandsOutput;

    #[inline]
    fn update(&mut self, input: (f64, f64)) -> Option<SpreadBollingerBandsOutput> {
        let (a, b) = input;
        if !a.is_finite() || !b.is_finite() {
            return None;
        }
        let spread = a - b;
        if self.window.len() == self.period {
            let old = self.window.pop_front().expect("non-empty");
            self.moments.evict(old);
        }
        self.window.push_back(spread);
        self.moments.push(spread);
        if self.moments.needs_reseed(self.period) {
            self.moments.reseed(self.window.iter().copied());
        }
        if self.window.len() < self.period {
            return None;
        }
        let middle = self.moments.mean(self.period);
        let sigma = self.moments.std_dev(self.period);
        let half_width = self.num_std * sigma;
        let upper = middle + half_width;
        let lower = middle - half_width;
        let percent_b = if half_width == 0.0 {
            0.5
        } else {
            (spread - lower) / (upper - lower)
        };
        Some(SpreadBollingerBandsOutput {
            middle,
            upper,
            lower,
            percent_b,
        })
    }

    fn reset(&mut self) {
        self.window.clear();
        self.moments.reset();
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
        "SpreadBollingerBands"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;
    use approx::assert_relative_eq;

    #[test]
    fn rejects_bad_parameters() {
        assert!(SpreadBollingerBands::new(1, 2.0).is_err());
        assert!(SpreadBollingerBands::new(20, 0.0).is_err());
        assert!(SpreadBollingerBands::new(20, -1.0).is_err());
        assert!(SpreadBollingerBands::new(20, f64::NAN).is_err());
        assert!(SpreadBollingerBands::new(2, 2.0).is_ok());
    }

    #[test]
    fn accessors_and_metadata() {
        let bb = SpreadBollingerBands::new(20, 2.5).unwrap();
        assert_eq!(bb.period(), 20);
        assert_eq!(bb.num_std(), 2.5);
        assert_eq!(bb.warmup_period(), 20);
        assert_eq!(bb.name(), "SpreadBollingerBands");
        assert!(!bb.is_ready());
    }

    #[test]
    fn warmup_returns_none() {
        let mut bb = SpreadBollingerBands::new(3, 2.0).unwrap();
        assert_eq!(bb.update((1.0, 0.0)), None);
        assert_eq!(bb.update((2.0, 0.0)), None);
        assert!(bb.update((3.0, 0.0)).is_some());
        assert!(bb.is_ready());
    }

    #[test]
    fn hand_computed_value() {
        // Spreads 1,2,3,4 (b = 0), period 4, num_std 2:
        //   mean = 2.5, σ = √1.25, upper = 2.5 + 2√1.25, lower = 2.5 − 2√1.25,
        //   %b at s = 4 ⇒ 0.8354102.
        let pairs = [(1.0, 0.0), (2.0, 0.0), (3.0, 0.0), (4.0, 0.0)];
        let out = SpreadBollingerBands::new(4, 2.0)
            .unwrap()
            .batch(&pairs)
            .into_iter()
            .flatten()
            .last()
            .unwrap();
        assert_relative_eq!(out.middle, 2.5, epsilon = 1e-9);
        assert_relative_eq!(out.upper, 4.736_067_977_499_79, epsilon = 1e-9);
        assert_relative_eq!(out.lower, 0.263_932_022_500_21, epsilon = 1e-9);
        assert_relative_eq!(out.percent_b, 0.835_410_196_624_97, epsilon = 1e-9);
    }

    #[test]
    fn flat_spread_collapses_band() {
        // a − b constant ⇒ σ = 0 ⇒ upper = middle = lower, %b = 0.5.
        let pairs: Vec<(f64, f64)> = (0..10)
            .map(|t| (5.0 + f64::from(t), f64::from(t)))
            .collect();
        let out = SpreadBollingerBands::new(5, 2.0)
            .unwrap()
            .batch(&pairs)
            .into_iter()
            .flatten()
            .last()
            .unwrap();
        assert_relative_eq!(out.upper, out.middle, epsilon = 1e-12);
        assert_relative_eq!(out.lower, out.middle, epsilon = 1e-12);
        assert_relative_eq!(out.percent_b, 0.5, epsilon = 1e-12);
    }

    #[test]
    fn bands_are_ordered() {
        let pairs: Vec<(f64, f64)> = (0..80)
            .map(|t| {
                let b = 100.0 + f64::from(t);
                (b + 3.0 * (f64::from(t) * 0.4).sin(), b)
            })
            .collect();
        let mut bb = SpreadBollingerBands::new(20, 2.0).unwrap();
        for out in bb.batch(&pairs).into_iter().flatten() {
            assert!(out.lower <= out.middle && out.middle <= out.upper);
        }
    }

    #[test]
    fn reset_clears_state() {
        let mut bb = SpreadBollingerBands::new(4, 2.0).unwrap();
        bb.batch(&[(1.0, 0.0), (2.0, 0.0), (3.0, 0.0), (4.0, 0.0), (5.0, 0.0)]);
        assert!(bb.is_ready());
        bb.reset();
        assert!(!bb.is_ready());
        assert_eq!(bb.update((1.0, 0.0)), None);
    }

    #[test]
    fn batch_equals_streaming() {
        let pairs: Vec<(f64, f64)> = (0..60)
            .map(|t| {
                let b = 30.0 + 0.7 * f64::from(t);
                (b + (f64::from(t) * 0.4).sin() * 1.5, b)
            })
            .collect();
        let batch = SpreadBollingerBands::new(15, 2.0).unwrap().batch(&pairs);
        let mut bb = SpreadBollingerBands::new(15, 2.0).unwrap();
        let streamed: Vec<_> = pairs.iter().map(|p| bb.update(*p)).collect();
        assert_eq!(batch, streamed);
    }

    #[test]
    fn rejects_period_above_maximum() {
        assert!(matches!(
            SpreadBollingerBands::new(crate::error::MAX_PERIOD + 1, 2.0),
            Err(Error::InvalidPeriod { .. })
        ));
    }
}
