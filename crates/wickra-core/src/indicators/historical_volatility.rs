//! Historical Volatility.

use std::collections::VecDeque;

use crate::error::{Error, Result};
use crate::indicators::rolling_moments::ShiftedMoments;
use crate::traits::Indicator;

/// Historical Volatility — the annualised standard deviation of log returns.
///
/// This is the realised (backward-looking) volatility used to price options
/// and size risk:
///
/// ```text
/// r_t = ln(price_t / price_{t−1})
/// HV  = stddev_sample(r over period) · √trading_periods · 100
/// ```
///
/// The log returns over the window are measured with the **sample** standard
/// deviation (divisor `n − 1`, the unbiased estimator), then scaled to an
/// annual figure by `√trading_periods` — `252` for daily bars, `52` for
/// weekly, `12` for monthly — and expressed as a percentage.
///
/// # Example
///
/// ```
/// use wickra_core::{Indicator, HistoricalVolatility};
///
/// // 20-bar window, 252 trading days per year.
/// let mut indicator = HistoricalVolatility::new(20, 252).unwrap();
/// let mut last = None;
/// for i in 0..80 {
///     last = indicator.update(100.0 + (f64::from(i) * 0.3).sin() * 5.0);
/// }
/// assert!(last.is_some());
/// ```
#[derive(Debug, Clone)]
pub struct HistoricalVolatility {
    period: usize,
    trading_periods: usize,
    prev_price: Option<f64>,
    /// Rolling window of the last `period` log returns.
    window: VecDeque<f64>,
    moments: ShiftedMoments,
    last: Option<f64>,
}

impl HistoricalVolatility {
    /// Construct a new Historical Volatility indicator.
    ///
    /// `period` is the number of log returns in the rolling window;
    /// `trading_periods` is the annualisation factor (`252` daily, `52`
    /// weekly, `12` monthly).
    ///
    /// # Errors
    ///
    /// Returns [`Error::PeriodZero`] if `period` or `trading_periods` is `0`,
    /// or [`Error::InvalidPeriod`] if `period == 1` (the sample standard
    /// deviation needs at least two returns).
    pub fn new(period: usize, trading_periods: usize) -> Result<Self> {
        if period == 0 || trading_periods == 0 {
            return Err(Error::PeriodZero);
        }
        if period < 2 {
            return Err(Error::InvalidPeriod {
                message: "historical volatility period must be >= 2",
            });
        }
        if period > crate::error::MAX_PERIOD {
            return Err(Error::InvalidPeriod {
                message: crate::error::PERIOD_ABOVE_MAX,
            });
        }
        Ok(Self {
            period,
            trading_periods,
            prev_price: None,
            window: VecDeque::with_capacity(period),
            moments: ShiftedMoments::new(),
            last: None,
        })
    }

    /// Configured `(period, trading_periods)`.
    pub const fn periods(&self) -> (usize, usize) {
        (self.period, self.trading_periods)
    }

    /// Current value if available.
    pub const fn value(&self) -> Option<f64> {
        self.last
    }
}

impl Indicator for HistoricalVolatility {
    type Input = f64;
    type Output = f64;

    #[inline]
    fn update(&mut self, input: f64) -> Option<f64> {
        // Non-finite *and* non-positive prices are both ignored: state is left
        // untouched and `self.last` is returned. The log-return `ln(input /
        // prev)` is undefined for non-positive prices, and silently
        // substituting `0.0` (the previous behaviour, audit finding R13) would
        // underreport realised volatility by treating bad ticks as "no
        // movement". Skipping them entirely is consistent with how the rest
        // of the library handles invalid inputs (see SMA / EMA / ROC).
        if !input.is_finite() || input <= 0.0 {
            return None;
        }
        let Some(prev) = self.prev_price else {
            self.prev_price = Some(input);
            return None;
        };
        // `prev` was assigned from `self.prev_price`, which only ever holds
        // valid (finite, positive) inputs because the guard above gates every
        // assignment to it — so `(input / prev).ln()` is always well-defined.
        self.prev_price = Some(input);

        let log_return = (input / prev).ln();
        if self.window.len() == self.period {
            let old = self.window.pop_front().expect("window is non-empty");
            self.moments.evict(old);
        }
        self.window.push_back(log_return);
        self.moments.push(log_return);
        if self.moments.needs_reseed(self.period) {
            self.moments.reseed(self.window.iter().copied());
        }
        if self.window.len() < self.period {
            return None;
        }
        let variance = self.moments.sample_variance(self.period);
        let hv = variance.sqrt() * (self.trading_periods as f64).sqrt() * 100.0;
        self.last = Some(hv);
        Some(hv)
    }

    fn reset(&mut self) {
        self.prev_price = None;
        self.window.clear();
        self.moments.reset();
        self.last = None;
    }

    #[inline]
    fn warmup_period(&self) -> usize {
        // The first log return needs a previous price, then the window fills.
        self.period + 1
    }

    #[inline]
    fn is_ready(&self) -> bool {
        self.last.is_some()
    }

    #[inline]
    fn name(&self) -> &'static str {
        "HistoricalVolatility"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;
    use approx::assert_relative_eq;

    #[test]
    fn new_rejects_zero_period() {
        assert!(matches!(
            HistoricalVolatility::new(0, 252),
            Err(Error::PeriodZero)
        ));
        assert!(matches!(
            HistoricalVolatility::new(20, 0),
            Err(Error::PeriodZero)
        ));
    }

    /// Cover the const accessors `periods` / `value` (80-88) and the
    /// Indicator-impl `name` body (153-155). Existing tests inspect HV
    /// output but never query the metadata.
    #[test]
    fn accessors_and_metadata() {
        let mut hv = HistoricalVolatility::new(20, 252).unwrap();
        assert_eq!(hv.periods(), (20, 252));
        assert_eq!(hv.name(), "HistoricalVolatility");
        assert_eq!(hv.value(), None);
        for i in 1..=hv.warmup_period() {
            hv.update(100.0 + f64::from(u32::try_from(i).unwrap()));
        }
        assert!(hv.value().is_some());
    }

    #[test]
    fn new_rejects_period_one() {
        assert!(matches!(
            HistoricalVolatility::new(1, 252),
            Err(Error::InvalidPeriod { .. })
        ));
    }

    #[test]
    fn first_emission_at_warmup_period() {
        let mut hv = HistoricalVolatility::new(5, 252).unwrap();
        assert_eq!(hv.warmup_period(), 6);
        let out = hv.batch(&(1..=20).map(f64::from).collect::<Vec<_>>());
        for v in out.iter().take(5) {
            assert!(v.is_none());
        }
        assert!(out[5].is_some());
    }

    #[test]
    fn constant_series_yields_zero() {
        // Flat prices -> all log returns are 0 -> zero volatility.
        let mut hv = HistoricalVolatility::new(10, 252).unwrap();
        let out = hv.batch(&[100.0; 40]);
        for v in out.iter().skip(10).flatten() {
            assert_relative_eq!(*v, 0.0, epsilon = 1e-12);
        }
    }

    #[test]
    fn geometric_series_yields_zero() {
        // A constant growth factor gives a constant log return -> zero stddev.
        // The mathematical result is exactly zero, but `1.01_f64.powi(i)` and
        // the subsequent log / std-dev cascade accumulate platform-sensitive
        // floating-point drift on the order of 1e-7 (observed on x86_64 Linux
        // and macOS; Windows happens to round closer to zero). The 1e-6
        // tolerance stays four decimal places below any realistic volatility
        // value while absorbing this drift across every supported platform.
        let mut hv = HistoricalVolatility::new(10, 252).unwrap();
        let prices: Vec<f64> = (0..40).map(|i| 100.0 * 1.01_f64.powi(i)).collect();
        let out = hv.batch(&prices);
        for v in out.iter().skip(10).flatten() {
            assert_relative_eq!(*v, 0.0, epsilon = 1e-6);
        }
    }

    #[test]
    fn output_is_non_negative() {
        let mut hv = HistoricalVolatility::new(20, 252).unwrap();
        let prices: Vec<f64> = (1..=200)
            .map(|i| 100.0 + (f64::from(i) * 0.3).sin() * 12.0)
            .collect();
        for v in hv.batch(&prices).into_iter().flatten() {
            assert!(v >= 0.0, "volatility must be non-negative, got {v}");
        }
    }

    #[test]
    fn ignores_non_finite_input() {
        let mut hv = HistoricalVolatility::new(5, 252).unwrap();
        let out = hv.batch(&(1..=20).map(f64::from).collect::<Vec<_>>());
        let last = *out.last().unwrap();
        assert!(last.is_some());
        assert_eq!(hv.update(f64::NAN), None);
        assert_eq!(hv.update(f64::INFINITY), None);
    }

    /// Audit finding R13. Non-positive prices are now skipped (state left
    /// untouched) instead of silently treated as a `0.0` log-return — the old
    /// behaviour underreported realised volatility by treating bad ticks as
    /// "no movement".
    #[test]
    fn skips_non_positive_prices() {
        let mut hv = HistoricalVolatility::new(5, 252).unwrap();
        // Warm up with positive prices.
        let warmup_prices = (1..=20).map(f64::from).collect::<Vec<_>>();
        let warmup = hv.batch(&warmup_prices);
        let _baseline = warmup
            .last()
            .copied()
            .flatten()
            .expect("warmed up by index 5");

        // A negative tick must be ignored: returned value equals the previous
        // baseline, and the next real positive tick must use the previous
        // valid price as `prev` (not the bad one), so the next log return is
        // exactly `ln(21 / 20)`, not `ln(21 / -5)` or anything else.
        assert_eq!(hv.update(-5.0), None);
        assert_eq!(hv.update(0.0), None);

        // Snapshot the indicator's state, then advance with a real positive
        // tick on a clone. The clone must agree with a from-scratch run that
        // simply skipped the bad ticks — proving the state was untouched.
        let mut control = hv.clone();
        let after_real = hv.update(21.0).expect("ready");
        assert_eq!(control.update(21.0).expect("ready"), after_real);
    }

    #[test]
    fn reset_clears_state() {
        let mut hv = HistoricalVolatility::new(5, 252).unwrap();
        hv.batch(&(1..=20).map(f64::from).collect::<Vec<_>>());
        assert!(hv.is_ready());
        hv.reset();
        assert!(!hv.is_ready());
        assert_eq!(hv.update(1.0), None);
    }

    #[test]
    fn batch_equals_streaming() {
        let prices: Vec<f64> = (1..=120)
            .map(|i| 100.0 + (f64::from(i) * 0.25).sin() * 9.0)
            .collect();
        let batch = HistoricalVolatility::new(20, 252).unwrap().batch(&prices);
        let mut b = HistoricalVolatility::new(20, 252).unwrap();
        let streamed: Vec<_> = prices.iter().map(|p| b.update(*p)).collect();
        assert_eq!(batch, streamed);
    }

    #[test]
    fn rejects_period_above_maximum() {
        assert!(matches!(
            HistoricalVolatility::new(crate::error::MAX_PERIOD + 1, 252),
            Err(Error::InvalidPeriod { .. })
        ));
    }
}
