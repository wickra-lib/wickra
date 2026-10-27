//! Elder Impulse System.

use crate::error::{Error, Result};
use crate::indicators::ema::Ema;
use crate::indicators::macd::MacdIndicator;
use crate::traits::Indicator;

/// Alexander Elder's Impulse System — a tri-state momentum gauge combining the
/// slope of an `EMA` trend filter with the slope of the `MACD` histogram.
///
/// On each bar Wickra reports:
///
/// - `+1` ("green / buy") when both the `EMA` trend and the `MACD` histogram
///   are rising bar-over-bar.
/// - `−1` ("red / sell") when both are falling.
/// - `0` ("blue / neutral") when the two disagree.
///
/// The defaults track Elder's *Come Into My Trading Room* parameterisation:
/// `EMA(13)` for the trend, `MACD(12, 26, 9)` for the histogram.
///
/// # Example
///
/// ```
/// use wickra_core::{ElderImpulse, Indicator};
///
/// let mut elder = ElderImpulse::classic();
/// let mut last = None;
/// for i in 0..120 {
///     last = elder.update(100.0 + f64::from(i));
/// }
/// assert!(last.is_some());
/// ```
#[derive(Debug, Clone)]
pub struct ElderImpulse {
    ema_period: usize,
    macd_fast: usize,
    macd_slow: usize,
    macd_signal: usize,
    ema: Ema,
    macd: MacdIndicator,
    prev_ema: Option<f64>,
    prev_hist: Option<f64>,
    current: Option<f64>,
}

impl ElderImpulse {
    /// # Errors
    /// Forwarded from [`Ema::new`] / [`MacdIndicator::new`].
    pub fn new(
        ema_period: usize,
        macd_fast: usize,
        macd_slow: usize,
        macd_signal: usize,
    ) -> Result<Self> {
        if ema_period == 0 {
            return Err(Error::PeriodZero);
        }
        if ema_period > crate::error::MAX_PERIOD {
            return Err(Error::InvalidPeriod {
                message: crate::error::PERIOD_ABOVE_MAX,
            });
        }
        Ok(Self {
            ema_period,
            macd_fast,
            macd_slow,
            macd_signal,
            ema: Ema::new(ema_period)?,
            macd: MacdIndicator::new(macd_fast, macd_slow, macd_signal)?,
            prev_ema: None,
            prev_hist: None,
            current: None,
        })
    }

    /// Elder's recommended defaults `(ema_period = 13, macd = 12/26/9)`.
    pub fn classic() -> Self {
        Self::new(13, 12, 26, 9).expect("classic Elder Impulse parameters are valid")
    }

    /// Configured `(ema_period, macd_fast, macd_slow, macd_signal)`.
    pub const fn periods(&self) -> (usize, usize, usize, usize) {
        (
            self.ema_period,
            self.macd_fast,
            self.macd_slow,
            self.macd_signal,
        )
    }
}

impl Indicator for ElderImpulse {
    type Input = f64;
    type Output = f64;

    #[inline]
    fn update(&mut self, input: f64) -> Option<f64> {
        // Feed both branches on every input so they warm in parallel.
        let ema_now = self.ema.update(input);
        let macd_now = self.macd.update(input);
        let (ema_now, macd_now) = (ema_now?, macd_now?);

        // The Impulse needs two consecutive readings on both branches to
        // judge direction. The first ready bar seeds prev_*; the second emits.
        let prev_ema = self.prev_ema;
        let prev_hist = self.prev_hist;
        self.prev_ema = Some(ema_now);
        self.prev_hist = Some(macd_now.histogram);
        let prev_ema = prev_ema?;
        let prev_hist = prev_hist?;

        let ema_rising = ema_now > prev_ema;
        let ema_falling = ema_now < prev_ema;
        let hist_rising = macd_now.histogram > prev_hist;
        let hist_falling = macd_now.histogram < prev_hist;

        let value = if ema_rising && hist_rising {
            1.0
        } else if ema_falling && hist_falling {
            -1.0
        } else {
            0.0
        };
        self.current = Some(value);
        Some(value)
    }

    fn reset(&mut self) {
        self.ema.reset();
        self.macd.reset();
        self.prev_ema = None;
        self.prev_hist = None;
        self.current = None;
    }

    #[inline]
    fn warmup_period(&self) -> usize {
        // MACD's warmup is slow + signal − 1; EMA's is ema_period. The
        // slowest branch fires the *first* impulse-ready reading, but
        // judging direction needs one *more* bar on top.
        let macd_warmup = self.macd_slow + self.macd_signal - 1;
        self.ema_period.max(macd_warmup) + 1
    }

    #[inline]
    fn is_ready(&self) -> bool {
        self.current.is_some()
    }

    #[inline]
    fn name(&self) -> &'static str {
        "ElderImpulse"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;

    #[test]
    fn rejects_zero_period() {
        assert!(matches!(
            ElderImpulse::new(0, 12, 26, 9),
            Err(Error::PeriodZero)
        ));
        assert!(matches!(
            ElderImpulse::new(13, 0, 26, 9),
            Err(Error::PeriodZero)
        ));
    }

    #[test]
    fn rejects_invalid_macd_params() {
        // MacdIndicator validates fast < slow.
        assert!(ElderImpulse::new(13, 26, 12, 9).is_err());
    }

    #[test]
    fn accessors_and_metadata() {
        let elder = ElderImpulse::classic();
        assert_eq!(elder.periods(), (13, 12, 26, 9));
        assert_eq!(elder.name(), "ElderImpulse");
    }

    #[test]
    fn classic_factory() {
        assert_eq!(ElderImpulse::classic().periods(), (13, 12, 26, 9));
    }

    #[test]
    fn constant_series_yields_neutral() {
        // Both EMA and MACD-histogram are flat on a constant series, so
        // neither is rising nor falling -> Impulse = 0.
        let mut elder = ElderImpulse::classic();
        let out = elder.batch(&[42.0_f64; 120]);
        // Take values from the post-warmup region.
        for v in out.iter().skip(40).flatten() {
            assert_eq!(*v, 0.0);
        }
    }

    #[test]
    fn pure_uptrend_signals_buy() {
        // Monotonic uptrend: EMA rises every bar; MACD histogram is positive
        // and (after the slow EMA catches up) also rising bar-over-bar.
        let mut elder = ElderImpulse::classic();
        for i in 1..=300 {
            elder.update(f64::from(i));
        }
        // The final reading should be +1 (buy) or 0 — never -1 on a clean
        // up trend.
        let v = elder.current.unwrap();
        assert!(v >= 0.0, "uptrend should not signal sell: {v}");
    }

    #[test]
    fn warmup_emits_first_value_at_warmup_period() {
        let mut elder = ElderImpulse::new(3, 2, 4, 3).unwrap();
        // MACD warmup: 4 + 3 - 1 = 6; EMA warmup: 3; max = 6; +1 for the
        // direction bar = 7.
        assert_eq!(elder.warmup_period(), 7);
        let prices: Vec<f64> = (1..=10).map(f64::from).collect();
        let out = elder.batch(&prices);
        for v in out.iter().take(6) {
            assert!(v.is_none());
        }
        assert!(out[6].is_some());
    }

    #[test]
    fn batch_equals_streaming() {
        let prices: Vec<f64> = (1..=200)
            .map(|i| 100.0 + (f64::from(i) * 0.2).sin() * 5.0)
            .collect();
        let mut a = ElderImpulse::classic();
        let mut b = ElderImpulse::classic();
        assert_eq!(
            a.batch(&prices),
            prices.iter().map(|p| b.update(*p)).collect::<Vec<_>>()
        );
    }

    #[test]
    fn reset_clears_state() {
        let mut elder = ElderImpulse::classic();
        elder.batch(&(1..=200).map(f64::from).collect::<Vec<_>>());
        assert!(elder.is_ready());
        elder.reset();
        assert!(!elder.is_ready());
    }

    #[test]
    fn rejects_ema_period_above_maximum() {
        assert!(matches!(
            ElderImpulse::new(crate::error::MAX_PERIOD + 1, 2, 4, 3),
            Err(Error::InvalidPeriod { .. })
        ));
    }
}
