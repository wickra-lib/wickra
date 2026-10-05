//! Rising Three Methods candlestick pattern.

use crate::ohlcv::Candle;
use crate::traits::Indicator;

/// Rising Three Methods — a 5-bar bullish continuation. A long white candle is
/// followed by three small black bars that drift back but stay inside its range
/// (a brief rest), then a second long white candle opens above the last rest
/// bar's close and closes above the first, resuming the advance (Nison; TA-Lib
/// `CDLRISEFALL3METHODS`).
///
/// ```text
/// long body = |close − open| >= 0.5 * (high − low)
/// small body = |close − open| <= 0.5 * body1
/// bar1 white & long
/// bar2, bar3, bar4 small black bodies, each overlapping bar1's high/low range
///                  (min(open, close) < high1 and max(open, close) > low1),
///                  with falling closes (close3 < close2, close4 < close3)
/// bar5 white & long, opening above bar4's close (open5 > close4)
///                  and closing above bar1's close (close5 > close1)
/// ```
///
/// Output is `+1.0` when the pattern completes and `0.0` otherwise. Rising Three
/// Methods is a single-direction (bullish-only) continuation, so it never emits
/// `−1.0`. The first four bars always return `0.0` because the five-bar window is
/// not yet filled. Body thresholds follow the geometric house style rather than
/// TA-Lib's rolling averages. Pattern-shape check only — no trend filter is
/// applied; combine with a trend indicator for actionable signals.
///
/// # Signed ±1 encoding
///
/// This detector emits the uniform candlestick sign convention shared across the
/// pattern family — `+1.0` bullish, `0.0` no pattern — so it drops straight into
/// a machine-learning feature matrix as a single dimension.
///
/// # Example
///
/// ```
/// use wickra_core::{Candle, Indicator, RisingThreeMethods};
///
/// let mut indicator = RisingThreeMethods::new();
/// indicator.update(Candle::new(10.0, 15.1, 9.9, 15.0, 1.0, 0).unwrap());
/// indicator.update(Candle::new(14.0, 14.1, 12.9, 13.0, 1.0, 1).unwrap());
/// indicator.update(Candle::new(13.5, 13.6, 12.4, 12.5, 1.0, 2).unwrap());
/// indicator.update(Candle::new(13.0, 13.1, 11.9, 12.0, 1.0, 3).unwrap());
/// let out = indicator
///     .update(Candle::new(12.5, 16.1, 12.4, 16.0, 1.0, 4).unwrap());
/// assert_eq!(out, Some(1.0));
/// ```
#[derive(Debug, Clone, Default)]
pub struct RisingThreeMethods {
    c1: Option<Candle>,
    c2: Option<Candle>,
    c3: Option<Candle>,
    c4: Option<Candle>,
    has_emitted: bool,
}

impl RisingThreeMethods {
    /// Construct a new Rising Three Methods detector.
    pub const fn new() -> Self {
        Self {
            c1: None,
            c2: None,
            c3: None,
            c4: None,
            has_emitted: false,
        }
    }
}

impl Indicator for RisingThreeMethods {
    type Input = Candle;
    type Output = f64;

    #[inline]
    fn update(&mut self, candle: Candle) -> Option<f64> {
        let bar1 = self.c1;
        let bar2 = self.c2;
        let bar3 = self.c3;
        let bar4 = self.c4;
        self.c1 = self.c2;
        self.c2 = self.c3;
        self.c3 = self.c4;
        self.c4 = Some(candle);
        let (Some(bar1), Some(bar2), Some(bar3), Some(bar4)) = (bar1, bar2, bar3, bar4) else {
            return None;
        };
        self.has_emitted = true;
        let range1 = bar1.high - bar1.low;
        if range1 <= 0.0 {
            return Some(0.0);
        }
        let body1 = bar1.close - bar1.open;
        if body1 < 0.5 * range1 {
            return Some(0.0); // bar1 must be a long white body
        }
        // The three middle bars are small black bodies whose real body
        // reaches into bar1's range (TA-Lib: part of each body within bar1).
        for mid in [bar2, bar3, bar4] {
            let body = mid.open - mid.close;
            if body <= 0.0
                || body > 0.5 * body1
                || mid.open.min(mid.close) >= bar1.high
                || mid.open.max(mid.close) <= bar1.low
            {
                return Some(0.0);
            }
        }
        // ... drifting down against the advance.
        if bar3.close >= bar2.close || bar4.close >= bar3.close {
            return Some(0.0);
        }
        // bar5 is a long white candle that opens above bar4's close and closes
        // above bar1's close.
        let body5 = candle.close - candle.open;
        if body5 > 0.0
            && body5 >= 0.5 * (candle.high - candle.low)
            && candle.open > bar4.close
            && candle.close > bar1.close
        {
            return Some(1.0);
        }
        Some(0.0)
    }

    fn reset(&mut self) {
        self.c1 = None;
        self.c2 = None;
        self.c3 = None;
        self.c4 = None;
        self.has_emitted = false;
    }

    #[inline]
    fn warmup_period(&self) -> usize {
        5
    }

    #[inline]
    fn is_ready(&self) -> bool {
        self.has_emitted
    }

    #[inline]
    fn name(&self) -> &'static str {
        "RisingThreeMethods"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;

    fn c(open: f64, high: f64, low: f64, close: f64, ts: i64) -> Candle {
        Candle::new(open, high, low, close, 1.0, ts).unwrap()
    }

    #[test]
    fn accessors_and_metadata() {
        let t = RisingThreeMethods::new();
        assert_eq!(t.name(), "RisingThreeMethods");
        assert_eq!(t.warmup_period(), 5);
        assert!(!t.is_ready());
    }

    #[test]
    fn rising_three_methods_is_plus_one() {
        let mut t = RisingThreeMethods::new();
        assert_eq!(t.update(c(10.0, 15.1, 9.9, 15.0, 0)), None);
        assert_eq!(t.update(c(14.0, 14.1, 12.9, 13.0, 1)), None);
        assert_eq!(t.update(c(13.5, 13.6, 12.4, 12.5, 2)), None);
        assert_eq!(t.update(c(13.0, 13.1, 11.9, 12.0, 3)), None);
        assert_eq!(t.update(c(12.5, 16.1, 12.4, 16.0, 4)), Some(1.0));
    }

    #[test]
    fn middle_bar_breaks_range_yields_zero() {
        let mut t = RisingThreeMethods::new();
        t.update(c(10.0, 15.1, 9.9, 15.0, 0));
        t.update(c(14.0, 14.1, 12.9, 13.0, 1));
        // bar3's whole body sits above bar1's high.
        t.update(c(15.5, 15.6, 15.1, 15.2, 2));
        t.update(c(13.0, 13.1, 11.9, 12.0, 3));
        assert_eq!(t.update(c(12.5, 16.1, 12.4, 16.0, 4)), Some(0.0));
    }

    #[test]
    fn bar5_not_new_high_yields_zero() {
        let mut t = RisingThreeMethods::new();
        t.update(c(10.0, 15.1, 9.9, 15.0, 0));
        t.update(c(14.0, 14.1, 12.9, 13.0, 1));
        t.update(c(13.5, 13.6, 12.4, 12.5, 2));
        t.update(c(13.0, 13.1, 11.9, 12.0, 3));
        // bar5 white but closes below bar1's close.
        assert_eq!(t.update(c(12.5, 14.6, 12.4, 14.5, 4)), Some(0.0));
    }

    #[test]
    fn first_four_bars_return_zero() {
        let mut t = RisingThreeMethods::new();
        assert_eq!(t.update(c(10.0, 15.1, 9.9, 15.0, 0)), None);
        assert_eq!(t.update(c(14.0, 14.1, 12.9, 13.0, 1)), None);
        assert_eq!(t.update(c(13.5, 13.6, 12.4, 12.5, 2)), None);
        assert_eq!(t.update(c(13.0, 13.1, 11.9, 12.0, 3)), None);
    }

    #[test]
    fn batch_equals_streaming() {
        let candles: Vec<Candle> = (0..40)
            .map(|i| {
                let base = 100.0 + i as f64;
                c(base, base + 5.2, base - 0.1, base + 5.0, i)
            })
            .collect();
        let mut a = RisingThreeMethods::new();
        let mut b = RisingThreeMethods::new();
        assert_eq!(
            a.batch(&candles),
            candles.iter().map(|x| b.update(*x)).collect::<Vec<_>>()
        );
    }

    #[test]
    fn reset_clears_state() {
        let mut t = RisingThreeMethods::new();
        t.update(c(10.0, 15.1, 9.9, 15.0, 0));
        t.update(c(14.0, 14.1, 12.9, 13.0, 1));
        t.update(c(13.5, 13.6, 12.4, 12.5, 2));
        t.update(c(13.0, 13.1, 11.9, 12.0, 3));
        t.update(c(12.5, 16.1, 12.4, 16.0, 4));
        assert!(t.is_ready());
        t.reset();
        assert!(!t.is_ready());
        assert_eq!(t.update(c(10.0, 15.1, 9.9, 15.0, 0)), None);
    }

    #[test]
    fn zero_range_first_bar_yields_zero() {
        let mut t = RisingThreeMethods::new();
        // Flat first bar (range1 == 0) -> rejected.
        t.update(c(10.0, 10.0, 10.0, 10.0, 0));
        t.update(c(14.0, 14.1, 12.9, 13.0, 1));
        t.update(c(13.5, 13.6, 12.4, 12.5, 2));
        t.update(c(13.0, 13.1, 11.9, 12.0, 3));
        assert_eq!(t.update(c(12.5, 16.1, 12.4, 16.0, 4)), Some(0.0));
    }

    #[test]
    fn short_first_body_yields_zero() {
        let mut t = RisingThreeMethods::new();
        // bar1 has a wide range but a tiny body -> not a long white body.
        t.update(c(10.0, 16.0, 9.0, 10.2, 0));
        t.update(c(14.0, 14.1, 12.9, 13.0, 1));
        t.update(c(13.5, 13.6, 12.4, 12.5, 2));
        t.update(c(13.0, 13.1, 11.9, 12.0, 3));
        assert_eq!(t.update(c(12.5, 16.1, 12.4, 16.0, 4)), Some(0.0));
    }

    /// The canonical pattern: bar1 10 -> 15 (range 5.2, body1 = 5, so middle
    /// bodies may be at most 2.5), black rests closing 13, 12.5, 12, bar5 12.5 -> 16.
    fn base() -> [Candle; 5] {
        [
            c(10.0, 15.1, 9.9, 15.0, 0),
            c(14.0, 14.1, 12.9, 13.0, 1),
            c(13.5, 13.6, 12.4, 12.5, 2),
            c(13.0, 13.1, 11.9, 12.0, 3),
            c(12.5, 16.1, 12.4, 16.0, 4),
        ]
    }

    fn run(bars: [Candle; 5]) -> Option<f64> {
        let mut t = RisingThreeMethods::new();
        bars.iter().map(|b| t.update(*b)).last().unwrap()
    }

    fn with(index: usize, candle: Candle) -> [Candle; 5] {
        let mut bars = base();
        bars[index] = candle;
        bars
    }

    #[test]
    fn hand_computed_pattern() {
        // body1 = 15 - 10 = 5 >= 0.5 * 5.2 = 2.6 (long white).
        // Middle bodies 1.0, 1.0, 1.0 <= 2.5, black, all inside 9.9..15.1.
        // Closes 13 > 12.5 > 12 (drifting down).
        // body5 = 3.5 >= 0.5 * 3.7 = 1.85, open5 12.5 > close4 12, close5 16 > 15.
        assert_eq!(run(base()), Some(1.0));
    }

    #[test]
    fn middle_shadow_outside_range_but_body_overlapping_fires() {
        // bar2's upper shadow pokes to 15.5 > high1 = 15.1; its body 14..13 is inside.
        assert_eq!(run(with(1, c(14.0, 15.5, 12.9, 13.0, 1))), Some(1.0));
        // bar2's body 15.4..14.9 straddles high1: min 14.9 < 15.1 -> still overlaps.
        assert_eq!(run(with(1, c(15.4, 15.5, 14.8, 14.9, 1))), Some(1.0));
        // bar4's lower shadow pokes below low1 = 9.9; body 10.4..10.0 overlaps.
        assert_eq!(run(with(3, c(10.4, 10.5, 9.0, 10.0, 3))), Some(1.0));
    }

    #[test]
    fn middle_body_fully_outside_range_yields_zero() {
        // Body bottom exactly at high1 (min == 15.1) -> outside.
        assert_eq!(run(with(1, c(15.6, 15.7, 15.0, 15.1, 1))), Some(0.0));
        // Body entirely below low1 = 9.9 (max 9.8 <= 9.9).
        assert_eq!(run(with(3, c(9.8, 9.9, 9.0, 9.2, 3))), Some(0.0));
        // Body top exactly at low1 (max == 9.9) -> outside.
        assert_eq!(run(with(3, c(9.9, 10.0, 9.0, 9.5, 3))), Some(0.0));
    }

    #[test]
    fn middle_body_colour_and_size_rules() {
        // White middle bar.
        assert_eq!(run(with(2, c(12.5, 13.6, 12.4, 13.5, 2))), Some(0.0));
        // Doji middle bar (body 0).
        assert_eq!(run(with(2, c(12.5, 13.6, 12.4, 12.5, 2))), Some(0.0));
        // Body 2.9 > 0.5 * body1 = 2.5.
        assert_eq!(run(with(1, c(14.9, 15.0, 11.9, 12.0, 1))), Some(0.0));
    }

    #[test]
    fn closes_not_drifting_down_yields_zero() {
        // close3 = 13.1 >= close2 = 13.0.
        assert_eq!(run(with(2, c(13.6, 13.7, 13.0, 13.1, 2))), Some(0.0));
        // close3 == close2.
        assert_eq!(run(with(2, c(13.6, 13.7, 12.9, 13.0, 2))), Some(0.0));
        // close4 = 12.6 >= close3 = 12.5.
        assert_eq!(run(with(3, c(13.0, 13.1, 12.5, 12.6, 3))), Some(0.0));
    }

    #[test]
    fn fifth_bar_conditions() {
        // Black bar5.
        assert_eq!(run(with(4, c(16.0, 16.1, 12.4, 15.5, 4))), Some(0.0));
        // Doji bar5 (body5 == 0).
        assert_eq!(run(with(4, c(16.0, 16.1, 12.4, 16.0, 4))), Some(0.0));
        // Not long: body 3.5 < 0.5 * range 8.0 = 4.0.
        assert_eq!(run(with(4, c(12.5, 20.0, 12.0, 16.0, 4))), Some(0.0));
        // open5 = 12.0 == close4 -> not above.
        assert_eq!(run(with(4, c(12.0, 16.1, 11.9, 16.0, 4))), Some(0.0));
        // close5 == close1 = 15.0 -> not above.
        assert_eq!(run(with(4, c(12.5, 15.1, 12.4, 15.0, 4))), Some(0.0));
    }

    #[test]
    fn first_value_lands_exactly_at_warmup_index() {
        let mut t = RisingThreeMethods::new();
        let out = t.batch(&base());
        let warm = t.warmup_period();
        assert!(out[..warm - 1].iter().all(Option::is_none));
        assert_eq!(out[warm - 1], Some(1.0));
    }

    fn mixed_series() -> Vec<Candle> {
        base().iter().cycle().take(30).copied().collect()
    }

    #[test]
    fn reset_replays_identically() {
        let candles = mixed_series();
        let fresh = RisingThreeMethods::new().batch(&candles);
        let mut t = RisingThreeMethods::new();
        let _ = t.batch(&candles);
        t.reset();
        assert_eq!(t.batch(&candles), fresh);
    }

    #[test]
    fn batch_nan_into_matches_streaming_bits() {
        let candles = mixed_series();
        let mut t = RisingThreeMethods::new();
        let streamed: Vec<f64> = candles
            .iter()
            .map(|x| t.update(*x).unwrap_or(f64::NAN))
            .collect();
        let mut out = vec![0.0; candles.len()];
        RisingThreeMethods::new().batch_nan_into(&candles, &mut out);
        assert!(streamed
            .iter()
            .zip(&out)
            .all(|(a, b)| a.to_bits() == b.to_bits()));
        assert!(streamed.contains(&1.0));
    }
}
