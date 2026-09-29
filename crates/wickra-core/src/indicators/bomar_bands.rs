//! Bomar Bands — adaptive percentage bands that contain a target fraction of
//! recent price.

use std::collections::VecDeque;

use crate::error::{Error, Result};
use crate::indicators::rolling_quantile::quantile_sorted;
use crate::indicators::sorted_window;
use crate::traits::Indicator;

/// Bomar Bands output.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BomarBandsOutput {
    /// Upper band: `middle + |middle| · p`.
    pub upper: f64,
    /// Middle line: the simple moving average over the window.
    pub middle: f64,
    /// Lower band: `middle − |middle| · p`.
    pub lower: f64,
}

/// Bomar Bands: percentage bands whose width adapts so that a fixed `coverage`
/// fraction of recent closes falls inside them.
///
/// The Bomar Bands predate Bollinger Bands; John Bollinger cites them as an
/// inspiration — percentage bands around a moving average, with the percentage
/// tuned so a fixed share (classically ~85%) of price stayed within. Wickra
/// realises that idea deterministically: the half-width is the `coverage`
/// quantile of the relative deviations from the midline, so by construction
/// `coverage` of the window's closes lie inside the bands.
///
/// ```text
/// middle = SMA(close, period)
/// dev_i  = | close_i / middle − 1 |          // relative distance from midline
/// p      = coverage-quantile of { dev_i }     // type-7 interpolation
/// upper  = middle + |middle| · p
/// lower  = middle − |middle| · p
/// ```
///
/// Unlike the fixed-percentage [`MaEnvelope`](crate::MaEnvelope), the offset
/// here is data-driven: the bands widen in turbulent regimes and tighten in
/// quiet ones without a volatility input. Unlike Bollinger Bands, the width is
/// an order statistic of the actual deviations rather than a multiple of the
/// standard deviation, so it is unaffected by the shape of the tails beyond the
/// `coverage` rank. When the midline is zero the relative deviation is
/// undefined and the bands collapse onto the midline.
///
/// # Example
///
/// ```
/// use wickra_core::{BomarBands, Indicator};
///
/// let mut indicator = BomarBands::new(20, 0.85).unwrap();
/// let mut last = None;
/// for i in 0..40 {
///     last = indicator.update(100.0 + f64::from(i % 7));
/// }
/// assert!(last.is_some());
/// ```
#[derive(Debug, Clone)]
pub struct BomarBands {
    period: usize,
    coverage: f64,
    window: VecDeque<f64>,
    /// The window's values in `total_cmp` order, kept sorted as it slides
    /// (from a period of [`MERGE_FROM`]; empty below it).
    sorted: Vec<f64>,
    /// The relative deviations from the mean, in ascending order.
    scratch: Vec<f64>,
}

/// The period from which the sorted deviations are merged from a window kept
/// sorted rather than sorted afresh: below it, sorting a short slice is
/// cheaper than keeping the window in order and merging two runs. Both give
/// the same sorted sequence, bit for bit.
const MERGE_FROM: usize = 32;

impl BomarBands {
    /// Construct new Bomar Bands.
    ///
    /// `coverage` is the target fraction of closes to contain, in `(0.0, 1.0]`.
    ///
    /// # Errors
    /// Returns [`Error::PeriodZero`] if `period == 0`, or
    /// [`Error::InvalidParameter`] if `coverage` is not a finite value in
    /// `(0.0, 1.0]`.
    pub fn new(period: usize, coverage: f64) -> Result<Self> {
        if period == 0 {
            return Err(Error::PeriodZero);
        }
        if period > crate::error::MAX_PERIOD {
            return Err(Error::InvalidPeriod {
                message: crate::error::PERIOD_ABOVE_MAX,
            });
        }
        if !coverage.is_finite() || coverage <= 0.0 || coverage > 1.0 {
            return Err(Error::InvalidParameter {
                message: "bomar bands coverage must be a finite value in (0.0, 1.0]",
            });
        }
        Ok(Self {
            period,
            coverage,
            window: VecDeque::with_capacity(period),
            sorted: Vec::with_capacity(period),
            scratch: Vec::with_capacity(period),
        })
    }

    /// Configured period.
    pub const fn period(&self) -> usize {
        self.period
    }

    /// Configured coverage fraction.
    pub const fn coverage(&self) -> f64 {
        self.coverage
    }
}

impl Indicator for BomarBands {
    type Input = f64;
    type Output = BomarBandsOutput;

    #[inline]
    fn update(&mut self, value: f64) -> Option<BomarBandsOutput> {
        if !value.is_finite() {
            return None;
        }
        let merge = self.period >= MERGE_FROM;
        if self.window.len() == self.period {
            let oldest = self.window.pop_front().expect("window is full");
            if merge {
                sorted_window::remove(&mut self.sorted, oldest);
            }
        }
        self.window.push_back(value);
        if merge {
            sorted_window::insert(&mut self.sorted, value);
        }
        if self.window.len() < self.period {
            return None;
        }
        let sum: f64 = self.window.iter().sum();
        let middle = sum / (self.period as f64);
        let denom = middle.abs();

        if denom == 0.0 {
            self.scratch.clear();
            self.scratch.resize(self.period, 0.0);
        } else if merge {
            // `|(v - middle) / denom|` falls towards the mean and rises past
            // it, so the sorted deviations come from merging the two runs.
            sorted_window::distances(
                &self.sorted,
                middle,
                |v| ((v - middle) / denom).abs(),
                &mut self.scratch,
            );
        } else {
            self.scratch.clear();
            let devs = self.window.iter().map(|&v| ((v - middle) / denom).abs());
            self.scratch.extend(devs);
            self.scratch.sort_by(f64::total_cmp);
        }
        let p = quantile_sorted(&self.scratch, self.coverage);
        let offset = denom * p;

        Some(BomarBandsOutput {
            upper: middle + offset,
            middle,
            lower: middle - offset,
        })
    }

    fn reset(&mut self) {
        self.window.clear();
        self.sorted.clear();
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
        "BomarBands"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;
    use approx::assert_relative_eq;

    /// Bands from a freshly sorted copy of `window`, as each update used to
    /// compute them.
    fn from_scratch(window: &[f64], coverage: f64) -> [f64; 3] {
        let middle = window.iter().sum::<f64>() / window.len() as f64;
        let denom = middle.abs();
        let mut devs: Vec<f64> = window
            .iter()
            .map(|&v| {
                if denom == 0.0 {
                    0.0
                } else {
                    ((v - middle) / denom).abs()
                }
            })
            .collect();
        devs.sort_by(f64::total_cmp);
        let offset = denom * quantile_sorted(&devs, coverage);
        [middle + offset, middle, middle - offset]
    }

    #[test]
    fn short_and_long_periods_match_a_fresh_sort() {
        let prices: Vec<f64> = (0..400)
            .map(|i| {
                let t = f64::from(i);
                100.0 + ((t * 0.07).sin() * 4.0).round() / 2.0 + (t * 0.31).cos()
            })
            .collect();
        for period in [5, MERGE_FROM - 1, MERGE_FROM, 60] {
            let mut bands = BomarBands::new(period, 0.85).unwrap();
            for (i, &price) in prices.iter().enumerate() {
                let got = bands.update(price);
                if i + 1 >= period {
                    let o = got.unwrap();
                    let want = from_scratch(&prices[i + 1 - period..=i], 0.85);
                    assert_eq!(
                        [o.upper.to_bits(), o.middle.to_bits(), o.lower.to_bits()],
                        want.map(f64::to_bits),
                        "period {period} at {i}"
                    );
                }
            }
        }
    }

    #[test]
    fn rejects_zero_period() {
        assert!(matches!(BomarBands::new(0, 0.85), Err(Error::PeriodZero)));
        assert!(BomarBands::new(1, 0.85).is_ok());
    }

    #[test]
    fn rejects_out_of_range_coverage() {
        assert!(matches!(
            BomarBands::new(20, 0.0),
            Err(Error::InvalidParameter { .. })
        ));
        assert!(matches!(
            BomarBands::new(20, 1.1),
            Err(Error::InvalidParameter { .. })
        ));
        assert!(matches!(
            BomarBands::new(20, -0.5),
            Err(Error::InvalidParameter { .. })
        ));
        assert!(matches!(
            BomarBands::new(20, f64::NAN),
            Err(Error::InvalidParameter { .. })
        ));
    }

    #[test]
    fn accessors_and_metadata() {
        let bb = BomarBands::new(20, 0.85).unwrap();
        assert_eq!(bb.period(), 20);
        assert_relative_eq!(bb.coverage(), 0.85, epsilon = 1e-12);
        assert_eq!(bb.warmup_period(), 20);
        assert_eq!(bb.name(), "BomarBands");
        assert!(!bb.is_ready());
    }

    #[test]
    fn warms_up_then_emits() {
        let mut bb = BomarBands::new(4, 0.85).unwrap();
        assert!(bb.update(100.0).is_none());
        assert!(bb.update(102.0).is_none());
        assert!(bb.update(98.0).is_none());
        assert!(bb.update(104.0).is_some());
        assert!(bb.is_ready());
    }

    #[test]
    fn known_bands() {
        // mean=101; |dev| = {1,1,3,3}/101; coverage 0.85 quantile -> 3/101.
        // offset = 101 * 3/101 = 3 -> upper 104, lower 98.
        let mut bb = BomarBands::new(4, 0.85).unwrap();
        let out = bb.batch(&[100.0, 102.0, 98.0, 104.0]);
        let last = out[3].unwrap();
        assert_relative_eq!(last.middle, 101.0, epsilon = 1e-9);
        assert_relative_eq!(last.upper, 104.0, epsilon = 1e-9);
        assert_relative_eq!(last.lower, 98.0, epsilon = 1e-9);
    }

    #[test]
    fn zero_midline_collapses_bands() {
        // Window mean exactly zero -> relative deviation undefined -> collapse.
        let mut bb = BomarBands::new(2, 0.85).unwrap();
        let out = bb.batch(&[3.0, -3.0]);
        let last = out[1].unwrap();
        assert_relative_eq!(last.middle, 0.0, epsilon = 1e-12);
        assert_relative_eq!(last.upper, 0.0, epsilon = 1e-12);
        assert_relative_eq!(last.lower, 0.0, epsilon = 1e-12);
    }

    #[test]
    fn rolling_window_evicts_oldest() {
        // Eight values through a period-4 window: only the last four survive,
        // reproducing the `known_bands` window.
        let mut bb = BomarBands::new(4, 0.85).unwrap();
        let out = bb.batch(&[50.0, 50.0, 50.0, 50.0, 100.0, 102.0, 98.0, 104.0]);
        let last = out[7].unwrap();
        assert_relative_eq!(last.middle, 101.0, epsilon = 1e-9);
        assert_relative_eq!(last.upper, 104.0, epsilon = 1e-9);
        assert_relative_eq!(last.lower, 98.0, epsilon = 1e-9);
    }

    #[test]
    fn reset_clears_state() {
        let mut bb = BomarBands::new(4, 0.85).unwrap();
        for v in [100.0, 102.0, 98.0, 104.0] {
            bb.update(v);
        }
        assert!(bb.is_ready());
        bb.reset();
        assert!(!bb.is_ready());
        assert!(bb.update(100.0).is_none());
    }
}
