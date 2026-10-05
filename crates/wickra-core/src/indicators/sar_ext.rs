//! Parabolic SAR Extended (SAREXT).

use crate::error::{Error, Result};
use crate::ohlcv::Candle;
use crate::traits::Indicator;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Trend {
    Up,
    Down,
}

/// One direction's acceleration-factor schedule (initial, step, maximum).
#[derive(Debug, Clone, Copy)]
struct Accel {
    init: f64,
    step: f64,
    max: f64,
}

impl Accel {
    fn validate(self) -> Result<Self> {
        if !(self.init.is_finite() && self.step.is_finite() && self.max.is_finite()) {
            return Err(Error::NonPositiveMultiplier);
        }
        if self.init <= 0.0 || self.step <= 0.0 || self.max <= 0.0 {
            return Err(Error::NonPositiveMultiplier);
        }
        if self.init > self.max {
            return Err(Error::InvalidPeriod {
                message: "acceleration init must be <= max",
            });
        }
        Ok(self)
    }
}

/// Parabolic SAR Extended (`SAREXT`): Wilder's Parabolic SAR with TA-Lib's
/// extended controls.
///
/// Beyond [`Psar`](crate::Psar) it adds:
/// - **`start_value`** — the initial SAR. `0` auto-seeds (long, like `Psar`);
///   a positive value starts a long phase at that SAR, a negative value starts a
///   short phase at its absolute value.
/// - **`offset_on_reverse`** — a fractional offset applied to the new SAR on each
///   reversal, pushing it further from price (`0` disables it).
/// - **separate long / short acceleration** — independent `(init, step, max)`
///   schedules for rising and falling phases.
///
/// The output is **signed**: a positive value during a long phase (SAR below
/// price) and a negative value during a short phase (SAR above price), so the
/// sign alone encodes the current trade direction.
///
/// # Example
///
/// ```
/// use wickra_core::{Candle, Indicator, SarExt};
///
/// let mut indicator =
///     SarExt::new(0.0, 0.0, 0.02, 0.02, 0.2, 0.02, 0.02, 0.2).unwrap();
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
pub struct SarExt {
    start_value: f64,
    offset_on_reverse: f64,
    long: Accel,
    short: Accel,

    initialised: bool,
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

impl SarExt {
    /// Construct an extended Parabolic SAR.
    ///
    /// Parameters mirror TA-Lib's `SAREXT`: `start_value`, `offset_on_reverse`,
    /// then the long `(init, step, max)` and short `(init, step, max)`
    /// acceleration schedules.
    ///
    /// # Errors
    /// Returns [`Error::NonPositiveMultiplier`] if any acceleration term is
    /// non-positive or non-finite, [`Error::InvalidPeriod`] if an `init` exceeds
    /// its `max`, and [`Error::NonPositiveMultiplier`] if `start_value` or
    /// `offset_on_reverse` is non-finite or `offset_on_reverse` is negative.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        start_value: f64,
        offset_on_reverse: f64,
        accel_init_long: f64,
        accel_long: f64,
        accel_max_long: f64,
        accel_init_short: f64,
        accel_short: f64,
        accel_max_short: f64,
    ) -> Result<Self> {
        if !start_value.is_finite() || !offset_on_reverse.is_finite() || offset_on_reverse < 0.0 {
            return Err(Error::NonPositiveMultiplier);
        }
        let long = Accel {
            init: accel_init_long,
            step: accel_long,
            max: accel_max_long,
        }
        .validate()?;
        let short = Accel {
            init: accel_init_short,
            step: accel_short,
            max: accel_max_short,
        }
        .validate()?;
        Ok(Self {
            start_value,
            offset_on_reverse,
            long,
            short,
            initialised: false,
            has_emitted: false,
            prev_high: f64::NAN,
            prev_low: f64::NAN,
            prev2_high: f64::NAN,
            prev2_low: f64::NAN,
            trend: Trend::Up,
            sar: f64::NAN,
            ep: f64::NAN,
            af: long.init,
        })
    }

    /// Wilder's defaults with no start value or reversal offset and symmetric
    /// `(0.02, 0.02, 0.20)` acceleration in both directions.
    pub fn classic() -> Self {
        Self::new(0.0, 0.0, 0.02, 0.02, 0.20, 0.02, 0.02, 0.20)
            .expect("classic SAREXT params are valid")
    }

    fn signed(&self, sar: f64) -> f64 {
        match self.trend {
            Trend::Up => sar,
            Trend::Down => -sar,
        }
    }
}

impl Indicator for SarExt {
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
            let predicted = self.sar + self.af * (self.ep - self.sar);
            match self.trend {
                Trend::Up => predicted.min(self.prev_low).min(self.prev2_low),
                Trend::Down => predicted.max(self.prev_high).max(self.prev2_high),
            }
        } else {
            // Second candle: TA-Lib's seed. A zero start value takes the
            // direction from the one-bar directional movement of the first two
            // candles (short when the down move dominates) and starts the SAR
            // at the first candle's opposite extreme; a signed start value
            // fixes both. The extreme point starts at this candle's extreme,
            // and TA-Lib's first step treats this candle as both today and
            // yesterday.
            let up_move = candle.high - self.prev_high;
            let down_move = self.prev_low - candle.low;
            let long = if self.start_value == 0.0 {
                !(down_move > 0.0 && down_move > up_move)
            } else {
                self.start_value > 0.0
            };
            let auto_sar = if long { self.prev_low } else { self.prev_high };
            self.sar = if self.start_value == 0.0 {
                auto_sar
            } else {
                self.start_value.abs()
            };
            if long {
                self.trend = Trend::Up;
                self.ep = candle.high;
                self.af = self.long.init;
            } else {
                self.trend = Trend::Down;
                self.ep = candle.low;
                self.af = self.short.init;
            }
            self.prev_high = candle.high;
            self.prev_low = candle.low;
            self.sar
        };
        let prev_h = self.prev_high;
        let prev_l = self.prev_low;

        let mut output_sar = new_sar;
        let reversed = match self.trend {
            Trend::Up => candle.low <= new_sar,
            Trend::Down => candle.high >= new_sar,
        };

        if reversed {
            // The new SAR is the prior extreme point, moved outside this bar's
            // and the previous bar's range if the reversal bar reached past it
            // (TA-Lib's reversal clamp), then offset.
            output_sar = match self.trend {
                Trend::Up => self.ep.max(prev_h).max(candle.high),
                Trend::Down => self.ep.min(prev_l).min(candle.low),
            };
            self.trend = match self.trend {
                Trend::Up => Trend::Down,
                Trend::Down => Trend::Up,
            };
            match self.trend {
                Trend::Up => {
                    output_sar -= output_sar.abs() * self.offset_on_reverse;
                    self.ep = candle.high;
                    self.af = self.long.init;
                }
                Trend::Down => {
                    output_sar += output_sar.abs() * self.offset_on_reverse;
                    self.ep = candle.low;
                    self.af = self.short.init;
                }
            }
        } else {
            match self.trend {
                Trend::Up => {
                    if candle.high > self.ep {
                        self.ep = candle.high;
                        self.af = (self.af + self.long.step).min(self.long.max);
                    }
                }
                Trend::Down => {
                    if candle.low < self.ep {
                        self.ep = candle.low;
                        self.af = (self.af + self.short.step).min(self.short.max);
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
        Some(self.signed(output_sar))
    }

    fn reset(&mut self) {
        self.initialised = false;
        self.has_emitted = false;
        self.prev_high = f64::NAN;
        self.prev_low = f64::NAN;
        self.prev2_high = f64::NAN;
        self.prev2_low = f64::NAN;
        self.trend = Trend::Up;
        self.sar = f64::NAN;
        self.ep = f64::NAN;
        self.af = self.long.init;
    }

    #[inline]
    fn warmup_period(&self) -> usize {
        2
    }

    #[inline]
    fn is_ready(&self) -> bool {
        self.has_emitted
    }

    #[inline]
    fn name(&self) -> &'static str {
        "SAREXT"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;

    fn c(h: f64, l: f64, cl: f64) -> Candle {
        Candle::new(cl, h, l, cl, 1.0, 0).unwrap()
    }

    fn classic() -> SarExt {
        SarExt::classic()
    }

    #[test]
    fn rejects_invalid_params() {
        // Non-positive acceleration terms.
        assert!(SarExt::new(0.0, 0.0, 0.0, 0.02, 0.2, 0.02, 0.02, 0.2).is_err());
        assert!(SarExt::new(0.0, 0.0, 0.02, 0.02, 0.2, 0.0, 0.02, 0.2).is_err());
        assert!(SarExt::new(0.0, 0.0, 0.30, 0.02, 0.2, 0.02, 0.02, 0.2).is_err());
        // Non-finite acceleration terms hit the finite guard in `Accel::validate`,
        // on both the long and the short schedule.
        assert!(SarExt::new(0.0, 0.0, f64::NAN, 0.02, 0.2, 0.02, 0.02, 0.2).is_err());
        assert!(SarExt::new(0.0, 0.0, 0.02, 0.02, 0.2, 0.02, f64::INFINITY, 0.2).is_err());
        // Bad start value / offset.
        assert!(SarExt::new(f64::NAN, 0.0, 0.02, 0.02, 0.2, 0.02, 0.02, 0.2).is_err());
        assert!(SarExt::new(0.0, -1.0, 0.02, 0.02, 0.2, 0.02, 0.02, 0.2).is_err());
    }

    #[test]
    fn accessors_and_metadata() {
        let s = classic();
        assert_eq!(s.warmup_period(), 2);
        assert_eq!(s.name(), "SAREXT");
        assert!(!s.is_ready());
    }

    #[test]
    fn seed_returns_none_then_emits() {
        let mut s = classic();
        assert_eq!(s.update(c(11.0, 9.0, 10.0)), None);
        assert!(!s.is_ready());
        assert!(s.update(c(12.0, 10.0, 11.0)).is_some());
        assert!(s.is_ready());
    }

    #[test]
    fn uptrend_is_positive_and_below_lows() {
        let candles: Vec<Candle> = (0..40)
            .map(|i| {
                let base = 100.0 + f64::from(i);
                c(base + 0.5, base - 0.5, base)
            })
            .collect();
        let mut s = classic();
        let ok = s
            .batch(&candles)
            .iter()
            .enumerate()
            .all(|(i, v)| v.is_none_or(|x| x > 0.0 && x <= candles[i].low + 1e-9));
        assert!(ok, "long-phase SAREXT must be positive and below the low");
    }

    #[test]
    fn downtrend_is_negative_and_above_highs() {
        let candles: Vec<Candle> = (0..40)
            .rev()
            .map(|i| {
                let base = 100.0 + f64::from(i);
                c(base + 0.5, base - 0.5, base)
            })
            .collect();
        let mut s = classic();
        let ok = s
            .batch(&candles)
            .iter()
            .enumerate()
            .skip(5)
            .all(|(i, v)| v.is_none_or(|x| x < 0.0 && -x >= candles[i].high - 1e-9));
        assert!(ok, "short-phase SAREXT must be negative and above the high");
    }

    #[test]
    fn positive_start_value_begins_long() {
        // start_value > 0 seeds a long phase: first emitted value is positive.
        let mut s = SarExt::new(95.0, 0.0, 0.02, 0.02, 0.2, 0.02, 0.02, 0.2).unwrap();
        assert_eq!(s.update(c(101.0, 99.0, 100.0)), None);
        let v = s.update(c(102.0, 100.0, 101.0)).unwrap();
        assert!(v > 0.0);
    }

    #[test]
    fn negative_start_value_begins_short() {
        // start_value < 0 seeds a short phase: first emitted value is negative.
        let mut s = SarExt::new(-105.0, 0.0, 0.02, 0.02, 0.2, 0.02, 0.02, 0.2).unwrap();
        assert_eq!(s.update(c(101.0, 99.0, 100.0)), None);
        let v = s.update(c(100.0, 98.0, 99.0)).unwrap();
        assert!(v < 0.0);
    }

    #[test]
    fn offset_on_reverse_pushes_sar_further() {
        // A V-shaped path forces a reversal; with an offset the reversal SAR is
        // pushed further from price than without one.
        let candles: Vec<Candle> = (0..12)
            .map(|i| {
                let base = if i < 6 {
                    100.0 - f64::from(i) * 2.0
                } else {
                    88.0 + f64::from(i - 6) * 2.0
                };
                c(base + 1.0, base - 1.0, base)
            })
            .collect();
        let plain = SarExt::new(0.0, 0.0, 0.02, 0.02, 0.2, 0.02, 0.02, 0.2)
            .unwrap()
            .batch(&candles);
        let offset = SarExt::new(0.0, 0.1, 0.02, 0.02, 0.2, 0.02, 0.02, 0.2)
            .unwrap()
            .batch(&candles);
        // The two configurations must diverge once a reversal with offset fires.
        assert_ne!(plain, offset);
    }

    #[test]
    fn batch_equals_streaming() {
        let candles: Vec<Candle> = (0..60)
            .map(|i| {
                let m = 100.0 + (f64::from(i) * 0.3).sin() * 8.0;
                c(m + 1.0, m - 1.0, m)
            })
            .collect();
        let mut a = classic();
        let mut b = classic();
        assert_eq!(
            a.batch(&candles),
            candles.iter().map(|x| b.update(*x)).collect::<Vec<_>>()
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
        let mut s = classic();
        let first = s.batch(&candles);
        assert!(s.is_ready());
        s.reset();
        assert!(!s.is_ready());
        assert_eq!(first, s.batch(&candles));
    }

    fn hl(high: f64, low: f64) -> Candle {
        c(high, low, f64::midpoint(high, low))
    }

    fn run(sar: SarExt, bars: &[(f64, f64)]) -> Vec<Option<f64>> {
        let mut sar = sar;
        let candles: Vec<Candle> = bars.iter().map(|&(h, l)| hl(h, l)).collect();
        sar.batch(&candles)
    }

    fn with(start: f64, offset: f64) -> SarExt {
        SarExt::new(start, offset, 0.02, 0.02, 0.2, 0.02, 0.02, 0.2).unwrap()
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
    fn rejects_every_invalid_parameter_with_its_variant() {
        let nonpos = |r: Result<SarExt>| matches!(r, Err(Error::NonPositiveMultiplier));
        assert!(nonpos(SarExt::new(
            f64::INFINITY,
            0.0,
            0.02,
            0.02,
            0.2,
            0.02,
            0.02,
            0.2
        )));
        assert!(nonpos(SarExt::new(
            0.0,
            f64::NAN,
            0.02,
            0.02,
            0.2,
            0.02,
            0.02,
            0.2
        )));
        assert!(nonpos(SarExt::new(
            0.0, -0.1, 0.02, 0.02, 0.2, 0.02, 0.02, 0.2
        )));
        assert!(nonpos(SarExt::new(
            0.0,
            0.0,
            0.02,
            0.02,
            f64::NAN,
            0.02,
            0.02,
            0.2
        )));
        assert!(nonpos(SarExt::new(
            0.0, 0.0, 0.02, 0.0, 0.2, 0.02, 0.02, 0.2
        )));
        assert!(nonpos(SarExt::new(
            0.0, 0.0, 0.02, 0.02, 0.0, 0.02, 0.02, 0.2
        )));
        assert!(nonpos(SarExt::new(
            0.0, 0.0, 0.02, 0.02, 0.2, 0.02, 0.0, 0.2
        )));
        assert!(nonpos(SarExt::new(
            0.0, 0.0, 0.02, 0.02, 0.2, 0.02, 0.02, -0.2
        )));
        let invalid = |r: Result<SarExt>| matches!(r, Err(Error::InvalidPeriod { .. }));
        assert!(invalid(SarExt::new(
            0.0, 0.0, 0.3, 0.02, 0.2, 0.02, 0.02, 0.2
        )));
        assert!(invalid(SarExt::new(
            0.0, 0.0, 0.02, 0.02, 0.2, 0.3, 0.02, 0.2
        )));
    }

    #[test]
    fn first_value_lands_at_index_one() {
        let out = run(classic(), &[(10.0, 8.0), (11.0, 9.0), (12.0, 10.0)]);
        assert!(out[0].is_none());
        assert!(out[1..].iter().all(Option::is_some));
    }

    #[test]
    fn hand_computed_auto_long_seed() {
        // Identical to Psar: up move 1 > down move −1 -> long, SAR = low0 8, EP 11.
        //   b2 8 + 0.02·3 = 8.06 (EP 12, AF 0.04); b3 8.06 + 0.04·3.94 = 8.2176
        //   (EP 13, AF 0.06); b4 8.2176 + 0.06·(13 − 8.2176) = 8.504544. Positive: long.
        let out = run(
            classic(),
            &[
                (10.0, 8.0),
                (11.0, 9.0),
                (12.0, 10.0),
                (13.0, 11.0),
                (14.0, 12.0),
            ],
        );
        assert_series(
            &out,
            &[None, Some(8.0), Some(8.06), Some(8.2176), Some(8.504_544)],
        );
    }

    #[test]
    fn hand_computed_auto_short_seed() {
        // down move 1 > 0 and > up move −1 -> short, SAR = high0 12, EP 9.
        //   b2 12 + 0.02·(9 − 12) = 11.94 (EP 8, AF 0.04)
        //   b3 11.94 + 0.04·(8 − 11.94) = 11.7824. Negative: short.
        let out = run(
            classic(),
            &[(12.0, 10.0), (11.0, 9.0), (10.0, 8.0), (9.0, 7.0)],
        );
        assert_series(&out, &[None, Some(-12.0), Some(-11.94), Some(-11.7824)]);
    }

    #[test]
    fn hand_computed_immediate_reversals() {
        // Long seed reversing on b1: down move 0 is not > 0 -> long, SAR 8, EP 9;
        // low 8 <= 8 -> short at max(9, 9, 9) = 9. b2: 9 + 0.02·(8 − 9) = 8.98,
        // clamp max(9, 9) = 9; high 9.5 >= 9 -> long at min(8, 8, 7.5) = 7.5.
        // b3: 7.5 + 0.02·2 = 7.54, clamp min(7.5, 8) = 7.5.
        let out = run(
            classic(),
            &[(10.0, 8.0), (9.0, 8.0), (9.5, 7.5), (10.5, 9.0)],
        );
        assert_series(&out, &[None, Some(-9.0), Some(7.5), Some(7.5)]);
        // Short seed reversing on b1: SAR 10, EP 7; high 10 >= 10 -> long at
        // min(7, 7, 7) = 7. b2: 7.06 clamped to min(7, 7) = 7.
        let out = run(classic(), &[(10.0, 8.0), (10.0, 7.0), (10.5, 8.0)]);
        assert_series(&out, &[None, Some(7.0), Some(7.0)]);
    }

    #[test]
    fn hand_computed_reversal_clamped_above_the_extreme_point() {
        // Long to b2 (SAR 8.06, EP 12, AF 0.04). b3 (13, 8): 8.2176, low 8
        // reverses; high 13 > EP 12 -> SAR = max(12, 12, 13) = 13 (short).
        // b4 (12, 7): 13 − 0.02·5 = 12.9, clamp max(13, 12) = 13.
        let bars = [
            (10.0, 8.0),
            (11.0, 9.0),
            (12.0, 10.0),
            (13.0, 8.0),
            (12.0, 7.0),
        ];
        let out = run(classic(), &bars);
        assert_series(
            &out,
            &[None, Some(8.0), Some(8.06), Some(-13.0), Some(-13.0)],
        );
    }

    #[test]
    fn positive_start_value_forces_long_at_that_sar() {
        // Auto mode would go short (down move 0.1 > up move −1, SAR −10).
        // start 7.5 forces long at SAR 7.5 with EP high1 = 9; b2: 7.5 +
        // 0.02·(9 − 7.5) = 7.53, clamp min(7.9, 7.9) keeps it.
        let bars = [(10.0, 8.0), (9.0, 7.9), (9.5, 8.5)];
        assert_series(&run(classic(), &bars[..2]), &[None, Some(-10.0)]);
        assert_series(&run(with(7.5, 0.0), &bars), &[None, Some(7.5), Some(7.53)]);
    }

    #[test]
    fn negative_start_value_forces_short_with_the_short_acceleration() {
        // Auto mode would go long; start −12 forces short at SAR 12, EP low1 = 9,
        // using the short schedule (0.05, 0.05, 0.3).
        //   b2 12 + 0.05·(9 − 12) = 11.85, clamp max(11, 11); EP 8.5, AF 0.10
        //   b3 11.85 + 0.10·(8.5 − 11.85) = 11.515, clamp max(10.5, 11)
        let sar = SarExt::new(-12.0, 0.0, 0.02, 0.02, 0.2, 0.05, 0.05, 0.3).unwrap();
        let out = run(sar, &[(10.0, 8.0), (11.0, 9.0), (10.5, 8.5), (10.0, 8.0)]);
        assert_series(&out, &[None, Some(-12.0), Some(-11.85), Some(-11.515)]);
    }

    #[test]
    fn forced_long_start_can_reverse_immediately_with_offset() {
        // start 8.5, offset 0.01: low 8.2 <= 8.5 reverses on b1 to
        // max(EP 11, 11, 11) = 11, pushed up by 1 % -> 11.11, short.
        let out = run(with(8.5, 0.01), &[(10.0, 8.0), (11.0, 8.2)]);
        assert_series(&out, &[None, Some(-11.11)]);
    }

    #[test]
    fn offset_on_reverse_hand_computed_both_directions() {
        // offset 0.1. b1: long seed, low 8 <= 8 -> short at 9 + 0.9 = 9.9.
        // b2: 9.9 + 0.02·(8 − 9.9) = 9.862, clamp max(9, 9); high 10 >= 9.862
        //     -> long at min(8, 8, 7.5) = 7.5, minus 0.75 = 6.75.
        // b3: 6.75 + 0.02·(10 − 6.75) = 6.815, clamp min(7.5, 8) keeps it.
        let out = run(
            with(0.0, 0.1),
            &[(10.0, 8.0), (9.0, 8.0), (10.0, 7.5), (11.0, 9.0)],
        );
        assert_series(&out, &[None, Some(-9.9), Some(6.75), Some(6.815)]);
    }

    #[test]
    fn separate_long_acceleration_with_cap() {
        // Long schedule (0.03, 0.01, 0.05):
        //   b1 8; b2 8 + 0.03·3 = 8.09 (AF 0.04); b3 8.09 + 0.04·3.91 = 8.2464
        //   (AF 0.05); b4 8.2464 + 0.05·4.7536 = 8.48408 (AF capped 0.05);
        //   b5 8.48408 + 0.05·(14 − 8.48408) = 8.759876.
        let sar = SarExt::new(0.0, 0.0, 0.03, 0.01, 0.05, 0.02, 0.02, 0.2).unwrap();
        let bars = [
            (10.0, 8.0),
            (11.0, 9.0),
            (12.0, 10.0),
            (13.0, 11.0),
            (14.0, 12.0),
            (15.0, 13.0),
        ];
        let out = run(sar, &bars);
        assert_series(
            &out,
            &[
                None,
                Some(8.0),
                Some(8.09),
                Some(8.2464),
                Some(8.484_08),
                Some(8.759_876),
            ],
        );
    }

    #[test]
    fn classic_matches_psar_magnitude() {
        let candles: Vec<Candle> = (0..80)
            .map(|i| {
                let m = 100.0 + (f64::from(i) * 0.3).sin() * 8.0;
                c(m + 1.0, m - 1.0, m)
            })
            .collect();
        let ext = classic().batch(&candles);
        let psar = crate::Psar::classic().batch(&candles);
        let same = ext.iter().zip(&psar).all(|(e, p)| e.map(f64::abs) == *p);
        assert!(same, "unsigned SAREXT must equal PSAR");
    }

    #[test]
    fn reset_matches_a_fresh_instance_and_batch_nan_into() {
        let candles: Vec<Candle> = (0..60)
            .map(|i| {
                let m = 100.0 + (f64::from(i) * 0.3).sin() * 8.0;
                c(m + 1.0, m - 1.0, m)
            })
            .collect();
        let mut s = with(-150.0, 0.05);
        let _ = s.batch(&candles);
        s.reset();
        let after_reset = s.batch(&candles);
        assert_eq!(after_reset, with(-150.0, 0.05).batch(&candles));
        let expected: Vec<u64> = after_reset
            .iter()
            .map(|v| v.unwrap_or(f64::NAN).to_bits())
            .collect();
        let mut out = vec![0.0; candles.len()];
        with(-150.0, 0.05).batch_nan_into(&candles, &mut out);
        let got: Vec<u64> = out.iter().map(|v| v.to_bits()).collect();
        assert_eq!(got, expected);
    }
}
