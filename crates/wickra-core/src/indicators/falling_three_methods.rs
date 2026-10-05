//! Falling Three Methods candlestick pattern.

use crate::ohlcv::Candle;
use crate::traits::Indicator;

/// Falling Three Methods — a 5-bar bearish continuation. A long black candle is
/// followed by three small white bars that drift up but stay inside its range (a
/// brief rest), then a second long black candle opens below the last rest bar's
/// close and closes below the first, resuming the decline (Nison; TA-Lib
/// `CDLRISEFALL3METHODS`).
///
/// ```text
/// long body = |close − open| >= 0.5 * (high − low)
/// small body = |close − open| <= 0.5 * body1
/// bar1 black & long
/// bar2, bar3, bar4 small white bodies, each overlapping bar1's high/low range
///                  (min(open, close) < high1 and max(open, close) > low1),
///                  with rising closes (close3 > close2, close4 > close3)
/// bar5 black & long, opening below bar4's close (open5 < close4)
///                  and closing below bar1's close (close5 < close1)
/// ```
///
/// Output is `−1.0` when the pattern completes and `0.0` otherwise. Falling Three
/// Methods is a single-direction (bearish-only) continuation, so it never emits
/// `+1.0`. The first four bars always return `0.0` because the five-bar window is
/// not yet filled. Body thresholds follow the geometric house style rather than
/// TA-Lib's rolling averages. Pattern-shape check only — no trend filter is
/// applied; combine with a trend indicator for actionable signals.
///
/// # Signed ±1 encoding
///
/// This detector emits the uniform candlestick sign convention shared across the
/// pattern family — `−1.0` bearish, `0.0` no pattern — so it drops straight into
/// a machine-learning feature matrix as a single dimension.
///
/// # Example
///
/// ```
/// use wickra_core::{Candle, FallingThreeMethods, Indicator};
///
/// let mut indicator = FallingThreeMethods::new();
/// indicator.update(Candle::new(15.0, 15.1, 9.9, 10.0, 1.0, 0).unwrap());
/// indicator.update(Candle::new(11.0, 12.1, 10.9, 12.0, 1.0, 1).unwrap());
/// indicator.update(Candle::new(11.5, 12.6, 11.4, 12.5, 1.0, 2).unwrap());
/// indicator.update(Candle::new(12.0, 13.1, 11.9, 13.0, 1.0, 3).unwrap());
/// let out = indicator
///     .update(Candle::new(12.5, 12.6, 8.9, 9.0, 1.0, 4).unwrap());
/// assert_eq!(out, Some(-1.0));
/// ```
#[derive(Debug, Clone, Default)]
pub struct FallingThreeMethods {
    c1: Option<Candle>,
    c2: Option<Candle>,
    c3: Option<Candle>,
    c4: Option<Candle>,
    has_emitted: bool,
}

impl FallingThreeMethods {
    /// Construct a new Falling Three Methods detector.
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

impl Indicator for FallingThreeMethods {
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
        let body1 = bar1.open - bar1.close;
        if body1 < 0.5 * range1 {
            return Some(0.0); // bar1 must be a long black body
        }
        // The three middle bars are small white bodies whose real body
        // reaches into bar1's range (TA-Lib: part of each body within bar1).
        for mid in [bar2, bar3, bar4] {
            let body = mid.close - mid.open;
            if body <= 0.0
                || body > 0.5 * body1
                || mid.open.min(mid.close) >= bar1.high
                || mid.open.max(mid.close) <= bar1.low
            {
                return Some(0.0);
            }
        }
        // ... drifting up against the decline.
        if bar3.close <= bar2.close || bar4.close <= bar3.close {
            return Some(0.0);
        }
        // bar5 is a long black candle that opens below bar4's close and closes
        // below bar1's close.
        let body5 = candle.open - candle.close;
        if body5 > 0.0
            && body5 >= 0.5 * (candle.high - candle.low)
            && candle.open < bar4.close
            && candle.close < bar1.close
        {
            return Some(-1.0);
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
        "FallingThreeMethods"
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
        let t = FallingThreeMethods::new();
        assert_eq!(t.name(), "FallingThreeMethods");
        assert_eq!(t.warmup_period(), 5);
        assert!(!t.is_ready());
    }

    #[test]
    fn falling_three_methods_is_minus_one() {
        let mut t = FallingThreeMethods::new();
        assert_eq!(t.update(c(15.0, 15.1, 9.9, 10.0, 0)), None);
        assert_eq!(t.update(c(11.0, 12.1, 10.9, 12.0, 1)), None);
        assert_eq!(t.update(c(11.5, 12.6, 11.4, 12.5, 2)), None);
        assert_eq!(t.update(c(12.0, 13.1, 11.9, 13.0, 3)), None);
        assert_eq!(t.update(c(12.5, 12.6, 8.9, 9.0, 4)), Some(-1.0));
    }

    #[test]
    fn middle_bar_breaks_range_yields_zero() {
        let mut t = FallingThreeMethods::new();
        t.update(c(15.0, 15.1, 9.9, 10.0, 0));
        t.update(c(11.0, 12.1, 10.9, 12.0, 1));
        // bar3's whole body sits below bar1's low.
        t.update(c(9.5, 9.85, 9.4, 9.8, 2));
        t.update(c(12.0, 13.1, 11.9, 13.0, 3));
        assert_eq!(t.update(c(12.5, 12.6, 8.9, 9.0, 4)), Some(0.0));
    }

    #[test]
    fn bar5_not_new_low_yields_zero() {
        let mut t = FallingThreeMethods::new();
        t.update(c(15.0, 15.1, 9.9, 10.0, 0));
        t.update(c(11.0, 12.1, 10.9, 12.0, 1));
        t.update(c(11.5, 12.6, 11.4, 12.5, 2));
        t.update(c(12.0, 13.1, 11.9, 13.0, 3));
        // bar5 black but closes above bar1's close.
        assert_eq!(t.update(c(12.5, 12.6, 10.4, 10.5, 4)), Some(0.0));
    }

    #[test]
    fn first_four_bars_return_zero() {
        let mut t = FallingThreeMethods::new();
        assert_eq!(t.update(c(15.0, 15.1, 9.9, 10.0, 0)), None);
        assert_eq!(t.update(c(11.0, 12.1, 10.9, 12.0, 1)), None);
        assert_eq!(t.update(c(11.5, 12.6, 11.4, 12.5, 2)), None);
        assert_eq!(t.update(c(12.0, 13.1, 11.9, 13.0, 3)), None);
    }

    #[test]
    fn batch_equals_streaming() {
        let candles: Vec<Candle> = (0..40)
            .map(|i| {
                let base = 200.0 - i as f64;
                c(base + 5.0, base + 5.1, base - 0.1, base, i)
            })
            .collect();
        let mut a = FallingThreeMethods::new();
        let mut b = FallingThreeMethods::new();
        assert_eq!(
            a.batch(&candles),
            candles.iter().map(|x| b.update(*x)).collect::<Vec<_>>()
        );
    }

    #[test]
    fn reset_clears_state() {
        let mut t = FallingThreeMethods::new();
        t.update(c(15.0, 15.1, 9.9, 10.0, 0));
        t.update(c(11.0, 12.1, 10.9, 12.0, 1));
        t.update(c(11.5, 12.6, 11.4, 12.5, 2));
        t.update(c(12.0, 13.1, 11.9, 13.0, 3));
        t.update(c(12.5, 12.6, 8.9, 9.0, 4));
        assert!(t.is_ready());
        t.reset();
        assert!(!t.is_ready());
        assert_eq!(t.update(c(15.0, 15.1, 9.9, 10.0, 0)), None);
    }

    #[test]
    fn zero_range_first_bar_yields_zero() {
        let mut t = FallingThreeMethods::new();
        // Flat first bar (range1 == 0) -> rejected.
        t.update(c(10.0, 10.0, 10.0, 10.0, 0));
        t.update(c(11.0, 12.1, 10.9, 12.0, 1));
        t.update(c(11.5, 12.6, 11.4, 12.5, 2));
        t.update(c(12.0, 13.1, 11.9, 13.0, 3));
        assert_eq!(t.update(c(12.5, 12.6, 8.9, 9.0, 4)), Some(0.0));
    }

    #[test]
    fn short_first_body_yields_zero() {
        let mut t = FallingThreeMethods::new();
        // bar1 has a wide range but a tiny body -> not a long black body.
        t.update(c(10.0, 16.0, 9.0, 10.2, 0));
        t.update(c(11.0, 12.1, 10.9, 12.0, 1));
        t.update(c(11.5, 12.6, 11.4, 12.5, 2));
        t.update(c(12.0, 13.1, 11.9, 13.0, 3));
        assert_eq!(t.update(c(12.5, 12.6, 8.9, 9.0, 4)), Some(0.0));
    }

    /// The canonical pattern: bar1 15 -> 10 (range 5.2, body1 = 5, so middle
    /// bodies may be at most 2.5), white rests closing 12, 12.5, 13, bar5 12.5 -> 9.
    fn base() -> [Candle; 5] {
        [
            c(15.0, 15.1, 9.9, 10.0, 0),
            c(11.0, 12.1, 10.9, 12.0, 1),
            c(11.5, 12.6, 11.4, 12.5, 2),
            c(12.0, 13.1, 11.9, 13.0, 3),
            c(12.5, 12.6, 8.9, 9.0, 4),
        ]
    }

    fn run(bars: [Candle; 5]) -> Option<f64> {
        let mut t = FallingThreeMethods::new();
        bars.iter().map(|b| t.update(*b)).last().unwrap()
    }

    fn with(index: usize, candle: Candle) -> [Candle; 5] {
        let mut bars = base();
        bars[index] = candle;
        bars
    }

    #[test]
    fn hand_computed_pattern() {
        // body1 = 15 - 10 = 5 >= 0.5 * 5.2 = 2.6 (long black).
        // Middle bodies 1.0 each <= 2.5, white, all inside 9.9..15.1.
        // Closes 12 < 12.5 < 13 (drifting up).
        // body5 = 3.5 >= 0.5 * 3.7 = 1.85, open5 12.5 < close4 13, close5 9 < 10.
        assert_eq!(run(base()), Some(-1.0));
    }

    #[test]
    fn middle_shadow_outside_range_but_body_overlapping_fires() {
        // bar2's lower shadow pokes to 9.0 < low1 = 9.9; its body 11..12 is inside.
        assert_eq!(run(with(1, c(11.0, 12.1, 9.0, 12.0, 1))), Some(-1.0));
        // bar2's body 9.6..10.1 straddles low1: max 10.1 > 9.9 -> still overlaps.
        assert_eq!(run(with(1, c(9.6, 10.2, 9.5, 10.1, 1))), Some(-1.0));
        // bar4's upper shadow pokes above high1 = 15.1; body 14.6..15.0 overlaps.
        assert_eq!(run(with(3, c(14.6, 16.0, 14.5, 15.0, 3))), Some(-1.0));
    }

    #[test]
    fn middle_body_fully_outside_range_yields_zero() {
        // Body top exactly at low1 (max == 9.9) -> outside.
        assert_eq!(run(with(1, c(9.4, 10.0, 9.3, 9.9, 1))), Some(0.0));
        // Body entirely above high1 = 15.1 (min 15.2 >= 15.1).
        assert_eq!(run(with(3, c(15.2, 15.6, 15.1, 15.5, 3))), Some(0.0));
        // Body bottom exactly at high1 (min == 15.1) -> outside.
        assert_eq!(run(with(3, c(15.1, 15.6, 15.0, 15.5, 3))), Some(0.0));
    }

    #[test]
    fn middle_body_colour_and_size_rules() {
        // Black middle bar.
        assert_eq!(run(with(2, c(12.5, 12.6, 11.4, 11.5, 2))), Some(0.0));
        // Doji middle bar (body 0).
        assert_eq!(run(with(2, c(12.5, 12.6, 11.4, 12.5, 2))), Some(0.0));
        // Body 3.0 > 0.5 * body1 = 2.5.
        assert_eq!(run(with(1, c(9.0, 12.1, 8.9, 12.0, 1))), Some(0.0));
    }

    #[test]
    fn closes_not_drifting_up_yields_zero() {
        // close3 = 11.9 <= close2 = 12.0.
        assert_eq!(run(with(2, c(11.5, 12.0, 11.4, 11.9, 2))), Some(0.0));
        // close3 == close2.
        assert_eq!(run(with(2, c(11.5, 12.1, 11.4, 12.0, 2))), Some(0.0));
        // close4 = 12.4 <= close3 = 12.5.
        assert_eq!(run(with(3, c(12.0, 12.5, 11.9, 12.4, 3))), Some(0.0));
    }

    #[test]
    fn fifth_bar_conditions() {
        // White bar5.
        assert_eq!(run(with(4, c(9.0, 12.6, 8.9, 9.5, 4))), Some(0.0));
        // Doji bar5 (body5 == 0).
        assert_eq!(run(with(4, c(9.0, 12.6, 8.9, 9.0, 4))), Some(0.0));
        // Not long: body 3.5 < 0.5 * range 8.0 = 4.0.
        assert_eq!(run(with(4, c(12.5, 13.0, 5.0, 9.0, 4))), Some(0.0));
        // open5 = 13.0 == close4 -> not below.
        assert_eq!(run(with(4, c(13.0, 13.1, 8.9, 9.0, 4))), Some(0.0));
        // close5 == close1 = 10.0 -> not below.
        assert_eq!(run(with(4, c(12.5, 12.6, 9.9, 10.0, 4))), Some(0.0));
    }

    #[test]
    fn first_value_lands_exactly_at_warmup_index() {
        let mut t = FallingThreeMethods::new();
        let out = t.batch(&base());
        let warm = t.warmup_period();
        assert!(out[..warm - 1].iter().all(Option::is_none));
        assert_eq!(out[warm - 1], Some(-1.0));
    }

    fn mixed_series() -> Vec<Candle> {
        base().iter().cycle().take(30).copied().collect()
    }

    #[test]
    fn reset_replays_identically() {
        let candles = mixed_series();
        let fresh = FallingThreeMethods::new().batch(&candles);
        let mut t = FallingThreeMethods::new();
        let _ = t.batch(&candles);
        t.reset();
        assert_eq!(t.batch(&candles), fresh);
    }

    #[test]
    fn batch_nan_into_matches_streaming_bits() {
        let candles = mixed_series();
        let mut t = FallingThreeMethods::new();
        let streamed: Vec<f64> = candles
            .iter()
            .map(|x| t.update(*x).unwrap_or(f64::NAN))
            .collect();
        let mut out = vec![0.0; candles.len()];
        FallingThreeMethods::new().batch_nan_into(&candles, &mut out);
        assert!(streamed
            .iter()
            .zip(&out)
            .all(|(a, b)| a.to_bits() == b.to_bits()));
        assert!(streamed.contains(&-1.0));
    }
}
