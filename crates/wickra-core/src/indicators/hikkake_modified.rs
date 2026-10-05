//! Modified Hikkake candlestick pattern.

use crate::ohlcv::Candle;
use crate::traits::Indicator;

/// Fraction of the second bar's range that counts as "near" its low or high.
const NEAR: f64 = 0.2;

/// Modified Hikkake — Dan Chesler's refinement of the [`Hikkake`](crate::Hikkake)
/// trap (TA-Lib `CDLHIKKAKEMOD`). Two nested inside bars coil the market; the
/// first inside bar already closes at the extreme the false break will run
/// towards, and the fourth bar breaks out of the second inside bar the wrong way.
///
/// ```text
/// bar2 inside bar1 : high2 < high1  &&  low2 > low1
/// bar3 inside bar2 : high3 < high2  &&  low3 > low2
/// bullish (+1.0): bar4 makes a lower high AND lower low than bar3,
///                 and bar2 closed near its low   (close2 <= low2  + 0.2 · range2)
/// bearish (−1.0): bar4 makes a higher high AND higher low than bar3,
///                 and bar2 closed near its high  (close2 >= high2 − 0.2 · range2)
/// ```
///
/// Output is `+1.0` (bullish), `−1.0` (bearish), or `0.0` otherwise. "Near" is a
/// fixed fifth of the second bar's range (TA-Lib's `Near` factor of 0.2 applied
/// geometrically rather than to a rolling average). The first three bars return
/// `None` because the four-bar window is not yet filled. The later confirmation
/// bar is not flagged separately. Pattern-shape check only — no trend filter is
/// applied; combine with a trend indicator for actionable signals.
///
/// # Signed ±1 encoding
///
/// This detector emits the uniform candlestick sign convention shared across the
/// pattern family — `+1.0` bullish, `−1.0` bearish, `0.0` no pattern — so it
/// drops straight into a machine-learning feature matrix as a single dimension.
///
/// # Example
///
/// ```
/// use wickra_core::{Candle, HikkakeModified, Indicator};
///
/// let mut indicator = HikkakeModified::new();
/// indicator.update(Candle::new(10.0, 15.0, 5.0, 12.0, 1.0, 0).unwrap());
/// indicator.update(Candle::new(11.0, 13.0, 7.0, 7.5, 1.0, 1).unwrap());
/// indicator.update(Candle::new(9.0, 12.0, 8.0, 10.0, 1.0, 2).unwrap());
/// let out = indicator
///     .update(Candle::new(9.0, 11.0, 6.0, 9.0, 1.0, 3).unwrap());
/// assert_eq!(out, Some(1.0));
/// ```
#[derive(Debug, Clone, Default)]
pub struct HikkakeModified {
    c1: Option<Candle>,
    c2: Option<Candle>,
    c3: Option<Candle>,
    has_emitted: bool,
}

impl HikkakeModified {
    /// Construct a new Modified Hikkake detector.
    pub const fn new() -> Self {
        Self {
            c1: None,
            c2: None,
            c3: None,
            has_emitted: false,
        }
    }
}

impl Indicator for HikkakeModified {
    type Input = Candle;
    type Output = f64;

    #[inline]
    fn update(&mut self, candle: Candle) -> Option<f64> {
        let (bar1, bar2, bar3) = (self.c1, self.c2, self.c3);
        self.c1 = self.c2;
        self.c2 = self.c3;
        self.c3 = Some(candle);
        let (Some(bar1), Some(bar2), Some(bar3)) = (bar1, bar2, bar3) else {
            return None;
        };
        self.has_emitted = true;
        // Two nested inside bars.
        if !(bar2.high < bar1.high
            && bar2.low > bar1.low
            && bar3.high < bar2.high
            && bar3.low > bar2.low)
        {
            return Some(0.0);
        }
        let near = NEAR * (bar2.high - bar2.low);
        // Bullish: false downside break, the first inside bar closed near its low.
        if candle.high < bar3.high && candle.low < bar3.low && bar2.close <= bar2.low + near {
            return Some(1.0);
        }
        // Bearish: false upside break, the first inside bar closed near its high.
        if candle.high > bar3.high && candle.low > bar3.low && bar2.close >= bar2.high - near {
            return Some(-1.0);
        }
        Some(0.0)
    }

    fn reset(&mut self) {
        self.c1 = None;
        self.c2 = None;
        self.c3 = None;
        self.has_emitted = false;
    }

    #[inline]
    fn warmup_period(&self) -> usize {
        4
    }

    #[inline]
    fn is_ready(&self) -> bool {
        self.has_emitted
    }

    #[inline]
    fn name(&self) -> &'static str {
        "HikkakeModified"
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
        let t = HikkakeModified::new();
        assert_eq!(t.name(), "HikkakeModified");
        assert_eq!(t.warmup_period(), 4);
        assert!(!t.is_ready());
    }

    #[test]
    fn bullish_modified_hikkake_is_plus_one() {
        let mut t = HikkakeModified::new();
        assert_eq!(t.update(c(10.0, 15.0, 5.0, 12.0, 0)), None);
        assert_eq!(t.update(c(11.0, 13.0, 7.0, 7.5, 1)), None);
        assert_eq!(t.update(c(9.0, 12.0, 8.0, 10.0, 2)), None);
        assert_eq!(t.update(c(9.0, 11.0, 6.0, 9.0, 3)), Some(1.0));
    }

    #[test]
    fn bearish_modified_hikkake_is_minus_one() {
        let mut t = HikkakeModified::new();
        assert_eq!(t.update(c(10.0, 15.0, 5.0, 12.0, 0)), None);
        assert_eq!(t.update(c(11.0, 13.0, 7.0, 12.5, 1)), None);
        assert_eq!(t.update(c(9.0, 12.0, 8.0, 10.0, 2)), None);
        assert_eq!(t.update(c(11.0, 14.0, 9.0, 13.0, 3)), Some(-1.0));
    }

    #[test]
    fn second_bar_close_not_near_extreme_yields_zero() {
        let mut t = HikkakeModified::new();
        t.update(c(10.0, 15.0, 5.0, 12.0, 0));
        // bar2 closes mid-range -> the close filter fails.
        t.update(c(11.0, 13.0, 7.0, 10.0, 1));
        t.update(c(9.0, 12.0, 8.0, 10.0, 2));
        assert_eq!(t.update(c(9.0, 11.0, 6.0, 9.0, 3)), Some(0.0));
    }

    #[test]
    fn not_double_inside_bar_yields_zero() {
        let mut t = HikkakeModified::new();
        t.update(c(10.0, 15.0, 5.0, 12.0, 0));
        t.update(c(11.0, 13.0, 7.0, 7.5, 1));
        // bar3 is not inside bar2.
        t.update(c(9.0, 14.0, 8.0, 10.0, 2));
        assert_eq!(t.update(c(9.0, 11.0, 6.0, 9.0, 3)), Some(0.0));
    }

    #[test]
    fn first_three_bars_return_none() {
        let mut t = HikkakeModified::new();
        assert_eq!(t.update(c(10.0, 15.0, 5.0, 12.0, 0)), None);
        assert_eq!(t.update(c(11.0, 13.0, 7.0, 7.5, 1)), None);
        assert_eq!(t.update(c(9.0, 12.0, 8.0, 10.0, 2)), None);
    }

    #[test]
    fn batch_equals_streaming() {
        let candles: Vec<Candle> = (0..40)
            .map(|i| {
                let base = 100.0 + i as f64;
                match i % 4 {
                    0 => c(base, base + 6.0, base - 6.0, base, i),
                    1 => c(base, base + 4.0, base - 4.0, base - 3.5, i),
                    2 => c(base, base + 2.0, base - 2.0, base, i),
                    _ => c(base, base + 1.0, base - 5.0, base, i),
                }
            })
            .collect();
        let mut a = HikkakeModified::new();
        let mut b = HikkakeModified::new();
        assert_eq!(
            a.batch(&candles),
            candles.iter().map(|x| b.update(*x)).collect::<Vec<_>>()
        );
    }

    #[test]
    fn reset_clears_state() {
        let mut t = HikkakeModified::new();
        t.update(c(10.0, 15.0, 5.0, 12.0, 0));
        t.update(c(11.0, 13.0, 7.0, 7.5, 1));
        t.update(c(9.0, 12.0, 8.0, 10.0, 2));
        t.update(c(9.0, 11.0, 6.0, 9.0, 3));
        assert!(t.is_ready());
        t.reset();
        assert!(!t.is_ready());
        assert_eq!(t.update(c(10.0, 15.0, 5.0, 12.0, 0)), None);
    }

    /// Feed four bars and return the last output.
    fn run(bars: [Candle; 4]) -> Option<f64> {
        let mut t = HikkakeModified::new();
        bars.iter().map(|b| t.update(*b)).last().unwrap()
    }

    // bar1 15/5; bar2 13/8 (range 5, near = 0.2 * 5 = 1.0); bar3 12/9.
    const BAR1: (f64, f64, f64, f64) = (10.0, 15.0, 5.0, 12.0);
    const BAR3: (f64, f64, f64, f64) = (10.0, 12.0, 9.0, 10.0);

    fn bar(t: (f64, f64, f64, f64), ts: i64) -> Candle {
        c(t.0, t.1, t.2, t.3, ts)
    }

    #[test]
    fn hand_computed_near_boundary_bullish() {
        // near = 0.2 * (13 - 8) = 1.0 -> bullish needs close2 <= 8 + 1.0 = 9.0.
        // bar4 high 11 < 12 and low 7 < 9: false downside break.
        let bar4 = c(9.0, 11.0, 7.0, 9.0, 3);
        assert_eq!(
            run([bar(BAR1, 0), c(11.0, 13.0, 8.0, 9.0, 1), bar(BAR3, 2), bar4]),
            Some(1.0)
        );
        assert_eq!(
            run([bar(BAR1, 0), c(11.0, 13.0, 8.0, 8.5, 1), bar(BAR3, 2), bar4]),
            Some(1.0)
        );
        // close2 = 9.01 > 9.0 -> not near the low.
        assert_eq!(
            run([
                bar(BAR1, 0),
                c(11.0, 13.0, 8.0, 9.01, 1),
                bar(BAR3, 2),
                bar4
            ]),
            Some(0.0)
        );
    }

    #[test]
    fn hand_computed_near_boundary_bearish() {
        // bearish needs close2 >= 13 - 1.0 = 12.0; bar4 high 14 > 12, low 10 > 9.
        let bar4 = c(11.0, 14.0, 10.0, 13.0, 3);
        assert_eq!(
            run([
                bar(BAR1, 0),
                c(11.0, 13.0, 8.0, 12.0, 1),
                bar(BAR3, 2),
                bar4
            ]),
            Some(-1.0)
        );
        // close2 = 11.99 < 12.0 -> not near the high.
        assert_eq!(
            run([
                bar(BAR1, 0),
                c(11.0, 13.0, 8.0, 11.99, 1),
                bar(BAR3, 2),
                bar4
            ]),
            Some(0.0)
        );
    }

    #[test]
    fn bullish_break_fails_each_condition() {
        let bar2 = c(11.0, 13.0, 8.0, 9.0, 1);
        // bar4 high equals bar3 high (not a lower high).
        assert_eq!(
            run([bar(BAR1, 0), bar2, bar(BAR3, 2), c(9.0, 12.0, 7.0, 9.0, 3)]),
            Some(0.0)
        );
        // bar4 low equals bar3 low (not a lower low).
        assert_eq!(
            run([
                bar(BAR1, 0),
                bar2,
                bar(BAR3, 2),
                c(10.0, 11.0, 9.0, 10.0, 3)
            ]),
            Some(0.0)
        );
        // bar4 is an outside bar (higher high, lower low).
        assert_eq!(
            run([
                bar(BAR1, 0),
                bar2,
                bar(BAR3, 2),
                c(10.0, 13.0, 7.0, 10.0, 3)
            ]),
            Some(0.0)
        );
        // bar4 is itself an inside bar.
        assert_eq!(
            run([
                bar(BAR1, 0),
                bar2,
                bar(BAR3, 2),
                c(10.0, 11.0, 9.5, 10.0, 3)
            ]),
            Some(0.0)
        );
    }

    #[test]
    fn bearish_break_fails_each_condition() {
        let bar2 = c(11.0, 13.0, 8.0, 12.0, 1);
        // bar4 high equals bar3 high.
        assert_eq!(
            run([
                bar(BAR1, 0),
                bar2,
                bar(BAR3, 2),
                c(11.0, 12.0, 10.0, 11.0, 3)
            ]),
            Some(0.0)
        );
        // bar4 low equals bar3 low.
        assert_eq!(
            run([
                bar(BAR1, 0),
                bar2,
                bar(BAR3, 2),
                c(11.0, 14.0, 9.0, 13.0, 3)
            ]),
            Some(0.0)
        );
        // A downside break with bar2 near its high is not bullish either.
        assert_eq!(
            run([bar(BAR1, 0), bar2, bar(BAR3, 2), c(9.0, 11.0, 7.0, 9.0, 3)]),
            Some(0.0)
        );
    }

    #[test]
    fn each_inside_bar_condition_is_required() {
        let bar4 = c(9.0, 11.0, 7.0, 9.0, 3);
        // high2 == high1.
        assert_eq!(
            run([bar(BAR1, 0), c(11.0, 15.0, 8.0, 9.0, 1), bar(BAR3, 2), bar4]),
            Some(0.0)
        );
        // low2 == low1.
        assert_eq!(
            run([bar(BAR1, 0), c(11.0, 13.0, 5.0, 6.0, 1), bar(BAR3, 2), bar4]),
            Some(0.0)
        );
        // high3 == high2.
        let bar2 = c(11.0, 13.0, 8.0, 9.0, 1);
        assert_eq!(
            run([bar(BAR1, 0), bar2, c(10.0, 13.0, 9.0, 10.0, 2), bar4]),
            Some(0.0)
        );
        // low3 == low2.
        assert_eq!(
            run([bar(BAR1, 0), bar2, c(10.0, 12.0, 8.0, 10.0, 2), bar4]),
            Some(0.0)
        );
    }

    #[test]
    fn first_value_lands_exactly_at_warmup_index() {
        let bars = [
            bar(BAR1, 0),
            c(11.0, 13.0, 8.0, 9.0, 1),
            bar(BAR3, 2),
            c(9.0, 11.0, 7.0, 9.0, 3),
            c(9.0, 11.0, 7.0, 9.0, 4),
        ];
        let mut t = HikkakeModified::new();
        let out = t.batch(&bars);
        let warm = t.warmup_period();
        assert!(out[..warm - 1].iter().all(Option::is_none));
        assert_eq!(out[warm - 1], Some(1.0));
        assert_eq!(out[warm], Some(0.0));
    }

    fn mixed_series() -> Vec<Candle> {
        (0..40)
            .map(|i| {
                let base = 100.0 + f64::from(i % 7);
                match i % 4 {
                    0 => c(base, base + 6.0, base - 6.0, base, i64::from(i)),
                    1 => c(base, base + 4.0, base - 4.0, base - 3.5, i64::from(i)),
                    2 => c(base, base + 2.0, base - 2.0, base, i64::from(i)),
                    _ => c(base, base + 1.0, base - 5.0, base, i64::from(i)),
                }
            })
            .collect()
    }

    #[test]
    fn reset_replays_identically() {
        let candles = mixed_series();
        let fresh = HikkakeModified::new().batch(&candles);
        let mut t = HikkakeModified::new();
        let _ = t.batch(&candles);
        t.reset();
        assert_eq!(t.batch(&candles), fresh);
    }

    #[test]
    fn batch_nan_into_matches_streaming_bits() {
        let candles = mixed_series();
        let mut t = HikkakeModified::new();
        let streamed: Vec<f64> = candles
            .iter()
            .map(|x| t.update(*x).unwrap_or(f64::NAN))
            .collect();
        let mut out = vec![0.0; candles.len()];
        HikkakeModified::new().batch_nan_into(&candles, &mut out);
        assert!(streamed
            .iter()
            .zip(&out)
            .all(|(a, b)| a.to_bits() == b.to_bits()));
    }
}
