#![allow(clippy::doc_markdown)]

//! Tristar — a three-doji reversal pattern.
//!
//! A Tristar is three consecutive Doji candles where the middle one gaps away
//! from its neighbours, forming a star. A bearish Tristar (top) has the middle
//! doji sitting above the other two; a bullish Tristar (bottom) has it below.
//!
//! - **Bearish** (`-1.0`): three dojis, the middle doji's body gaps above the
//!   first (`min(o2, c2) > max(o1, c1)`) and the third does not reach higher
//!   (`max(o3, c3) < max(o2, c2)`).
//! - **Bullish** (`+1.0`): three dojis, the middle doji's body gaps below the
//!   first (`max(o2, c2) < min(o1, c1)`) and the third does not reach lower
//!   (`min(o3, c3) > min(o2, c2)`).
//! - Otherwise the output is `0.0`.
//!
//! The body gap of the middle doji is the defining feature (Nison; TA-Lib
//! `CDLTRISTAR`). A doji is a candle whose body is `<= 0.1 * range`. The
//! three-bar lookback means
//! the first value lands on the third candle.

use crate::ohlcv::Candle;
use crate::traits::Indicator;

/// Top of a candle's real body.
fn body_top(candle: Candle) -> f64 {
    candle.open.max(candle.close)
}

/// Bottom of a candle's real body.
fn body_bottom(candle: Candle) -> f64 {
    candle.open.min(candle.close)
}

/// Whether a candle is a doji (body small relative to range).
fn is_doji(candle: Candle) -> bool {
    let body = (candle.close - candle.open).abs();
    let range = candle.high - candle.low;
    range > 0.0 && body <= 0.1 * range
}

/// Tristar — three-doji star reversal detector.
/// # Example
///
/// ```
/// use wickra_core::{Tristar, Candle, Indicator};
///
/// let mut indicator = Tristar::new();
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
pub struct Tristar {
    c1: Option<Candle>,
    c2: Option<Candle>,
    last_value: Option<f64>,
}

impl Tristar {
    /// Construct a new `Tristar`.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Latest emitted signal if available.
    pub const fn value(&self) -> Option<f64> {
        self.last_value
    }
}

impl Indicator for Tristar {
    type Input = Candle;
    type Output = f64;

    #[inline]
    fn update(&mut self, candle: Candle) -> Option<f64> {
        let (Some(first), Some(middle)) = (self.c1, self.c2) else {
            self.c1 = self.c2;
            self.c2 = Some(candle);
            self.last_value = None;
            return None;
        };
        let v = if is_doji(first) && is_doji(middle) && is_doji(candle) {
            if body_bottom(middle) > body_top(first) && body_top(candle) < body_top(middle) {
                -1.0
            } else if body_top(middle) < body_bottom(first)
                && body_bottom(candle) > body_bottom(middle)
            {
                1.0
            } else {
                0.0
            }
        } else {
            0.0
        };
        self.c1 = self.c2;
        self.c2 = Some(candle);
        self.last_value = Some(v);
        Some(v)
    }

    fn reset(&mut self) {
        self.c1 = None;
        self.c2 = None;
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
        "Tristar"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;

    /// A doji centred at `mid` (tiny body, symmetric shadows).
    fn doji(mid: f64) -> Candle {
        Candle::new_unchecked(mid, mid + 1.0, mid - 1.0, mid + 0.02, 0.0, 0)
    }

    /// A non-doji (big body).
    fn solid(open: f64, close: f64) -> Candle {
        Candle::new_unchecked(
            open,
            open.max(close) + 0.1,
            open.min(close) - 0.1,
            close,
            0.0,
            0,
        )
    }

    #[test]
    fn accessors_and_metadata() {
        let t = Tristar::new();
        assert_eq!(t.warmup_period(), 3);
        assert_eq!(t.name(), "Tristar");
        assert!(!t.is_ready());
        assert_eq!(t.value(), None);
    }

    #[test]
    fn first_two_bars_seed_without_signal() {
        let mut t = Tristar::new();
        assert_eq!(t.update(doji(100.0)), None);
        assert_eq!(t.update(doji(100.0)), None);
        assert!(t.update(doji(100.0)).is_some());
    }

    #[test]
    fn bearish_tristar_top() {
        // middle doji centred above the two neighbours -> top -> -1.
        let mut t = Tristar::new();
        t.update(doji(100.0));
        t.update(doji(105.0)); // middle, highest
        assert_eq!(t.update(doji(100.0)), Some(-1.0));
    }

    #[test]
    fn bullish_tristar_bottom() {
        let mut t = Tristar::new();
        t.update(doji(100.0));
        t.update(doji(95.0)); // middle, lowest
        assert_eq!(t.update(doji(100.0)), Some(1.0));
    }

    #[test]
    fn non_doji_is_zero() {
        let mut t = Tristar::new();
        t.update(doji(100.0));
        t.update(solid(100.0, 110.0)); // not a doji
        assert_eq!(t.update(doji(100.0)), Some(0.0));
    }

    #[test]
    fn reset_clears_state() {
        let mut t = Tristar::new();
        t.update(doji(100.0));
        t.update(doji(105.0));
        t.update(doji(100.0));
        assert!(t.is_ready());
        t.reset();
        assert!(!t.is_ready());
        assert_eq!(t.update(doji(100.0)), None);
    }

    #[test]
    fn batch_equals_streaming() {
        let candles: Vec<Candle> = (0..40)
            .map(|i| doji(100.0 + (f64::from(i) * 0.4).sin() * 5.0))
            .collect();
        let batch = Tristar::new().batch(&candles);
        let mut b = Tristar::new();
        let streamed: Vec<_> = candles.iter().map(|x| b.update(*x)).collect();
        assert_eq!(batch, streamed);
    }

    /// A doji with an explicit body `open -> close` and a range of 2.
    fn doji_body(open: f64, close: f64) -> Candle {
        let mid = f64::midpoint(open, close);
        Candle::new_unchecked(open, mid + 1.0, mid - 1.0, close, 0.0, 0)
    }

    fn run(bars: [Candle; 3]) -> Option<f64> {
        let mut t = Tristar::new();
        bars.iter().map(|b| t.update(*b)).last().unwrap()
    }

    #[test]
    fn hand_computed_body_gap_with_overlapping_shadows() {
        // doji(100): body 100..100.02, shadows 99..101; doji(100.5): body
        // 100.5..100.52. Shadows overlap but body bottom 100.5 > top 100.02,
        // and the third top 100.12 < 100.52 -> bearish.
        assert_eq!(run([doji(100.0), doji(100.5), doji(100.1)]), Some(-1.0));
        // Mirror: doji(99.5) body top 99.52 < 100 = first body bottom,
        // third bottom 99.9 > 99.5 -> bullish.
        assert_eq!(run([doji(100.0), doji(99.5), doji(99.9)]), Some(1.0));
    }

    #[test]
    fn bearish_gap_rules() {
        // Middle body bottom equal to the first body top: no gap.
        let first = doji_body(100.0, 100.02);
        assert_eq!(
            run([first, doji_body(100.02, 100.04), doji(100.0)]),
            Some(0.0)
        );
        // Third doji reaches higher than the middle body top.
        assert_eq!(run([doji(100.0), doji(105.0), doji(105.5)]), Some(0.0));
        // Third doji's body top equal to the middle body top.
        assert_eq!(run([doji(100.0), doji(105.0), doji(105.0)]), Some(0.0));
    }

    #[test]
    fn bullish_gap_rules() {
        // Middle body top equal to the first body bottom: no gap.
        let first = doji_body(100.0, 100.02);
        assert_eq!(
            run([first, doji_body(99.98, 100.0), doji(100.0)]),
            Some(0.0)
        );
        // Third doji reaches lower than the middle body bottom.
        assert_eq!(run([doji(100.0), doji(95.0), doji(94.5)]), Some(0.0));
        // Third doji's body bottom equal to the middle body bottom.
        assert_eq!(run([doji(100.0), doji(95.0), doji(95.0)]), Some(0.0));
    }

    #[test]
    fn every_bar_must_be_a_doji() {
        assert_eq!(
            run([solid(100.0, 101.0), doji(105.0), doji(100.0)]),
            Some(0.0)
        );
        assert_eq!(
            run([doji(100.0), doji(105.0), solid(100.0, 99.0)]),
            Some(0.0)
        );
        assert_eq!(
            run([doji(100.0), doji(95.0), solid(100.0, 101.0)]),
            Some(0.0)
        );
        // A zero-range bar is never a doji.
        let flat = Candle::new_unchecked(105.0, 105.0, 105.0, 105.0, 0.0, 0);
        assert_eq!(run([doji(100.0), flat, doji(100.0)]), Some(0.0));
    }

    #[test]
    fn doji_threshold_is_ten_percent_of_range() {
        // range 2.5, 0.1 * 2.5 = 0.25: body 0.25 is a doji, body 0.5 is not.
        assert!(is_doji(Candle::new_unchecked(
            100.0, 101.25, 98.75, 100.25, 0.0, 0
        )));
        assert!(!is_doji(Candle::new_unchecked(
            100.0, 101.25, 98.75, 100.5, 0.0, 0
        )));
        assert!(!is_doji(Candle::new_unchecked(
            100.0, 100.0, 100.0, 100.0, 0.0, 0
        )));
    }

    #[test]
    fn first_value_lands_exactly_at_warmup_index() {
        let mut t = Tristar::new();
        let out = t.batch(&[doji(100.0), doji(105.0), doji(100.0), doji(100.0)]);
        let warm = t.warmup_period();
        assert!(out[..warm - 1].iter().all(Option::is_none));
        assert_eq!(out[warm - 1], Some(-1.0));
        assert_eq!(t.value(), Some(0.0));
    }

    fn mixed_series() -> Vec<Candle> {
        let bars = [
            doji(100.0),
            doji(105.0),
            doji(100.0),
            doji(95.0),
            doji(100.0),
            solid(100.0, 104.0),
        ];
        bars.iter().cycle().take(30).copied().collect()
    }

    #[test]
    fn reset_replays_identically() {
        let candles = mixed_series();
        let fresh = Tristar::new().batch(&candles);
        let mut t = Tristar::new();
        let _ = t.batch(&candles);
        t.reset();
        assert_eq!(t.value(), None);
        assert_eq!(t.batch(&candles), fresh);
    }

    #[test]
    fn batch_nan_into_matches_streaming_bits() {
        let candles = mixed_series();
        let mut t = Tristar::new();
        let streamed: Vec<f64> = candles
            .iter()
            .map(|x| t.update(*x).unwrap_or(f64::NAN))
            .collect();
        let mut out = vec![0.0; candles.len()];
        Tristar::new().batch_nan_into(&candles, &mut out);
        assert!(streamed
            .iter()
            .zip(&out)
            .all(|(a, b)| a.to_bits() == b.to_bits()));
        assert!(streamed.contains(&1.0) && streamed.contains(&-1.0));
    }
}
