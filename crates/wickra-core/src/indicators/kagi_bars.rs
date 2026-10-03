//! Kagi bar builder — reversal-amount line segments on close prices.

use crate::error::{Error, Result};
use crate::ohlcv::Candle;
use crate::traits::BarBuilder;

/// One completed Kagi line segment (the vertical run between two reversals).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct KagiBar {
    /// Price where the segment began (the previous reversal point).
    pub start: f64,
    /// Extreme price the segment reached before reversing.
    pub end: f64,
    /// `+1` for a rising segment, `-1` for a falling segment.
    pub direction: i8,
}

/// Kagi bar builder using the fixed reversal-amount method on close prices.
///
/// A Kagi chart is one continuous line that extends in its current direction as
/// long as price makes new extremes, and turns when price retraces by at least
/// `reversal` from the latest extreme. This builder emits the **completed
/// segment** each time the line turns:
///
/// - The first candle seeds the start price; the first subsequent move (of any
///   size) sets the initial direction.
/// - While the trend holds, new extremes extend the current segment silently.
/// - A retracement of `>= reversal` closes the current segment (returned from
///   [`BarBuilder::update`]) and starts a new one in the opposite direction.
///
/// At most one segment completes per candle, so `update` returns either an empty
/// vector or a single [`KagiBar`].
///
/// # Example
///
/// ```
/// use wickra_core::{BarBuilder, Candle, KagiBars};
///
/// let flat = |price: f64| Candle::new(price, price, price, price, 1.0, 0).unwrap();
/// let mut kagi = KagiBars::new(2.0).unwrap();
/// kagi.update(flat(10.0)); // seed
/// kagi.update(flat(15.0)); // rise to 15
/// let bars = kagi.update(flat(12.0)); // retrace >= 2 -> closes the up segment
/// assert_eq!(bars.len(), 1);
/// assert_eq!(bars[0].direction, 1);
/// ```
#[derive(Debug, Clone)]
pub struct KagiBars {
    reversal: f64,
    dir: i8,
    extreme: Option<f64>,
    segment_start: f64,
}

impl KagiBars {
    /// Construct a Kagi builder with the given reversal amount.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidPeriod`] if `reversal` is not finite and positive.
    pub fn new(reversal: f64) -> Result<Self> {
        if !reversal.is_finite() || reversal <= 0.0 {
            return Err(Error::InvalidPeriod {
                message: "reversal must be finite and positive",
            });
        }
        Ok(Self {
            reversal,
            dir: 0,
            extreme: None,
            segment_start: 0.0,
        })
    }

    /// Configured reversal amount.
    pub const fn reversal(&self) -> f64 {
        self.reversal
    }

    /// Current extreme price (or the seed price before any move).
    pub const fn extreme(&self) -> Option<f64> {
        self.extreme
    }
}

impl BarBuilder for KagiBars {
    type Bar = KagiBar;

    fn update(&mut self, candle: Candle) -> Vec<KagiBar> {
        let close = candle.close;
        let Some(mut ext) = self.extreme else {
            self.extreme = Some(close);
            self.segment_start = close;
            return Vec::new();
        };
        let mut bars = Vec::new();
        match self.dir {
            0 => {
                if close > ext {
                    self.dir = 1;
                    ext = close;
                } else if close < ext {
                    self.dir = -1;
                    ext = close;
                }
            }
            1 => {
                if close > ext {
                    ext = close;
                } else if close <= ext - self.reversal {
                    bars.push(KagiBar {
                        start: self.segment_start,
                        end: ext,
                        direction: 1,
                    });
                    self.segment_start = ext;
                    self.dir = -1;
                    ext = close;
                }
            }
            _ => {
                if close < ext {
                    ext = close;
                } else if close >= ext + self.reversal {
                    bars.push(KagiBar {
                        start: self.segment_start,
                        end: ext,
                        direction: -1,
                    });
                    self.segment_start = ext;
                    self.dir = 1;
                    ext = close;
                }
            }
        }
        self.extreme = Some(ext);
        bars
    }

    fn reset(&mut self) {
        self.dir = 0;
        self.extreme = None;
        self.segment_start = 0.0;
    }

    #[inline]
    fn name(&self) -> &'static str {
        "KagiBars"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_relative_eq;

    fn flat(price: f64) -> Candle {
        Candle::new(price, price, price, price, 1.0, 0).unwrap()
    }

    #[test]
    fn rejects_invalid_reversal() {
        assert!(matches!(
            KagiBars::new(0.0),
            Err(Error::InvalidPeriod { .. })
        ));
        assert!(matches!(
            KagiBars::new(-2.0),
            Err(Error::InvalidPeriod { .. })
        ));
        assert!(matches!(
            KagiBars::new(f64::INFINITY),
            Err(Error::InvalidPeriod { .. })
        ));
    }

    #[test]
    fn accessors_and_metadata() {
        let kagi = KagiBars::new(2.0).unwrap();
        assert_eq!(kagi.name(), "KagiBars");
        assert_relative_eq!(kagi.reversal(), 2.0, epsilon = 1e-12);
        assert_eq!(kagi.extreme(), None);
    }

    #[test]
    fn seeds_then_establishes_up_direction() {
        let mut kagi = KagiBars::new(2.0).unwrap();
        assert_eq!(kagi.update(flat(10.0)), Vec::new()); // seed
        assert_eq!(kagi.extreme(), Some(10.0));
        assert_eq!(kagi.update(flat(11.0)), Vec::new()); // first move sets dir up
        assert_eq!(kagi.extreme(), Some(11.0));
    }

    #[test]
    fn establishes_down_direction_from_seed() {
        let mut kagi = KagiBars::new(2.0).unwrap();
        kagi.update(flat(10.0));
        assert_eq!(kagi.update(flat(9.0)), Vec::new()); // first move sets dir down
        assert_eq!(kagi.extreme(), Some(9.0));
    }

    #[test]
    fn extends_without_emitting() {
        let mut kagi = KagiBars::new(2.0).unwrap();
        kagi.update(flat(10.0));
        kagi.update(flat(11.0));
        assert_eq!(kagi.update(flat(15.0)), Vec::new()); // new high, extend
        assert_eq!(kagi.extreme(), Some(15.0));
    }

    #[test]
    fn reversal_closes_up_segment() {
        let mut kagi = KagiBars::new(2.0).unwrap();
        kagi.update(flat(10.0));
        kagi.update(flat(11.0));
        kagi.update(flat(15.0));
        let bars = kagi.update(flat(12.0)); // retrace 3 >= 2
        assert_eq!(bars.len(), 1);
        assert_eq!(bars[0].direction, 1);
        assert_relative_eq!(bars[0].start, 10.0, epsilon = 1e-12);
        assert_relative_eq!(bars[0].end, 15.0, epsilon = 1e-12);
        assert_eq!(kagi.extreme(), Some(12.0));
    }

    #[test]
    fn reversal_closes_down_segment() {
        let mut kagi = KagiBars::new(2.0).unwrap();
        kagi.update(flat(10.0));
        kagi.update(flat(11.0));
        kagi.update(flat(15.0));
        kagi.update(flat(12.0)); // now dir down, segment_start 15, extreme 12
        let bars = kagi.update(flat(20.0)); // rise 8 >= 2 -> closes down segment
        assert_eq!(bars.len(), 1);
        assert_eq!(bars[0].direction, -1);
        assert_relative_eq!(bars[0].start, 15.0, epsilon = 1e-12);
        assert_relative_eq!(bars[0].end, 12.0, epsilon = 1e-12);
    }

    #[test]
    fn small_pullback_does_not_reverse() {
        let mut kagi = KagiBars::new(2.0).unwrap();
        kagi.update(flat(10.0));
        kagi.update(flat(11.0));
        kagi.update(flat(15.0));
        assert_eq!(kagi.update(flat(14.0)), Vec::new()); // retrace 1 < 2
        assert_eq!(kagi.extreme(), Some(15.0));
    }

    #[test]
    fn down_trend_small_bounce_does_not_reverse() {
        let mut kagi = KagiBars::new(2.0).unwrap();
        kagi.update(flat(10.0));
        kagi.update(flat(9.0)); // dir down
        kagi.update(flat(5.0)); // extreme 5
        assert_eq!(kagi.update(flat(6.0)), Vec::new()); // bounce 1 < 2
        assert_eq!(kagi.extreme(), Some(5.0));
    }

    #[test]
    fn reset_clears_state() {
        let mut kagi = KagiBars::new(2.0).unwrap();
        kagi.update(flat(10.0));
        kagi.update(flat(15.0));
        kagi.reset();
        assert_eq!(kagi.extreme(), None);
        assert_eq!(kagi.update(flat(99.0)), Vec::new());
        assert_eq!(kagi.extreme(), Some(99.0));
    }

    #[test]
    fn batch_collects_completed_segments() {
        let mut kagi = KagiBars::new(2.0).unwrap();
        let candles = [
            flat(10.0),
            flat(15.0),
            flat(12.0), // closes up segment
            flat(20.0), // closes down segment
        ];
        let bars = kagi.batch(&candles);
        assert_eq!(bars.len(), 2);
        assert_eq!(bars[0].direction, 1);
        assert_eq!(bars[1].direction, -1);
    }
}
