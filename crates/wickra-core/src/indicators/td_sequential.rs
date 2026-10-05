#![allow(clippy::doc_markdown)]

//! Tom DeMark TD Sequential (Setup + Countdown).
//!
//! TD Sequential is DeMark's flagship two-phase exhaustion pattern:
//!
//! 1. **Setup phase** — 9 consecutive bars whose close is less-than (buy
//!    setup) or greater-than (sell setup) the close 4 bars earlier. The
//!    setup *completes* on the 9th bar.
//! 2. **Countdown phase** — after a completed setup, count up to 13 bars
//!    that satisfy the countdown comparison (buy countdown: `close <= low`
//!    two bars earlier; sell countdown: `close >= high` two bars earlier).
//!    Countdown bars do not need to be consecutive. The 13th bar must also
//!    trade through the close of countdown bar 8 (low at or below it for a
//!    buy, high at or above it for a sell); otherwise it is deferred.
//!
//! A completed countdown (13) signals exhaustion in the direction of the
//! original setup and is the canonical DeMark reversal signal.
//!
//! Output struct `TdSequentialOutput`:
//!
//! - `setup`: signed setup count (positive for buy setup, negative for sell
//!   setup, 0 when no streak is active; capped at ±9).
//! - `countdown`: signed countdown count (positive for buy countdown, negative
//!   for sell countdown, 0 when no countdown is active; capped at ±13).
//! - `direction`: `+1.0` if a buy countdown is currently active, `-1.0` if a
//!   sell countdown is active, `0.0` otherwise. The countdown direction is
//!   set when the originating setup completes and stays valid until the
//!   countdown finishes or is invalidated by an opposite-direction setup.

use std::collections::VecDeque;

use crate::error::{Error, Result};
use crate::ohlcv::Candle;
use crate::traits::Indicator;

/// Direction of an active TD Sequential countdown phase.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Direction {
    None,
    Buy,
    Sell,
}

/// Output of [`TdSequential`]: setup count, countdown count, and active
/// countdown direction.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TdSequentialOutput {
    /// Signed setup count: +N for an active buy setup of length `N`, −N for
    /// a sell setup of length `N`, 0 if neither streak is active. Capped at
    /// ±9 (the canonical setup target).
    pub setup: f64,
    /// Signed countdown count: +N for an active buy countdown of length `N`,
    /// −N for a sell countdown of length `N`, 0 if no countdown is active.
    /// Capped at ±13.
    pub countdown: f64,
    /// Direction of the active countdown: `+1.0` for buy, `−1.0` for sell,
    /// `0.0` if no countdown is currently active.
    pub direction: f64,
}

/// TD Sequential state machine: combined Setup (1-9) + Countdown (1-13).
/// # Example
///
/// ```
/// use wickra_core::{TdSequential, Candle, Indicator};
///
/// let mut indicator = TdSequential::new(4, 9, 2, 13).unwrap();
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
pub struct TdSequential {
    // Rolling window of recent candles. We need up to 5 closes back (for the
    // setup rule which compares close[i] vs close[i-4]) and the high/low from
    // 2 bars ago (for the countdown rule).
    candles: VecDeque<Candle>,
    setup_lookback: usize,
    setup_target: usize,
    countdown_lookback: usize,
    countdown_target: usize,
    buy_setup: usize,
    sell_setup: usize,
    buy_countdown: usize,
    sell_countdown: usize,
    /// Close of countdown bar `countdown_target − 5` (bar 8 of 13), which bar
    /// 13 must reach; `NaN` until that bar is counted.
    qualifier_close: f64,
    countdown_dir: Direction,
    ready: bool,
}

impl TdSequential {
    /// Construct a TD Sequential with explicit lookbacks and targets. The
    /// canonical DeMark configuration is `setup_lookback = 4`, `setup_target =
    /// 9`, `countdown_lookback = 2`, `countdown_target = 13`.
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
        // Need to keep enough candles for both rules: setup uses close[-N];
        // countdown uses high/low[-M]. Reserve `max(N, M) + 1` slots.
        let cap = setup_lookback.max(countdown_lookback) + 1;
        Ok(Self {
            candles: VecDeque::with_capacity(cap),
            setup_lookback,
            setup_target,
            countdown_lookback,
            countdown_target,
            buy_setup: 0,
            sell_setup: 0,
            buy_countdown: 0,
            sell_countdown: 0,
            qualifier_close: f64::NAN,
            countdown_dir: Direction::None,
            ready: false,
        })
    }

    /// DeMark's classic configuration: setup `lookback = 4, target = 9`,
    /// countdown `lookback = 2, target = 13`.
    pub fn classic() -> Self {
        Self::new(4, 9, 2, 13).expect("classic TD Sequential parameters are valid")
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

impl Indicator for TdSequential {
    type Input = Candle;
    type Output = TdSequentialOutput;

    fn update(&mut self, candle: Candle) -> Option<TdSequentialOutput> {
        let cap = self.setup_lookback.max(self.countdown_lookback) + 1;
        if self.candles.len() == cap {
            self.candles.pop_front();
        }
        // The required minimum history is `max(setup_lookback,
        // countdown_lookback)` previous bars. Once we have that many, we can
        // evaluate both rules.
        let need = self.setup_lookback.max(self.countdown_lookback);
        if self.candles.len() < need {
            self.candles.push_back(candle);
            return None;
        }

        // --- Setup rule: compare to close[setup_lookback bars ago] ---
        // After `need` candles are buffered, the candle at offset `need - L`
        // from the front is the one `L` bars before the new candle (0-based
        // count: `front()` is `need` bars ago).
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

        // --- Countdown activation: when a setup completes, arm the countdown
        // in the same direction; an opposite-direction setup invalidates any
        // active countdown.
        if self.buy_setup == self.setup_target {
            if self.countdown_dir != Direction::Buy {
                self.buy_countdown = 0;
                self.sell_countdown = 0;
                self.qualifier_close = f64::NAN;
            }
            self.countdown_dir = Direction::Buy;
        } else if self.sell_setup == self.setup_target {
            if self.countdown_dir != Direction::Sell {
                self.buy_countdown = 0;
                self.sell_countdown = 0;
                self.qualifier_close = f64::NAN;
            }
            self.countdown_dir = Direction::Sell;
        }

        // --- Countdown rule: compare close to high/low `countdown_lookback`
        // bars ago. Only the active direction advances. Once a countdown
        // reaches `countdown_target`, the strict `< countdown_target` guard
        // keeps it pinned so the caller can detect the "13" signal on this
        // bar and any subsequent bar until a new setup arms a fresh run.
        let cd_ref_idx = need - self.countdown_lookback;
        let cd_ref = &self.candles[cd_ref_idx];
        match self.countdown_dir {
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

        let setup = if self.buy_setup > 0 {
            self.buy_setup as f64
        } else if self.sell_setup > 0 {
            -(self.sell_setup as f64)
        } else {
            0.0
        };
        let (countdown, direction) = match self.countdown_dir {
            Direction::Buy => (self.buy_countdown as f64, 1.0),
            Direction::Sell => (-(self.sell_countdown as f64), -1.0),
            Direction::None => (0.0, 0.0),
        };

        Some(TdSequentialOutput {
            setup,
            countdown,
            direction,
        })
    }

    fn reset(&mut self) {
        self.candles.clear();
        self.buy_setup = 0;
        self.sell_setup = 0;
        self.buy_countdown = 0;
        self.sell_countdown = 0;
        self.qualifier_close = f64::NAN;
        self.countdown_dir = Direction::None;
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
        "TDSequential"
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
    fn pure_uptrend_completes_sell_setup_then_progresses_countdown() {
        // Strictly increasing closes -> sell setup increments every bar past
        // warmup, reaching -9 by index 12 (warmup is 4 + 1). After that,
        // every bar continues to make a higher close, so each subsequent bar
        // also makes a higher close than the high 2 bars ago — the sell
        // countdown increments on each bar after activation.
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
        let mut td = TdSequential::classic();
        let out = td.batch(&candles);

        // Warmup: indices 0..3 yield None (need=4 prior closes).
        for v in out.iter().take(4) {
            assert!(v.is_none());
        }
        // After index 12, setup reaches -9 (completed). From the next bar on,
        // countdown begins to increment.
        let at_12 = out[12].expect("setup ready");
        assert_eq!(at_12.setup, -9.0);
        assert_eq!(at_12.direction, -1.0); // countdown direction armed

        // Each subsequent bar makes close > high[i-2], so the sell countdown
        // advances by one per bar; by some later index it caps at -13.
        let later = out[30].expect("ready");
        assert_eq!(later.direction, -1.0);
        assert_eq!(later.countdown, -13.0);
    }

    #[test]
    fn pure_downtrend_completes_buy_setup_then_progresses_countdown() {
        // Strictly decreasing closes -> buy setup increments every bar past
        // warmup, reaching 9 by index 12. After activation, every subsequent
        // bar satisfies close <= low[i-2], so the buy countdown advances by
        // one per bar and pins at +13.
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
        let mut td = TdSequential::classic();
        let out = td.batch(&candles);

        // Warmup: indices 0..3 yield None.
        for v in out.iter().take(4) {
            assert!(v.is_none());
        }
        let at_12 = out[12].expect("setup ready");
        assert_eq!(at_12.setup, 9.0);
        assert_eq!(at_12.direction, 1.0); // buy direction armed

        // By idx 30 the buy countdown has saturated at +13.
        let later = out[30].expect("ready");
        assert_eq!(later.direction, 1.0);
        assert_eq!(later.countdown, 13.0);
    }

    #[test]
    fn flat_series_emits_zero_setup_and_no_countdown() {
        // All closes equal -> never completes any setup; countdown never
        // activates; setup, countdown, direction all stay at 0.
        let candles: Vec<Candle> = (0..30).map(|i| c(10.5, 9.5, 10.0, i64::from(i))).collect();
        let mut td = TdSequential::classic();
        let out = td.batch(&candles);
        for v in out.iter().skip(5) {
            let o = v.expect("ready post-warmup");
            assert_eq!(o.setup, 0.0);
            assert_eq!(o.countdown, 0.0);
            assert_eq!(o.direction, 0.0);
        }
    }

    #[test]
    fn batch_equals_streaming() {
        let candles: Vec<Candle> = (0..60)
            .map(|i| {
                let m = 100.0 + (f64::from(i) * 0.3).sin() * 5.0;
                c(m + 1.0, m - 1.0, m, i64::from(i))
            })
            .collect();
        let mut a = TdSequential::classic();
        let mut b = TdSequential::classic();
        assert_eq!(
            a.batch(&candles),
            candles.iter().map(|x| b.update(*x)).collect::<Vec<_>>()
        );
    }

    #[test]
    fn rejects_invalid_params() {
        assert!(matches!(
            TdSequential::new(0, 9, 2, 13),
            Err(Error::PeriodZero)
        ));
        assert!(matches!(
            TdSequential::new(4, 0, 2, 13),
            Err(Error::PeriodZero)
        ));
        assert!(matches!(
            TdSequential::new(4, 9, 0, 13),
            Err(Error::PeriodZero)
        ));
        assert!(matches!(
            TdSequential::new(4, 9, 2, 0),
            Err(Error::PeriodZero)
        ));
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
        let mut td = TdSequential::classic();
        td.batch(&candles);
        assert!(td.is_ready());
        td.reset();
        assert!(!td.is_ready());
        assert_eq!(td.update(candles[0]), None);
    }

    #[test]
    fn accessors_and_metadata() {
        let td = TdSequential::classic();
        assert_eq!(td.params(), (4, 9, 2, 13));
        assert_eq!(td.warmup_period(), 5);
        assert_eq!(td.name(), "TDSequential");
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

    fn countdowns(out: &[Option<TdSequentialOutput>]) -> Vec<Option<f64>> {
        out.iter().map(|o| o.map(|v| v.countdown)).collect()
    }

    #[test]
    fn buy_bar_13_is_deferred_until_low_reaches_bar_8_close() {
        let mut td = TdSequential::classic();
        let out = td.batch(&from_closes(&buy_deferral_closes()));
        let cd = countdowns(&out);
        assert_eq!(cd[19], Some(8.0));
        assert_eq!(cd[23], Some(12.0));
        assert!(cd[24..28].iter().all(|v| *v == Some(12.0)));
        assert_eq!(cd[28], Some(13.0));
        // idx 27: closes 90, 95, 95, 89 vs closes 4 back (80, 79, 78, 77)
        // form a sell setup of 4 while the buy countdown stays armed.
        let at_27 = out[27].unwrap();
        assert_eq!(at_27.setup, -4.0);
        assert_eq!(at_27.direction, 1.0);
        // idx 28: 80 < close[24] = 90 starts a new buy setup of 1.
        assert_eq!(out[28].unwrap().setup, 1.0);
    }

    #[test]
    fn sell_bar_13_is_deferred_until_high_reaches_bar_8_close() {
        let mut td = TdSequential::classic();
        let out = td.batch(&from_closes(&sell_deferral_closes()));
        let cd = countdowns(&out);
        assert_eq!(cd[19], Some(-8.0));
        assert_eq!(cd[23], Some(-12.0));
        assert!(cd[24..28].iter().all(|v| *v == Some(-12.0)));
        assert_eq!(cd[28], Some(-13.0));
        let at_27 = out[27].unwrap();
        assert_eq!(at_27.setup, 4.0);
        assert_eq!(at_27.direction, -1.0);
    }

    #[test]
    fn qualifier_close_is_bar_8_close() {
        let mut td = TdSequential::classic();
        let candles = from_closes(&sell_deferral_closes());
        for candle in &candles[..19] {
            td.update(*candle);
        }
        assert!(td.qualifier_close.is_nan());
        td.update(candles[19]);
        assert_eq!(td.qualifier_close.to_bits(), 119.0_f64.to_bits());
    }

    #[test]
    fn short_target_has_no_qualifier() {
        // countdown_target = 3 <= 5: no bar 8 exists, so the qualifier stays
        // NaN and the final bar completes unconditionally (idx 12, 13, 14).
        let closes: Vec<f64> = (70..=100).rev().map(f64::from).collect();
        let mut buy = TdSequential::new(4, 9, 2, 3).unwrap();
        let cd = countdowns(&buy.batch(&from_closes(&closes)));
        assert_eq!(cd[12], Some(1.0));
        assert_eq!(cd[14], Some(3.0));
        assert_eq!(cd[30], Some(3.0));
        assert!(buy.qualifier_close.is_nan());

        let rising: Vec<f64> = closes.iter().map(|x| 200.0 - x).collect();
        let mut sell = TdSequential::new(4, 9, 2, 3).unwrap();
        let cd = countdowns(&sell.batch(&from_closes(&rising)));
        assert_eq!(cd[14], Some(-3.0));
        assert_eq!(cd[30], Some(-3.0));
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
        let mut td = TdSequential::classic();
        let out: Vec<Option<TdSequentialOutput>> = candles.iter().map(|x| td.update(*x)).collect();
        let at_25 = out[25].unwrap();
        assert_eq!((at_25.setup, at_25.countdown), (0.0, 12.0));
        assert_eq!(out[33].unwrap().countdown, 12.0);
        // idx 34: invalidated; close 88 >= high[32] = 86.5 so the sell
        // countdown starts at 1 on the same bar and reaches 13 at idx 46.
        let at_34 = out[34].unwrap();
        assert_eq!(
            (at_34.setup, at_34.countdown, at_34.direction),
            (-9.0, -1.0, -1.0)
        );
        assert_eq!(out[46].unwrap().countdown, -13.0);

        let mut probe = TdSequential::classic();
        for candle in &candles[..34] {
            probe.update(*candle);
        }
        assert_eq!(probe.qualifier_close.to_bits(), 81.0_f64.to_bits());
        probe.update(candles[34]);
        assert!(probe.qualifier_close.is_nan());

        // And back again: a buy setup after the sell countdown re-arms buy.
        let mut back = closes.clone();
        back.extend((60..=119).rev().map(f64::from));
        let mut td2 = TdSequential::classic();
        let last = td2
            .batch(&from_closes(&back))
            .last()
            .copied()
            .flatten()
            .unwrap();
        assert_eq!((last.countdown, last.direction), (13.0, 1.0));
    }

    #[test]
    fn first_value_lands_at_warmup_minus_one() {
        let candles = from_closes(&buy_deferral_closes());
        for (sl, cl) in [(4, 2), (2, 6), (1, 1)] {
            let mut td = TdSequential::new(sl, 9, cl, 13).unwrap();
            let warm = td.warmup_period();
            let out = td.batch(&candles);
            assert!(out[..warm - 1].iter().all(Option::is_none));
            assert!(out[warm - 1].is_some());
        }
    }

    #[test]
    fn reset_reproduces_fresh_run() {
        let candles = from_closes(&sell_deferral_closes());
        let mut fresh = TdSequential::classic();
        let expected = fresh.batch(&candles);
        let mut td = TdSequential::classic();
        td.batch(&from_closes(&buy_deferral_closes()));
        td.reset();
        assert!(td.qualifier_close.is_nan());
        assert_eq!(td.batch(&candles), expected);
    }

    #[test]
    fn batch_equals_streaming_on_deferral_series() {
        let candles = from_closes(&buy_deferral_closes());
        let mut a = TdSequential::classic();
        let mut b = TdSequential::classic();
        let streamed: Vec<Option<TdSequentialOutput>> =
            candles.iter().map(|x| b.update(*x)).collect();
        assert_eq!(a.batch(&candles), streamed);
    }
}
