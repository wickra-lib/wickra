//! Three-Line-Break bar builder — line-break chart segments driven by close prices.

use std::collections::VecDeque;

use crate::error::{Error, Result};
use crate::ohlcv::Candle;
use crate::traits::BarBuilder;

/// One completed line-break line.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LineBreakBar {
    /// Price where the line began (the previous line's far edge).
    pub open: f64,
    /// Price where the line ended (the new close that drew it).
    pub close: f64,
    /// `+1` for a rising line, `-1` for a falling line.
    pub direction: i8,
}

/// Three-Line-Break bar builder using the classic close-based reversal rule.
///
/// A line-break chart draws a new line in the trend direction whenever the close
/// makes a new extreme, and only reverses when the close breaks the extreme of the
/// previous `lines` lines (three by default — hence "three-line break"). This filters
/// minor noise: a pullback that fails to exceed the last three lines is ignored
/// entirely, so the chart isolates meaningful reversals.
///
/// This is the **bar-builder** counterpart of the
/// [`ThreeLineBreak`](crate::ThreeLineBreak) indicator: the indicator reports the
/// current line *state* as a streaming value, whereas this builder emits each
/// completed line as a [`LineBreakBar`] so you can reconstruct the full line-break
/// chart. At most one line forms per candle, so [`BarBuilder::update`] returns either
/// an empty vector or a single bar.
///
/// Construction rules:
///
/// - The first candle seeds a reference close and prints nothing.
/// - The first subsequent move (up or down) draws the first line.
/// - In an up-trend a close above the last line's top extends it (a new up line); a
///   close below the lowest low of the last `lines` lines reverses to a down line.
///   The down-trend is symmetric.
///
/// # Example
///
/// ```
/// use wickra_core::{BarBuilder, Candle, ThreeLineBreakBars};
///
/// let flat = |price: f64| Candle::new(price, price, price, price, 1.0, 0).unwrap();
/// let mut bars = ThreeLineBreakBars::new(3).unwrap();
/// bars.update(flat(10.0));           // seed
/// let first = bars.update(flat(11.0)); // first up line
/// assert_eq!(first.len(), 1);
/// assert_eq!(first[0].direction, 1);
/// ```
#[derive(Debug, Clone)]
pub struct ThreeLineBreakBars {
    lines: usize,
    seed: Option<f64>,
    recent: VecDeque<LineBreakBar>,
}

impl ThreeLineBreakBars {
    /// Construct a line-break builder that reverses on a break of the last `lines`
    /// lines (3 for the classic three-line break).
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
            seed: None,
            recent: VecDeque::with_capacity(lines),
        })
    }

    /// Configured number of lines a reversal must break.
    pub const fn lines(&self) -> usize {
        self.lines
    }

    /// Number of recent lines currently tracked for the reversal test.
    pub fn tracked(&self) -> usize {
        self.recent.len()
    }

    fn push_line(&mut self, bar: LineBreakBar) {
        if self.recent.len() == self.lines {
            self.recent.pop_front();
        }
        self.recent.push_back(bar);
    }

    fn lowest_low(&self) -> f64 {
        self.recent
            .iter()
            .map(|bar| bar.open.min(bar.close))
            .fold(f64::INFINITY, f64::min)
    }

    fn highest_high(&self) -> f64 {
        self.recent
            .iter()
            .map(|bar| bar.open.max(bar.close))
            .fold(f64::NEG_INFINITY, f64::max)
    }
}

impl BarBuilder for ThreeLineBreakBars {
    type Bar = LineBreakBar;

    fn update(&mut self, candle: Candle) -> Vec<LineBreakBar> {
        let close = candle.close;
        let Some(last) = self.recent.back().copied() else {
            // No line yet: seed, then draw the first line on the first move.
            let Some(seed) = self.seed else {
                self.seed = Some(close);
                return Vec::new();
            };
            let bar = if close > seed {
                LineBreakBar {
                    open: seed,
                    close,
                    direction: 1,
                }
            } else if close < seed {
                LineBreakBar {
                    open: seed,
                    close,
                    direction: -1,
                }
            } else {
                return Vec::new();
            };
            self.push_line(bar);
            return vec![bar];
        };
        let new_bar = if last.direction > 0 {
            if close > last.close {
                Some(LineBreakBar {
                    open: last.close,
                    close,
                    direction: 1,
                })
            } else if close < self.lowest_low() {
                Some(LineBreakBar {
                    open: last.close,
                    close,
                    direction: -1,
                })
            } else {
                None
            }
        } else if close < last.close {
            Some(LineBreakBar {
                open: last.close,
                close,
                direction: -1,
            })
        } else if close > self.highest_high() {
            Some(LineBreakBar {
                open: last.close,
                close,
                direction: 1,
            })
        } else {
            None
        };
        if let Some(bar) = new_bar {
            self.push_line(bar);
            vec![bar]
        } else {
            Vec::new()
        }
    }

    fn reset(&mut self) {
        self.seed = None;
        self.recent.clear();
    }

    #[inline]
    fn name(&self) -> &'static str {
        "ThreeLineBreakBars"
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
    fn rejects_zero_lines() {
        assert!(matches!(ThreeLineBreakBars::new(0), Err(Error::PeriodZero)));
    }

    #[test]
    fn accessors_and_metadata() {
        let bars = ThreeLineBreakBars::new(3).unwrap();
        assert_eq!(bars.lines(), 3);
        assert_eq!(bars.tracked(), 0);
        assert_eq!(bars.name(), "ThreeLineBreakBars");
    }

    #[test]
    fn seed_then_first_line() {
        let mut bars = ThreeLineBreakBars::new(3).unwrap();
        assert_eq!(bars.update(flat(10.0)), Vec::new()); // seed
        let first = bars.update(flat(11.0));
        assert_eq!(first.len(), 1);
        assert_eq!(first[0].direction, 1);
        assert_relative_eq!(first[0].open, 10.0, epsilon = 1e-12);
        assert_relative_eq!(first[0].close, 11.0, epsilon = 1e-12);
    }

    #[test]
    fn new_high_extends_up_line() {
        let mut bars = ThreeLineBreakBars::new(3).unwrap();
        bars.update(flat(10.0));
        bars.update(flat(11.0)); // line 1 up
        let cont = bars.update(flat(12.0)); // new high -> extend
        assert_eq!(cont.len(), 1);
        assert_eq!(cont[0].direction, 1);
        assert_relative_eq!(cont[0].open, 11.0, epsilon = 1e-12);
    }

    #[test]
    fn small_pullback_prints_nothing() {
        let mut bars = ThreeLineBreakBars::new(3).unwrap();
        bars.update(flat(10.0));
        bars.update(flat(11.0)); // line 1
        bars.update(flat(12.0)); // line 2
        bars.update(flat(13.0)); // line 3, lows are 10/11/12
        assert_eq!(bars.update(flat(10.5)), Vec::new()); // not > 13, not < 10
    }

    #[test]
    fn reversal_breaks_three_lines() {
        let mut bars = ThreeLineBreakBars::new(3).unwrap();
        bars.update(flat(10.0));
        bars.update(flat(11.0)); // line 1, low 10
        bars.update(flat(12.0)); // line 2, low 11
        bars.update(flat(13.0)); // line 3, low 12
        let rev = bars.update(flat(9.0)); // 9 < lowest low 10 -> reverse
        assert_eq!(rev.len(), 1);
        assert_eq!(rev[0].direction, -1);
        assert_relative_eq!(rev[0].open, 13.0, epsilon = 1e-12);
        assert_relative_eq!(rev[0].close, 9.0, epsilon = 1e-12);
    }

    #[test]
    fn reset_clears_state() {
        let mut bars = ThreeLineBreakBars::new(3).unwrap();
        bars.update(flat(10.0));
        bars.update(flat(11.0));
        bars.reset();
        assert_eq!(bars.tracked(), 0);
        assert_eq!(bars.update(flat(50.0)), Vec::new()); // re-seeds
    }

    #[test]
    fn flat_first_move_prints_nothing() {
        let mut bars = ThreeLineBreakBars::new(3).unwrap();
        assert_eq!(bars.update(flat(10.0)), Vec::new()); // seed
        assert_eq!(bars.update(flat(10.0)), Vec::new()); // equal to seed -> no line
    }

    #[test]
    fn first_line_down_then_down_trend_and_reversal() {
        let mut bars = ThreeLineBreakBars::new(3).unwrap();
        assert_eq!(bars.update(flat(10.0)), Vec::new()); // seed
        let first = bars.update(flat(9.0)); // first line down
        assert_eq!(first.len(), 1);
        assert_eq!(first[0].direction, -1);
        assert_relative_eq!(first[0].open, 10.0, epsilon = 1e-12);
        assert_relative_eq!(first[0].close, 9.0, epsilon = 1e-12);
        let cont = bars.update(flat(8.0)); // new low extends the down line
        assert_eq!(cont.len(), 1);
        assert_eq!(cont[0].direction, -1);
        assert_relative_eq!(cont[0].open, 9.0, epsilon = 1e-12);
        bars.update(flat(7.0)); // third down line; highs are 10/9/8
        assert_eq!(bars.update(flat(7.5)), Vec::new()); // not < 7, not > highest high 10
        let rev = bars.update(flat(11.0)); // > highest high 10 -> reverse up
        assert_eq!(rev.len(), 1);
        assert_eq!(rev[0].direction, 1);
        assert_relative_eq!(rev[0].open, 7.0, epsilon = 1e-12);
    }

    #[test]
    fn batch_concatenates_completed_lines() {
        let mut bars = ThreeLineBreakBars::new(3).unwrap();
        let candles = [flat(10.0), flat(11.0), flat(12.0), flat(13.0)];
        let out = bars.batch(&candles);
        // seed at 10, then three rising lines.
        assert_eq!(out.len(), 3);
        assert!(out.iter().all(|b| b.direction == 1));
    }
}
