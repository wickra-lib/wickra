//! Ichimoku Kinko Hyo — the five-line cloud chart.
//!
//! The Ichimoku system bundles five distinct lines computed from highs, lows
//! and closes:
//!
//! - **Tenkan-sen** (Conversion Line): midpoint of the last `tenkan_period`
//!   highs and lows (default 9).
//! - **Kijun-sen** (Base Line): midpoint over `kijun_period` (default 26).
//! - **Senkou Span A** (Leading A): `(tenkan + kijun) / 2`, shifted *forward*
//!   by the displacement.
//! - **Senkou Span B** (Leading B): midpoint over `senkou_b_period` (default
//!   52), also shifted forward by the displacement.
//! - **Chikou Span** (Lagging Span): the current close, shifted *backwards* by
//!   the displacement.
//!
//! # Displacement is counted inclusively
//!
//! The displacement counts the current bar as bar 1, the way the built-in
//! Ichimoku on `TradingView` does (it plots with `offset = displacement - 1`). The
//! shift in bars is therefore `displacement - 1`: 25 bars at the classic
//! setting of 26, and no shift at all for `displacement = 1`.
//!
//! The two Senkou Spans form the **Kumo** (cloud). At step *n* the visible
//! Senkou A/B are the values computed at step *n − (displacement − 1)*. A chart
//! draws the Chikou *displacement − 1* bars to the left of the close it shows;
//! in a streaming setting the only Chikou we can emit at step *n* is the close
//! from step *n − (displacement − 1)*, which is what `chikou` holds.

#![allow(clippy::too_many_arguments)]

use std::collections::VecDeque;

use crate::error::{Error, Result};
use crate::ohlcv::Candle;
use crate::traits::Indicator;

/// All five Ichimoku lines at one step.
///
/// `tenkan` and `kijun` reflect data up to and including the current bar.
/// `senkou_a` / `senkou_b` are the leading-span values *visible at the current
/// bar*, computed `displacement - 1` bars ago. `chikou` is the close from
/// `displacement - 1` bars ago (its "lagging" placement on charts). The
/// displacement is counted inclusively; see the module docs.
///
/// Any field that is not yet defined (insufficient history) is `None`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct IchimokuOutput {
    /// Tenkan-sen — midpoint of the last `tenkan_period` highs/lows.
    pub tenkan: Option<f64>,
    /// Kijun-sen — midpoint of the last `kijun_period` highs/lows.
    pub kijun: Option<f64>,
    /// Senkou Span A as visible at the current bar (computed from
    /// `(tenkan + kijun) / 2` at step `n - (displacement - 1)`).
    pub senkou_a: Option<f64>,
    /// Senkou Span B as visible at the current bar (computed from the
    /// `senkou_b_period` midpoint at step `n - (displacement - 1)`).
    pub senkou_b: Option<f64>,
    /// Chikou Span — the close from `displacement - 1` bars ago.
    pub chikou: Option<f64>,
}

/// Ichimoku Kinko Hyo indicator.
///
/// Standard parameters are `(9, 26, 52, 26)`. The first fully-populated output
/// (every field `Some`) appears after `senkou_b_period + displacement - 1`
/// candles — 77 bars at the defaults — because Senkou B needs its own 52-bar
/// midpoint and is then shifted forward 25 bars (the displacement of 26 counts
/// the current bar; see the module docs).
///
/// # Example
///
/// ```
/// use wickra_core::{Candle, Ichimoku, Indicator};
///
/// let mut ichi = Ichimoku::classic();
/// for i in 0..120 {
///     let p = 100.0 + f64::from(i);
///     let candle = Candle::new(p, p + 2.0, p - 2.0, p + 1.0, 0.0, i64::from(i)).unwrap();
///     ichi.update(candle);
/// }
/// let out = ichi.value().unwrap();
/// assert!(out.tenkan.is_some() && out.kijun.is_some());
/// assert!(out.senkou_a.is_some() && out.senkou_b.is_some());
/// assert!(out.chikou.is_some());
/// ```
#[derive(Debug, Clone)]
pub struct Ichimoku {
    tenkan_period: usize,
    kijun_period: usize,
    senkou_b_period: usize,
    displacement: usize,
    // Rolling window of recent highs/lows for the longest lookback we need.
    highs: VecDeque<f64>,
    lows: VecDeque<f64>,
    // Past (tenkan+kijun)/2 values used to emit the displaced Senkou A.
    senkou_a_history: VecDeque<f64>,
    // Past Senkou B midpoint values used to emit the displaced Senkou B.
    senkou_b_history: VecDeque<f64>,
    // Past closes for the lagging Chikou span.
    close_history: VecDeque<f64>,
    last: Option<IchimokuOutput>,
    /// Whether a value has been emitted since the last reset. The trait
    /// defines `is_ready` as exactly that, and the state this used to key
    /// off changed at a different moment.
    has_emitted: bool,
}

impl Ichimoku {
    /// Construct an Ichimoku indicator with custom periods.
    ///
    /// `tenkan_period` is the short midpoint window (default 9), `kijun_period`
    /// the medium (default 26), `senkou_b_period` the long (default 52), and
    /// `displacement` the forward/backward shift (default 26), counted
    /// inclusively: the spans move `displacement - 1` bars.
    ///
    /// # Errors
    ///
    /// Returns [`Error::PeriodZero`] if any of `tenkan_period`, `kijun_period`,
    /// `senkou_b_period`, or `displacement` is zero, and [`Error::InvalidPeriod`]
    /// if the periods are not in strictly increasing order
    /// (`tenkan < kijun < senkou_b`).
    pub fn new(
        tenkan_period: usize,
        kijun_period: usize,
        senkou_b_period: usize,
        displacement: usize,
    ) -> Result<Self> {
        if tenkan_period == 0 || kijun_period == 0 || senkou_b_period == 0 || displacement == 0 {
            return Err(Error::PeriodZero);
        }
        if tenkan_period >= kijun_period || kijun_period >= senkou_b_period {
            return Err(Error::InvalidPeriod {
                message: "Ichimoku periods must satisfy tenkan < kijun < senkou_b",
            });
        }
        let cap = senkou_b_period;
        Ok(Self {
            tenkan_period,
            kijun_period,
            senkou_b_period,
            displacement,
            highs: VecDeque::with_capacity(cap),
            lows: VecDeque::with_capacity(cap),
            senkou_a_history: VecDeque::with_capacity(displacement),
            senkou_b_history: VecDeque::with_capacity(displacement),
            close_history: VecDeque::with_capacity(displacement),
            last: None,
            has_emitted: false,
        })
    }

    /// Classical `(9, 26, 52, 26)` configuration.
    pub fn classic() -> Self {
        Self::new(9, 26, 52, 26).expect("classic Ichimoku periods are valid")
    }

    /// Configured periods as `(tenkan, kijun, senkou_b, displacement)`.
    pub const fn periods(&self) -> (usize, usize, usize, usize) {
        (
            self.tenkan_period,
            self.kijun_period,
            self.senkou_b_period,
            self.displacement,
        )
    }

    /// Most recent output if at least one bar has been consumed.
    pub const fn value(&self) -> Option<IchimokuOutput> {
        self.last
    }

    /// Midpoint of the last `n` highs/lows. Assumes `self.highs.len() >= n`
    /// (the caller checks).
    fn midpoint(&self, n: usize) -> f64 {
        let len = self.highs.len();
        let start = len - n;
        let mut hi = f64::NEG_INFINITY;
        let mut lo = f64::INFINITY;
        for i in start..len {
            hi = hi.max(self.highs[i]);
            lo = lo.min(self.lows[i]);
        }
        f64::midpoint(hi, lo)
    }
}

impl Indicator for Ichimoku {
    type Input = Candle;
    type Output = IchimokuOutput;

    fn update(&mut self, candle: Candle) -> Option<IchimokuOutput> {
        // Ring-buffer the new bar; cap at the longest lookback.
        if self.highs.len() == self.senkou_b_period {
            self.highs.pop_front();
            self.lows.pop_front();
        }
        self.highs.push_back(candle.high);
        self.lows.push_back(candle.low);

        let tenkan =
            (self.highs.len() >= self.tenkan_period).then(|| self.midpoint(self.tenkan_period));
        let kijun =
            (self.highs.len() >= self.kijun_period).then(|| self.midpoint(self.kijun_period));
        let senkou_b_now =
            (self.highs.len() >= self.senkou_b_period).then(|| self.midpoint(self.senkou_b_period));

        // Today's contribution to the leading spans (will become visible after
        // `displacement - 1` more bars).
        let senkou_a_now = match (tenkan, kijun) {
            (Some(t), Some(k)) => Some(f64::midpoint(t, k)),
            _ => None,
        };

        // The currently-visible Senkou A/B at this bar are the values that were
        // computed `displacement - 1` bars ago. We always push the freshly-computed
        // `senkou_a_now` / `senkou_b_now` to keep the history aligned 1:1 with
        // bars; NaN encodes "no value yet" so the buffer indices stay simple.
        let push_or_nan = |q: &mut VecDeque<f64>, v: Option<f64>, cap: usize| {
            if q.len() == cap {
                q.pop_front();
            }
            q.push_back(v.unwrap_or(f64::NAN));
        };
        push_or_nan(&mut self.senkou_a_history, senkou_a_now, self.displacement);
        push_or_nan(&mut self.senkou_b_history, senkou_b_now, self.displacement);

        // The buffer holds `displacement` values including the one just
        // pushed, so once it is full `self.senkou_*_history.front()` is the
        // value buffered `displacement - 1` updates ago: the inclusive count.
        let take_front = |q: &VecDeque<f64>, cap: usize| -> Option<f64> {
            if q.len() == cap {
                let v = q[0];
                if v.is_nan() {
                    None
                } else {
                    Some(v)
                }
            } else {
                None
            }
        };
        let senkou_a = take_front(&self.senkou_a_history, self.displacement);
        let senkou_b = take_front(&self.senkou_b_history, self.displacement);

        // Chikou: close from `displacement - 1` bars ago (same inclusive count).
        if self.close_history.len() == self.displacement {
            self.close_history.pop_front();
        }
        self.close_history.push_back(candle.close);
        let chikou = (self.close_history.len() == self.displacement).then(|| self.close_history[0]);

        let out = IchimokuOutput {
            tenkan,
            kijun,
            senkou_a,
            senkou_b,
            chikou,
        };
        self.last = Some(out);
        self.has_emitted = true;
        Some(out)
    }

    fn reset(&mut self) {
        self.has_emitted = false;
        self.highs.clear();
        self.lows.clear();
        self.senkou_a_history.clear();
        self.senkou_b_history.clear();
        self.close_history.clear();
        self.last = None;
    }

    #[inline]
    fn warmup_period(&self) -> usize {
        // A row is emitted from the first bar: every component is an
        // `Option`, and they fill in as the history allows. The last of them,
        // senkou_b displaced forward, needs
        // `senkou_b_period + displacement - 1` bars -- but that is when the row
        // becomes *complete*, not when a value first appears, and this method
        // promises the latter.
        1
    }

    #[inline]
    fn is_ready(&self) -> bool {
        self.has_emitted
    }

    #[inline]
    fn name(&self) -> &'static str {
        "Ichimoku"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;
    use approx::assert_relative_eq;

    fn c(h: f64, l: f64, cl: f64, i: i64) -> Candle {
        Candle::new(cl, h, l, cl, 0.0, i).unwrap()
    }

    fn ramp(n: i64) -> Vec<Candle> {
        (0..n)
            .map(|i| {
                let p = 100.0 + f64::from(i32::try_from(i).unwrap());
                c(p + 2.0, p - 2.0, p + 1.0, i)
            })
            .collect()
    }

    #[test]
    fn rejects_zero_periods() {
        assert!(matches!(
            Ichimoku::new(0, 26, 52, 26),
            Err(Error::PeriodZero)
        ));
        assert!(matches!(
            Ichimoku::new(9, 0, 52, 26),
            Err(Error::PeriodZero)
        ));
        assert!(matches!(
            Ichimoku::new(9, 26, 0, 26),
            Err(Error::PeriodZero)
        ));
        assert!(matches!(
            Ichimoku::new(9, 26, 52, 0),
            Err(Error::PeriodZero)
        ));
    }

    #[test]
    fn rejects_non_increasing_periods() {
        assert!(matches!(
            Ichimoku::new(26, 26, 52, 26),
            Err(Error::InvalidPeriod { .. })
        ));
        assert!(matches!(
            Ichimoku::new(9, 52, 52, 26),
            Err(Error::InvalidPeriod { .. })
        ));
        assert!(matches!(
            Ichimoku::new(52, 26, 9, 26),
            Err(Error::InvalidPeriod { .. })
        ));
    }

    #[test]
    fn accessors_and_metadata() {
        let ichi = Ichimoku::classic();
        assert_eq!(ichi.periods(), (9, 26, 52, 26));
        assert_eq!(ichi.warmup_period(), 1);
        assert_eq!(ichi.name(), "Ichimoku");
        assert!(ichi.value().is_none());
    }

    #[test]
    fn tenkan_emits_at_period() {
        let mut ichi = Ichimoku::classic();
        let candles = ramp(10);
        let out = ichi.batch(&candles);
        // The 9th update is the first time tenkan has 9 highs/lows.
        for (i, o) in out.iter().enumerate() {
            let v = o.unwrap();
            if i < 8 {
                assert!(v.tenkan.is_none(), "tenkan must be None until 9 bars");
            } else {
                assert!(v.tenkan.is_some(), "tenkan must be Some from bar 9 on");
            }
        }
    }

    #[test]
    fn fully_populated_after_warmup() {
        let mut ichi = Ichimoku::classic();
        let candles = ramp(120);
        let out = ichi.batch(&candles);
        let last = out.last().unwrap().unwrap();
        assert!(last.tenkan.is_some());
        assert!(last.kijun.is_some());
        assert!(last.senkou_a.is_some());
        assert!(last.senkou_b.is_some());
        assert!(last.chikou.is_some());
        assert!(ichi.is_ready());
    }

    #[test]
    fn ramp_tenkan_equals_window_midpoint() {
        // On a strict ramp the midpoint of the last 9 (high, low) candles is
        // the midpoint of the first and last bar in that window.
        let mut ichi = Ichimoku::classic();
        let candles = ramp(20);
        let out = ichi.batch(&candles);
        // At index 8 (9th bar), the window is bars 0..=8 with highs 102..110
        // and lows 98..106. Midpoint = (110 + 98) / 2 = 104.
        let v = out[8].unwrap();
        assert_relative_eq!(v.tenkan.unwrap(), 104.0, epsilon = 1e-12);
    }

    #[test]
    fn chikou_is_close_displacement_minus_one_bars_back() {
        let mut ichi = Ichimoku::classic();
        let candles = ramp(60);
        let out = ichi.batch(&candles);
        // Displacement = 26, counted inclusively: at bar index 25, chikou is
        // the close from bar 0, 25 bars back.
        assert!(out[24].unwrap().chikou.is_none());
        let v = out[25].unwrap();
        assert_relative_eq!(v.chikou.unwrap(), candles[0].close, epsilon = 1e-12);
        let v = out[50].unwrap();
        assert_relative_eq!(v.chikou.unwrap(), candles[25].close, epsilon = 1e-12);
    }

    #[test]
    fn senkou_spans_shift_forward_displacement_minus_one_bars() {
        let mut ichi = Ichimoku::classic();
        let candles = ramp(120);
        let out = ichi.batch(&candles);
        // Senkou A first exists at index 25 (kijun's first bar), so it becomes
        // visible 25 bars later, at index 50, and not one bar before.
        assert!(out[49].unwrap().senkou_a.is_none());
        let past = out[25].unwrap();
        let expected_a = f64::midpoint(past.tenkan.unwrap(), past.kijun.unwrap());
        assert_relative_eq!(
            out[50].unwrap().senkou_a.unwrap(),
            expected_a,
            epsilon = 1e-12
        );
        // Senkou B: the 52-bar midpoint computed at index 51 is visible at 76,
        // the 77th bar.
        assert!(out[75].unwrap().senkou_b.is_none());
        let hi = candles[..52]
            .iter()
            .map(|c| c.high)
            .fold(f64::NEG_INFINITY, f64::max);
        let lo = candles[..52]
            .iter()
            .map(|c| c.low)
            .fold(f64::INFINITY, f64::min);
        assert_relative_eq!(
            out[76].unwrap().senkou_b.unwrap(),
            f64::midpoint(hi, lo),
            epsilon = 1e-12
        );
    }

    #[test]
    fn batch_equals_streaming() {
        let candles = ramp(120);
        let mut a = Ichimoku::classic();
        let mut b = Ichimoku::classic();
        let batched = a.batch(&candles);
        let streamed: Vec<_> = candles.iter().map(|c| b.update(*c)).collect();
        assert_eq!(batched.len(), streamed.len());
        for (lhs, rhs) in batched.iter().zip(streamed.iter()) {
            let (l, r) = (lhs.unwrap(), rhs.unwrap());
            assert_eq!(l.tenkan, r.tenkan);
            assert_eq!(l.kijun, r.kijun);
            assert_eq!(l.senkou_a, r.senkou_a);
            assert_eq!(l.senkou_b, r.senkou_b);
            assert_eq!(l.chikou, r.chikou);
        }
    }

    #[test]
    fn reset_clears_state() {
        let mut ichi = Ichimoku::classic();
        ichi.batch(&ramp(100));
        assert!(ichi.is_ready());
        ichi.reset();
        assert!(!ichi.is_ready());
        assert!(ichi.value().is_none());
    }

    #[test]
    fn custom_periods_accepted() {
        let mut ichi = Ichimoku::new(5, 10, 20, 10).unwrap();
        let out = ichi.batch(&ramp(40));
        let last = out.last().unwrap().unwrap();
        assert!(last.tenkan.is_some());
        assert!(last.senkou_a.is_some());
    }
}
