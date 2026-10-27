//! Three Line Break — the close-driven line-break chart trend, as a direction.

use crate::error::{Error, Result};
use crate::ohlcv::Candle;
use crate::traits::Indicator;

/// Three Line Break — the trend direction of a line-break ("kakushi") chart, where
/// a reversal requires the close to break the extreme of the last `lines` lines.
///
/// ```text
/// continue the trend when close exceeds the prior line's end
/// reverse the trend when close breaks beyond the extreme of the last `lines` lines
/// output = current line direction: +1 (up), −1 (down)
/// ```
///
/// A line-break chart ignores time and small moves entirely: it draws a new line
/// only when the close makes a new extreme in the trend, and flips direction only
/// when the close reverses past the high (or low) of the last `lines` lines —
/// classically **three**. This filters out minor pullbacks, so the emitted
/// direction stays in a trend until a genuinely significant reversal. Distinct from
/// the candlestick [`ThreeLineStrike`](crate::ThreeLineStrike) (a fixed four-bar
/// pattern); this is the line-break *chart type* reduced to its trend state. See
/// also the alt-chart "Three-Line-Break Bars" builder.
///
/// The output is `+1.0` / `−1.0`. The first bar seeds the reference price; the
/// direction is emitted once the first line is drawn (data-dependent;
/// `warmup_period` returns the minimum `2`). Each `update` is O(`lines`).
///
/// # Example
///
/// ```
/// use wickra_core::{Candle, Indicator, ThreeLineBreak};
///
/// let mut indicator = ThreeLineBreak::new(3).unwrap();
/// let mut last = None;
/// for i in 0..20 {
///     let close = 100.0 + f64::from(i);
///     let c = Candle::new(close, close, close, close, 1_000.0, 0).unwrap();
///     last = indicator.update(c);
/// }
/// assert_eq!(last, Some(1.0));
/// ```
#[derive(Debug, Clone)]
pub struct ThreeLineBreak {
    lines: usize,
    line_values: Vec<f64>,
    dir: i8,
    last: Option<f64>,
}

impl ThreeLineBreak {
    /// Construct a Three Line Break requiring `lines` lines to reverse (classic 3).
    ///
    /// # Errors
    ///
    /// Returns [`Error::PeriodZero`] if `lines == 0`.
    pub fn new(lines: usize) -> Result<Self> {
        if lines == 0 {
            return Err(Error::PeriodZero);
        }
        if lines > crate::error::MAX_PERIOD {
            return Err(Error::InvalidPeriod {
                message: crate::error::PERIOD_ABOVE_MAX,
            });
        }
        Ok(Self {
            lines,
            line_values: Vec::with_capacity(lines + 1),
            dir: 0,
            last: None,
        })
    }

    /// Configured number of lines required to reverse.
    pub const fn lines(&self) -> usize {
        self.lines
    }

    /// Current direction if available.
    pub const fn value(&self) -> Option<f64> {
        self.last
    }

    fn push_line(&mut self, close: f64, dir: i8) {
        self.dir = dir;
        self.line_values.push(close);
        if self.line_values.len() > self.lines {
            self.line_values.remove(0);
        }
    }
}

impl Indicator for ThreeLineBreak {
    type Input = Candle;
    type Output = f64;

    #[inline]
    fn update(&mut self, candle: Candle) -> Option<f64> {
        let close = candle.close;
        let Some(&prior) = self.line_values.last() else {
            // Seed the reference price; no line yet.
            self.line_values.push(close);
            return None;
        };
        if self.dir >= 0 {
            if close > prior {
                self.push_line(close, 1);
            } else {
                let low = self
                    .line_values
                    .iter()
                    .copied()
                    .fold(f64::INFINITY, f64::min);
                if close < low {
                    self.push_line(close, -1);
                }
            }
        } else if close < prior {
            self.push_line(close, -1);
        } else {
            let high = self
                .line_values
                .iter()
                .copied()
                .fold(f64::NEG_INFINITY, f64::max);
            if close > high {
                self.push_line(close, 1);
            }
        }
        if self.dir == 0 {
            return None;
        }
        let v = f64::from(self.dir);
        self.last = Some(v);
        Some(v)
    }

    fn reset(&mut self) {
        self.line_values.clear();
        self.dir = 0;
        self.last = None;
    }

    #[inline]
    fn warmup_period(&self) -> usize {
        2
    }

    #[inline]
    fn is_ready(&self) -> bool {
        self.last.is_some()
    }

    #[inline]
    fn name(&self) -> &'static str {
        "ThreeLineBreak"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;

    fn c(close: f64) -> Candle {
        Candle::new_unchecked(close, close, close, close, 1_000.0, 0)
    }

    #[test]
    fn rejects_zero_lines() {
        assert!(matches!(ThreeLineBreak::new(0), Err(Error::PeriodZero)));
    }

    #[test]
    fn accessors_and_metadata() {
        let t = ThreeLineBreak::new(3).unwrap();
        assert_eq!(t.lines(), 3);
        assert_eq!(t.warmup_period(), 2);
        assert_eq!(t.name(), "ThreeLineBreak");
        assert!(!t.is_ready());
        assert_eq!(t.value(), None);
    }

    #[test]
    fn uptrend_is_plus_one() {
        let mut t = ThreeLineBreak::new(3).unwrap();
        let candles: Vec<Candle> = (0..20).map(|i| c(100.0 + f64::from(i))).collect();
        let out = t.batch(&candles);
        assert!(out[0].is_none());
        assert_eq!(out[1], Some(1.0));
        assert_eq!(out.last().unwrap(), &Some(1.0));
    }

    #[test]
    fn downtrend_is_minus_one() {
        let mut t = ThreeLineBreak::new(3).unwrap();
        let candles: Vec<Candle> = (0..20).map(|i| c(100.0 - f64::from(i))).collect();
        let last = t.batch(&candles).into_iter().flatten().last().unwrap();
        assert_eq!(last, -1.0);
    }

    #[test]
    fn small_pullback_does_not_reverse() {
        // Rise to build 3 up-lines, then a small dip that does not break the
        // 3-line low keeps the direction up.
        let mut t = ThreeLineBreak::new(3).unwrap();
        t.batch(&[c(100.0), c(101.0), c(102.0), c(103.0)]); // up-lines at 101,102,103
                                                            // close 102.5 is below the prior line (103) but above the 3-line low (101) -> no reversal.
        assert_eq!(t.update(c(102.5)), Some(1.0));
    }

    #[test]
    fn break_of_three_line_extreme_reverses() {
        let mut t = ThreeLineBreak::new(3).unwrap();
        t.batch(&[c(100.0), c(101.0), c(102.0), c(103.0)]); // lines 101,102,103, dir up
                                                            // close 100.5 breaks below the 3-line low (101) -> reverse to down.
        assert_eq!(t.update(c(100.5)), Some(-1.0));
    }

    #[test]
    fn reset_clears_state() {
        let mut t = ThreeLineBreak::new(3).unwrap();
        t.batch(&(0..10).map(|i| c(100.0 + f64::from(i))).collect::<Vec<_>>());
        assert!(t.is_ready());
        t.reset();
        assert!(!t.is_ready());
        assert_eq!(t.value(), None);
        assert_eq!(t.update(c(100.0)), None);
    }

    #[test]
    fn flat_close_emits_none_until_a_line_forms() {
        let mut t = ThreeLineBreak::new(3).unwrap();
        assert_eq!(t.update(c(100.0)), None);
        // An identical close draws no line, so the direction stays unset.
        assert_eq!(t.update(c(100.0)), None);
        assert!(!t.is_ready());
    }

    #[test]
    fn batch_equals_streaming() {
        let candles: Vec<Candle> = (0..80)
            .map(|i| c(100.0 + (f64::from(i) * 0.25).sin() * 9.0))
            .collect();
        let batch = ThreeLineBreak::new(3).unwrap().batch(&candles);
        let mut b = ThreeLineBreak::new(3).unwrap();
        let streamed: Vec<_> = candles.iter().map(|x| b.update(*x)).collect();
        assert_eq!(batch, streamed);
    }

    #[test]
    fn rejects_lines_above_maximum() {
        assert!(matches!(
            ThreeLineBreak::new(crate::error::MAX_PERIOD + 1),
            Err(Error::InvalidPeriod { .. })
        ));
    }
}
