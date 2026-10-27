//! Pretty Good Oscillator (PGO).

use crate::error::{Error, Result};
use crate::indicators::ema::Ema;
use crate::indicators::sma::Sma;
use crate::indicators::true_range::TrueRange;
use crate::ohlcv::Candle;
use crate::traits::Indicator;

/// Mark Johnson's Pretty Good Oscillator — displacement of the close from its
/// `period`-bar `SMA`, normalised by the `period`-bar `EMA` of the True Range.
///
/// ```text
/// PGO_t = (close_t − SMA(close, period)_t) / EMA(TR_t, period)
/// ```
///
/// The numerator is positive when the close is above its mean of the last
/// `period` bars and negative when below. The denominator is the EMA-smoothed
/// volatility scale, so PGO is roughly "how many ATR-equivalents is the close
/// away from its mean?". Johnson's heuristic: cross above `+3` is a long entry,
/// below `−3` a short entry.
///
/// The first output lands once both inner indicators have warmed up — for the
/// shared `period` parameter, that is exactly `period` candles in.
///
/// # Example
///
/// ```
/// use wickra_core::{Candle, Indicator, Pgo};
///
/// let mut pgo = Pgo::new(14).unwrap();
/// let mut last = None;
/// for i in 0..40 {
///     let p = 100.0 + f64::from(i);
///     let candle = Candle::new(p, p + 1.0, p - 1.0, p, 1.0, i64::from(i)).unwrap();
///     last = pgo.update(candle);
/// }
/// assert!(last.is_some());
/// ```
#[derive(Debug, Clone)]
pub struct Pgo {
    period: usize,
    sma: Sma,
    tr: TrueRange,
    ema_tr: Ema,
    current: Option<f64>,
}

impl Pgo {
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
        Ok(Self {
            period,
            sma: Sma::new(period)?,
            tr: TrueRange::new(),
            ema_tr: Ema::new(period)?,
            current: None,
        })
    }

    /// Configured period.
    pub const fn period(&self) -> usize {
        self.period
    }
}

impl Indicator for Pgo {
    type Input = Candle;
    type Output = f64;

    #[inline]
    fn update(&mut self, candle: Candle) -> Option<f64> {
        let mean = self.sma.update(candle.close);
        // TrueRange always emits (it falls back to high − low without a
        // previous close), so we can unwrap the inner option safely.
        let tr = self.tr.update(candle).expect("TrueRange always emits");
        let ema_tr = self.ema_tr.update(tr);
        let mean = mean?;
        let ema_tr = ema_tr?;
        if ema_tr <= 0.0 {
            // Pathological window of perfectly flat candles: divisor zero.
            // Hold the previous value rather than blow up.
            return self.current;
        }
        let value = (candle.close - mean) / ema_tr;
        self.current = Some(value);
        Some(value)
    }

    fn reset(&mut self) {
        self.sma.reset();
        self.tr.reset();
        self.ema_tr.reset();
        self.current = None;
    }

    #[inline]
    fn warmup_period(&self) -> usize {
        // Both inner state machines reach readiness at exactly `period`
        // candles, so PGO emits at the same boundary.
        self.period
    }

    #[inline]
    fn is_ready(&self) -> bool {
        self.current.is_some()
    }

    #[inline]
    fn name(&self) -> &'static str {
        "PGO"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;
    use approx::assert_relative_eq;

    fn candle(close: f64, high: f64, low: f64, ts: i64) -> Candle {
        Candle::new(close, high, low, close, 1.0, ts).unwrap()
    }

    #[test]
    fn rejects_zero_period() {
        assert!(matches!(Pgo::new(0), Err(Error::PeriodZero)));
    }

    #[test]
    fn accessors_and_metadata() {
        let mut p = Pgo::new(14).unwrap();
        assert_eq!(p.period(), 14);
        assert_eq!(p.warmup_period(), 14);
        assert_eq!(p.name(), "PGO");
        assert!(!p.is_ready());
        for i in 0..14 {
            p.update(candle(10.0, 11.0, 9.0, i));
        }
        assert!(p.is_ready());
    }

    #[test]
    fn flat_close_yields_zero_numerator() {
        // Constant close -> SMA == close, so numerator is 0 regardless of the
        // TR-EMA in the denominator (which is non-zero thanks to spread).
        let mut p = Pgo::new(5).unwrap();
        let mut out = None;
        for i in 0..20 {
            out = p.update(candle(10.0, 11.0, 9.0, i));
        }
        let v = out.unwrap();
        assert_relative_eq!(v, 0.0, epsilon = 1e-12);
    }

    #[test]
    fn warmup_emits_first_value_at_period() {
        let mut p = Pgo::new(3).unwrap();
        for i in 0..2 {
            assert_eq!(p.update(candle(10.0, 11.0, 9.0, i)), None);
        }
        assert!(p.update(candle(10.0, 11.0, 9.0, 2)).is_some());
    }

    #[test]
    fn close_above_mean_is_positive() {
        // Rising series: latest close sits above its SMA, so PGO > 0.
        let mut p = Pgo::new(5).unwrap();
        for i in 0..20 {
            let c = 10.0 + f64::from(i);
            p.update(candle(c, c + 0.5, c - 0.5, i64::from(i)));
        }
        // Use the last value implicitly.
        let last = p.update(candle(40.0, 40.5, 39.5, 20)).expect("PGO is warm");
        assert!(
            last > 0.0,
            "PGO on rising series should be positive: {last}"
        );
    }

    #[test]
    fn zero_tr_holds_value() {
        // Every candle is a single point (high == low == close): TR is zero,
        // EMA(TR) collapses to zero -> PGO holds its previous value.
        let mut p = Pgo::new(3).unwrap();
        p.update(candle(10.0, 10.0, 10.0, 0));
        p.update(candle(10.0, 10.0, 10.0, 1));
        let v = p.update(candle(10.0, 10.0, 10.0, 2));
        // With zero denominator on the first ready step we have no previous
        // value, so the indicator stays unset.
        assert!(v.is_none(), "expected hold, got {v:?}");
    }

    #[test]
    fn batch_equals_streaming() {
        let candles: Vec<Candle> = (0..60_i64)
            .map(|i| {
                let c = 100.0 + (i as f64 * 0.3).sin() * 8.0;
                candle(c, c + 1.0, c - 1.0, i)
            })
            .collect();
        let batch = Pgo::new(14).unwrap().batch(&candles);
        let mut b = Pgo::new(14).unwrap();
        let streamed: Vec<_> = candles.iter().map(|c| b.update(*c)).collect();
        assert_eq!(batch, streamed);
    }

    #[test]
    fn reset_clears_state() {
        let mut p = Pgo::new(5).unwrap();
        for i in 0..20 {
            p.update(candle(10.0, 11.0, 9.0, i));
        }
        assert!(p.is_ready());
        p.reset();
        assert!(!p.is_ready());
        assert_eq!(p.update(candle(10.0, 11.0, 9.0, 0)), None);
    }

    #[test]
    fn rejects_period_above_maximum() {
        assert!(matches!(
            Pgo::new(crate::error::MAX_PERIOD + 1),
            Err(Error::InvalidPeriod { .. })
        ));
    }
}
