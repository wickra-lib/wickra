//! Morning Star / Evening Star candlestick pattern.

use crate::ohlcv::Candle;
use crate::traits::Indicator;

/// How far bar 3 must close into bar 1's body, as a fraction of that body
/// (TA-Lib's default `penetration`).
const PENETRATION: f64 = 0.3;

/// Morning Star / Evening Star — a 3-bar reversal pattern.
///
/// **Morning Star** (bullish, `+1.0`):
/// 1. Bar 1 is a long red candle.
/// 2. Bar 2 has a small body (the "star") whose body gaps below Bar 1's body
///    (`max(open2, close2) < close1`) — colour does not matter.
/// 3. Bar 3 is a long green candle that closes at least 30% of the way up into
///    Bar 1's body (`close3 > close1 + 0.3 · body1`).
///
/// **Evening Star** (bearish, `−1.0`): the mirror image — long green, a small
/// body gapping above it (`min(open2, close2) > close1`), long red closing at
/// least 30% of the way down into Bar 1's body (`close3 < close1 − 0.3 · body1`).
///
/// The star's body gap is the defining feature (Nison; TA-Lib `CDLMORNINGSTAR`
/// / `CDLEVENINGSTAR`); the 30% penetration is TA-Lib's default.
///
/// The "long" qualifier is enforced by requiring the outer bars' bodies to be
/// at least twice the size of the star's body. Pattern-shape check only — no
/// trend filter is applied; combine with a trend indicator for actionable
/// signals.
///
/// # Signed ±1 encoding
///
/// This detector already emits the uniform candlestick sign convention shared
/// across the pattern family — `+1.0` bullish, `−1.0` bearish, `0.0` no
/// pattern — so it drops straight into a machine-learning feature matrix where
/// the bullish and bearish variants of the pattern occupy a single dimension.
///
/// # Example
///
/// ```
/// use wickra_core::{Candle, Indicator, MorningEveningStar};
///
/// let mut indicator = MorningEveningStar::new();
/// indicator.update(Candle::new(12.0, 12.2, 9.5, 10.0, 1.0, 0).unwrap());
/// indicator.update(Candle::new(9.9, 10.1, 9.7, 9.95, 1.0, 1).unwrap());
/// let out = indicator
///     .update(Candle::new(10.1, 12.0, 10.0, 11.8, 1.0, 2).unwrap());
/// assert_eq!(out, Some(1.0));
/// ```
#[derive(Debug, Clone, Default)]
pub struct MorningEveningStar {
    prev: Option<Candle>,
    prev_prev: Option<Candle>,
    has_emitted: bool,
}

impl MorningEveningStar {
    /// Construct a new Morning / Evening Star detector.
    pub const fn new() -> Self {
        Self {
            prev: None,
            prev_prev: None,
            has_emitted: false,
        }
    }
}

impl Indicator for MorningEveningStar {
    type Input = Candle;
    type Output = f64;

    #[inline]
    fn update(&mut self, candle: Candle) -> Option<f64> {
        let pp = self.prev_prev;
        let p = self.prev;
        self.prev_prev = self.prev;
        self.prev = Some(candle);
        let (Some(b1), Some(b2)) = (pp, p) else {
            return None;
        };
        self.has_emitted = true;
        let body1 = (b1.close - b1.open).abs();
        let body2 = (b2.close - b2.open).abs();
        let body3 = (candle.close - candle.open).abs();
        if body1 <= 0.0 || body3 <= 0.0 {
            return Some(0.0);
        }
        // Star body must be small relative to the outer bars.
        if body1 < 2.0 * body2 || body3 < 2.0 * body2 {
            return Some(0.0);
        }
        let penetration = PENETRATION * body1;
        let bar1_red = b1.close < b1.open;
        let bar1_green = b1.close > b1.open;
        let bar3_green = candle.close > candle.open;
        let bar3_red = candle.close < candle.open;
        let star_top = b2.open.max(b2.close);
        let star_bottom = b2.open.min(b2.close);
        if bar1_red && bar3_green && star_top < b1.close && candle.close > b1.close + penetration {
            Some(1.0)
        } else if bar1_green
            && bar3_red
            && star_bottom > b1.close
            && candle.close < b1.close - penetration
        {
            Some(-1.0)
        } else {
            Some(0.0)
        }
    }

    fn reset(&mut self) {
        self.prev = None;
        self.prev_prev = None;
        self.has_emitted = false;
    }

    #[inline]
    fn warmup_period(&self) -> usize {
        3
    }

    #[inline]
    fn is_ready(&self) -> bool {
        self.has_emitted
    }

    #[inline]
    fn name(&self) -> &'static str {
        "MorningEveningStar"
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
        let m = MorningEveningStar::new();
        assert_eq!(m.name(), "MorningEveningStar");
        assert_eq!(m.warmup_period(), 3);
        assert!(!m.is_ready());
    }

    #[test]
    fn morning_star_is_plus_one() {
        let mut m = MorningEveningStar::new();
        // Long red 12 -> 10 (body 2). Star small body. Long green 10.1 -> 11.8 (body 1.7).
        // Mid of bar 1 = 11. Bar 3 closes at 11.8 > 11.
        assert_eq!(m.update(c(12.0, 12.2, 9.5, 10.0, 0)), None);
        assert_eq!(m.update(c(9.9, 10.1, 9.7, 9.95, 1)), None);
        assert_eq!(m.update(c(10.1, 12.0, 10.0, 11.8, 2)), Some(1.0));
    }

    #[test]
    fn evening_star_is_minus_one() {
        let mut m = MorningEveningStar::new();
        // Long green 10 -> 12, star, long red 11.9 -> 10.2 (body 1.7).
        // Mid of bar 1 = 11. Bar 3 closes at 10.2 < 11.
        assert_eq!(m.update(c(10.0, 12.2, 9.8, 12.0, 0)), None);
        assert_eq!(m.update(c(12.1, 12.3, 11.9, 12.05, 1)), None);
        assert_eq!(m.update(c(11.9, 12.0, 10.1, 10.2, 2)), Some(-1.0));
    }

    #[test]
    fn big_star_body_is_not_star() {
        let mut m = MorningEveningStar::new();
        m.update(c(12.0, 12.2, 9.5, 10.0, 0));
        // Star body 1.5 -> body1=2, body3 needs to be >= 3 to satisfy 2*body2.
        m.update(c(9.5, 11.5, 9.5, 11.0, 1));
        assert_eq!(m.update(c(10.1, 12.0, 10.0, 11.8, 2)), Some(0.0));
    }

    #[test]
    fn first_two_bars_return_zero() {
        let mut m = MorningEveningStar::new();
        assert_eq!(m.update(c(12.0, 12.2, 9.5, 10.0, 0)), None);
        assert_eq!(m.update(c(9.9, 10.1, 9.7, 9.95, 1)), None);
    }

    #[test]
    fn batch_equals_streaming() {
        let candles: Vec<Candle> = (0..40)
            .map(|i| {
                let base = 100.0 + i as f64;
                match i % 3 {
                    0 => c(base + 2.0, base + 2.5, base - 0.5, base, i),
                    1 => c(base + 0.1, base + 0.3, base - 0.1, base + 0.15, i),
                    _ => c(base, base + 2.5, base - 0.5, base + 2.0, i),
                }
            })
            .collect();
        let mut a = MorningEveningStar::new();
        let mut b = MorningEveningStar::new();
        assert_eq!(
            a.batch(&candles),
            candles.iter().map(|x| b.update(*x)).collect::<Vec<_>>()
        );
    }

    #[test]
    fn reset_clears_state() {
        let mut m = MorningEveningStar::new();
        m.update(c(12.0, 12.2, 9.5, 10.0, 0));
        m.update(c(9.9, 10.1, 9.7, 9.95, 1));
        m.update(c(10.1, 12.0, 10.0, 11.8, 2));
        assert!(m.is_ready());
        m.reset();
        assert!(!m.is_ready());
        assert_eq!(m.update(c(12.0, 12.2, 9.5, 10.0, 0)), None);
    }

    #[test]
    fn doji_outer_bar_yields_zero() {
        // Bar1 is a doji (body == 0): body1 == 0 -> guard triggers, returns 0.
        let mut m = MorningEveningStar::new();
        m.update(c(10.0, 11.0, 9.0, 10.0, 0)); // doji bar1
        m.update(c(9.9, 10.1, 9.7, 9.95, 1));
        assert_eq!(m.update(c(10.1, 12.0, 10.0, 11.8, 2)), Some(0.0));
    }

    #[test]
    fn same_direction_bars_yield_zero() {
        // Bar1 red, star small, bar3 also red (wrong direction) -> falls through to else 0.
        let mut m = MorningEveningStar::new();
        m.update(c(12.0, 12.2, 9.5, 10.0, 0)); // long red (body 2)
        m.update(c(9.9, 10.1, 9.7, 9.95, 1)); // small star
                                              // Bar3 red, closes below mid (11); doesn't match morning star (bar3 must be green)
                                              // and also doesn't match evening star (bar1 must be green).
        assert_eq!(m.update(c(11.0, 11.2, 9.0, 9.5, 2)), Some(0.0));
    }

    fn run(bars: [Candle; 3]) -> Option<f64> {
        let mut m = MorningEveningStar::new();
        bars.iter().map(|b| m.update(*b)).last().unwrap()
    }

    // Morning: bar1 red 12 -> 10 (body1 = 2, penetration = 0.3 * 2 = 0.6, so
    // close3 must exceed 10 + 0.6 = 10.6; the old midpoint rule needed > 11).
    const RED1: (f64, f64, f64, f64) = (12.0, 12.2, 9.5, 10.0);
    // Star body 9.9 / 9.95: top 9.95 < close1 = 10 (body gap), body2 = 0.05.
    const LOW_STAR: (f64, f64, f64, f64) = (9.9, 10.1, 9.7, 9.95);
    // Evening: bar1 green 10 -> 12 (close3 must be below 12 - 0.6 = 11.4).
    const GREEN1: (f64, f64, f64, f64) = (10.0, 12.2, 9.8, 12.0);
    // Star body 12.05 / 12.1: bottom 12.05 > close1 = 12 (body gap).
    const HIGH_STAR: (f64, f64, f64, f64) = (12.1, 12.3, 11.9, 12.05);

    fn bar(t: (f64, f64, f64, f64), ts: i64) -> Candle {
        c(t.0, t.1, t.2, t.3, ts)
    }

    #[test]
    fn morning_penetration_between_30_and_50_percent_fires() {
        // close3 = 10.8: 10.8 > 10.6 (30%) although below the 11.0 midpoint.
        assert_eq!(
            run([bar(RED1, 0), bar(LOW_STAR, 1), c(10.1, 10.9, 10.0, 10.8, 2)]),
            Some(1.0)
        );
    }

    #[test]
    fn morning_penetration_below_30_percent_is_zero() {
        // close3 = 10.5 < 10.6 -> not deep enough (body3 = 0.4 >= 2 * 0.05).
        assert_eq!(
            run([bar(RED1, 0), bar(LOW_STAR, 1), c(10.1, 10.6, 10.0, 10.5, 2)]),
            Some(0.0)
        );
    }

    #[test]
    fn evening_penetration_between_30_and_50_percent_fires() {
        // close3 = 11.2 < 11.4 (30%) although above the 11.0 midpoint.
        assert_eq!(
            run([
                bar(GREEN1, 0),
                bar(HIGH_STAR, 1),
                c(11.9, 12.0, 11.1, 11.2, 2)
            ]),
            Some(-1.0)
        );
    }

    #[test]
    fn evening_penetration_below_30_percent_is_zero() {
        // close3 = 11.5 > 11.4 -> not deep enough.
        assert_eq!(
            run([
                bar(GREEN1, 0),
                bar(HIGH_STAR, 1),
                c(11.9, 12.0, 11.4, 11.5, 2)
            ]),
            Some(0.0)
        );
    }

    #[test]
    fn morning_star_body_must_gap_below_close1() {
        // Star top max(9.9, 10.05) = 10.05 >= close1 = 10 -> no body gap.
        let star = c(9.9, 10.1, 9.7, 10.05, 1);
        assert_eq!(
            run([bar(RED1, 0), star, c(10.1, 12.0, 10.0, 11.8, 2)]),
            Some(0.0)
        );
        // Star top exactly at close1 is still no gap.
        let star = c(9.95, 10.1, 9.7, 10.0, 1);
        assert_eq!(
            run([bar(RED1, 0), star, c(10.1, 12.0, 10.0, 11.8, 2)]),
            Some(0.0)
        );
        // Only the body matters: a star whose upper shadow reaches into bar1's
        // body still gaps.
        let star = c(9.9, 10.5, 9.7, 9.95, 1);
        assert_eq!(
            run([bar(RED1, 0), star, c(10.1, 12.0, 10.0, 11.8, 2)]),
            Some(1.0)
        );
    }

    #[test]
    fn evening_star_body_must_gap_above_close1() {
        // Star bottom min(12.1, 11.95) = 11.95 <= close1 = 12 -> no body gap.
        let star = c(12.1, 12.3, 11.9, 11.95, 1);
        assert_eq!(
            run([bar(GREEN1, 0), star, c(11.9, 12.0, 10.1, 10.2, 2)]),
            Some(0.0)
        );
        // Lower shadow reaching into bar1's body does not matter.
        let star = c(12.1, 12.3, 11.5, 12.05, 1);
        assert_eq!(
            run([bar(GREEN1, 0), star, c(11.9, 12.0, 10.1, 10.2, 2)]),
            Some(-1.0)
        );
    }

    #[test]
    fn doji_third_bar_yields_zero() {
        assert_eq!(
            run([bar(RED1, 0), bar(LOW_STAR, 1), c(10.5, 11.0, 10.0, 10.5, 2)]),
            Some(0.0)
        );
    }

    #[test]
    fn third_body_too_small_relative_to_star_yields_zero() {
        // body2 = 0.3 (body1 = 2 >= 0.6 passes) but body3 = 0.5 < 0.6.
        let star = c(9.3, 9.7, 9.2, 9.6, 1);
        assert_eq!(
            run([bar(RED1, 0), star, c(10.4, 11.0, 10.3, 10.9, 2)]),
            Some(0.0)
        );
    }

    #[test]
    fn green_first_bar_with_green_third_yields_zero() {
        // Bar1 green, bar3 green -> neither branch.
        assert_eq!(
            run([
                bar(GREEN1, 0),
                bar(HIGH_STAR, 1),
                c(10.1, 12.0, 10.0, 11.8, 2)
            ]),
            Some(0.0)
        );
    }

    #[test]
    fn first_value_lands_exactly_at_warmup_index() {
        let bars = [bar(RED1, 0), bar(LOW_STAR, 1), c(10.1, 10.9, 10.0, 10.8, 2)];
        let mut m = MorningEveningStar::new();
        let out = m.batch(&bars);
        let warm = m.warmup_period();
        assert!(out[..warm - 1].iter().all(Option::is_none));
        assert_eq!(out[warm - 1], Some(1.0));
    }

    fn mixed_series() -> Vec<Candle> {
        let bars = [
            bar(RED1, 0),
            bar(LOW_STAR, 1),
            c(10.1, 10.9, 10.0, 10.8, 2),
            bar(GREEN1, 3),
            bar(HIGH_STAR, 4),
            c(11.9, 12.0, 11.1, 11.2, 5),
        ];
        bars.iter().cycle().take(30).copied().collect()
    }

    #[test]
    fn reset_replays_identically() {
        let candles = mixed_series();
        let fresh = MorningEveningStar::new().batch(&candles);
        let mut m = MorningEveningStar::new();
        let _ = m.batch(&candles);
        m.reset();
        assert_eq!(m.batch(&candles), fresh);
    }

    #[test]
    fn batch_nan_into_matches_streaming_bits() {
        let candles = mixed_series();
        let mut m = MorningEveningStar::new();
        let streamed: Vec<f64> = candles
            .iter()
            .map(|x| m.update(*x).unwrap_or(f64::NAN))
            .collect();
        let mut out = vec![0.0; candles.len()];
        MorningEveningStar::new().batch_nan_into(&candles, &mut out);
        assert!(streamed
            .iter()
            .zip(&out)
            .all(|(a, b)| a.to_bits() == b.to_bits()));
        assert!(streamed.contains(&1.0) && streamed.contains(&-1.0));
    }
}
