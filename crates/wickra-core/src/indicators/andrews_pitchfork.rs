//! Andrews Pitchfork — median line and parallels off the last three swing pivots.

use std::collections::VecDeque;

use crate::error::{Error, Result};
use crate::ohlcv::Candle;
use crate::traits::Indicator;

/// Output of [`AndrewsPitchfork`]: the three pitchfork lines projected to the
/// current bar.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AndrewsPitchforkOutput {
    /// The median line — from the handle pivot through the midpoint of the other two.
    pub median: f64,
    /// The upper parallel (through the higher of the two anchor pivots).
    pub upper: f64,
    /// The lower parallel (through the lower of the two anchor pivots).
    pub lower: f64,
}

/// A confirmed swing pivot: its bar index and price.
#[derive(Debug, Clone, Copy)]
struct Pivot {
    index: f64,
    price: f64,
    is_high: bool,
}

/// Andrews Pitchfork — Alan Andrews' median-line tool drawn from the three most
/// recent **swing pivots**, projected forward to the current bar.
///
/// ```text
/// detect alternating swing highs/lows with a `strength`-bar fractal
/// P0 = handle (oldest of the last three), P1, P2 = the next two
/// M  = midpoint of P1 and P2
/// median(t) = P0 + slope·(t − t0)          slope = (M − P0) / (M_t − t0)
/// upper / lower = median(t) offset by the vertical gap to the higher / lower anchor
/// ```
///
/// The pitchfork projects a "fork" of three parallel lines: a central **median
/// line** drawn from a starting pivot through the midpoint of a later swing, plus
/// two parallels passing through that swing's high and low. Price tends to
/// oscillate around the median line and find support/resistance at the parallels.
/// This streaming version detects the pivots automatically with a symmetric
/// fractal of half-width `strength` (so each pivot is confirmed `strength` bars
/// late) and keeps the three most recent alternating swings.
///
/// Because it depends on swing structure, readiness is **data-dependent**: the
/// first output appears once three alternating pivots have been confirmed.
/// `warmup_period` returns the minimum bars to confirm a single pivot. Each
/// `update` is O(`strength`).
///
/// # Example
///
/// ```
/// use wickra_core::{Candle, Indicator, AndrewsPitchfork};
///
/// let mut indicator = AndrewsPitchfork::new(2).unwrap();
/// let mut last = None;
/// for i in 0..120 {
///     let base = 100.0 + (f64::from(i) * 0.4).sin() * 10.0;
///     let c = Candle::new(base, base + 1.0, base - 1.0, base, 1_000.0, 0).unwrap();
///     last = indicator.update(c);
/// }
/// // A swinging series eventually establishes a pitchfork.
/// let _ = last;
/// ```
#[derive(Debug, Clone)]
pub struct AndrewsPitchfork {
    strength: usize,
    window: VecDeque<Candle>,
    pivots: Vec<Pivot>,
    count: usize,
    last: Option<AndrewsPitchforkOutput>,
}

impl AndrewsPitchfork {
    /// Construct an Andrews Pitchfork with the given fractal `strength` (bars on
    /// each side of a pivot).
    ///
    /// # Errors
    ///
    /// Returns [`Error::PeriodZero`] if `strength == 0`.
    pub fn new(strength: usize) -> Result<Self> {
        if strength == 0 {
            return Err(Error::PeriodZero);
        }
        if strength > crate::error::MAX_PERIOD {
            return Err(Error::InvalidPeriod {
                message: crate::error::PERIOD_ABOVE_MAX,
            });
        }
        Ok(Self {
            strength,
            window: VecDeque::with_capacity(2 * strength + 1),
            pivots: Vec::new(),
            count: 0,
            last: None,
        })
    }

    /// Configured fractal strength.
    pub const fn strength(&self) -> usize {
        self.strength
    }

    /// Current value if available.
    pub const fn value(&self) -> Option<AndrewsPitchforkOutput> {
        self.last
    }

    /// Record a freshly confirmed pivot, keeping the last three alternating swings.
    fn record_pivot(&mut self, pivot: Pivot) {
        if let Some(last) = self.pivots.last_mut() {
            if last.is_high == pivot.is_high {
                // Same kind: keep the more extreme one (and its index).
                let more_extreme = if pivot.is_high {
                    pivot.price > last.price
                } else {
                    pivot.price < last.price
                };
                if more_extreme {
                    *last = pivot;
                }
                return;
            }
        }
        self.pivots.push(pivot);
        if self.pivots.len() > 3 {
            self.pivots.remove(0);
        }
    }

    fn project(&self, tc: f64) -> Option<AndrewsPitchforkOutput> {
        let [p0, p1, p2] = self.pivots.as_slice() else {
            return None;
        };
        let mid_t = f64::midpoint(p1.index, p2.index);
        let mid_p = f64::midpoint(p1.price, p2.price);
        let slope = (mid_p - p0.price) / (mid_t - p0.index);
        let median = p0.price + slope * (tc - p0.index);
        let off1 = p1.price - (p0.price + slope * (p1.index - p0.index));
        let off2 = p2.price - (p0.price + slope * (p2.index - p0.index));
        Some(AndrewsPitchforkOutput {
            median,
            upper: median + off1.max(off2),
            lower: median + off1.min(off2),
        })
    }
}

impl Indicator for AndrewsPitchfork {
    type Input = Candle;
    type Output = AndrewsPitchforkOutput;

    fn update(&mut self, candle: Candle) -> Option<AndrewsPitchforkOutput> {
        self.count += 1;
        let span = 2 * self.strength + 1;
        if self.window.len() == span {
            self.window.pop_front();
        }
        self.window.push_back(candle);
        if self.window.len() == span {
            let center = self.window[self.strength];
            let is_high = self
                .window
                .iter()
                .enumerate()
                .all(|(i, c)| i == self.strength || c.high < center.high);
            let is_low = self
                .window
                .iter()
                .enumerate()
                .all(|(i, c)| i == self.strength || c.low > center.low);
            // Absolute index of the center bar (1-based count minus the right span).
            let center_index = (self.count - 1 - self.strength) as f64;
            if is_high && !is_low {
                self.record_pivot(Pivot {
                    index: center_index,
                    price: center.high,
                    is_high: true,
                });
            } else if is_low && !is_high {
                self.record_pivot(Pivot {
                    index: center_index,
                    price: center.low,
                    is_high: false,
                });
            }
        }
        let tc = (self.count - 1) as f64;
        if let Some(out) = self.project(tc) {
            self.last = Some(out);
            return Some(out);
        }
        None
    }

    fn reset(&mut self) {
        self.window.clear();
        self.pivots.clear();
        self.count = 0;
        self.last = None;
    }

    #[inline]
    fn warmup_period(&self) -> usize {
        2 * self.strength + 1
    }

    #[inline]
    fn is_ready(&self) -> bool {
        self.last.is_some()
    }

    #[inline]
    fn name(&self) -> &'static str {
        "AndrewsPitchfork"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;

    fn c(high: f64, low: f64) -> Candle {
        Candle::new_unchecked(
            f64::midpoint(high, low),
            high,
            low,
            f64::midpoint(high, low),
            1_000.0,
            0,
        )
    }

    /// A clean zig-zag that prints alternating swing highs and lows.
    fn zigzag() -> Vec<Candle> {
        let mut out = Vec::new();
        for i in 0..120 {
            let base = 100.0 + (f64::from(i) * 0.5).sin() * 10.0;
            out.push(c(base + 1.0, base - 1.0));
        }
        out
    }

    #[test]
    fn rejects_zero_strength() {
        assert!(matches!(AndrewsPitchfork::new(0), Err(Error::PeriodZero)));
    }

    #[test]
    fn accessors_and_metadata() {
        let p = AndrewsPitchfork::new(2).unwrap();
        assert_eq!(p.strength(), 2);
        assert_eq!(p.warmup_period(), 5);
        assert_eq!(p.name(), "AndrewsPitchfork");
        assert!(!p.is_ready());
        assert_eq!(p.value(), None);
    }

    #[test]
    fn none_before_three_pivots() {
        let mut p = AndrewsPitchfork::new(2).unwrap();
        // Too few bars to ever confirm three alternating pivots.
        let out = p.batch(&[c(101.0, 99.0), c(102.0, 100.0), c(101.0, 99.0)]);
        assert!(out.iter().all(Option::is_none));
    }

    #[test]
    fn eventually_emits_on_swings() {
        let mut p = AndrewsPitchfork::new(2).unwrap();
        let out = p.batch(&zigzag());
        assert!(
            out.iter().any(Option::is_some),
            "a swinging series should form a pitchfork"
        );
        assert!(p.is_ready());
    }

    #[test]
    fn upper_at_or_above_lower() {
        let mut p = AndrewsPitchfork::new(2).unwrap();
        for o in p.batch(&zigzag()).into_iter().flatten() {
            assert!(
                o.upper >= o.lower,
                "upper {} below lower {}",
                o.upper,
                o.lower
            );
        }
    }

    #[test]
    fn reset_clears_state() {
        let mut p = AndrewsPitchfork::new(2).unwrap();
        p.batch(&zigzag());
        assert!(p.is_ready());
        p.reset();
        assert!(!p.is_ready());
        assert_eq!(p.value(), None);
        assert_eq!(p.strength(), 2);
    }

    #[test]
    fn record_pivot_keeps_more_extreme_same_kind() {
        let mut p = AndrewsPitchfork::new(2).unwrap();
        p.record_pivot(Pivot {
            index: 0.0,
            price: 100.0,
            is_high: true,
        });
        // A higher high of the same kind replaces the stored one.
        p.record_pivot(Pivot {
            index: 1.0,
            price: 105.0,
            is_high: true,
        });
        assert_eq!(p.pivots.len(), 1);
        assert_eq!(p.pivots[0].price, 105.0);
        // A lower high of the same kind is ignored.
        p.record_pivot(Pivot {
            index: 2.0,
            price: 102.0,
            is_high: true,
        });
        assert_eq!(p.pivots.len(), 1);
        assert_eq!(p.pivots[0].price, 105.0);
        // A low pivot of the other kind is appended.
        p.record_pivot(Pivot {
            index: 3.0,
            price: 90.0,
            is_high: false,
        });
        assert_eq!(p.pivots.len(), 2);
        // A lower low of the same kind replaces the stored low.
        p.record_pivot(Pivot {
            index: 4.0,
            price: 85.0,
            is_high: false,
        });
        assert_eq!(p.pivots[1].price, 85.0);
        // A higher low of the same kind is ignored.
        p.record_pivot(Pivot {
            index: 5.0,
            price: 88.0,
            is_high: false,
        });
        assert_eq!(p.pivots[1].price, 85.0);
    }

    #[test]
    fn batch_equals_streaming() {
        let candles = zigzag();
        let batch = AndrewsPitchfork::new(2).unwrap().batch(&candles);
        let mut b = AndrewsPitchfork::new(2).unwrap();
        let streamed: Vec<_> = candles.iter().map(|x| b.update(*x)).collect();
        assert_eq!(batch, streamed);
    }

    #[test]
    fn rejects_strength_above_maximum() {
        assert!(matches!(
            AndrewsPitchfork::new(crate::error::MAX_PERIOD + 1),
            Err(Error::InvalidPeriod { .. })
        ));
    }
}
