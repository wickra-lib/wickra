//! Run bar builder (simplified López de Prado) — sample on runs of same-signed ticks.

use crate::error::{Error, Result};
use crate::ohlcv::Candle;
use crate::traits::BarBuilder;

/// One completed run bar.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RunBar {
    /// Open of the first candle in the bar.
    pub open: f64,
    /// Highest high across the bar.
    pub high: f64,
    /// Lowest low across the bar.
    pub low: f64,
    /// Close of the candle that closed the bar.
    pub close: f64,
    /// Length of the run that closed the bar (`== run_length`).
    pub length: usize,
    /// `+1` if a buy run closed the bar, `-1` if a sell run.
    pub direction: i8,
}

/// Run bar builder — a **simplified** form of López de Prado's run bars.
///
/// A *run* is an uninterrupted sequence of same-signed ticks: a streak of up-ticks
/// (a buy run) or down-ticks (a sell run), with unchanged closes extending the
/// current run. This builder counts the current run's length and closes a bar when
/// it reaches `run_length`; a tick in the opposite direction restarts the run from
/// one. Where [`ImbalanceBars`](crate::ImbalanceBars) sample on the *net* signed
/// imbalance (which oscillating flow can cancel back to zero), run bars sample on
/// *persistence*: they fire only when the market pushes the same way without
/// interruption, making them a cleaner sequential-trend detector.
///
/// **Simplification.** The full method estimates a *dynamic* expected run length
/// from an EWMA and can weight runs by volume or traded value. This builder uses a
/// **fixed** run-length threshold on unweighted ticks. See López de Prado (2018),
/// ch. 2, for the adaptive estimator and weighted variants.
///
/// At most one bar closes per candle, so [`BarBuilder::update`] returns either an
/// empty vector or a single [`RunBar`].
///
/// # Example
///
/// ```
/// use wickra_core::{BarBuilder, Candle, RunBars};
///
/// let flat = |price: f64| Candle::new(price, price, price, price, 1.0, 0).unwrap();
/// let mut bars = RunBars::new(3).unwrap();
/// bars.update(flat(10.0));            // seed
/// bars.update(flat(11.0));            // run 1
/// bars.update(flat(12.0));            // run 2
/// let out = bars.update(flat(13.0));  // run 3 -> close
/// assert_eq!(out.len(), 1);
/// assert_eq!(out[0].direction, 1);
/// ```
#[derive(Debug, Clone)]
pub struct RunBars {
    run_length: usize,
    count: usize,
    open: f64,
    high: f64,
    low: f64,
    close: f64,
    prev_close: Option<f64>,
    run_sign: i8,
    run_len: usize,
}

impl RunBars {
    /// Construct a run-bar builder that closes a bar on a run of `run_length` ticks.
    ///
    /// # Errors
    ///
    /// Returns [`Error::PeriodZero`] if `run_length == 0`.
    pub fn new(run_length: usize) -> Result<Self> {
        if run_length == 0 {
            return Err(Error::PeriodZero);
        }
        if run_length > crate::error::MAX_PERIOD {
            return Err(Error::InvalidPeriod {
                message: crate::error::PERIOD_ABOVE_MAX,
            });
        }
        Ok(Self {
            run_length,
            count: 0,
            open: 0.0,
            high: 0.0,
            low: 0.0,
            close: 0.0,
            prev_close: None,
            run_sign: 0,
            run_len: 0,
        })
    }

    /// Configured run length that closes a bar.
    pub const fn run_length(&self) -> usize {
        self.run_length
    }

    /// Length of the in-progress run.
    pub const fn run(&self) -> usize {
        self.run_len
    }
}

impl BarBuilder for RunBars {
    type Bar = RunBar;

    fn update(&mut self, candle: Candle) -> Vec<RunBar> {
        if self.count == 0 {
            self.open = candle.open;
            self.high = candle.high;
            self.low = candle.low;
        } else {
            self.high = self.high.max(candle.high);
            self.low = self.low.min(candle.low);
        }
        self.close = candle.close;
        self.count += 1;
        if let Some(prev) = self.prev_close {
            let directional = if candle.close > prev {
                1
            } else if candle.close < prev {
                -1
            } else {
                0
            };
            if directional == 0 {
                // A flat tick extends the current run (if one is under way).
                if self.run_sign != 0 {
                    self.run_len += 1;
                }
            } else if directional == self.run_sign {
                self.run_len += 1;
            } else {
                self.run_sign = directional;
                self.run_len = 1;
            }
        }
        self.prev_close = Some(candle.close);
        if self.run_sign == 0 || self.run_len < self.run_length {
            return Vec::new();
        }
        let bar = RunBar {
            open: self.open,
            high: self.high,
            low: self.low,
            close: self.close,
            length: self.run_len,
            direction: self.run_sign,
        };
        self.count = 0;
        self.run_sign = 0;
        self.run_len = 0;
        vec![bar]
    }

    fn reset(&mut self) {
        self.count = 0;
        self.prev_close = None;
        self.run_sign = 0;
        self.run_len = 0;
    }

    #[inline]
    fn name(&self) -> &'static str {
        "RunBars"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flat(price: f64) -> Candle {
        Candle::new(price, price, price, price, 1.0, 0).unwrap()
    }

    #[test]
    fn rejects_zero_run_length() {
        assert!(matches!(RunBars::new(0), Err(Error::PeriodZero)));
    }

    #[test]
    fn accessors_and_metadata() {
        let bars = RunBars::new(5).unwrap();
        assert_eq!(bars.run_length(), 5);
        assert_eq!(bars.run(), 0);
        assert_eq!(bars.name(), "RunBars");
    }

    #[test]
    fn buy_run_closes_up_bar() {
        let mut bars = RunBars::new(3).unwrap();
        bars.update(flat(10.0)); // seed
        bars.update(flat(11.0)); // run 1
        bars.update(flat(12.0)); // run 2
        let out = bars.update(flat(13.0)); // run 3
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].direction, 1);
        assert_eq!(out[0].length, 3);
    }

    #[test]
    fn sell_run_closes_down_bar() {
        let mut bars = RunBars::new(3).unwrap();
        bars.update(flat(10.0));
        bars.update(flat(9.0)); // run 1
        bars.update(flat(8.0)); // run 2
        let out = bars.update(flat(7.0)); // run 3
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].direction, -1);
    }

    #[test]
    fn opposite_tick_restarts_run() {
        let mut bars = RunBars::new(3).unwrap();
        bars.update(flat(10.0));
        bars.update(flat(11.0)); // up run 1
        bars.update(flat(12.0)); // up run 2
        bars.update(flat(11.0)); // down -> run restarts at 1
        assert_eq!(bars.run(), 1);
    }

    #[test]
    fn flat_tick_extends_run() {
        let mut bars = RunBars::new(3).unwrap();
        bars.update(flat(10.0));
        bars.update(flat(11.0)); // run 1
        bars.update(flat(11.0)); // flat -> run 2
        let out = bars.update(flat(12.0)); // run 3
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].direction, 1);
    }

    #[test]
    fn reset_clears_state() {
        let mut bars = RunBars::new(3).unwrap();
        bars.update(flat(10.0));
        bars.update(flat(11.0));
        bars.reset();
        assert_eq!(bars.run(), 0);
        assert_eq!(bars.update(flat(50.0)), Vec::new());
    }

    #[test]
    fn batch_concatenates_completed_bars() {
        let mut bars = RunBars::new(2).unwrap();
        let candles = [
            flat(10.0),
            flat(11.0), // run 1
            flat(12.0), // run 2 -> close
            flat(13.0), // run 1
            flat(14.0), // run 2 -> close
        ];
        let out = bars.batch(&candles);
        assert_eq!(out.len(), 2);
        assert!(out.iter().all(|b| b.direction == 1));
    }
}
