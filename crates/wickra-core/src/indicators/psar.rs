//! Parabolic SAR (Wilder).

use crate::error::{Error, Result};
use crate::ohlcv::Candle;
use crate::traits::Indicator;

/// Trade direction in the SAR state machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Trend {
    Up,
    Down,
}

/// Parabolic Stop And Reverse.
///
/// Implementation follows Wilder's original recursion: each step computes a new
/// SAR from the previous SAR, extreme point (EP) and acceleration factor (AF);
/// the trend flips when price crosses the SAR.
///
/// # Example
///
/// ```
/// use wickra_core::{Candle, Indicator, Psar};
///
/// let mut indicator = Psar::new(0.02, 0.02, 0.2).unwrap();
/// let mut last = None;
/// for i in 0..80 {
///     let base = 100.0 + f64::from(i);
///     let candle =
///         Candle::new(base, base + 2.0, base - 2.0, base + 1.0, 10.0, i64::from(i)).unwrap();
///     last = indicator.update(candle);
/// }
/// assert!(last.is_some());
/// ```
#[derive(Debug, Clone)]
pub struct Psar {
    af_start: f64,
    af_step: f64,
    af_max: f64,

    /// `true` once the first candle has been observed and the seed values
    /// (`prev_high`, `prev_low`, `sar`, `ep`) are valid. `false` is the
    /// constructor / `reset()` state in which the compute-fields hold
    /// `f64::NAN` sentinels.
    initialised: bool,
    /// `true` once `update` has returned the first `Some(sar)`. Drives
    /// [`Indicator::is_ready`] so it matches the convention of every other
    /// indicator: `is_ready() == true` ↔ the most recent `update` produced
    /// (or could produce) a real value. PSAR's seed candle returns `None`
    /// while `initialised` flips to `true`, which is why `is_ready` cannot
    /// just mirror `initialised`.
    has_emitted: bool,
    prev_high: f64,
    prev_low: f64,
    prev2_high: f64,
    prev2_low: f64,
    trend: Trend,
    sar: f64,
    ep: f64,
    af: f64,
}

impl Psar {
    /// Construct PSAR with explicit acceleration parameters.
    ///
    /// # Errors
    /// Returns [`Error::NonPositiveMultiplier`] / [`Error::InvalidPeriod`] for invalid params.
    pub fn new(af_start: f64, af_step: f64, af_max: f64) -> Result<Self> {
        if !af_start.is_finite() || !af_step.is_finite() || !af_max.is_finite() {
            return Err(Error::NonPositiveMultiplier);
        }
        if af_start <= 0.0 || af_step <= 0.0 || af_max <= 0.0 {
            return Err(Error::NonPositiveMultiplier);
        }
        if af_start > af_max {
            return Err(Error::InvalidPeriod {
                message: "af_start must be <= af_max",
            });
        }
        Ok(Self {
            af_start,
            af_step,
            af_max,
            initialised: false,
            has_emitted: false,
            // NaN sentinels: any read of these fields before the seed candle
            // overwrites them is a logic bug. The `initialised` flag gates
            // every read, and the `debug_assert!` in `update` makes the
            // invariant explicit so a future refactor cannot silently treat a
            // sentinel as a real price.
            prev_high: f64::NAN,
            prev_low: f64::NAN,
            prev2_high: f64::NAN,
            prev2_low: f64::NAN,
            trend: Trend::Up,
            sar: f64::NAN,
            ep: f64::NAN,
            af: af_start,
        })
    }

    /// Wilder's defaults: `(0.02, 0.02, 0.20)`.
    pub fn classic() -> Self {
        Self::new(0.02, 0.02, 0.20).expect("classic PSAR params are valid")
    }
}

impl Indicator for Psar {
    type Input = Candle;
    type Output = f64;

    fn update(&mut self, candle: Candle) -> Option<f64> {
        if !self.initialised {
            // The first candle only seeds the state; the first SAR is emitted
            // on the second.
            self.prev_high = candle.high;
            self.prev_low = candle.low;
            self.initialised = true;
            return None;
        }

        let new_sar = if self.has_emitted {
            // Predicted SAR for this period, clamped so it never sits inside
            // the ranges of the two bars before it (Wilder's rule; TA-Lib
            // clamps tomorrow's SAR with today's and yesterday's extremes --
            // the same rule, one bar earlier).
            let predicted = self.sar + self.af * (self.ep - self.sar);
            match self.trend {
                Trend::Up => predicted.min(self.prev_low).min(self.prev2_low),
                Trend::Down => predicted.max(self.prev_high).max(self.prev2_high),
            }
        } else {
            // Second candle: TA-Lib's seed. The direction comes from the
            // one-bar directional movement of the first two candles (short
            // when the down move dominates), the SAR starts at the first
            // candle's opposite extreme and the extreme point at this
            // candle's. TA-Lib's first step treats this candle as both today
            // and yesterday, so it is also the "bar before last" of the next
            // clamp.
            let up_move = candle.high - self.prev_high;
            let down_move = self.prev_low - candle.low;
            if down_move > 0.0 && down_move > up_move {
                self.trend = Trend::Down;
                self.sar = self.prev_high;
                self.ep = candle.low;
            } else {
                self.trend = Trend::Up;
                self.sar = self.prev_low;
                self.ep = candle.high;
            }
            self.prev_high = candle.high;
            self.prev_low = candle.low;
            self.sar
        };
        let prev_h = self.prev_high;
        let prev_l = self.prev_low;

        let mut output_sar = new_sar;

        // Check for trend reversal.
        let reversed = match self.trend {
            Trend::Up => candle.low <= new_sar,
            Trend::Down => candle.high >= new_sar,
        };

        if reversed {
            // Flip trend, reset AF and EP, place SAR at the prior EP -- moved
            // outside this bar's and the previous bar's range if the reversal
            // bar reached past it (TA-Lib's reversal clamp).
            output_sar = match self.trend {
                Trend::Up => self.ep.max(prev_h).max(candle.high),
                Trend::Down => self.ep.min(prev_l).min(candle.low),
            };
            self.trend = match self.trend {
                Trend::Up => Trend::Down,
                Trend::Down => Trend::Up,
            };
            self.ep = match self.trend {
                Trend::Up => candle.high,
                Trend::Down => candle.low,
            };
            self.af = self.af_start;
        } else {
            // Update EP and AF if a new extreme has been reached.
            match self.trend {
                Trend::Up => {
                    if candle.high > self.ep {
                        self.ep = candle.high;
                        self.af = (self.af + self.af_step).min(self.af_max);
                    }
                }
                Trend::Down => {
                    if candle.low < self.ep {
                        self.ep = candle.low;
                        self.af = (self.af + self.af_step).min(self.af_max);
                    }
                }
            }
        }

        self.sar = output_sar;
        self.prev2_high = self.prev_high;
        self.prev2_low = self.prev_low;
        self.prev_high = candle.high;
        self.prev_low = candle.low;
        self.has_emitted = true;
        Some(output_sar)
    }

    fn reset(&mut self) {
        // Restore every field to its constructor state. The compute fields
        // return to `f64::NAN` sentinels so a future refactor that reads them
        // before re-seeding cannot silently treat `0.0` as a real price.
        self.initialised = false;
        self.has_emitted = false;
        self.prev_high = f64::NAN;
        self.prev_low = f64::NAN;
        self.prev2_high = f64::NAN;
        self.prev2_low = f64::NAN;
        self.trend = Trend::Up;
        self.sar = f64::NAN;
        self.ep = f64::NAN;
        self.af = self.af_start;
    }

    #[inline]
    fn warmup_period(&self) -> usize {
        2
    }

    #[inline]
    fn is_ready(&self) -> bool {
        // Match the convention of every other indicator: `is_ready` flips to
        // `true` only once a real value has been returned. The previous
        // implementation returned `self.initialised`, which is `true` *after*
        // the seed candle (which itself returns `None`) — so a streaming
        // consumer that wrote `if ind.is_ready() { use(ind.update(c)?) }`
        // would hit a `None` it didn't expect. (Audit finding R6.)
        self.has_emitted
    }

    #[inline]
    fn name(&self) -> &'static str {
        "PSAR"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;

    fn c(h: f64, l: f64, cl: f64) -> Candle {
        Candle::new(cl, h, l, cl, 1.0, 0).unwrap()
    }

    #[test]
    fn first_candle_returns_none() {
        let mut psar = Psar::classic();
        assert_eq!(psar.update(c(11.0, 9.0, 10.0)), None);
    }

    #[test]
    fn pure_uptrend_sar_below_lows() {
        let candles: Vec<Candle> = (0..40)
            .map(|i| {
                let base = 100.0 + f64::from(i);
                c(base + 0.5, base - 0.5, base)
            })
            .collect();
        let mut psar = Psar::classic();
        // `all()` with `is_none_or` keeps every reachable arm on the hot path —
        // the previous filter_map / violation-Vec construction had a cold
        // "violation found" tuple branch that was unreachable on a clean
        // uptrend, leaving its line uncovered by Codecov.
        let ok = psar
            .batch(&candles)
            .iter()
            .enumerate()
            .all(|(i, sar)| sar.is_none_or(|s| s <= candles[i].low + 1e-9));
        assert!(ok, "SAR sat above a candle's low on a pure uptrend");
    }

    #[test]
    fn pure_downtrend_sar_above_highs() {
        let candles: Vec<Candle> = (0..40)
            .rev()
            .map(|i| {
                let base = 100.0 + f64::from(i);
                c(base + 0.5, base - 0.5, base)
            })
            .collect();
        let mut psar = Psar::classic();
        // After the trend establishes downward, SAR should sit above highs.
        // Same `all()` + `is_none_or` shape as `pure_uptrend_sar_below_lows`
        // so the violation-tuple branch never appears as a cold path.
        let ok = psar
            .batch(&candles)
            .iter()
            .enumerate()
            .skip(5)
            .all(|(i, sar)| sar.is_none_or(|s| s >= candles[i].high - 1e-9));
        assert!(ok, "SAR sat below a candle's high on a pure downtrend");
    }

    #[test]
    fn batch_equals_streaming() {
        let candles: Vec<Candle> = (0..60)
            .map(|i| {
                let m = 100.0 + (f64::from(i) * 0.3).sin() * 8.0;
                c(m + 1.0, m - 1.0, m)
            })
            .collect();
        let mut a = Psar::classic();
        let mut b = Psar::classic();
        assert_eq!(
            a.batch(&candles),
            candles.iter().map(|x| b.update(*x)).collect::<Vec<_>>()
        );
    }

    /// Cover the Indicator-impl `warmup_period` (206-208) and `name`
    /// (220-222). PSAR's warmup is the constant 2 (seed candle + first
    /// emitting candle); the name is the literal "PSAR".
    #[test]
    fn accessors_and_metadata() {
        let psar = Psar::classic();
        assert_eq!(psar.warmup_period(), 2);
        assert_eq!(psar.name(), "PSAR");
    }

    #[test]
    fn rejects_invalid_params() {
        assert!(Psar::new(0.0, 0.02, 0.20).is_err());
        assert!(Psar::new(0.02, 0.0, 0.20).is_err());
        assert!(Psar::new(0.30, 0.02, 0.20).is_err());
        assert!(Psar::new(f64::NAN, 0.02, 0.20).is_err());
    }

    #[test]
    fn is_ready_only_after_first_some_value() {
        // Audit R6: the previous implementation flipped `is_ready` to true on
        // the seed candle (which returns `None`), making the convention
        // `is_ready == last_value.is_some()` a lie. The new gate is
        // `has_emitted`, set when `update` returns its first `Some`.
        let mut psar = Psar::classic();
        assert!(!psar.is_ready(), "fresh PSAR must not be ready");
        let first = psar.update(c(11.0, 9.0, 10.0));
        assert!(first.is_none(), "seed candle returns None by design");
        assert!(
            !psar.is_ready(),
            "is_ready must stay false until a Some value is produced"
        );
        let second = psar.update(c(12.0, 10.0, 11.0));
        assert!(second.is_some(), "second candle must emit");
        assert!(
            psar.is_ready(),
            "is_ready must flip to true once a real value has been returned"
        );
    }

    #[test]
    fn reset_allows_clean_reuse() {
        let candles: Vec<Candle> = (0..40)
            .map(|i| {
                let base = 100.0 + f64::from(i);
                c(base + 0.5, base - 0.5, base)
            })
            .collect();
        let mut psar = Psar::classic();
        let first = psar.batch(&candles);
        assert!(psar.is_ready());
        psar.reset();
        assert!(!psar.is_ready());
        // A reset instance must reproduce a pristine run bit for bit.
        let second = psar.batch(&candles);
        assert_eq!(first, second);
    }

    fn hl(high: f64, low: f64) -> Candle {
        c(high, low, f64::midpoint(high, low))
    }

    fn run(bars: &[(f64, f64)]) -> Vec<Option<f64>> {
        let candles: Vec<Candle> = bars.iter().map(|&(h, l)| hl(h, l)).collect();
        Psar::classic().batch(&candles)
    }

    fn assert_series(got: &[Option<f64>], expected: &[Option<f64>]) {
        assert_eq!(got.len(), expected.len());
        for (g, e) in got.iter().zip(expected) {
            assert_eq!(g.is_some(), e.is_some());
            if let (Some(g), Some(e)) = (g, e) {
                approx::assert_relative_eq!(*g, *e, epsilon = 1e-12);
            }
        }
    }

    #[test]
    fn rejects_every_invalid_parameter() {
        assert!(matches!(
            Psar::new(f64::NAN, 0.02, 0.2),
            Err(Error::NonPositiveMultiplier)
        ));
        assert!(matches!(
            Psar::new(0.02, f64::INFINITY, 0.2),
            Err(Error::NonPositiveMultiplier)
        ));
        assert!(matches!(
            Psar::new(0.02, 0.02, f64::NAN),
            Err(Error::NonPositiveMultiplier)
        ));
        assert!(matches!(
            Psar::new(-0.02, 0.02, 0.2),
            Err(Error::NonPositiveMultiplier)
        ));
        assert!(matches!(
            Psar::new(0.02, -0.02, 0.2),
            Err(Error::NonPositiveMultiplier)
        ));
        assert!(matches!(
            Psar::new(0.02, 0.02, 0.0),
            Err(Error::NonPositiveMultiplier)
        ));
        assert!(matches!(
            Psar::new(0.3, 0.02, 0.2),
            Err(Error::InvalidPeriod { .. })
        ));
        assert!(Psar::new(0.2, 0.02, 0.2).is_ok());
    }

    #[test]
    fn first_value_lands_at_index_one() {
        let out = run(&[(10.0, 8.0), (11.0, 9.0), (12.0, 10.0)]);
        assert_eq!(Psar::classic().warmup_period(), 2);
        assert!(out[0].is_none());
        assert!(out[1..].iter().all(Option::is_some));
    }

    #[test]
    fn hand_computed_long_seed() {
        // b1: up move 1 > down move −1 -> long. SAR = low0 = 8, EP = high1 = 11.
        //     low 9 > 8, no reversal; high 11 is not above EP: AF stays 0.02.
        // b2: 8 + 0.02·(11 − 8) = 8.06, clamp min(9, 9) keeps 8.06; EP 12, AF 0.04.
        // b3: 8.06 + 0.04·(12 − 8.06) = 8.2176, clamp min(10, 9); EP 13, AF 0.06.
        // b4: 8.2176 + 0.06·(13 − 8.2176) = 8.504544, clamp min(11, 10).
        let out = run(&[
            (10.0, 8.0),
            (11.0, 9.0),
            (12.0, 10.0),
            (13.0, 11.0),
            (14.0, 12.0),
        ]);
        assert_series(
            &out,
            &[None, Some(8.0), Some(8.06), Some(8.2176), Some(8.504_544)],
        );
    }

    #[test]
    fn hand_computed_short_seed() {
        // b1: down move 10 − 9 = 1 > 0 and > up move 11 − 12 = −1 -> short.
        //     SAR = high0 = 12, EP = low1 = 9. high 11 < 12, no reversal.
        // b2: 12 + 0.02·(9 − 12) = 11.94, clamp max(11, 11) keeps it; EP 8, AF 0.04.
        // b3: 11.94 + 0.04·(8 − 11.94) = 11.7824, clamp max(10, 11) keeps it.
        let out = run(&[(12.0, 10.0), (11.0, 9.0), (10.0, 8.0), (9.0, 7.0)]);
        assert_series(&out, &[None, Some(12.0), Some(11.94), Some(11.7824)]);
    }

    #[test]
    fn equal_moves_seed_long() {
        // Outside bar: up move 1 == down move 1 -> not short, so long with
        // SAR 8 / EP 11; low 7 <= 8 reverses immediately to the EP, clamped
        // to max(11, high1 11, high 11) = 11.
        let out = run(&[(10.0, 8.0), (11.0, 7.0)]);
        assert_series(&out, &[None, Some(11.0)]);
    }

    #[test]
    fn hand_computed_immediate_reversal_long_to_short_then_back() {
        // b1: up −1, down 0 (not > 0) -> long, SAR 8, EP 9. low 8 <= 8 reverses:
        //     SAR = max(EP 9, prev high 9 (b1 itself), high 9) = 9; short, EP 8.
        // b2: 9 + 0.02·(8 − 9) = 8.98, clamp max(9, 9) = 9 (b1 is also the bar
        //     before last). high 9.5 >= 9 reverses: SAR = min(EP 8, prev low 8,
        //     low 7.5) = 7.5 (the clamp applies), long, EP 9.5, AF 0.02.
        // b3: 7.5 + 0.02·(9.5 − 7.5) = 7.54, clamp min(7.5, 8) = 7.5.
        let out = run(&[(10.0, 8.0), (9.0, 8.0), (9.5, 7.5), (10.5, 9.0)]);
        assert_series(&out, &[None, Some(9.0), Some(7.5), Some(7.5)]);
    }

    #[test]
    fn hand_computed_immediate_reversal_short_to_long() {
        // b1: up 0, down 1 -> short, SAR = high0 10, EP 7. high 10 >= 10
        //     reverses: SAR = min(EP 7, prev low 7, low 7) = 7; long, EP 10.
        // b2: 7 + 0.02·(10 − 7) = 7.06, clamp min(7, 7) = 7; EP 10.5.
        // b3: 7 + 0.04·(10.5 − 7) = 7.14, clamp min(8, 7) = 7.
        let out = run(&[(10.0, 8.0), (10.0, 7.0), (10.5, 8.0), (11.0, 9.0)]);
        assert_series(&out, &[None, Some(7.0), Some(7.0), Some(7.0)]);
    }

    #[test]
    fn hand_computed_reversal_clamped_above_the_extreme_point() {
        // Long seed as in `hand_computed_long_seed` up to b2 (SAR 8.06, EP 12, AF 0.04).
        // b3 (13, 8): 8.06 + 0.04·(12 − 8.06) = 8.2176; low 8 <= 8.2176
        //     reverses. The bar's high 13 is above EP 12, so SAR =
        //     max(12, prev high 12, 13) = 13. Short, EP 8, AF 0.02.
        // b4 (12, 7): 13 + 0.02·(8 − 13) = 12.9, clamp max(13, 12) = 13; EP 7, AF 0.04.
        // b5 (11.5, 6.5): 13 + 0.04·(7 − 13) = 12.76, clamp max(12, 13) = 13.
        let out = run(&[
            (10.0, 8.0),
            (11.0, 9.0),
            (12.0, 10.0),
            (13.0, 8.0),
            (12.0, 7.0),
            (11.5, 6.5),
        ]);
        assert_series(
            &out,
            &[
                None,
                Some(8.0),
                Some(8.06),
                Some(13.0),
                Some(13.0),
                Some(13.0),
            ],
        );
    }

    #[test]
    fn acceleration_factor_caps_at_max() {
        // AF (0.1, 0.1, 0.2): b2 AF 0.1 -> 0.2, then capped at 0.2.
        //   b1 SAR 8, EP 11.
        //   b2 8 + 0.1·3 = 8.3; EP 12, AF 0.2.
        //   b3 8.3 + 0.2·(12 − 8.3) = 9.04, clamped to min(b2 low 10, b1 low 9) = 9;
        //      EP 13, AF min(0.3, 0.2) = 0.2.
        //   b4 9 + 0.2·(13 − 9) = 9.8, clamp min(11, 10) keeps it.
        let candles: Vec<Candle> = [
            (10.0, 8.0),
            (11.0, 9.0),
            (12.0, 10.0),
            (13.0, 11.0),
            (14.0, 12.0),
        ]
        .iter()
        .map(|&(h, l)| hl(h, l))
        .collect();
        let out = Psar::new(0.1, 0.1, 0.2).unwrap().batch(&candles);
        assert_series(&out, &[None, Some(8.0), Some(8.3), Some(9.0), Some(9.8)]);
    }

    #[test]
    fn reset_matches_a_fresh_instance_and_batch_nan_into() {
        let candles: Vec<Candle> = (0..60)
            .map(|i| {
                let m = 100.0 + (f64::from(i) * 0.3).sin() * 8.0;
                c(m + 1.0, m - 1.0, m)
            })
            .collect();
        let mut psar = Psar::classic();
        let _ = psar.batch(&candles);
        psar.reset();
        let after_reset = psar.batch(&candles);
        assert_eq!(after_reset, Psar::classic().batch(&candles));
        let expected: Vec<u64> = after_reset
            .iter()
            .map(|v| v.unwrap_or(f64::NAN).to_bits())
            .collect();
        let mut out = vec![0.0; candles.len()];
        Psar::classic().batch_nan_into(&candles, &mut out);
        let got: Vec<u64> = out.iter().map(|v| v.to_bits()).collect();
        assert_eq!(got, expected);
    }
}
