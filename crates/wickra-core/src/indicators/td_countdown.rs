#![allow(clippy::doc_markdown)]

//! Tom DeMark TD Countdown (standalone 13-bar countdown).
//!
//! The Countdown is the second half of DeMark's TD Sequential, packaged
//! here as a standalone indicator that runs the setup-detection phase
//! internally and then exposes only the countdown count (and direction)
//! to callers who don't need the running setup state.
//!
//! - **Setup detection** (internal): 9 consecutive bars whose close is
//!   less-than (buy setup) or greater-than (sell setup) the close
//!   `setup_lookback` bars earlier.
//! - **Buy countdown** advances on bars where `close[i] <= low[i -
//!   countdown_lookback]` (need not be consecutive). Saturates at
//!   `countdown_target` (13 in DeMark's classic configuration).
//! - **Sell countdown** advances on bars where `close[i] >= high[i -
//!   countdown_lookback]`.
//! - **Bar-13 qualifier:** the final countdown bar must also trade through the
//!   close of countdown bar `countdown_target − 5` (bar 8 of 13): its low at or
//!   below that close for a buy, its high at or above it for a sell. A bar that
//!   meets the comparison but not the qualifier is deferred, not counted.
//! - An opposite-direction setup completion invalidates the active
//!   countdown (count resets to zero in the new direction).
//!
//! Output is a signed counter: positive for an active buy countdown,
//! negative for an active sell countdown, and `0.0` when no countdown is
//! currently armed.
//!
//! This indicator differs from [`crate::TdSequential`] only in its
//! output shape: callers who only need the countdown value (and not the
//! running setup count) can use this for a smaller streaming payload.

use std::collections::VecDeque;

use crate::error::{Error, Result};
use crate::ohlcv::Candle;
use crate::traits::Indicator;

/// Direction of an active TD Countdown phase.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Direction {
    None,
    Buy,
    Sell,
}

/// TD Countdown — standalone 13-bar countdown.
/// # Example
///
/// ```
/// use wickra_core::{TdCountdown, Candle, Indicator};
///
/// let mut indicator = TdCountdown::new(4, 9, 2, 13).unwrap();
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
pub struct TdCountdown {
    setup_lookback: usize,
    setup_target: usize,
    countdown_lookback: usize,
    countdown_target: usize,
    candles: VecDeque<Candle>,
    buy_setup: usize,
    sell_setup: usize,
    buy_countdown: usize,
    sell_countdown: usize,
    /// Close of countdown bar `countdown_target − 5` (bar 8 of 13), which bar
    /// 13 must reach; `NaN` until that bar is counted.
    qualifier_close: f64,
    direction: Direction,
    ready: bool,
}

impl TdCountdown {
    /// Construct a TD Countdown with explicit lookbacks and targets. The
    /// canonical DeMark configuration is `setup_lookback = 4`,
    /// `setup_target = 9`, `countdown_lookback = 2`, `countdown_target = 13`.
    ///
    /// # Errors
    ///
    /// Returns [`Error::PeriodZero`] if any argument is zero.
    pub fn new(
        setup_lookback: usize,
        setup_target: usize,
        countdown_lookback: usize,
        countdown_target: usize,
    ) -> Result<Self> {
        if setup_lookback == 0
            || setup_target == 0
            || countdown_lookback == 0
            || countdown_target == 0
        {
            return Err(Error::PeriodZero);
        }
        let cap = setup_lookback.max(countdown_lookback) + 1;
        Ok(Self {
            setup_lookback,
            setup_target,
            countdown_lookback,
            countdown_target,
            candles: VecDeque::with_capacity(cap),
            buy_setup: 0,
            sell_setup: 0,
            buy_countdown: 0,
            sell_countdown: 0,
            qualifier_close: f64::NAN,
            direction: Direction::None,
            ready: false,
        })
    }

    /// DeMark's classic configuration: setup `lookback = 4, target = 9`,
    /// countdown `lookback = 2, target = 13`.
    pub fn classic() -> Self {
        Self::new(4, 9, 2, 13).expect("classic TD Countdown parameters are valid")
    }

    /// Configured `(setup_lookback, setup_target, countdown_lookback,
    /// countdown_target)`.
    pub const fn params(&self) -> (usize, usize, usize, usize) {
        (
            self.setup_lookback,
            self.setup_target,
            self.countdown_lookback,
            self.countdown_target,
        )
    }
}

impl Indicator for TdCountdown {
    type Input = Candle;
    type Output = f64;

    fn update(&mut self, candle: Candle) -> Option<f64> {
        let need = self.setup_lookback.max(self.countdown_lookback);
        let cap = need + 1;
        if self.candles.len() == cap {
            self.candles.pop_front();
        }
        if self.candles.len() < need {
            self.candles.push_back(candle);
            return None;
        }

        // Setup rule: compare to close[setup_lookback bars ago].
        let setup_ref_idx = need - self.setup_lookback;
        let setup_ref_close = self.candles[setup_ref_idx].close;
        if candle.close < setup_ref_close {
            self.buy_setup = (self.buy_setup + 1).min(self.setup_target);
            self.sell_setup = 0;
        } else if candle.close > setup_ref_close {
            self.sell_setup = (self.sell_setup + 1).min(self.setup_target);
            self.buy_setup = 0;
        } else {
            self.buy_setup = 0;
            self.sell_setup = 0;
        }

        if self.buy_setup == self.setup_target {
            if self.direction != Direction::Buy {
                self.buy_countdown = 0;
                self.sell_countdown = 0;
                self.qualifier_close = f64::NAN;
            }
            self.direction = Direction::Buy;
        } else if self.sell_setup == self.setup_target {
            if self.direction != Direction::Sell {
                self.buy_countdown = 0;
                self.sell_countdown = 0;
                self.qualifier_close = f64::NAN;
            }
            self.direction = Direction::Sell;
        }

        let cd_ref = self.candles[need - self.countdown_lookback];
        match self.direction {
            Direction::Buy => {
                if candle.close <= cd_ref.low && self.buy_countdown < self.countdown_target {
                    // The final bar must also trade at or below the close of
                    // countdown bar 8; otherwise it is deferred.
                    let next = self.buy_countdown + 1;
                    if next < self.countdown_target
                        || (self.qualifier_close.is_nan() || candle.low <= self.qualifier_close)
                    {
                        self.buy_countdown = next;
                        if next + 5 == self.countdown_target {
                            self.qualifier_close = candle.close;
                        }
                    }
                }
            }
            Direction::Sell => {
                if candle.close >= cd_ref.high && self.sell_countdown < self.countdown_target {
                    // The final bar must also trade at or above the close of
                    // countdown bar 8; otherwise it is deferred.
                    let next = self.sell_countdown + 1;
                    if next < self.countdown_target
                        || (self.qualifier_close.is_nan() || candle.high >= self.qualifier_close)
                    {
                        self.sell_countdown = next;
                        if next + 5 == self.countdown_target {
                            self.qualifier_close = candle.close;
                        }
                    }
                }
            }
            Direction::None => {}
        }

        self.candles.push_back(candle);
        self.ready = true;

        let v = match self.direction {
            Direction::Buy => self.buy_countdown as f64,
            Direction::Sell => -(self.sell_countdown as f64),
            Direction::None => 0.0,
        };
        Some(v)
    }

    fn reset(&mut self) {
        self.candles.clear();
        self.buy_setup = 0;
        self.sell_setup = 0;
        self.buy_countdown = 0;
        self.sell_countdown = 0;
        self.qualifier_close = f64::NAN;
        self.direction = Direction::None;
        self.ready = false;
    }

    #[inline]
    fn warmup_period(&self) -> usize {
        self.setup_lookback.max(self.countdown_lookback) + 1
    }

    #[inline]
    fn is_ready(&self) -> bool {
        self.ready
    }

    #[inline]
    fn name(&self) -> &'static str {
        "TDCountdown"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;

    fn c(high: f64, low: f64, close: f64, ts: i64) -> Candle {
        Candle::new_unchecked(close, high, low, close, 0.0, ts)
    }

    #[test]
    fn pure_uptrend_completes_setup_then_runs_sell_countdown_to_minus_13() {
        let candles: Vec<Candle> = (1..=40)
            .map(|i| {
                c(
                    f64::from(i) + 0.5,
                    f64::from(i) - 0.5,
                    f64::from(i),
                    i64::from(i),
                )
            })
            .collect();
        let mut td = TdCountdown::classic();
        let out = td.batch(&candles);
        // Warmup: 4 None values.
        for v in out.iter().take(4) {
            assert!(v.is_none());
        }
        // At idx 12 the sell setup completes; on the same bar the
        // countdown rule fires once because close > high[i-2] for a
        // strictly-rising series, so countdown == -1.
        assert_eq!(out[12].expect("ready"), -1.0);
        // After enough bars the countdown saturates at -13.
        assert_eq!(out[30].expect("ready"), -13.0);
    }

    #[test]
    fn pure_downtrend_completes_setup_then_runs_buy_countdown_to_plus_13() {
        let candles: Vec<Candle> = (1..=40)
            .rev()
            .enumerate()
            .map(|(k, i)| {
                c(
                    f64::from(i) + 0.5,
                    f64::from(i) - 0.5,
                    f64::from(i),
                    i64::try_from(k).unwrap(),
                )
            })
            .collect();
        let mut td = TdCountdown::classic();
        let out = td.batch(&candles);
        for v in out.iter().take(4) {
            assert!(v.is_none());
        }
        // At idx 12 the buy setup completes; on the same bar the
        // countdown rule fires once because close < low[i-2] for a
        // strictly-falling series, so countdown == +1.
        assert_eq!(out[12].expect("ready"), 1.0);
        // After enough bars the countdown saturates at +13.
        assert_eq!(out[30].expect("ready"), 13.0);
    }

    #[test]
    fn flat_series_never_arms_countdown() {
        let candles: Vec<Candle> = (0..30).map(|i| c(10.5, 9.5, 10.0, i64::from(i))).collect();
        let mut td = TdCountdown::classic();
        for v in td.batch(&candles).into_iter().flatten() {
            assert_eq!(v, 0.0);
        }
    }

    #[test]
    fn batch_equals_streaming() {
        let candles: Vec<Candle> = (0..80)
            .map(|i| {
                let m = 100.0 + (f64::from(i) * 0.3).sin() * 5.0;
                c(m + 1.0, m - 1.0, m, i64::from(i))
            })
            .collect();
        let mut a = TdCountdown::classic();
        let mut b = TdCountdown::classic();
        assert_eq!(
            a.batch(&candles),
            candles.iter().map(|x| b.update(*x)).collect::<Vec<_>>()
        );
    }

    #[test]
    fn rejects_invalid_params() {
        assert!(matches!(
            TdCountdown::new(0, 9, 2, 13),
            Err(Error::PeriodZero)
        ));
        assert!(matches!(
            TdCountdown::new(4, 0, 2, 13),
            Err(Error::PeriodZero)
        ));
        assert!(matches!(
            TdCountdown::new(4, 9, 0, 13),
            Err(Error::PeriodZero)
        ));
        assert!(matches!(
            TdCountdown::new(4, 9, 2, 0),
            Err(Error::PeriodZero)
        ));
    }

    #[test]
    fn reset_clears_state() {
        let candles: Vec<Candle> = (1..=30)
            .map(|i| {
                c(
                    f64::from(i) + 0.5,
                    f64::from(i) - 0.5,
                    f64::from(i),
                    i64::from(i),
                )
            })
            .collect();
        let mut td = TdCountdown::classic();
        td.batch(&candles);
        assert!(td.is_ready());
        td.reset();
        assert!(!td.is_ready());
        assert_eq!(td.update(candles[0]), None);
    }

    #[test]
    fn accessors_and_metadata() {
        let td = TdCountdown::classic();
        assert_eq!(td.params(), (4, 9, 2, 13));
        assert_eq!(td.warmup_period(), 5);
        assert_eq!(td.name(), "TDCountdown");
    }

    /// Candles with a +-0.5 range around each close, timestamped by index.
    fn from_closes(closes: &[f64]) -> Vec<Candle> {
        closes
            .iter()
            .enumerate()
            .map(|(k, &m)| c(m + 0.5, m - 0.5, m, i64::try_from(k).unwrap()))
            .collect()
    }

    /// Buy-side deferral series. Closes fall 100 -> 77 (idx 0..=23): the buy
    /// setup completes at idx 12 (countdown 1), countdown bar 8 is idx 19
    /// (close 81, the stored qualifier) and idx 23 reaches countdown 12.
    /// A rally (90, 95, 95) follows, then idx 27 closes at 89 <= low[25] =
    /// 94.5 (countdown comparison met) but its low 88.5 > 81, so bar 13 is
    /// deferred. Idx 28 closes at 80 <= low[26] = 94.5 with low 79.5 <= 81,
    /// which completes the countdown. The rally only builds a sell setup of 4.
    fn buy_deferral_closes() -> Vec<f64> {
        let mut closes: Vec<f64> = (77..=100).rev().map(f64::from).collect();
        closes.extend([90.0, 95.0, 95.0, 89.0, 80.0]);
        closes
    }

    /// Mirror image of [`buy_deferral_closes`] around 100: the sell qualifier
    /// is close 119 at idx 19; idx 27 (high 111.5 < 119) is deferred and idx
    /// 28 (high 120.5 >= 119) completes the sell countdown.
    fn sell_deferral_closes() -> Vec<f64> {
        buy_deferral_closes().iter().map(|x| 200.0 - x).collect()
    }

    #[test]
    fn buy_bar_13_is_deferred_until_low_reaches_bar_8_close() {
        let mut td = TdCountdown::classic();
        let out = td.batch(&from_closes(&buy_deferral_closes()));
        assert_eq!(out[19], Some(8.0));
        assert_eq!(out[23], Some(12.0));
        // Rally bars and the deferred bar all keep the count at 12.
        assert!(out[24..28].iter().all(|v| *v == Some(12.0)));
        assert_eq!(out[28], Some(13.0));
    }

    #[test]
    fn sell_bar_13_is_deferred_until_high_reaches_bar_8_close() {
        let mut td = TdCountdown::classic();
        let out = td.batch(&from_closes(&sell_deferral_closes()));
        assert_eq!(out[19], Some(-8.0));
        assert_eq!(out[23], Some(-12.0));
        assert!(out[24..28].iter().all(|v| *v == Some(-12.0)));
        assert_eq!(out[28], Some(-13.0));
    }

    #[test]
    fn qualifier_close_is_bar_8_close() {
        let mut td = TdCountdown::classic();
        let candles = from_closes(&buy_deferral_closes());
        for candle in &candles[..19] {
            td.update(*candle);
        }
        assert!(td.qualifier_close.is_nan());
        td.update(candles[19]);
        assert_eq!(td.qualifier_close.to_bits(), 81.0_f64.to_bits());
    }

    #[test]
    fn short_target_has_no_qualifier() {
        // countdown_target = 3 <= 5: no bar 8 exists, so the qualifier stays
        // NaN and the final bar completes unconditionally (idx 12, 13, 14).
        let closes: Vec<f64> = (70..=100).rev().map(f64::from).collect();
        let candles = from_closes(&closes);
        let mut buy = TdCountdown::new(4, 9, 2, 3).unwrap();
        let out = buy.batch(&candles);
        assert_eq!(out[12], Some(1.0));
        assert_eq!(out[14], Some(3.0));
        assert_eq!(out[30], Some(3.0));
        assert!(buy.qualifier_close.is_nan());

        let rising: Vec<f64> = closes.iter().map(|x| 200.0 - x).collect();
        let mut sell = TdCountdown::new(4, 9, 2, 3).unwrap();
        let out = sell.batch(&from_closes(&rising));
        assert_eq!(out[14], Some(-3.0));
        assert_eq!(out[30], Some(-3.0));
        assert!(sell.qualifier_close.is_nan());
    }

    #[test]
    fn opposite_setup_invalidates_and_clears_qualifier() {
        // Buy countdown reaches 12 at idx 23 with the qualifier stored (81);
        // then closes rise 78, 79, ... Idx 24 (78 < 80) and idx 25 (79 == 79)
        // do not count, so the sell setup runs idx 26..=34 and completes at
        // idx 34 (close 88), resetting everything.
        let mut closes: Vec<f64> = (77..=100).rev().map(f64::from).collect();
        closes.extend((78..=120).map(f64::from));
        let candles = from_closes(&closes);
        let mut td = TdCountdown::classic();
        let out: Vec<Option<f64>> = candles.iter().map(|x| td.update(*x)).collect();
        assert_eq!(out[23], Some(12.0));
        assert_eq!(out[33], Some(12.0));
        // idx 34: invalidated; close 88 >= high[32] = 86.5 so the sell
        // countdown starts at 1 on the same bar and reaches 13 at idx 46.
        assert_eq!(out[34], Some(-1.0));
        assert_eq!(out[46], Some(-13.0));

        let mut probe = TdCountdown::classic();
        for candle in &candles[..34] {
            probe.update(*candle);
        }
        assert_eq!(probe.qualifier_close.to_bits(), 81.0_f64.to_bits());
        probe.update(candles[34]);
        assert!(probe.qualifier_close.is_nan());

        // And back again: a buy setup after the sell countdown re-arms buy.
        let mut back = closes.clone();
        back.extend((60..=119).rev().map(f64::from));
        let mut td2 = TdCountdown::classic();
        let out2 = td2.batch(&from_closes(&back));
        let last = out2.last().copied().flatten().unwrap();
        assert_eq!(last.to_bits(), 13.0_f64.to_bits());
    }

    #[test]
    fn first_value_lands_at_warmup_minus_one() {
        let candles = from_closes(&buy_deferral_closes());
        for (sl, cl) in [(4, 2), (2, 6), (1, 1)] {
            let mut td = TdCountdown::new(sl, 9, cl, 13).unwrap();
            let warm = td.warmup_period();
            let out = td.batch(&candles);
            assert!(out[..warm - 1].iter().all(Option::is_none));
            assert!(out[warm - 1].is_some());
        }
    }

    #[test]
    fn reset_reproduces_fresh_run() {
        let candles = from_closes(&sell_deferral_closes());
        let mut fresh = TdCountdown::classic();
        let expected = fresh.batch(&candles);
        let mut td = TdCountdown::classic();
        td.batch(&from_closes(&buy_deferral_closes()));
        td.reset();
        assert!(td.qualifier_close.is_nan());
        assert_eq!(td.batch(&candles), expected);
    }

    #[test]
    fn batch_nan_into_matches_streaming() {
        let candles = from_closes(&buy_deferral_closes());
        let mut a = TdCountdown::classic();
        let mut out = vec![0.0; candles.len()];
        a.batch_nan_into(&candles, &mut out);
        let mut b = TdCountdown::classic();
        let streamed: Vec<f64> = candles
            .iter()
            .map(|x| b.update(*x).unwrap_or(f64::NAN))
            .collect();
        assert!(out
            .iter()
            .zip(&streamed)
            .all(|(x, y)| x.to_bits() == y.to_bits()));
    }
}
