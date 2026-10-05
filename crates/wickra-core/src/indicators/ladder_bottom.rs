//! Ladder Bottom candlestick pattern.

use crate::ohlcv::Candle;
use crate::traits::Indicator;

/// Ladder Bottom — a 5-bar bullish reversal. Three long black candles step the
/// market down like rungs of a ladder, a fourth black candle finally shows an
/// upper shadow (the first sign of buying), and a white candle then gaps up into
/// its body and closes above the fourth bar's high to confirm the turn (Nison;
/// TA-Lib `CDLLADDERBOTTOM`).
///
/// ```text
/// bar1, bar2, bar3 black, with consecutively lower opens AND closes
/// bar4 black with an upper shadow         (high4 > open4)
/// bar5 white, opens above bar4's body      (open5 > open4)
///      and closes above bar4's high          (close5 > high4)
/// ```
///
/// Output is `+1.0` when the pattern completes and `0.0` otherwise. Ladder Bottom
/// is a single-direction (bullish-only) reversal, so it never emits `−1.0`. The
/// first four bars always return `0.0` because the five-bar window is not yet
/// filled. Pattern-shape check only — no trend filter is applied; combine with a
/// trend indicator for actionable signals.
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
/// use wickra_core::{Candle, Indicator, LadderBottom};
///
/// let mut indicator = LadderBottom::new();
/// indicator.update(Candle::new(20.0, 20.1, 17.9, 18.0, 1.0, 0).unwrap());
/// indicator.update(Candle::new(18.0, 18.1, 15.9, 16.0, 1.0, 1).unwrap());
/// indicator.update(Candle::new(16.0, 16.1, 13.9, 14.0, 1.0, 2).unwrap());
/// indicator.update(Candle::new(14.0, 15.0, 12.4, 12.5, 1.0, 3).unwrap());
/// let out = indicator
///     .update(Candle::new(15.0, 17.1, 14.9, 17.0, 1.0, 4).unwrap());
/// assert_eq!(out, Some(1.0));
/// ```
#[derive(Debug, Clone, Default)]
pub struct LadderBottom {
    c1: Option<Candle>,
    c2: Option<Candle>,
    c3: Option<Candle>,
    c4: Option<Candle>,
    has_emitted: bool,
}

impl LadderBottom {
    /// Construct a new Ladder Bottom detector.
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

impl Indicator for LadderBottom {
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
        if bar1.close < bar1.open
            && bar2.close < bar2.open
            && bar3.close < bar3.open
            && bar2.open < bar1.open
            && bar2.close < bar1.close
            && bar3.open < bar2.open
            && bar3.close < bar2.close
            && bar4.close < bar4.open
            && bar4.high > bar4.open
            && candle.close > candle.open
            && candle.open > bar4.open
            && candle.close > bar4.high
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
        "LadderBottom"
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
        let t = LadderBottom::new();
        assert_eq!(t.name(), "LadderBottom");
        assert_eq!(t.warmup_period(), 5);
        assert!(!t.is_ready());
    }

    #[test]
    fn ladder_bottom_is_plus_one() {
        let mut t = LadderBottom::new();
        assert_eq!(t.update(c(20.0, 20.1, 17.9, 18.0, 0)), None);
        assert_eq!(t.update(c(18.0, 18.1, 15.9, 16.0, 1)), None);
        assert_eq!(t.update(c(16.0, 16.1, 13.9, 14.0, 2)), None);
        assert_eq!(t.update(c(14.0, 15.0, 12.4, 12.5, 3)), None);
        assert_eq!(t.update(c(15.0, 17.1, 14.9, 17.0, 4)), Some(1.0));
    }

    #[test]
    fn fourth_bar_without_upper_shadow_yields_zero() {
        let mut t = LadderBottom::new();
        t.update(c(20.0, 20.1, 17.9, 18.0, 0));
        t.update(c(18.0, 18.1, 15.9, 16.0, 1));
        t.update(c(16.0, 16.1, 13.9, 14.0, 2));
        // bar4 opens at its high -> no upper shadow.
        t.update(c(14.0, 14.0, 12.4, 12.5, 3));
        assert_eq!(t.update(c(15.0, 17.1, 14.9, 17.0, 4)), Some(0.0));
    }

    #[test]
    fn not_three_descending_blacks_yields_zero() {
        let mut t = LadderBottom::new();
        // bar2 is not lower than bar1.
        t.update(c(20.0, 20.1, 17.9, 18.0, 0));
        t.update(c(21.0, 21.1, 18.9, 19.0, 1));
        t.update(c(16.0, 16.1, 13.9, 14.0, 2));
        t.update(c(14.0, 15.0, 12.4, 12.5, 3));
        assert_eq!(t.update(c(15.0, 17.1, 14.9, 17.0, 4)), Some(0.0));
    }

    #[test]
    fn first_four_bars_return_zero() {
        let mut t = LadderBottom::new();
        assert_eq!(t.update(c(20.0, 20.1, 17.9, 18.0, 0)), None);
        assert_eq!(t.update(c(18.0, 18.1, 15.9, 16.0, 1)), None);
        assert_eq!(t.update(c(16.0, 16.1, 13.9, 14.0, 2)), None);
        assert_eq!(t.update(c(14.0, 15.0, 12.4, 12.5, 3)), None);
    }

    #[test]
    fn batch_equals_streaming() {
        let candles: Vec<Candle> = (0..40)
            .map(|i| {
                let base = 200.0 - i as f64;
                c(base, base + 0.1, base - 2.1, base - 2.0, i)
            })
            .collect();
        let mut a = LadderBottom::new();
        let mut b = LadderBottom::new();
        assert_eq!(
            a.batch(&candles),
            candles.iter().map(|x| b.update(*x)).collect::<Vec<_>>()
        );
    }

    #[test]
    fn reset_clears_state() {
        let mut t = LadderBottom::new();
        t.update(c(20.0, 20.1, 17.9, 18.0, 0));
        t.update(c(18.0, 18.1, 15.9, 16.0, 1));
        t.update(c(16.0, 16.1, 13.9, 14.0, 2));
        t.update(c(14.0, 15.0, 12.4, 12.5, 3));
        t.update(c(15.0, 17.1, 14.9, 17.0, 4));
        assert!(t.is_ready());
        t.reset();
        assert!(!t.is_ready());
        assert_eq!(t.update(c(20.0, 20.1, 17.9, 18.0, 0)), None);
    }

    /// The canonical pattern from the example.
    fn base() -> [Candle; 5] {
        [
            c(20.0, 20.1, 17.9, 18.0, 0),
            c(18.0, 18.1, 15.9, 16.0, 1),
            c(16.0, 16.1, 13.9, 14.0, 2),
            c(14.0, 15.0, 12.4, 12.5, 3),
            c(15.0, 17.1, 14.9, 17.0, 4),
        ]
    }

    fn run(bars: [Candle; 5]) -> Option<f64> {
        let mut t = LadderBottom::new();
        bars.iter().map(|b| t.update(*b)).last().unwrap()
    }

    fn with(index: usize, candle: Candle) -> [Candle; 5] {
        let mut bars = base();
        bars[index] = candle;
        bars
    }

    #[test]
    fn close5_must_exceed_high4() {
        // high4 = 15.0. bar5 opens at 14.5 (> open4 = 14.0) and is white.
        // close5 = 15.0 is not > 15.0 -> no pattern.
        assert_eq!(run(with(4, c(14.5, 15.2, 14.4, 15.0, 4))), Some(0.0));
        // close5 = 15.05 > 15.0 -> pattern.
        assert_eq!(run(with(4, c(14.5, 15.2, 14.4, 15.05, 4))), Some(1.0));
    }

    #[test]
    fn every_condition_is_required() {
        assert_eq!(run(base()), Some(1.0));
        // bar1 white.
        assert_eq!(run(with(0, c(18.5, 20.1, 18.4, 19.0, 0))), Some(0.0));
        // bar2 white.
        assert_eq!(run(with(1, c(17.0, 18.1, 15.9, 17.5, 1))), Some(0.0));
        // bar3 white.
        assert_eq!(run(with(2, c(14.5, 15.5, 13.9, 15.0, 2))), Some(0.0));
        // open2 >= open1.
        assert_eq!(run(with(1, c(20.5, 20.6, 15.9, 16.0, 1))), Some(0.0));
        // close2 >= close1.
        assert_eq!(run(with(1, c(19.0, 19.1, 18.4, 18.5, 1))), Some(0.0));
        // open3 >= open2.
        assert_eq!(run(with(2, c(18.5, 18.6, 13.9, 14.0, 2))), Some(0.0));
        // close3 >= close2.
        assert_eq!(run(with(2, c(17.0, 17.1, 16.4, 16.5, 2))), Some(0.0));
        // bar4 white.
        assert_eq!(run(with(3, c(12.5, 15.0, 12.4, 14.0, 3))), Some(0.0));
        // bar5 black.
        assert_eq!(run(with(4, c(17.0, 17.1, 14.9, 16.0, 4))), Some(0.0));
        // open5 <= open4.
        assert_eq!(run(with(4, c(13.5, 17.1, 13.4, 17.0, 4))), Some(0.0));
    }

    #[test]
    fn first_value_lands_exactly_at_warmup_index() {
        let mut t = LadderBottom::new();
        let out = t.batch(&base());
        let warm = t.warmup_period();
        assert!(out[..warm - 1].iter().all(Option::is_none));
        assert_eq!(out[warm - 1], Some(1.0));
    }

    fn mixed_series() -> Vec<Candle> {
        let mut candles: Vec<Candle> = Vec::new();
        for round in 0..6 {
            for (k, bar) in base().iter().enumerate() {
                let ts = i64::try_from(round * 5 + k).unwrap();
                candles.push(Candle::new(bar.open, bar.high, bar.low, bar.close, 1.0, ts).unwrap());
            }
        }
        candles
    }

    #[test]
    fn reset_replays_identically() {
        let candles = mixed_series();
        let fresh = LadderBottom::new().batch(&candles);
        let mut t = LadderBottom::new();
        let _ = t.batch(&candles);
        t.reset();
        assert_eq!(t.batch(&candles), fresh);
    }

    #[test]
    fn batch_nan_into_matches_streaming_bits() {
        let candles = mixed_series();
        let mut t = LadderBottom::new();
        let streamed: Vec<f64> = candles
            .iter()
            .map(|x| t.update(*x).unwrap_or(f64::NAN))
            .collect();
        let mut out = vec![0.0; candles.len()];
        LadderBottom::new().batch_nan_into(&candles, &mut out);
        assert!(streamed
            .iter()
            .zip(&out)
            .all(|(a, b)| a.to_bits() == b.to_bits()));
        assert!(streamed.contains(&1.0));
    }
}
