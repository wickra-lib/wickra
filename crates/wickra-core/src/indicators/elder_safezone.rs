//! Elder `SafeZone` Stop — a trailing stop set by the average noise penetration.

use std::collections::VecDeque;

use crate::error::{Error, Result};
use crate::ohlcv::Candle;
use crate::traits::Indicator;

/// Output of [`ElderSafeZone`]: the active stop level and the trend direction.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ElderSafeZoneOutput {
    /// The `SafeZone` stop level — below price when long, above price when short.
    pub value: f64,
    /// Trend direction: `+1.0` long, `-1.0` short.
    pub direction: f64,
}

/// Elder `SafeZone` Stop — Alexander Elder's stop placed a multiple of the
/// **average market noise** away from price.
///
/// ```text
/// long  market noise = average downside penetration = mean( prev_low − low | low < prev_low )
/// short market noise = average upside  penetration = mean( high − prev_high | high > prev_high )
/// long  stop = ratchet_up(   low_t  − coeff · avg_down_penetration )
/// short stop = ratchet_down( high_t + coeff · avg_up_penetration   )
/// ```
///
/// Elder defines *noise* in an uptrend as the part of each bar that pokes below
/// the previous bar's low (a "downside penetration"). Averaging those
/// penetrations over a lookback and placing the stop `coeff` multiples below the
/// current low keeps the stop just outside normal pullbacks while still exiting on
/// a genuine reversal. The stop trails in the trend's favour and flips when price
/// closes through it. The average uses only the bars that actually penetrated
/// (Elder's definition), so a noiseless trend gives a tight stop at the bar's
/// extreme.
///
/// The first bar seeds the prior candle; the next `period` bars accumulate the
/// penetration statistics, so the first stop lands after `period + 1` inputs.
/// Each `update` is O(1).
///
/// # Example
///
/// ```
/// use wickra_core::{Candle, Indicator, ElderSafeZone};
///
/// let mut indicator = ElderSafeZone::new(14, 2.0).unwrap();
/// let mut last = None;
/// for i in 0..60 {
///     let base = 100.0 + f64::from(i);
///     let c = Candle::new(base, base + 2.0, base - 2.0, base + 1.0, 1_000.0, 0).unwrap();
///     last = indicator.update(c);
/// }
/// assert!(last.is_some());
/// ```
#[derive(Debug, Clone)]
pub struct ElderSafeZone {
    period: usize,
    coeff: f64,
    prev: Option<Candle>,
    down_pen: VecDeque<f64>,
    up_pen: VecDeque<f64>,
    down_sum: f64,
    up_sum: f64,
    down_count: usize,
    up_count: usize,
    direction: f64,
    stop: f64,
    last: Option<ElderSafeZoneOutput>,
}

impl ElderSafeZone {
    /// Construct an Elder `SafeZone` stop with the given averaging `period` and
    /// noise `coeff`icient.
    ///
    /// # Errors
    ///
    /// Returns [`Error::PeriodZero`] if `period == 0` and
    /// [`Error::NonPositiveMultiplier`] if `coeff` is not finite and positive.
    pub fn new(period: usize, coeff: f64) -> Result<Self> {
        if period == 0 {
            return Err(Error::PeriodZero);
        }
        if period > crate::error::MAX_PERIOD {
            return Err(Error::InvalidPeriod {
                message: crate::error::PERIOD_ABOVE_MAX,
            });
        }
        if !coeff.is_finite() || coeff <= 0.0 {
            return Err(Error::NonPositiveMultiplier);
        }
        Ok(Self {
            period,
            coeff,
            prev: None,
            down_pen: VecDeque::with_capacity(period),
            up_pen: VecDeque::with_capacity(period),
            down_sum: 0.0,
            up_sum: 0.0,
            down_count: 0,
            up_count: 0,
            direction: 0.0,
            stop: 0.0,
            last: None,
        })
    }

    /// Configured `(period, coeff)`.
    pub const fn params(&self) -> (usize, f64) {
        (self.period, self.coeff)
    }

    /// Current value if available.
    pub const fn value(&self) -> Option<ElderSafeZoneOutput> {
        self.last
    }

    fn push(window: &mut VecDeque<f64>, sum: &mut f64, count: &mut usize, period: usize, pen: f64) {
        if window.len() == period {
            let old = window.pop_front().expect("non-empty");
            *sum -= old;
            if old > 0.0 {
                *count -= 1;
            }
        }
        window.push_back(pen);
        *sum += pen;
        if pen > 0.0 {
            *count += 1;
        }
    }

    fn avg(sum: f64, count: usize) -> f64 {
        if count == 0 {
            0.0
        } else {
            sum / count as f64
        }
    }
}

impl Indicator for ElderSafeZone {
    type Input = Candle;
    type Output = ElderSafeZoneOutput;

    fn update(&mut self, candle: Candle) -> Option<ElderSafeZoneOutput> {
        let Some(prev) = self.prev else {
            self.prev = Some(candle);
            return None;
        };
        let dp = (prev.low - candle.low).max(0.0);
        let up = (candle.high - prev.high).max(0.0);
        self.prev = Some(candle);

        Self::push(
            &mut self.down_pen,
            &mut self.down_sum,
            &mut self.down_count,
            self.period,
            dp,
        );
        Self::push(
            &mut self.up_pen,
            &mut self.up_sum,
            &mut self.up_count,
            self.period,
            up,
        );
        if self.down_pen.len() < self.period {
            return None;
        }

        let avg_down = Self::avg(self.down_sum, self.down_count);
        let avg_up = Self::avg(self.up_sum, self.up_count);

        if self.direction == 0.0 {
            self.direction = 1.0;
            self.stop = candle.low - self.coeff * avg_down;
        } else if self.direction > 0.0 {
            let raw = candle.low - self.coeff * avg_down;
            self.stop = self.stop.max(raw);
            if candle.close < self.stop {
                self.direction = -1.0;
                self.stop = candle.high + self.coeff * avg_up;
            }
        } else {
            let raw = candle.high + self.coeff * avg_up;
            self.stop = self.stop.min(raw);
            if candle.close > self.stop {
                self.direction = 1.0;
                self.stop = candle.low - self.coeff * avg_down;
            }
        }

        let out = ElderSafeZoneOutput {
            value: self.stop,
            direction: self.direction,
        };
        self.last = Some(out);
        Some(out)
    }

    fn reset(&mut self) {
        self.prev = None;
        self.down_pen.clear();
        self.up_pen.clear();
        self.down_sum = 0.0;
        self.up_sum = 0.0;
        self.down_count = 0;
        self.up_count = 0;
        self.direction = 0.0;
        self.stop = 0.0;
        self.last = None;
    }

    #[inline]
    fn warmup_period(&self) -> usize {
        self.period + 1
    }

    #[inline]
    fn is_ready(&self) -> bool {
        self.last.is_some()
    }

    #[inline]
    fn name(&self) -> &'static str {
        "ElderSafeZone"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;

    fn c(high: f64, low: f64, close: f64) -> Candle {
        Candle::new_unchecked(f64::midpoint(high, low), high, low, close, 1_000.0, 0)
    }

    #[test]
    fn rejects_invalid_params() {
        assert!(matches!(ElderSafeZone::new(0, 2.0), Err(Error::PeriodZero)));
        assert!(matches!(
            ElderSafeZone::new(14, 0.0),
            Err(Error::NonPositiveMultiplier)
        ));
        assert!(matches!(
            ElderSafeZone::new(14, -1.0),
            Err(Error::NonPositiveMultiplier)
        ));
    }

    #[test]
    fn accessors_and_metadata() {
        let e = ElderSafeZone::new(14, 2.0).unwrap();
        assert_eq!(e.params(), (14, 2.0));
        assert_eq!(e.warmup_period(), 15);
        assert_eq!(e.name(), "ElderSafeZone");
        assert!(!e.is_ready());
        assert_eq!(e.value(), None);
    }

    #[test]
    fn first_emission_at_warmup_period() {
        let mut e = ElderSafeZone::new(3, 2.0).unwrap();
        let candles: Vec<Candle> = (0..8)
            .map(|i| {
                let base = 100.0 + f64::from(i);
                c(base + 1.0, base - 1.0, base)
            })
            .collect();
        let out = e.batch(&candles);
        let warmup = e.warmup_period(); // 4
        assert_eq!(warmup, 4);
        for v in out.iter().take(warmup - 1) {
            assert!(v.is_none());
        }
        assert!(out[warmup - 1].is_some());
    }

    #[test]
    fn uptrend_keeps_stop_below_price() {
        let mut e = ElderSafeZone::new(5, 2.0).unwrap();
        let candles: Vec<Candle> = (0..60)
            .map(|i| {
                let base = 100.0 + 2.0 * f64::from(i);
                c(base + 1.0, base - 1.0, base + 0.5)
            })
            .collect();
        for (o, candle) in e.batch(&candles).into_iter().zip(candles.iter()) {
            if let Some(o) = o {
                assert_eq!(o.direction, 1.0);
                assert!(o.value <= candle.close);
            }
        }
    }

    #[test]
    fn noiseless_trend_stop_sits_at_low() {
        // Every bar makes a higher low -> no downside penetration -> avg 0 ->
        // the stop sits exactly at the bar's low.
        let mut e = ElderSafeZone::new(3, 2.0).unwrap();
        let candles: Vec<Candle> = (0..10)
            .map(|i| {
                let base = 100.0 + f64::from(i);
                c(base + 1.0, base - 1.0, base + 0.5)
            })
            .collect();
        let out = e.batch(&candles);
        let last_candle = candles.last().unwrap();
        let last = out.last().unwrap().unwrap();
        assert!((last.value - last_candle.low).abs() < 1e-9);
    }

    #[test]
    fn flips_on_reversal() {
        let mut candles: Vec<Candle> = (0..40)
            .map(|i| {
                let base = 100.0 + f64::from(i);
                c(base + 1.0, base - 1.0, base + 0.5)
            })
            .collect();
        candles.extend((0..40).map(|i| {
            let base = 140.0 - f64::from(i);
            c(base + 1.0, base - 1.0, base - 0.5)
        }));
        let mut e = ElderSafeZone::new(5, 2.0).unwrap();
        let dirs: Vec<f64> = e
            .batch(&candles)
            .into_iter()
            .flatten()
            .map(|o| o.direction)
            .collect();
        assert!(dirs.iter().any(|&d| d > 0.0));
        assert!(dirs.iter().any(|&d| d < 0.0));
    }

    #[test]
    fn reset_clears_state() {
        let mut e = ElderSafeZone::new(5, 2.0).unwrap();
        let candles: Vec<Candle> = (0..40)
            .map(|i| {
                let base = 100.0 + f64::from(i);
                c(base + 1.0, base - 1.0, base + 0.5)
            })
            .collect();
        e.batch(&candles);
        assert!(e.is_ready());
        e.reset();
        assert!(!e.is_ready());
        assert_eq!(e.value(), None);
        assert_eq!(e.update(candles[0]), None);
    }

    #[test]
    fn batch_equals_streaming() {
        let candles: Vec<Candle> = (0..120)
            .map(|i| {
                let base = 100.0 + (f64::from(i) * 0.25).sin() * 9.0;
                c(base + 2.0, base - 1.5, base + 0.5)
            })
            .collect();
        let batch = ElderSafeZone::new(14, 2.0).unwrap().batch(&candles);
        let mut b = ElderSafeZone::new(14, 2.0).unwrap();
        let streamed: Vec<_> = candles.iter().map(|c| b.update(*c)).collect();
        assert_eq!(batch, streamed);
    }

    #[test]
    fn rejects_period_above_maximum() {
        assert!(matches!(
            ElderSafeZone::new(crate::error::MAX_PERIOD + 1, 2.0),
            Err(Error::InvalidPeriod { .. })
        ));
    }
}
