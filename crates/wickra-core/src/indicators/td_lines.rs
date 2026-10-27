#![allow(clippy::doc_markdown)]

//! Tom DeMark TD Lines (TDST — TD Setup Trend Support / Resistance levels).
//!
//! Once a TD Setup completes in either direction, DeMark defines two
//! horizontal trend levels derived from the nine bars of that setup:
//!
//! - **TDST resistance** is the highest high among the nine bars of the
//!   most-recently-completed **buy** setup. A break above resistance
//!   invalidates the setup's bullish reversal thesis.
//! - **TDST support** is the lowest low among the nine bars of the
//!   most-recently-completed **sell** setup. A break below support
//!   invalidates the setup's bearish reversal thesis.
//!
//! Until a setup completes in a given direction, the corresponding level
//! is `f64::NAN` (no level defined). Once a level is set it stays at its
//! value until the next completed setup in that direction updates it.
//!
//! This implementation tracks both the buy and sell setup state machines
//! in parallel (sharing the same `lookback` / `target` parameters as
//! [`crate::TdSetup`]) and records the bar extremes during the active
//! streak so the level can be emitted the moment the setup completes.

use std::collections::VecDeque;

use crate::error::{Error, Result};
use crate::ohlcv::Candle;
use crate::traits::Indicator;

/// Output of [`TdLines`]: the latest TDST resistance / support pair.
///
/// `resistance` is set after a completed buy setup (the highest high of
/// the nine setup bars); `support` is set after a completed sell setup
/// (the lowest low of the nine setup bars). Either field is `f64::NAN`
/// until the first setup in that direction completes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TdLinesOutput {
    /// Latest TDST resistance, or `NAN` if no buy setup has completed yet.
    ///
    /// The two levels are established independently, so one can be a real price
    /// while the other is still `NAN`. `update` withholds the output entirely
    /// (`None`) until at least one of them exists, so a returned value always
    /// carries at least one level. `NAN` is the encoding because the C ABI
    /// mirrors this struct as two plain `double`s.
    pub resistance: f64,
    /// Latest TDST support, or `NAN` if no sell setup has completed yet.
    ///
    /// See [`Self::resistance`] for when a level can be `NAN`.
    pub support: f64,
}

/// TD Lines (TDST) — setup-derived horizontal support / resistance.
/// # Example
///
/// ```
/// use wickra_core::{TdLines, Candle, Indicator};
///
/// let mut indicator = TdLines::new(4, 9).unwrap();
/// // `None` during warmup, then `Some(_)` once enough bars are seen.
/// let mut out = None;
/// for i in 0..40i64 {
///     let p = 100.0 + (i as f64 * 0.4).sin() * 5.0;
///     let candle = Candle::new(p, p + 1.5, p - 1.5, p + 0.3, 1_000.0, i).unwrap();
///     out = indicator.update(candle);
/// }
/// let _ = out;
/// ```
#[derive(Debug, Clone)]
pub struct TdLines {
    lookback: usize,
    target: usize,
    closes: VecDeque<f64>,
    buy_count: usize,
    sell_count: usize,
    /// Highest high observed during the *current* buy-setup run (running
    /// extreme, resets when the buy run resets).
    buy_run_max_high: f64,
    /// Lowest low observed during the *current* sell-setup run.
    sell_run_min_low: f64,
    /// Set once a buy setup completes; `None` means no TDST resistance exists
    /// yet. Kept as an `Option` rather than a `NAN` sentinel so "unset" is not
    /// inferred from the bit pattern of a value that is supposed to be a price.
    resistance: Option<f64>,
    /// Set once a sell setup completes; `None` means no TDST support exists yet.
    support: Option<f64>,
    ready: bool,
}

impl TdLines {
    /// Construct a TD Lines with explicit lookback and target. The
    /// canonical DeMark configuration is `lookback = 4`, `target = 9`.
    ///
    /// # Errors
    ///
    /// Returns [`Error::PeriodZero`] if either argument is zero.
    pub fn new(lookback: usize, target: usize) -> Result<Self> {
        if lookback == 0 || target == 0 {
            return Err(Error::PeriodZero);
        }
        Ok(Self {
            lookback,
            target,
            closes: VecDeque::with_capacity(lookback + 1),
            buy_count: 0,
            sell_count: 0,
            buy_run_max_high: f64::NEG_INFINITY,
            sell_run_min_low: f64::INFINITY,
            resistance: None,
            support: None,
            ready: false,
        })
    }

    /// DeMark's classic configuration: `lookback = 4`, `target = 9`.
    pub fn classic() -> Self {
        Self::new(4, 9).expect("classic TD Lines parameters are valid")
    }

    /// Configured `(lookback, target)`.
    pub const fn params(&self) -> (usize, usize) {
        (self.lookback, self.target)
    }
}

impl Indicator for TdLines {
    type Input = Candle;
    type Output = TdLinesOutput;

    fn update(&mut self, candle: Candle) -> Option<TdLinesOutput> {
        if self.closes.len() > self.lookback {
            self.closes.pop_front();
        }
        if self.closes.len() < self.lookback {
            self.closes.push_back(candle.close);
            return None;
        }
        let reference = *self.closes.front().expect("non-empty after the guard");
        self.closes.push_back(candle.close);

        if candle.close < reference {
            // Continue / start a buy-setup run; if the sell run breaks
            // here, reset its running extreme.
            if self.buy_count == 0 {
                self.buy_run_max_high = candle.high;
            } else {
                self.buy_run_max_high = self.buy_run_max_high.max(candle.high);
            }
            self.buy_count = (self.buy_count + 1).min(self.target);
            self.sell_count = 0;
            self.sell_run_min_low = f64::INFINITY;
            if self.buy_count == self.target {
                self.resistance = Some(self.buy_run_max_high);
            }
        } else if candle.close > reference {
            if self.sell_count == 0 {
                self.sell_run_min_low = candle.low;
            } else {
                self.sell_run_min_low = self.sell_run_min_low.min(candle.low);
            }
            self.sell_count = (self.sell_count + 1).min(self.target);
            self.buy_count = 0;
            self.buy_run_max_high = f64::NEG_INFINITY;
            if self.sell_count == self.target {
                self.support = Some(self.sell_run_min_low);
            }
        } else {
            // Equality breaks both runs.
            self.buy_count = 0;
            self.sell_count = 0;
            self.buy_run_max_high = f64::NEG_INFINITY;
            self.sell_run_min_low = f64::INFINITY;
        }

        // Neither level established means there is nothing to report yet. The
        // trait defines `None` as "insufficient inputs to produce a defined
        // value", and a pair of NaNs is exactly that — on a flat series the old
        // code emitted it on every bar forever, and it reached the bindings as
        // two NaNs in a flat output buffer.
        if self.resistance.is_none() && self.support.is_none() {
            return None;
        }
        self.ready = true;
        Some(TdLinesOutput {
            resistance: self.resistance.unwrap_or(f64::NAN),
            support: self.support.unwrap_or(f64::NAN),
        })
    }

    fn reset(&mut self) {
        self.closes.clear();
        self.buy_count = 0;
        self.sell_count = 0;
        self.buy_run_max_high = f64::NEG_INFINITY;
        self.sell_run_min_low = f64::INFINITY;
        self.resistance = None;
        self.support = None;
        self.ready = false;
    }

    /// Lower bound only: a TDST line needs a *completed* setup, which depends on
    /// the data, so the first value can arrive arbitrarily later than this — and
    /// on a series that never completes a setup, never.
    #[inline]
    fn warmup_period(&self) -> usize {
        self.lookback + 1
    }

    #[inline]
    fn is_ready(&self) -> bool {
        self.ready
    }

    #[inline]
    fn name(&self) -> &'static str {
        "TDLines"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;
    use approx::assert_relative_eq;

    fn c(high: f64, low: f64, close: f64, ts: i64) -> Candle {
        Candle::new_unchecked(close, high, low, close, 0.0, ts)
    }

    #[test]
    fn uptrend_completes_sell_setup_and_sets_support() {
        // Strictly rising series -> sell setup completes at bar index 12
        // (warmup 5 + 8 advances). The lowest low across bars 4..=12 is
        // the low at idx 4 since the series is strictly rising.
        let candles: Vec<Candle> = (1..=20)
            .map(|i| {
                c(
                    f64::from(i) + 0.5,
                    f64::from(i) - 0.5,
                    f64::from(i),
                    i64::from(i),
                )
            })
            .collect();
        let mut lines = TdLines::classic();
        let out = lines.batch(&candles);
        // Nothing is emitted before a setup completes: neither level exists, so
        // there is no defined value to report.
        assert!(out[..12].iter().all(Option::is_none));
        // After completion at idx 12, support is the low of bar idx 4 = 4.5.
        // Resistance stays NaN because no buy setup ever completes here.
        let after = out[12].expect("the completed sell setup emits");
        assert!(after.resistance.is_nan());
        assert_relative_eq!(after.support, 4.5, epsilon = 1e-12);
        // Subsequent bars (still increasing, sell setup saturating) keep
        // the running extreme at the original low.
        let final_out = out[19].expect("ready");
        assert_relative_eq!(final_out.support, 4.5, epsilon = 1e-12);
    }

    #[test]
    fn downtrend_completes_buy_setup_and_sets_resistance() {
        let candles: Vec<Candle> = (1..=20)
            .rev()
            .enumerate()
            .map(|(i, v)| {
                c(
                    f64::from(v) + 0.5,
                    f64::from(v) - 0.5,
                    f64::from(v),
                    i64::try_from(i).unwrap(),
                )
            })
            .collect();
        let mut lines = TdLines::classic();
        let out = lines.batch(&candles);
        // Buy setup completes at idx 12. The highest high during the
        // buy run is the high of bar idx 4 (since the series is strictly
        // decreasing): low/high of bar 4 are computed below.
        let after = out[12].expect("ready");
        assert!(after.support.is_nan());
        // The high at idx 4 in the reversed series is value 16 + 0.5.
        assert_relative_eq!(after.resistance, 16.5, epsilon = 1e-12);
    }

    #[test]
    fn flat_series_never_emits() {
        // All closes equal -> neither setup advances -> no level ever exists, so
        // nothing is emitted. This used to yield `Some` with two NaNs on every
        // bar past the warmup.
        let candles: Vec<Candle> = (0..30).map(|i| c(10.5, 9.5, 10.0, i64::from(i))).collect();
        let mut lines = TdLines::classic();
        let out = lines.batch(&candles);
        assert!(out.iter().all(Option::is_none));
        assert!(!lines.is_ready());
    }

    #[test]
    fn batch_equals_streaming() {
        let candles: Vec<Candle> = (0..80)
            .map(|i| {
                let m = 100.0 + (f64::from(i) * 0.3).sin() * 5.0;
                c(m + 1.0, m - 1.0, m, i64::from(i))
            })
            .collect();
        let mut a = TdLines::classic();
        let mut b = TdLines::classic();
        let av = a.batch(&candles);
        let bv: Vec<_> = candles.iter().map(|x| b.update(*x)).collect();
        assert_eq!(av.len(), bv.len());
        for (i, (x, y)) in av.iter().zip(bv.iter()).enumerate() {
            assert_eq!(x.is_some(), y.is_some(), "row {i} option mismatch");
            if let (Some(a), Some(b)) = (x, y) {
                assert_eq!(
                    a.support.is_nan(),
                    b.support.is_nan(),
                    "row {i} support nan flag"
                );
                assert_eq!(
                    a.resistance.is_nan(),
                    b.resistance.is_nan(),
                    "row {i} resistance nan flag"
                );
                // The NaN flags match, so a NaN on one side is a NaN on both.
                let close = |x: f64, y: f64| x.is_nan() || (x - y).abs() <= 1e-12;
                assert!(close(a.support, b.support), "row {i} support");
                assert!(close(a.resistance, b.resistance), "row {i} resistance");
            }
        }
    }

    #[test]
    fn rejects_invalid_params() {
        assert!(matches!(TdLines::new(0, 9), Err(Error::PeriodZero)));
        assert!(matches!(TdLines::new(4, 0), Err(Error::PeriodZero)));
    }

    #[test]
    fn reset_clears_state() {
        let candles: Vec<Candle> = (1..=20)
            .map(|i| {
                c(
                    f64::from(i) + 0.5,
                    f64::from(i) - 0.5,
                    f64::from(i),
                    i64::from(i),
                )
            })
            .collect();
        let mut lines = TdLines::classic();
        lines.batch(&candles);
        assert!(lines.is_ready());
        lines.reset();
        assert!(!lines.is_ready());
        assert_eq!(lines.update(candles[0]), None);
    }

    #[test]
    fn accessors_and_metadata() {
        let lines = TdLines::classic();
        assert_eq!(lines.params(), (4, 9));
        assert_eq!(lines.warmup_period(), 5);
        assert_eq!(lines.name(), "TDLines");
    }
}
