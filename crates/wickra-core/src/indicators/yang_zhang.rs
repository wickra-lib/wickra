//! Yang-Zhang Volatility (drift- and gap-robust OHLC estimator).

use std::collections::VecDeque;

use crate::error::{Error, Result};
use crate::indicators::rolling_moments::ShiftedMoments;
use crate::ohlcv::Candle;
use crate::traits::Indicator;

/// Yang-Zhang Volatility — combines overnight, open-to-close and
/// Rogers-Satchell volatilities into a single drift- and gap-robust
/// estimator.
///
/// Yang & Zhang (2000) showed that the three estimators below are
/// independent under a driftless GBM with overnight gaps, so a convex
/// combination of their (sample) variances has minimum estimation variance
/// at a specific blending factor `k`:
///
/// ```text
/// k        = 0.34 / (1.34 + (n + 1) / (n − 1))
/// σ²_on    = sample_var(ln(O_t / C_{t-1})    over n bars)         // overnight
/// σ²_oc    = sample_var(ln(C_t / O_t)        over n bars)         // open-to-close
/// σ²_rs    = mean(ln(H/C)·ln(H/O) + ln(L/C)·ln(L/O) over n bars)  // Rogers-Satchell
/// σ²_YZ    = σ²_on + k · σ²_oc + (1 − k) · σ²_rs
/// out      = √max(σ²_YZ, 0) · √trading_periods · 100
/// ```
///
/// The "sample" variance uses Bessel's correction (divisor `n − 1`), the
/// same convention as [`HistoricalVolatility`](crate::HistoricalVolatility).
///
/// This is the gold-standard OHLC estimator for assets with both
/// overnight gaps and intraday drift — equities, futures, and any
/// market that doesn't trade continuously. For pure intraday data
/// (where `C_{t-1} == O_t`), the overnight term vanishes and
/// Rogers-Satchell alone is sufficient.
///
/// # Example
///
/// ```
/// use wickra_core::{Candle, Indicator, YangZhangVolatility};
///
/// let mut indicator = YangZhangVolatility::new(20, 252).unwrap();
/// let mut last = None;
/// for i in 0..40 {
///     let base = 100.0 + f64::from(i);
///     let candle = Candle::new(base, base + 2.0, base - 2.0, base + 0.5, 1.0, i64::from(i))
///         .unwrap();
///     last = indicator.update(candle);
/// }
/// assert!(last.is_some());
/// ```
#[derive(Debug, Clone)]
pub struct YangZhangVolatility {
    period: usize,
    trading_periods: usize,
    k: f64,
    prev_close: Option<f64>,
    // Each window stores one f64 per bar in the rolling window.
    overnight: VecDeque<f64>,
    open_close: VecDeque<f64>,
    rs_samples: VecDeque<f64>,
    on_moments: ShiftedMoments,
    oc_moments: ShiftedMoments,
    sum_rs: f64,
    last: Option<f64>,
}

impl YangZhangVolatility {
    /// Construct a Yang-Zhang Volatility estimator.
    ///
    /// `period` is the rolling window of bars; `trading_periods` is the
    /// annualisation factor (`252` daily, `52` weekly, `12` monthly, or
    /// `1` for raw per-bar volatility).
    ///
    /// # Errors
    ///
    /// Returns [`Error::PeriodZero`] if either parameter is `0`, or
    /// [`Error::InvalidPeriod`] if `period < 2` (the sample variances
    /// inside Yang-Zhang need at least two samples).
    pub fn new(period: usize, trading_periods: usize) -> Result<Self> {
        if period == 0 || trading_periods == 0 {
            return Err(Error::PeriodZero);
        }
        if period < 2 {
            return Err(Error::InvalidPeriod {
                message: "Yang-Zhang period must be >= 2",
            });
        }
        if period > crate::error::MAX_PERIOD {
            return Err(Error::InvalidPeriod {
                message: crate::error::PERIOD_ABOVE_MAX,
            });
        }
        let n = period as f64;
        let k = 0.34 / (1.34 + (n + 1.0) / (n - 1.0));
        Ok(Self {
            period,
            trading_periods,
            k,
            prev_close: None,
            overnight: VecDeque::with_capacity(period),
            open_close: VecDeque::with_capacity(period),
            rs_samples: VecDeque::with_capacity(period),
            on_moments: ShiftedMoments::new(),
            oc_moments: ShiftedMoments::new(),
            sum_rs: 0.0,
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

    /// The Yang-Zhang blending factor `k` for this configuration.
    pub const fn k(&self) -> f64 {
        self.k
    }
}

impl Indicator for YangZhangVolatility {
    type Input = Candle;
    type Output = f64;

    fn update(&mut self, candle: Candle) -> Option<f64> {
        // The overnight log-return needs the previous bar's close. On the
        // first candle there is no previous close, so we only seed
        // `prev_close` and return None without touching any window.
        let Some(prev_c) = self.prev_close else {
            self.prev_close = Some(candle.close);
            return None;
        };
        self.prev_close = Some(candle.close);

        // Per-bar samples. `Candle::new` guarantees finite, positive OHLC
        // and the ordering invariants, so every ratio is well-defined.
        let on_sample = (candle.open / prev_c).ln();
        let oc_sample = (candle.close / candle.open).ln();
        let log_hc = (candle.high / candle.close).ln();
        let log_ho = (candle.high / candle.open).ln();
        let log_lc = (candle.low / candle.close).ln();
        let log_lo = (candle.low / candle.open).ln();
        let rs_sample = log_hc.mul_add(log_ho, log_lc * log_lo);

        // Roll the three windows.
        if self.overnight.len() == self.period {
            let old_on = self.overnight.pop_front().expect("window non-empty");
            self.on_moments.evict(old_on);
            let old_oc = self.open_close.pop_front().expect("window non-empty");
            self.oc_moments.evict(old_oc);
            let old_rs = self.rs_samples.pop_front().expect("window non-empty");
            self.sum_rs -= old_rs;
        }
        self.overnight.push_back(on_sample);
        self.on_moments.push(on_sample);
        self.open_close.push_back(oc_sample);
        self.oc_moments.push(oc_sample);
        if self.on_moments.needs_reseed(self.period) {
            self.on_moments.reseed(self.overnight.iter().copied());
            self.oc_moments.reseed(self.open_close.iter().copied());
        }
        self.rs_samples.push_back(rs_sample);
        self.sum_rs += rs_sample;

        if self.overnight.len() < self.period {
            return None;
        }

        let n = self.period as f64;
        // Sample variances (Bessel's correction), accumulated around a window
        // reference point so the two terms cannot cancel each other away.
        let var_on = self.on_moments.sample_variance(self.period);
        let var_oc = self.oc_moments.sample_variance(self.period);
        // Rogers-Satchell mean: each per-bar sample is already >= 0 by
        // construction, so the mean cannot be negative outside of FP.
        let var_rs = (self.sum_rs / n).max(0.0);

        let total = var_on + self.k * var_oc + (1.0 - self.k) * var_rs;
        let sigma = total.max(0.0).sqrt();
        let out = sigma * (self.trading_periods as f64).sqrt() * 100.0;
        self.last = Some(out);
        Some(out)
    }

    fn reset(&mut self) {
        self.prev_close = None;
        self.overnight.clear();
        self.open_close.clear();
        self.rs_samples.clear();
        self.on_moments.reset();
        self.oc_moments.reset();
        self.sum_rs = 0.0;
        self.last = None;
    }

    #[inline]
    fn warmup_period(&self) -> usize {
        // One bar to seed `prev_close`, then `period` more bars to fill
        // the rolling windows. First emit lands at index `period`, i.e.
        // the `(period + 1)`-th input.
        self.period + 1
    }

    #[inline]
    fn is_ready(&self) -> bool {
        self.last.is_some()
    }

    #[inline]
    fn name(&self) -> &'static str {
        "YangZhangVolatility"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;
    use approx::assert_relative_eq;

    fn candle(o: f64, h: f64, l: f64, c: f64, ts: i64) -> Candle {
        Candle::new(o, h, l, c, 1.0, ts).unwrap()
    }

    #[test]
    fn rejects_zero_period() {
        assert!(matches!(
            YangZhangVolatility::new(0, 252),
            Err(Error::PeriodZero)
        ));
        assert!(matches!(
            YangZhangVolatility::new(20, 0),
            Err(Error::PeriodZero)
        ));
    }

    #[test]
    fn rejects_period_one() {
        assert!(matches!(
            YangZhangVolatility::new(1, 252),
            Err(Error::InvalidPeriod { .. })
        ));
    }

    #[test]
    fn accessors_and_metadata() {
        let yz = YangZhangVolatility::new(20, 252).unwrap();
        assert_eq!(yz.periods(), (20, 252));
        assert_eq!(yz.value(), None);
        assert_eq!(yz.warmup_period(), 21);
        assert_eq!(yz.name(), "YangZhangVolatility");
        assert!(!yz.is_ready());

        // k = 0.34 / (1.34 + 21/19) ≈ 0.139
        let n = 20.0;
        let expected_k = 0.34 / (1.34 + (n + 1.0) / (n - 1.0));
        assert_relative_eq!(yz.k(), expected_k, epsilon = 1e-12);
    }

    #[test]
    fn zero_movement_yields_zero() {
        // O == H == L == C and constant across bars -> every per-bar sample
        // is zero, all three variances are zero, output is zero.
        let candles: Vec<Candle> = (0..30).map(|i| candle(10.0, 10.0, 10.0, 10.0, i)).collect();
        let mut yz = YangZhangVolatility::new(14, 1).unwrap();
        for v in yz.batch(&candles).into_iter().flatten() {
            assert_relative_eq!(v, 0.0, epsilon = 1e-12);
        }
    }

    #[test]
    fn output_is_non_negative() {
        let mut yz = YangZhangVolatility::new(14, 252).unwrap();
        let candles: Vec<Candle> = (0..200)
            .map(|i| {
                let base = 100.0 + (f64::from(i) * 0.3).sin() * 12.0;
                let half = 0.5 + (f64::from(i) * 0.13).cos().abs() * 1.5;
                let open = base - 0.1;
                let close = base + 0.2;
                candle(open, base + half, base - half, close, i64::from(i))
            })
            .collect();
        for v in yz.batch(&candles).into_iter().flatten() {
            assert!(v >= 0.0, "Yang-Zhang must be non-negative: {v}");
        }
    }

    #[test]
    fn annualisation_scales_by_sqrt_trading_periods() {
        let candles: Vec<Candle> = (0..40)
            .map(|i| {
                let base = 100.0 + (f64::from(i) * 0.3).sin() * 5.0;
                let half = 1.0 + (f64::from(i) * 0.2).cos().abs();
                candle(
                    base - 0.05,
                    base + half,
                    base - half,
                    base + 0.3,
                    i64::from(i),
                )
            })
            .collect();
        let raw = YangZhangVolatility::new(10, 1).unwrap().batch(&candles);
        let annual = YangZhangVolatility::new(10, 252).unwrap().batch(&candles);
        let scale = (252.0_f64).sqrt();
        for (r, a) in raw.iter().zip(annual.iter()) {
            assert_eq!(r.is_some(), a.is_some(), "warmup mismatch");
            if let (Some(r), Some(a)) = (r, a) {
                assert_relative_eq!(*a, r * scale, epsilon = 1e-9);
            }
        }
    }

    #[test]
    fn first_emission_at_warmup_period() {
        // period = 5 -> first ready at index 5 (the 6th candle): one bar
        // seeds prev_close, the next 5 fill the rolling window.
        let candles: Vec<Candle> = (0..20_i64)
            .map(|i| {
                let base = 100.0 + (i as f64 * 0.4).sin() * 3.0;
                candle(base, base + 1.0, base - 1.0, base + 0.2, i)
            })
            .collect();
        let mut yz = YangZhangVolatility::new(5, 1).unwrap();
        assert_eq!(yz.warmup_period(), 6);
        let out = yz.batch(&candles);
        for v in out.iter().take(5) {
            assert!(v.is_none(), "indicator must still be warming up");
        }
        assert!(
            out[5].is_some(),
            "first value lands at warmup_period - 1 = 5"
        );
    }

    #[test]
    fn batch_equals_streaming() {
        let candles: Vec<Candle> = (0..80)
            .map(|i| {
                let base = 100.0 + (f64::from(i) * 0.25).sin() * 6.0;
                let half = 1.0 + (f64::from(i) * 0.15).cos().abs();
                candle(
                    base - 0.05,
                    base + half,
                    base - half,
                    base + 0.5,
                    i64::from(i),
                )
            })
            .collect();
        let batch = YangZhangVolatility::new(14, 252).unwrap().batch(&candles);
        let mut streamer = YangZhangVolatility::new(14, 252).unwrap();
        let streamed: Vec<_> = candles.iter().map(|c| streamer.update(*c)).collect();
        assert_eq!(batch, streamed);
    }

    #[test]
    fn reset_clears_state() {
        let candles: Vec<Candle> = (0..30).map(|i| candle(10.0, 11.0, 9.0, 10.5, i)).collect();
        let mut yz = YangZhangVolatility::new(14, 252).unwrap();
        yz.batch(&candles);
        assert!(yz.is_ready());
        yz.reset();
        assert!(!yz.is_ready());
        assert_eq!(yz.value(), None);
        assert_eq!(yz.update(candles[0]), None);
    }

    #[test]
    fn intraday_data_collapses_to_rs_only() {
        // If `O_t == C_{t-1}` for every bar (perfect intraday continuity),
        // the overnight log-return is zero and `var_on == 0`. If the
        // open-to-close return is also constant across bars, `var_oc == 0`.
        // Yang-Zhang then reduces to `(1-k) · var_rs`. The arithmetic
        // checks out against the closed form.
        //
        // Construct a series where every bar opens at the previous close
        // and has a constant intraday shape: O=10, H=11, L=9, C=10 every
        // bar. Then ln(O_t/C_{t-1}) = 0, ln(C/O) = 0, and the RS sample
        // is `2 · (ln(11/10) · ln(11/10))` (the ln(9/10)·ln(9/10) term
        // matches numerically).
        let candles: Vec<Candle> = (0..30).map(|i| candle(10.0, 11.0, 9.0, 10.0, i)).collect();
        let mut yz = YangZhangVolatility::new(10, 1).unwrap();
        let out = yz.batch(&candles);

        let log_hc = (11.0_f64 / 10.0_f64).ln();
        let log_ho = (11.0_f64 / 10.0_f64).ln();
        let log_lc = (9.0_f64 / 10.0_f64).ln();
        let log_lo = (9.0_f64 / 10.0_f64).ln();
        let rs_sample = log_hc * log_ho + log_lc * log_lo;
        let n = 10.0;
        let k = 0.34 / (1.34 + (n + 1.0) / (n - 1.0));
        // var_on = var_oc = 0 because every sample equals the mean (0).
        let total = (1.0 - k) * rs_sample;
        let expected = total.max(0.0).sqrt() * 100.0;

        for v in out.iter().skip(11).flatten() {
            assert_relative_eq!(*v, expected, epsilon = 1e-9);
        }
    }

    #[test]
    fn rejects_period_above_maximum() {
        assert!(matches!(
            YangZhangVolatility::new(crate::error::MAX_PERIOD + 1, 252),
            Err(Error::InvalidPeriod { .. })
        ));
    }
}
