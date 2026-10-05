//! Better Volume (Barry Taylor, emini-watch) — volume-bar classification.

use std::collections::VecDeque;

use crate::error::{Error, Result};
use crate::ohlcv::Candle;
use crate::traits::Indicator;

/// Code of a bar with no notable volume signature.
const BETTER_VOLUME_NEUTRAL: f64 = 0.0;
/// Code of a low-volume bar (lowest volume of the lookback).
const BETTER_VOLUME_LOW: f64 = 1.0;
/// Code of a high-churn bar (highest volume per unit of range).
const BETTER_VOLUME_CHURN: f64 = 2.0;
/// Code of a climax bar (highest volume · range); `+3` on an up bar, `−3` on a
/// down bar.
const BETTER_VOLUME_CLIMAX: f64 = 3.0;
/// Code of a bar that is both a climax and a high-churn bar.
const BETTER_VOLUME_CLIMAX_CHURN: f64 = 4.0;

/// Better Volume — Barry Taylor's (emini-watch) classification of each volume
/// bar by its effort (volume) against its result (range) over a `period`-bar
/// lookback.
///
/// ```text
/// range      = high − low
/// low volume : volume          is a new low  of the lookback   -> 1
/// climax     : volume · range  is a new high of the lookback   -> +3 up bar, −3 down bar
/// churn      : volume / range  is a new high of the lookback   -> 2
/// both       : climax and churn on the same bar                -> 4
/// otherwise                                                    -> 0
/// ```
///
/// The rules are applied in that order and a later match overrides an earlier
/// one, as in Taylor's original. "New high / low" means strictly beyond every one
/// of the previous `period − 1` bars, so a run of identical bars stays neutral.
/// A zero-range bar has no defined volume per range and never counts as churn.
///
/// Reading the codes (Volume Spread Analysis): a **climax** is the most effort
/// meeting the widest result — often the start or end of a move; **churn** is
/// heavy volume going nowhere — professional absorption near turning points; and
/// a **low-volume** bar shows the absence of interest that marks pullbacks and
/// tests.
///
/// `Input = Candle`, `Output = f64` (one of `0, 1, 2, ±3, 4`),
/// `warmup_period == period`.
///
/// # Example
///
/// ```
/// use wickra_core::{BetterVolume, Candle, Indicator};
///
/// let mut bv = BetterVolume::new(3).unwrap();
/// bv.update(Candle::new(100.0, 101.0, 99.0, 100.5, 1_000.0, 0).unwrap());
/// bv.update(Candle::new(100.5, 101.5, 99.5, 101.0, 1_000.0, 1).unwrap());
/// // A wide up bar with the most volume · range, but not the most volume per
/// // point of range: a climax up bar.
/// let out = bv.update(Candle::new(101.0, 105.0, 100.5, 104.5, 2_000.0, 2).unwrap());
/// assert_eq!(out, Some(3.0));
/// ```
#[derive(Debug, Clone)]
pub struct BetterVolume {
    period: usize,
    /// `(volume, volume · range, volume / range)` of the last `period` bars;
    /// the last entry is `None` on a zero-range bar.
    window: VecDeque<(f64, f64, Option<f64>)>,
    last: Option<f64>,
}

impl BetterVolume {
    /// Construct a new Better Volume classifier with the given lookback `period`.
    ///
    /// # Errors
    ///
    /// Returns [`Error::PeriodZero`] if `period == 0`.
    pub fn new(period: usize) -> Result<Self> {
        if period == 0 {
            return Err(Error::PeriodZero);
        }
        if period > crate::error::MAX_PERIOD {
            return Err(Error::InvalidPeriod {
                message: crate::error::PERIOD_ABOVE_MAX,
            });
        }
        Ok(Self {
            period,
            window: VecDeque::with_capacity(period),
            last: None,
        })
    }

    /// Configured lookback period.
    pub const fn period(&self) -> usize {
        self.period
    }

    /// Current value if available.
    pub const fn value(&self) -> Option<f64> {
        self.last
    }
}

impl Indicator for BetterVolume {
    type Input = Candle;
    type Output = f64;

    #[inline]
    fn update(&mut self, candle: Candle) -> Option<f64> {
        let range = candle.high - candle.low;
        let volume = candle.volume;
        let churn = (range > 0.0).then(|| volume / range);
        if self.window.len() == self.period {
            self.window.pop_front();
        }
        self.window.push_back((volume, volume * range, churn));
        if self.window.len() < self.period {
            return None;
        }
        // Extremes over the previous `period − 1` bars (the current bar excluded).
        let previous = self.window.iter().take(self.period - 1);
        let (mut min_vol, mut max_climax, mut max_churn) =
            (f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY);
        for &(v, c, ch) in previous {
            min_vol = min_vol.min(v);
            max_climax = max_climax.max(c);
            if let Some(ch) = ch {
                max_churn = max_churn.max(ch);
            }
        }
        let is_climax = volume * range > max_climax;
        let is_churn = churn.is_some_and(|ch| ch > max_churn);
        let mut code = BETTER_VOLUME_NEUTRAL;
        if volume < min_vol {
            code = BETTER_VOLUME_LOW;
        }
        if is_climax {
            code = if candle.close >= candle.open {
                BETTER_VOLUME_CLIMAX
            } else {
                -BETTER_VOLUME_CLIMAX
            };
        }
        if is_churn {
            code = BETTER_VOLUME_CHURN;
        }
        if is_climax && is_churn {
            code = BETTER_VOLUME_CLIMAX_CHURN;
        }
        self.last = Some(code);
        Some(code)
    }

    fn reset(&mut self) {
        self.window.clear();
        self.last = None;
    }

    #[inline]
    fn warmup_period(&self) -> usize {
        self.period
    }

    #[inline]
    fn is_ready(&self) -> bool {
        self.last.is_some()
    }

    #[inline]
    fn name(&self) -> &'static str {
        "BetterVolume"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;

    /// A bar from `low` to `high`, opening at `open` and closing at `close`.
    fn bar(open: f64, high: f64, low: f64, close: f64, volume: f64) -> Candle {
        Candle::new_unchecked(open, high, low, close, volume, 0)
    }

    fn steady() -> Candle {
        bar(100.0, 102.0, 100.0, 101.0, 1_000.0)
    }

    #[test]
    fn rejects_zero_period() {
        assert!(matches!(BetterVolume::new(0), Err(Error::PeriodZero)));
    }

    #[test]
    fn accessors_and_metadata() {
        let bv = BetterVolume::new(20).unwrap();
        assert_eq!(bv.period(), 20);
        assert_eq!(bv.warmup_period(), 20);
        assert_eq!(bv.name(), "BetterVolume");
        assert!(!bv.is_ready());
        assert_eq!(bv.value(), None);
    }

    #[test]
    fn first_emission_at_warmup_period() {
        let mut bv = BetterVolume::new(3).unwrap();
        let out = bv.batch(&[steady(), steady(), steady()]);
        assert!(out[0].is_none() && out[1].is_none());
        assert!(out[2].is_some());
    }

    #[test]
    fn steady_bars_are_neutral() {
        let mut bv = BetterVolume::new(5).unwrap();
        let out = bv.batch(&[steady(); 20]);
        assert_eq!(out.last().copied().flatten(), Some(BETTER_VOLUME_NEUTRAL));
    }

    #[test]
    fn low_volume_bar() {
        let mut bv = BetterVolume::new(3).unwrap();
        bv.batch(&[steady(), steady()]);
        let v = bv.update(bar(100.0, 102.0, 100.0, 101.0, 200.0));
        assert_eq!(v, Some(BETTER_VOLUME_LOW));
    }

    #[test]
    fn climax_bars_carry_direction() {
        let mut up = BetterVolume::new(3).unwrap();
        up.batch(&[steady(), steady()]);
        // Wide range and heavy volume, but less volume per point than before.
        assert_eq!(
            up.update(bar(100.0, 110.0, 100.0, 109.0, 4_000.0)),
            Some(BETTER_VOLUME_CLIMAX)
        );
        let mut down = BetterVolume::new(3).unwrap();
        down.batch(&[steady(), steady()]);
        assert_eq!(
            down.update(bar(109.0, 110.0, 100.0, 100.5, 4_000.0)),
            Some(-BETTER_VOLUME_CLIMAX)
        );
    }

    #[test]
    fn churn_bar() {
        let mut bv = BetterVolume::new(3).unwrap();
        bv.batch(&[steady(), steady()]);
        // Same range, a bit more volume -> more volume per point, but a
        // narrower bar keeps volume · range below the previous highs.
        assert_eq!(
            bv.update(bar(100.0, 101.0, 100.0, 100.5, 1_500.0)),
            Some(BETTER_VOLUME_CHURN)
        );
    }

    #[test]
    fn climax_and_churn_together() {
        let mut bv = BetterVolume::new(3).unwrap();
        bv.batch(&[steady(), steady()]);
        assert_eq!(
            bv.update(bar(100.0, 103.0, 100.0, 102.0, 10_000.0)),
            Some(BETTER_VOLUME_CLIMAX_CHURN)
        );
    }

    #[test]
    fn zero_range_bars_never_churn() {
        let mut bv = BetterVolume::new(3).unwrap();
        let flat = bar(100.0, 100.0, 100.0, 100.0, 0.0);
        for v in bv.batch(&[flat; 10]).into_iter().flatten() {
            assert_eq!(v, BETTER_VOLUME_NEUTRAL);
        }
    }

    #[test]
    fn reset_clears_state() {
        let mut bv = BetterVolume::new(3).unwrap();
        bv.batch(&[steady(); 5]);
        assert!(bv.is_ready());
        bv.reset();
        assert!(!bv.is_ready());
        assert_eq!(bv.value(), None);
        assert_eq!(bv.update(steady()), None);
    }

    #[test]
    fn batch_equals_streaming() {
        let candles: Vec<Candle> = (0..60)
            .map(|i| {
                let f = f64::from(i);
                let mid = 100.0 + (f * 0.3).sin() * 4.0;
                let half = 1.0 + (f * 0.7).cos().abs() * 2.0;
                bar(
                    mid - 0.2,
                    mid + half,
                    mid - half,
                    mid + 0.2,
                    1_000.0 + (f * 0.5).sin() * 600.0,
                )
            })
            .collect();
        let mut a = BetterVolume::new(10).unwrap();
        let mut b = BetterVolume::new(10).unwrap();
        let batch = a.batch(&candles);
        let streamed: Vec<_> = candles.iter().map(|c| b.update(*c)).collect();
        assert_eq!(batch, streamed);
    }

    #[test]
    fn rejects_oversized_period() {
        let too_long = crate::error::MAX_PERIOD + 1;
        assert!(matches!(
            BetterVolume::new(too_long),
            Err(Error::InvalidPeriod { .. })
        ));
    }

    fn mixed(len: i32) -> Vec<Candle> {
        (0..len)
            .map(|i| {
                let f = f64::from(i);
                let mid = 100.0 + (f * 0.3).sin() * 4.0;
                let half = (f * 0.7).cos().abs() * 2.0;
                let drift = (f * 1.1).sin() * 0.5;
                bar(
                    mid - drift,
                    mid + half,
                    mid - half,
                    mid + drift,
                    1_000.0 + (f * 0.5).sin() * 600.0,
                )
            })
            .collect()
    }

    #[test]
    fn first_value_lands_exactly_at_warmup_index() {
        let candles = mixed(30);
        let mut bv = BetterVolume::new(7).unwrap();
        let warmup = bv.warmup_period();
        let out = bv.batch(&candles);
        assert!(out.iter().take(warmup - 1).all(Option::is_none));
        assert!(out.iter().skip(warmup - 1).all(Option::is_some));
    }

    #[test]
    fn reset_replays_identically_to_fresh_instance() {
        let candles = mixed(60);
        let mut used = BetterVolume::new(10).unwrap();
        used.batch(&candles);
        used.reset();
        let replay = used.batch(&candles);
        assert_eq!(replay, BetterVolume::new(10).unwrap().batch(&candles));
    }

    #[test]
    fn batch_nan_into_matches_streaming_bits() {
        let candles = mixed(80);
        let mut nan_out = vec![0.0; candles.len()];
        BetterVolume::new(6)
            .unwrap()
            .batch_nan_into(&candles, &mut nan_out);
        let mut streamer = BetterVolume::new(6).unwrap();
        let identical = candles.iter().zip(&nan_out).all(|(candle, v)| {
            streamer.update(*candle).unwrap_or(f64::NAN).to_bits() == v.to_bits()
        });
        assert!(identical);
    }

    /// Hand-computed: two `steady` bars have volume `1000`, range `2`,
    /// `volume · range = 2000`, `volume / range = 500`. The third bar has volume
    /// `900`, range `3`: `900 < 1000` (low volume, code 1), but
    /// `900 · 3 = 2700 > 2000` (climax) overrides it; `900 / 3 = 300 < 500` is no
    /// churn. It closes above its open, so the code is `+3`.
    #[test]
    fn climax_overrides_low_volume() {
        let mut bv = BetterVolume::new(3).unwrap();
        bv.batch(&[steady(), steady()]);
        assert_eq!(
            bv.update(bar(100.0, 103.0, 100.0, 102.0, 900.0)),
            Some(BETTER_VOLUME_CLIMAX)
        );
    }

    /// Hand-computed: churn overrides low volume. Volume `600`, range `1`:
    /// `600 < 1000` (low), `600 · 1 = 600 < 2000` (no climax),
    /// `600 / 1 = 600 > 500` (churn) -> code 2.
    #[test]
    fn churn_overrides_low_volume() {
        let mut bv = BetterVolume::new(3).unwrap();
        bv.batch(&[steady(), steady()]);
        assert_eq!(
            bv.update(bar(100.0, 101.0, 100.0, 100.5, 600.0)),
            Some(BETTER_VOLUME_CHURN)
        );
    }

    #[test]
    fn climax_on_unchanged_close_counts_as_up_bar() {
        let mut bv = BetterVolume::new(3).unwrap();
        bv.batch(&[steady(), steady()]);
        assert_eq!(
            bv.update(bar(105.0, 110.0, 100.0, 105.0, 4_000.0)),
            Some(BETTER_VOLUME_CLIMAX)
        );
    }

    #[test]
    fn ties_with_previous_extremes_stay_neutral() {
        // Equal volume is not a new low, equal `volume · range` not a new climax,
        // equal `volume / range` not new churn: strictly-beyond comparisons.
        let mut bv = BetterVolume::new(4).unwrap();
        bv.batch(&[
            steady(),
            bar(100.0, 104.0, 100.0, 103.0, 1_000.0),
            bar(100.0, 101.0, 100.0, 100.5, 2_000.0),
        ]);
        // min volume 1000, max climax 4000, max churn 2000.
        assert_eq!(
            bv.update(bar(100.0, 104.0, 100.0, 103.0, 1_000.0)),
            Some(BETTER_VOLUME_NEUTRAL)
        );
    }

    #[test]
    fn every_code_from_one_rolling_window() {
        // A single `period = 3` stream that walks through all six codes.
        let mut bv = BetterVolume::new(3).unwrap();
        let candles = [
            steady(),
            steady(),
            steady(),                                  // 0: ties everywhere
            bar(100.0, 102.0, 100.0, 101.0, 500.0),    // 1: vol 500 < 1000
            bar(100.0, 101.0, 100.0, 100.5, 1_200.0),  // 2: churn 1200 > 500, climax 1200 < 2000
            bar(100.0, 110.0, 100.0, 109.0, 1_000.0),  // +3: climax 10000, churn 100
            bar(109.0, 110.0, 90.0, 91.0, 1_000.0),    // −3: climax 20000, churn 50
            bar(100.0, 104.0, 100.0, 103.0, 50_000.0), // 4: climax 200000, churn 12500
        ];
        let out: Vec<Option<f64>> = bv.batch(&candles);
        let expected = [
            None,
            None,
            Some(0.0),
            Some(1.0),
            Some(2.0),
            Some(3.0),
            Some(-3.0),
            Some(4.0),
        ];
        assert_eq!(out, expected);
    }

    #[test]
    fn period_one_compares_against_an_empty_lookback() {
        // With no previous bars every extreme is unbeaten: a ranged bar is both a
        // climax and churn (4); a zero-range bar is a climax only (±3).
        let mut bv = BetterVolume::new(1).unwrap();
        assert_eq!(bv.update(steady()), Some(BETTER_VOLUME_CLIMAX_CHURN));
        assert_eq!(
            bv.update(bar(100.0, 100.0, 100.0, 100.0, 10.0)),
            Some(BETTER_VOLUME_CLIMAX)
        );
    }
}
