#![allow(clippy::doc_markdown)]

//! Tom DeMark TD Differential — a three-close buying/selling-pressure reversal.
//!
//! TD Differential flags an exhaustion bar: price has closed lower (higher) two
//! bars running, yet buying (selling) pressure is already shifting. Pressure is
//! measured against DeMark's *true* range, which folds in the previous close:
//! `buying = close − TrueLow`, `selling = TrueHigh − close`, with
//! `TrueLow = min(low, close[−1])` and `TrueHigh = max(high, close[−1])` (Jason
//! Perl, *DeMark Indicators*, 2008).
//!
//! - **Buy signal** (`+1.0`) on bar `i` when:
//!   1. `close[i] < close[i − 1]` and `close[i − 1] < close[i − 2]` (two lower closes)
//!   2. `buying[i]  > buying[i − 1]`                                  (buying pressure rises)
//!   3. `selling[i] < selling[i − 1]`                                 (selling pressure falls)
//! - **Sell signal** (`-1.0`) on bar `i` when:
//!   1. `close[i] > close[i − 1]` and `close[i − 1] > close[i − 2]` (two higher closes)
//!   2. `selling[i] > selling[i − 1]`
//!   3. `buying[i]  < buying[i − 1]`
//! - Otherwise the output is `0.0`.
//!
//! The pressure of bar `i − 1` needs the close of bar `i − 2`, so the first
//! value lands on the third input candle.

use crate::ohlcv::Candle;
use crate::traits::Indicator;

/// TD Differential — three-close reversal pattern detector.
/// # Example
///
/// ```
/// use wickra_core::{TdDifferential, Candle, Indicator};
///
/// let mut indicator = TdDifferential::new();
/// // `None` during warmup, then `Some(_)` once enough bars are seen.
/// let mut out = None;
/// for i in 0..40i64 {
///     let p = 100.0 + (i as f64 * 0.4).sin() * 5.0;
///     let candle = Candle::new(p, p + 1.5, p - 1.5, p + 0.3, 1_000.0, i).unwrap();
///     out = indicator.update(candle);
/// }
/// let _ = out;
/// ```
#[derive(Debug, Clone, Default)]
pub struct TdDifferential {
    prev2: Option<Candle>,
    prev: Option<Candle>,
    last_value: Option<f64>,
}

impl TdDifferential {
    /// Construct a new `TdDifferential`.
    pub fn new() -> Self {
        Self::default()
    }

    /// Latest emitted signal if available.
    pub const fn value(&self) -> Option<f64> {
        self.last_value
    }
}

/// `(buying, selling)` pressure of `bar` against the previous close.
fn pressures(bar: &Candle, prev_close: f64) -> (f64, f64) {
    let true_low = bar.low.min(prev_close);
    let true_high = bar.high.max(prev_close);
    (bar.close - true_low, true_high - bar.close)
}

impl Indicator for TdDifferential {
    type Input = Candle;
    type Output = f64;

    #[inline]
    fn update(&mut self, candle: Candle) -> Option<f64> {
        let (Some(prev2), Some(prev)) = (self.prev2, self.prev) else {
            self.prev2 = self.prev;
            self.prev = Some(candle);
            return None;
        };
        let (buying_now, selling_now) = pressures(&candle, prev.close);
        let (buying_prev, selling_prev) = pressures(&prev, prev2.close);

        let v = if candle.close < prev.close
            && prev.close < prev2.close
            && buying_now > buying_prev
            && selling_now < selling_prev
        {
            1.0
        } else if candle.close > prev.close
            && prev.close > prev2.close
            && selling_now > selling_prev
            && buying_now < buying_prev
        {
            -1.0
        } else {
            0.0
        };

        self.prev2 = Some(prev);
        self.prev = Some(candle);
        self.last_value = Some(v);
        Some(v)
    }

    fn reset(&mut self) {
        self.prev2 = None;
        self.prev = None;
        self.last_value = None;
    }

    #[inline]
    fn warmup_period(&self) -> usize {
        3
    }

    #[inline]
    fn is_ready(&self) -> bool {
        self.last_value.is_some()
    }

    #[inline]
    fn name(&self) -> &'static str {
        "TDDifferential"
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
    fn buy_signal_after_two_lower_closes_with_shifting_pressure() {
        // Closes 10 -> 9 -> 8.5 (two lower closes).
        // Bar 1: TrueLow = min(8, 10) = 8 -> buying 1; TrueHigh = max(10, 10) = 10 -> selling 1.
        // Bar 2: TrueLow = min(7, 9) = 7 -> buying 1.5 > 1; TrueHigh = max(9, 9) = 9 -> selling 0.5 < 1.
        let mut td = TdDifferential::new();
        assert_eq!(td.update(c(11.0, 9.0, 10.0, 0)), None);
        assert_eq!(td.update(c(10.0, 8.0, 9.0, 1)), None);
        assert_eq!(td.update(c(9.0, 7.0, 8.5, 2)), Some(1.0));
    }

    #[test]
    fn single_lower_close_is_not_enough() {
        // Same last bar, but the bar before closed higher than its predecessor.
        let mut td = TdDifferential::new();
        td.update(c(9.0, 7.0, 8.0, 0));
        td.update(c(10.0, 8.0, 9.0, 1));
        assert_eq!(td.update(c(9.0, 7.0, 8.5, 2)), Some(0.0));
    }

    #[test]
    fn sell_signal_after_two_higher_closes_with_shifting_pressure() {
        // Closes 8 -> 9 -> 9.8 (two higher closes).
        // Bar 1: TrueLow = min(8, 8) = 8 -> buying 1; TrueHigh = max(10, 8) = 10 -> selling 1.
        // Bar 2: TrueLow = min(9.5, 9) = 9 -> buying 0.8 < 1; TrueHigh = 11.5 -> selling 1.7 > 1.
        let mut td = TdDifferential::new();
        assert_eq!(td.update(c(9.0, 7.0, 8.0, 0)), None);
        assert_eq!(td.update(c(10.0, 8.0, 9.0, 1)), None);
        assert_relative_eq!(td.update(c(11.5, 9.5, 9.8, 2)).unwrap(), -1.0);
    }

    #[test]
    fn no_signal_on_neutral_bars() {
        // Identical bars -> equality everywhere -> zero.
        let mut td = TdDifferential::new();
        assert_eq!(td.update(c(10.0, 8.0, 9.0, 0)), None);
        assert_eq!(td.update(c(10.0, 8.0, 9.0, 1)), None);
        assert_eq!(td.update(c(10.0, 8.0, 9.0, 2)), Some(0.0));
    }

    #[test]
    fn batch_equals_streaming() {
        let candles: Vec<Candle> = (0..40)
            .map(|i| {
                let m = 100.0 + (f64::from(i) * 0.3).sin() * 5.0;
                c(m + 1.0, m - 1.0, m, i64::from(i))
            })
            .collect();
        let mut a = TdDifferential::new();
        let mut b = TdDifferential::new();
        assert_eq!(
            a.batch(&candles),
            candles.iter().map(|x| b.update(*x)).collect::<Vec<_>>()
        );
    }

    #[test]
    fn output_only_in_canonical_set() {
        // Every emitted value is in {-1, 0, +1}.
        let candles: Vec<Candle> = (0..120)
            .map(|i| {
                let m = 100.0 + (f64::from(i) * 0.5).sin() * 5.0;
                c(m + 1.0, m - 1.0, m, i64::from(i))
            })
            .collect();
        let mut td = TdDifferential::new();
        for v in td.batch(&candles).into_iter().flatten() {
            assert!(v == -1.0 || v == 0.0 || v == 1.0, "unexpected value {v}");
        }
    }

    #[test]
    fn reset_clears_state() {
        let mut td = TdDifferential::new();
        td.update(c(10.0, 8.0, 9.0, 0));
        td.update(c(11.0, 9.0, 10.0, 1));
        td.update(c(12.0, 10.0, 11.0, 2));
        assert!(td.is_ready());
        td.reset();
        assert!(!td.is_ready());
        assert_eq!(td.update(c(10.0, 8.0, 9.0, 3)), None);
        assert_eq!(td.value(), None);
    }

    #[test]
    fn accessors_and_metadata() {
        let td = TdDifferential::new();
        assert_eq!(td.warmup_period(), 3);
        assert_eq!(td.name(), "TDDifferential");
        assert_eq!(td.value(), None);
    }

    #[test]
    fn buy_signal_needs_true_high_on_gap_down() {
        // Bar 1 gaps below the prior close 10: h 9.2, l 8.8, c 9.
        //   TrueLow = min(8.8, 10) = 8.8 -> buying 0.2
        //   TrueHigh = max(9.2, 10) = 10 -> selling 1.0 (plain range: 0.2)
        // Bar 2: h 9, l 8, c 8.5, prior close 9.
        //   buying = 8.5 - 8 = 0.5 > 0.2; selling = 9 - 8.5 = 0.5 < 1.0 -> +1.
        // With the plain high the selling test (0.5 < 0.2) would fail.
        let mut td = TdDifferential::new();
        assert_eq!(td.update(c(11.0, 9.0, 10.0, 0)), None);
        assert_eq!(td.update(c(9.2, 8.8, 9.0, 1)), None);
        assert_eq!(td.update(c(9.0, 8.0, 8.5, 2)), Some(1.0));
        assert_eq!(td.value(), Some(1.0));
    }

    #[test]
    fn sell_signal_needs_true_low_on_gap_up() {
        // Bar 1 gaps above the prior close 10: h 11.2, l 10.8, c 11.
        //   TrueLow = min(10.8, 10) = 10 -> buying 1.0 (plain range: 0.2)
        //   TrueHigh = 11.2 -> selling 0.2
        // Bar 2: h 12, l 11, c 11.5, prior close 11.
        //   selling = 12 - 11.5 = 0.5 > 0.2; buying = 11.5 - 11 = 0.5 < 1.0 -> -1.
        let mut td = TdDifferential::new();
        assert_eq!(td.update(c(11.0, 9.0, 10.0, 0)), None);
        assert_eq!(td.update(c(11.2, 10.8, 11.0, 1)), None);
        assert_eq!(td.update(c(12.0, 11.0, 11.5, 2)), Some(-1.0));
    }

    #[test]
    fn two_lower_closes_without_pressure_shift_is_neutral() {
        // Closes 10 -> 9 -> 8 with identical +-1 ranges.
        // Bar 1: buying = 9 - 8 = 1, selling = max(10, 10) - 9 = 1.
        // Bar 2: buying = 8 - 7 = 1 (not > 1) -> no buy signal.
        let mut td = TdDifferential::new();
        td.update(c(11.0, 9.0, 10.0, 0));
        td.update(c(10.0, 8.0, 9.0, 1));
        assert_eq!(td.update(c(9.0, 7.0, 8.0, 2)), Some(0.0));
    }

    #[test]
    fn two_higher_closes_without_pressure_shift_is_neutral() {
        // Closes 8 -> 9 -> 10 with identical +-1 ranges: selling stays 1.
        let mut td = TdDifferential::new();
        td.update(c(9.0, 7.0, 8.0, 0));
        td.update(c(10.0, 8.0, 9.0, 1));
        assert_eq!(td.update(c(11.0, 9.0, 10.0, 2)), Some(0.0));
    }

    #[test]
    fn pressures_use_true_range() {
        // Prior close 12 above the bar: TrueHigh 12, TrueLow 9.
        let (buying, selling) = pressures(&c(11.0, 9.0, 10.0, 0), 12.0);
        assert_eq!((buying, selling), (1.0, 2.0));
        // Prior close 7 below the bar: TrueLow 7, TrueHigh 11.
        let (buying, selling) = pressures(&c(11.0, 9.0, 10.0, 0), 7.0);
        assert_eq!((buying, selling), (3.0, 1.0));
    }

    #[test]
    fn first_value_lands_at_warmup_minus_one() {
        let candles: Vec<Candle> = (0..6)
            .map(|i| c(11.0, 9.0, 10.0 + f64::from(i), i64::from(i)))
            .collect();
        let mut td = TdDifferential::new();
        let warm = td.warmup_period();
        let out = td.batch(&candles);
        assert!(out[..warm - 1].iter().all(Option::is_none));
        assert!(out[warm - 1..].iter().all(Option::is_some));
    }

    #[test]
    fn reset_reproduces_fresh_run() {
        let candles: Vec<Candle> = (0..60)
            .map(|i| {
                let m = 100.0 + (f64::from(i) * 0.7).sin() * 5.0;
                c(
                    m + 1.0 + (f64::from(i) * 0.3).cos(),
                    m - 1.0,
                    m,
                    i64::from(i),
                )
            })
            .collect();
        let mut fresh = TdDifferential::new();
        let expected = fresh.batch(&candles);
        let mut td = TdDifferential::new();
        td.batch(&candles[..17]);
        td.reset();
        assert_eq!(td.batch(&candles), expected);
    }

    #[test]
    fn batch_nan_into_matches_streaming() {
        let candles: Vec<Candle> = (0..60)
            .map(|i| {
                let m = 100.0 + (f64::from(i) * 0.7).sin() * 5.0;
                c(
                    m + 1.0 + (f64::from(i) * 0.3).cos(),
                    m - 1.0,
                    m,
                    i64::from(i),
                )
            })
            .collect();
        let mut a = TdDifferential::new();
        let mut out = vec![0.0; candles.len()];
        a.batch_nan_into(&candles, &mut out);
        let mut b = TdDifferential::new();
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
