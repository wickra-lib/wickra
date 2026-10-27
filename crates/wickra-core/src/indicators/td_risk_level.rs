#![allow(clippy::doc_markdown)]

//! Tom DeMark TD Risk Level — protective-stop levels derived from setup
//! extremes.
//!
//! DeMark proposes a quantitative stop level for trades taken on the back
//! of a completed setup. The risk level is computed from the bar that
//! made the most-extreme price during the setup run and that bar's true
//! range:
//!
//! - **Buy risk** (the protective stop for a long position taken on a
//!   completed buy setup) is `low_extreme_bar.low - true_range_extreme_bar`.
//!   `low_extreme_bar` is the bar with the lowest low among the setup's
//!   bars; `true_range_extreme_bar` is its true range
//!   (`max(high - low, |high - prev_close|, |low - prev_close|)`).
//! - **Sell risk** (the protective stop for a short position taken on a
//!   completed sell setup) is `high_extreme_bar.high +
//!   true_range_extreme_bar`.
//!
//! The level is set the moment a setup completes and stays at that value
//! until the next setup in that direction completes. Either field is
//! `f64::NAN` until the first setup in that direction completes.

use std::collections::VecDeque;

use crate::error::{Error, Result};
use crate::ohlcv::Candle;
use crate::traits::Indicator;

/// Output of [`TdRiskLevel`]: the latest buy- and sell-side protective
/// stop levels derived from the most-recently-completed setup in each
/// direction. Either field is `f64::NAN` until the first setup in that
/// direction completes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TdRiskLevelOutput {
    /// Protective-stop level for a long position taken on a completed
    /// buy setup. `NAN` until the first buy setup completes.
    ///
    /// The two levels are established independently, so one can be a real price
    /// while the other is still `NAN`. `update` withholds the output entirely
    /// (`None`) until at least one of them exists, so a returned value always
    /// carries at least one level. `NAN` is the encoding because the C ABI
    /// mirrors this struct as two plain `double`s.
    pub buy_risk: f64,
    /// Protective-stop level for a short position taken on a completed
    /// sell setup. `NAN` until the first sell setup completes.
    ///
    /// See [`Self::buy_risk`] for when a level can be `NAN`.
    pub sell_risk: f64,
}

/// Track the bar making the running extreme of the current run, together
/// with its true range.
#[derive(Debug, Clone, Copy)]
struct ExtremeBar {
    price: f64,
    true_range: f64,
}

/// TD Risk Level — setup-derived protective-stop levels.
/// # Example
///
/// ```
/// use wickra_core::{TdRiskLevel, Candle, Indicator};
///
/// let mut indicator = TdRiskLevel::new(4, 9).unwrap();
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
pub struct TdRiskLevel {
    lookback: usize,
    target: usize,
    closes: VecDeque<f64>,
    prev: Option<Candle>,
    buy_count: usize,
    sell_count: usize,
    /// Extreme (lowest low) bar of the active buy-setup run.
    buy_extreme: Option<ExtremeBar>,
    /// Extreme (highest high) bar of the active sell-setup run.
    sell_extreme: Option<ExtremeBar>,
    /// Set once a buy setup completes; `None` means no level exists yet. Kept as
    /// an `Option` rather than a `NAN` sentinel so "unset" is not inferred from
    /// the bit pattern of a value that is supposed to be a price.
    buy_risk: Option<f64>,
    /// Set once a sell setup completes; `None` means no level exists yet.
    sell_risk: Option<f64>,
    ready: bool,
}

fn true_range(candle: Candle, prev: Option<Candle>) -> f64 {
    let hl = candle.high - candle.low;
    if let Some(p) = prev {
        let hc = (candle.high - p.close).abs();
        let lc = (candle.low - p.close).abs();
        hl.max(hc).max(lc)
    } else {
        hl
    }
}

impl TdRiskLevel {
    /// Construct a TD Risk Level with explicit lookback and target. The
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
            prev: None,
            buy_count: 0,
            sell_count: 0,
            buy_extreme: None,
            sell_extreme: None,
            buy_risk: None,
            sell_risk: None,
            ready: false,
        })
    }

    /// DeMark's classic configuration: `lookback = 4`, `target = 9`.
    pub fn classic() -> Self {
        Self::new(4, 9).expect("classic TD Risk Level parameters are valid")
    }

    /// Configured `(lookback, target)`.
    pub const fn params(&self) -> (usize, usize) {
        (self.lookback, self.target)
    }
}

impl Indicator for TdRiskLevel {
    type Input = Candle;
    type Output = TdRiskLevelOutput;

    fn update(&mut self, candle: Candle) -> Option<TdRiskLevelOutput> {
        let tr = true_range(candle, self.prev);
        if self.closes.len() > self.lookback {
            self.closes.pop_front();
        }
        if self.closes.len() < self.lookback {
            self.closes.push_back(candle.close);
            self.prev = Some(candle);
            return None;
        }
        let reference = *self.closes.front().expect("non-empty after the guard");
        self.closes.push_back(candle.close);

        if candle.close < reference {
            // Buy setup run.
            let new_extreme = ExtremeBar {
                price: candle.low,
                true_range: tr,
            };
            self.buy_extreme = Some(match self.buy_extreme {
                Some(e) if e.price <= candle.low => e,
                _ => new_extreme,
            });
            self.buy_count = (self.buy_count + 1).min(self.target);
            self.sell_count = 0;
            self.sell_extreme = None;
            if self.buy_count == self.target {
                let e = self.buy_extreme.expect("set above when buy_count > 0");
                self.buy_risk = Some(e.price - e.true_range);
            }
        } else if candle.close > reference {
            // Sell setup run.
            let new_extreme = ExtremeBar {
                price: candle.high,
                true_range: tr,
            };
            self.sell_extreme = Some(match self.sell_extreme {
                Some(e) if e.price >= candle.high => e,
                _ => new_extreme,
            });
            self.sell_count = (self.sell_count + 1).min(self.target);
            self.buy_count = 0;
            self.buy_extreme = None;
            if self.sell_count == self.target {
                let e = self.sell_extreme.expect("set above when sell_count > 0");
                self.sell_risk = Some(e.price + e.true_range);
            }
        } else {
            self.buy_count = 0;
            self.sell_count = 0;
            self.buy_extreme = None;
            self.sell_extreme = None;
        }

        self.prev = Some(candle);
        // Neither level established means there is nothing to report yet. The
        // trait defines `None` as "insufficient inputs to produce a defined
        // value", and a pair of NaNs is exactly that — on a flat series the old
        // code emitted it on every bar forever, and it reached the bindings as
        // two NaNs in a flat output buffer.
        if self.buy_risk.is_none() && self.sell_risk.is_none() {
            return None;
        }
        self.ready = true;
        Some(TdRiskLevelOutput {
            buy_risk: self.buy_risk.unwrap_or(f64::NAN),
            sell_risk: self.sell_risk.unwrap_or(f64::NAN),
        })
    }

    fn reset(&mut self) {
        self.closes.clear();
        self.prev = None;
        self.buy_count = 0;
        self.sell_count = 0;
        self.buy_extreme = None;
        self.sell_extreme = None;
        self.buy_risk = None;
        self.sell_risk = None;
        self.ready = false;
    }

    /// Lower bound only: a risk level needs a *completed* setup, which depends
    /// on the data, so the first value can arrive arbitrarily later than this —
    /// and on a series that never completes a setup, never.
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
        "TDRiskLevel"
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
    fn uptrend_sets_sell_risk_above_highest_high_of_setup() {
        // Strictly rising closes -> sell setup completes at idx 12.
        // The sell run starts at idx 4 (first bar that has close >
        // close[i-4]). The highest high during the run is the bar at
        // idx 12 (since the series is strictly increasing).
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
        let mut td = TdRiskLevel::classic();
        let out = td.batch(&candles);
        let after = out[12].expect("ready");
        assert!(after.buy_risk.is_nan());
        // High at idx 12 is 13.5; the true range there is 1.0 (1.0 vs
        // |13.5-12|=1.5 vs |12.5-12|=0.5 -> max=1.5). So sell_risk =
        // 13.5 + 1.5 = 15.0.
        assert_relative_eq!(after.sell_risk, 15.0, epsilon = 1e-12);
    }

    #[test]
    fn flat_series_never_emits() {
        // Neither setup advances, so no level ever exists and nothing is
        // emitted. This used to yield `Some` with two NaNs on every bar.
        let candles: Vec<Candle> = (0..30).map(|i| c(10.5, 9.5, 10.0, i64::from(i))).collect();
        let mut td = TdRiskLevel::classic();
        let out = td.batch(&candles);
        assert!(out.iter().all(Option::is_none));
        assert!(!td.is_ready());
    }

    #[test]
    fn batch_equals_streaming() {
        let candles: Vec<Candle> = (0..80)
            .map(|i| {
                let m = 100.0 + (f64::from(i) * 0.3).sin() * 5.0;
                c(m + 1.0, m - 1.0, m, i64::from(i))
            })
            .collect();
        let mut a = TdRiskLevel::classic();
        let mut b = TdRiskLevel::classic();
        let av = a.batch(&candles);
        let bv: Vec<_> = candles.iter().map(|x| b.update(*x)).collect();
        assert_eq!(av.len(), bv.len());
        for (i, (x, y)) in av.iter().zip(bv.iter()).enumerate() {
            assert_eq!(x.is_some(), y.is_some(), "row {i} option mismatch");
            if let (Some(a), Some(b)) = (x, y) {
                assert_eq!(a.buy_risk.is_nan(), b.buy_risk.is_nan());
                assert_eq!(a.sell_risk.is_nan(), b.sell_risk.is_nan());
                // The NaN flags match, so a NaN on one side is a NaN on both.
                let close = |x: f64, y: f64| x.is_nan() || (x - y).abs() <= 1e-12;
                assert!(close(a.buy_risk, b.buy_risk), "row {i} buy risk");
                assert!(close(a.sell_risk, b.sell_risk), "row {i} sell risk");
            }
        }
    }

    #[test]
    fn rejects_invalid_params() {
        assert!(matches!(TdRiskLevel::new(0, 9), Err(Error::PeriodZero)));
        assert!(matches!(TdRiskLevel::new(4, 0), Err(Error::PeriodZero)));
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
        let mut td = TdRiskLevel::classic();
        td.batch(&candles);
        assert!(td.is_ready());
        td.reset();
        assert!(!td.is_ready());
        assert_eq!(td.update(candles[0]), None);
    }

    #[test]
    fn accessors_and_metadata() {
        let td = TdRiskLevel::classic();
        assert_eq!(td.params(), (4, 9));
        assert_eq!(td.warmup_period(), 5);
        assert_eq!(td.name(), "TDRiskLevel");
    }
}
