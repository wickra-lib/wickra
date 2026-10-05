//! Demand Index (James Sibbet).

use std::collections::VecDeque;

use crate::error::{Error, Result};
use crate::indicators::ema::Ema;
use crate::ohlcv::Candle;
use crate::traits::Indicator;

/// Cap on the exponent of the pressure ratio (as in the `TradeStation` function),
/// keeping `exp` finite on a violent bar.
const MAX_EXPONENT: f64 = 88.0;

/// James Sibbet's Demand Index — the ratio of buying pressure to selling
/// pressure, bounded in `[−100, +100]`.
///
/// Each bar's volume (relative to its `period` average) is assigned in full to
/// the side the weighted close moved towards, and the other side receives that
/// volume damped exponentially by the size of the move against the market's
/// typical range. Both pressures are exponentially smoothed and compared. This is
/// Sibbet's construction as published in the `TradeStation` `DemandIndex`
/// function:
///
/// ```text
/// WC     = (high + low + 2·close) / 4
/// ratio  = (WC_t − WC_{t−1}) / min(WC_t, WC_{t−1})
/// vol    = volume / SMA(volume, period)
/// K      = 3·WC / SMA(max(high, high_{t−1}) − min(low, low_{t−1}), period)
/// damped = vol / exp(min(K · |ratio|, 88))
/// ratio > 0:  BP = vol,     SP = damped
/// otherwise:  BP = damped,  SP = vol
/// B, S   = EMA(BP, period), EMA(SP, period)     (seeded with the first value)
/// DI     = +100 · (1 − S / B)   if B > S
///          −100 · (1 − B / S)   if B < S
///          0                    if B = S
/// ```
///
/// Positive readings mean buying pressure dominates, negative selling pressure;
/// the magnitude says by how much. A bar whose averages or weighted closes are
/// zero carries no measurable pressure and repeats the previous reading. The
/// first value lands once the two-bar range average is full, on bar `period + 1`.
///
/// # Example
///
/// ```
/// use wickra_core::{Candle, DemandIndex, Indicator};
///
/// let mut indicator = DemandIndex::new(10).unwrap();
/// let mut last = None;
/// for i in 0..120 {
///     let base = 100.0 + f64::from(i);
///     let candle =
///         Candle::new(base, base + 2.0, base - 2.0, base + 1.0, 50.0, i64::from(i)).unwrap();
///     last = indicator.update(candle);
/// }
/// assert!(last.is_some());
/// ```
#[derive(Debug, Clone)]
pub struct DemandIndex {
    period: usize,
    buy: Ema,
    sell: Ema,
    prev: Option<Candle>,
    volumes: VecDeque<f64>,
    volume_sum: f64,
    ranges: VecDeque<f64>,
    range_sum: f64,
    last: f64,
    ready: bool,
}

impl DemandIndex {
    /// Construct a new Demand Index with the given averaging period.
    ///
    /// # Errors
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
        let alpha = 2.0 / (period as f64 + 1.0);
        Ok(Self {
            period,
            buy: Ema::with_alpha(alpha)?,
            sell: Ema::with_alpha(alpha)?,
            prev: None,
            volumes: VecDeque::with_capacity(period),
            volume_sum: 0.0,
            ranges: VecDeque::with_capacity(period),
            range_sum: 0.0,
            last: 0.0,
            ready: false,
        })
    }

    /// Configured averaging period.
    pub const fn period(&self) -> usize {
        self.period
    }

    /// Push `x` into a fixed-length window with a running sum.
    fn push(window: &mut VecDeque<f64>, sum: &mut f64, len: usize, x: f64) {
        if window.len() == len {
            *sum -= window.pop_front().expect("window is non-empty");
        }
        window.push_back(x);
        *sum += x;
    }
}

/// The weighted close `(high + low + 2·close) / 4`.
fn weighted_close(c: &Candle) -> f64 {
    (c.high + c.low + 2.0 * c.close) * 0.25
}

impl Indicator for DemandIndex {
    type Input = Candle;
    type Output = f64;

    #[inline]
    fn update(&mut self, candle: Candle) -> Option<f64> {
        let period = self.period;
        Self::push(
            &mut self.volumes,
            &mut self.volume_sum,
            period,
            candle.volume,
        );
        let prev = self.prev.replace(candle)?;
        let two_bar_range = candle.high.max(prev.high) - candle.low.min(prev.low);
        Self::push(&mut self.ranges, &mut self.range_sum, period, two_bar_range);
        if self.ranges.len() < period {
            return None;
        }
        let n = period as f64;
        let avg_range = self.range_sum / n;
        let avg_volume = self.volume_sum / n;
        let wc = weighted_close(&candle);
        let wc_prev = weighted_close(&prev);
        if wc != 0.0 && wc_prev != 0.0 && avg_range != 0.0 && avg_volume != 0.0 {
            let ratio = (wc - wc_prev) / wc.min(wc_prev);
            let vol = candle.volume / avg_volume;
            let exponent = ((3.0 * wc / avg_range) * ratio.abs()).min(MAX_EXPONENT);
            let damped = vol / exponent.exp();
            let (bp, sp) = if ratio > 0.0 {
                (vol, damped)
            } else {
                (damped, vol)
            };
            let b = self.buy.update(bp).unwrap_or(bp);
            let s = self.sell.update(sp).unwrap_or(sp);
            self.last = if b > s {
                100.0 * (1.0 - s / b)
            } else if b < s {
                -100.0 * (1.0 - b / s)
            } else {
                0.0
            };
        }
        self.ready = true;
        Some(self.last)
    }

    fn reset(&mut self) {
        self.buy.reset();
        self.sell.reset();
        self.prev = None;
        self.volumes.clear();
        self.volume_sum = 0.0;
        self.ranges.clear();
        self.range_sum = 0.0;
        self.last = 0.0;
        self.ready = false;
    }

    #[inline]
    fn warmup_period(&self) -> usize {
        // One seed bar for the previous candle, then `period` two-bar ranges.
        self.period + 1
    }

    #[inline]
    fn is_ready(&self) -> bool {
        self.ready
    }

    #[inline]
    fn name(&self) -> &'static str {
        "DemandIndex"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;
    use approx::assert_relative_eq;

    fn c(open: f64, high: f64, low: f64, close: f64, volume: f64, ts: i64) -> Candle {
        Candle::new(open, high, low, close, volume, ts).unwrap()
    }

    #[test]
    fn rejects_zero_period() {
        assert!(matches!(DemandIndex::new(0), Err(Error::PeriodZero)));
    }

    #[test]
    fn accessors_and_metadata() {
        let di = DemandIndex::new(10).unwrap();
        assert_eq!(di.period(), 10);
        assert_eq!(di.name(), "DemandIndex");
        assert_eq!(di.warmup_period(), 11);
    }

    #[test]
    fn constant_series_yields_zero() {
        // Flat bars have no range -> no measurable pressure -> DI stays at 0.
        let candles: Vec<Candle> = (0..40)
            .map(|i| c(10.0, 10.0, 10.0, 10.0, 100.0, i))
            .collect();
        let mut di = DemandIndex::new(5).unwrap();
        for v in di.batch(&candles).into_iter().flatten() {
            assert_relative_eq!(v, 0.0, epsilon = 1e-12);
        }
    }

    #[test]
    fn rising_series_yields_positive_signal() {
        // Strictly rising closes on constant volume -> pressure is positive every
        // bar -> smoothed DI must end up strictly positive.
        let candles: Vec<Candle> = (0..40)
            .map(|i| {
                let f = i as f64;
                c(100.0 + f, 101.0 + f, 99.0 + f, 100.5 + f, 100.0, i)
            })
            .collect();
        let mut di = DemandIndex::new(5).unwrap();
        let out = di.batch(&candles);
        let last = out.iter().filter_map(|x| *x).next_back().unwrap();
        assert!(last > 0.0, "rising series must yield positive DI");
    }

    #[test]
    fn falling_series_yields_negative_signal() {
        let candles: Vec<Candle> = (0..40)
            .map(|i| {
                let f = i as f64;
                c(200.0 - f, 201.0 - f, 199.0 - f, 199.5 - f, 100.0, i)
            })
            .collect();
        let mut di = DemandIndex::new(5).unwrap();
        let out = di.batch(&candles);
        let last = out.iter().filter_map(|x| *x).next_back().unwrap();
        assert!(last < 0.0, "falling series must yield negative DI");
    }

    #[test]
    fn zero_weighted_close_contributes_no_signal() {
        // The first bars have a zero weighted close -> no pressure is measured.
        // We then continue with a non-zero series and confirm output behaves.
        let mut di = DemandIndex::new(3).unwrap();
        di.update(c(0.0, 0.0, 0.0, 0.0, 100.0, 0));
        // Bar 2 sees a zero previous weighted close -> no pressure.
        di.update(c(0.0, 1.0, 0.0, 1.0, 100.0, 1));
        // Subsequent bars now have non-zero prev_close.
        di.update(c(1.0, 2.0, 1.0, 2.0, 100.0, 2));
        // Just check that nothing exploded; the range average fills on bar 4.
        let v = di.update(c(2.0, 3.0, 2.0, 3.0, 100.0, 3));
        assert!(v.is_some());
        assert!(v.unwrap().is_finite());
    }

    #[test]
    fn batch_equals_streaming() {
        let candles: Vec<Candle> = (0..100i64)
            .map(|i| {
                let f = i as f64;
                let mid = 100.0 + (f * 0.2).sin() * 5.0;
                c(
                    mid,
                    mid + 1.5,
                    mid - 1.5,
                    mid + 0.3,
                    80.0 + (i % 5) as f64,
                    i,
                )
            })
            .collect();
        let mut a = DemandIndex::new(10).unwrap();
        let mut b = DemandIndex::new(10).unwrap();
        assert_eq!(
            a.batch(&candles),
            candles.iter().map(|x| b.update(*x)).collect::<Vec<_>>()
        );
    }

    #[test]
    fn reset_clears_state() {
        let candles: Vec<Candle> = (0..40)
            .map(|i| {
                let f = i as f64;
                c(100.0 + f, 101.0 + f, 99.0 + f, 100.5 + f, 100.0, i)
            })
            .collect();
        let mut di = DemandIndex::new(5).unwrap();
        di.batch(&candles);
        assert!(di.is_ready());
        di.reset();
        assert!(!di.is_ready());
        assert_eq!(di.update(candles[0]), None);
    }

    fn wavy_series() -> Vec<Candle> {
        (0..60i64)
            .map(|i| {
                let f = i as f64;
                let mid = 100.0 + (f * 0.37).sin() * 4.0 + f * 0.05;
                c(
                    mid,
                    mid + 1.0 + (f * 0.11).cos().abs(),
                    mid - 1.2,
                    mid + (f * 0.5).sin() * 0.8,
                    50.0 + (i % 7) as f64 * 10.0,
                    i,
                )
            })
            .collect()
    }

    #[test]
    fn rejects_period_above_maximum() {
        assert!(matches!(
            DemandIndex::new(crate::error::MAX_PERIOD + 1),
            Err(Error::InvalidPeriod { .. })
        ));
    }

    #[test]
    fn first_value_lands_exactly_at_warmup_minus_one() {
        let candles = wavy_series();
        for period in [1usize, 3, 7] {
            let mut di = DemandIndex::new(period).unwrap();
            let out = di.batch(&candles);
            let warm = di.warmup_period();
            assert_eq!(warm, period + 1);
            assert!(out[..warm - 1].iter().all(Option::is_none));
            assert!(out[warm - 1].is_some());
        }
    }

    #[test]
    fn not_ready_before_warmup_completes() {
        let candles = wavy_series();
        let mut di = DemandIndex::new(4).unwrap();
        for candle in &candles[..4] {
            assert_eq!(di.update(*candle), None);
            assert!(!di.is_ready());
        }
        assert!(di.update(candles[4]).is_some());
        assert!(di.is_ready());
    }

    #[test]
    fn reset_replays_identically_to_fresh_instance() {
        let candles = wavy_series();
        let mut di = DemandIndex::new(5).unwrap();
        let first = di.batch(&candles);
        di.reset();
        let second = di.batch(&candles);
        let fresh = DemandIndex::new(5).unwrap().batch(&candles);
        assert_eq!(first, second);
        assert_eq!(second, fresh);
    }

    #[test]
    fn batch_nan_into_is_bit_identical_to_streaming() {
        let candles = wavy_series();
        let mut batch_di = DemandIndex::new(6).unwrap();
        let mut out = vec![0.0; candles.len()];
        batch_di.batch_nan_into(&candles, &mut out);
        let mut stream_di = DemandIndex::new(6).unwrap();
        let streamed: Vec<f64> = candles
            .iter()
            .map(|x| stream_di.update(*x).unwrap_or(f64::NAN))
            .collect();
        assert!(out
            .iter()
            .zip(&streamed)
            .all(|(a, b)| a.to_bits() == b.to_bits()));
    }

    #[test]
    fn hand_computed_period_one_up_then_down() {
        // period 1 -> EMA alpha = 2 / 2 = 1, so B and S equal the raw pressures.
        let mut di = DemandIndex::new(1).unwrap();
        // Bar 0: WC = (11 + 9 + 2*10) / 4 = 10. Seed bar -> None.
        assert_eq!(di.update(c(10.0, 11.0, 9.0, 10.0, 100.0, 0)), None);
        // Bar 1: WC = (12 + 10 + 2*11.5) / 4 = 11.25.
        // ratio = (11.25 - 10) / min(11.25, 10) = 0.125
        // range = max(12, 11) - min(10, 9) = 3, avg_range = 3; avg_volume = 200 -> vol = 1
        // K = 3 * 11.25 / 3 = 11.25; exponent = 11.25 * 0.125 = 1.40625
        // ratio > 0 -> BP = 1, SP = exp(-1.40625)
        // B > S -> DI = 100 * (1 - exp(-1.40625)) = 75.493_946...
        let up = di.update(c(10.0, 12.0, 10.0, 11.5, 200.0, 1)).unwrap();
        assert_relative_eq!(up, 100.0 * (1.0 - (-1.40625f64).exp()), epsilon = 1e-12);
        assert_relative_eq!(up, 75.493_946_075_447_41, epsilon = 1e-9);
        // Bar 2: WC = (11.5 + 9.5 + 2*10) / 4 = 10.25.
        // ratio = (10.25 - 11.25) / 10.25 = -1 / 10.25
        // range = max(11.5, 12) - min(9.5, 10) = 2.5; avg_volume = 50 -> vol = 1
        // K = 3 * 10.25 / 2.5 = 12.3; exponent = 12.3 / 10.25 = 1.2
        // ratio <= 0 -> BP = exp(-1.2), SP = 1
        // B < S -> DI = -100 * (1 - exp(-1.2)) = -69.880_578...
        let down = di.update(c(11.0, 11.5, 9.5, 10.0, 50.0, 2)).unwrap();
        assert_relative_eq!(down, -100.0 * (1.0 - (-1.2f64).exp()), epsilon = 1e-9);
        assert_relative_eq!(down, -69.880_578_808_779_77, epsilon = 1e-9);
    }

    #[test]
    fn equal_pressure_returns_exact_zero() {
        // Identical candles with a non-zero range and volume: the weighted close
        // is unchanged -> ratio = 0 -> exponent 0 -> damped = vol, so BP = SP and
        // B == S, which yields exactly 0 (not a repeat of the previous reading).
        let mut di = DemandIndex::new(1).unwrap();
        di.update(c(10.0, 11.0, 9.0, 10.0, 100.0, 0));
        let up = di.update(c(10.0, 12.0, 10.0, 11.5, 200.0, 1)).unwrap();
        assert!(up > 0.0);
        let flat = di.update(c(10.0, 12.0, 10.0, 11.5, 200.0, 2)).unwrap();
        assert_eq!(flat.to_bits(), 0.0f64.to_bits());
        // Longer period: a constant non-degenerate bar feeds equal pressures into
        // both EMAs, so they stay equal.
        let mut di5 = DemandIndex::new(5).unwrap();
        let candle = c(10.0, 12.0, 8.0, 10.0, 100.0, 0);
        let out = di5.batch(&[candle; 12]);
        assert!(out
            .iter()
            .flatten()
            .all(|v| v.to_bits() == 0.0f64.to_bits()));
    }

    #[test]
    fn zero_volume_repeats_previous_reading() {
        // avg_volume == 0 -> no measurable pressure -> the last value is repeated.
        let mut di = DemandIndex::new(1).unwrap();
        di.update(c(10.0, 11.0, 9.0, 10.0, 100.0, 0));
        let up = di.update(c(10.0, 12.0, 10.0, 11.5, 200.0, 1)).unwrap();
        let held = di.update(c(11.0, 11.5, 9.5, 10.0, 0.0, 2)).unwrap();
        assert_eq!(held.to_bits(), up.to_bits());
        // Zero volume from the start: the reading stays at its initial 0.
        let mut quiet = DemandIndex::new(2).unwrap();
        let candles: Vec<Candle> = (0..6)
            .map(|i| {
                c(
                    10.0 + i as f64,
                    11.0 + i as f64,
                    9.0 + i as f64,
                    10.5 + i as f64,
                    0.0,
                    i,
                )
            })
            .collect();
        assert!(quiet
            .batch(&candles)
            .iter()
            .flatten()
            .all(|v| v.to_bits() == 0.0f64.to_bits()));
    }

    #[test]
    fn exponent_is_capped_at_max_exponent() {
        // Bar 0: flat at 1 -> WC = 1. Bar 1: flat at 1000 -> WC = 1000.
        // ratio = 999 / 1 = 999; range = 1000 - 1 = 999; K = 3 * 1000 / 999
        // K * |ratio| = 3000 > 88 -> capped at 88 -> SP = 1 / exp(88), a
        // positive value (uncapped, exp(-3000) would underflow to 0).
        let mut di = DemandIndex::new(1).unwrap();
        di.update(c(1.0, 1.0, 1.0, 1.0, 100.0, 0));
        let v = di
            .update(c(1000.0, 1000.0, 1000.0, 1000.0, 100.0, 1))
            .unwrap();
        let sell = di.sell.value().unwrap();
        assert_eq!(sell.to_bits(), (1.0 / MAX_EXPONENT.exp()).to_bits());
        assert!(sell > 0.0);
        assert_eq!(di.buy.value(), Some(1.0));
        assert_relative_eq!(v, 100.0, epsilon = 1e-12);
    }
}
