//! Average Directional Index (ADX) with +DI / -DI components.

use crate::error::{Error, Result};
use crate::ohlcv::Candle;
use crate::traits::Indicator;

/// ADX output: the three Wilder lines.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AdxOutput {
    /// Plus Directional Indicator.
    pub plus_di: f64,
    /// Minus Directional Indicator.
    pub minus_di: f64,
    /// Average Directional Index (smoothed |DX|).
    pub adx: f64,
}

/// Wilder's Average Directional Index.
///
/// Uses Wilder smoothing throughout. First `period` candles seed the directional
/// movement / true range sums; the next `period` candles produce DX values that
/// seed the ADX. The first complete `AdxOutput` is emitted after `2 * period`
/// candles.
///
/// # Example
///
/// ```
/// use wickra_core::{Candle, Indicator, Adx};
///
/// let mut indicator = Adx::new(5).unwrap();
/// let mut last = None;
/// for i in 0..80 {
///     let base = 100.0 + f64::from(i);
///     let candle =
///         Candle::new(base, base + 2.0, base - 2.0, base + 1.0, 10.0, i64::from(i)).unwrap();
///     last = indicator.update(candle);
/// }
/// assert!(last.is_some());
/// ```
#[allow(clippy::struct_field_names)] // adx_value pairs with adx (the output line) — renaming hurts clarity
#[derive(Debug, Clone)]
pub struct Adx {
    period: usize,
    prev: Option<Candle>,

    // Wilder-smoothed sums during seeding.
    tr_seed: f64,
    plus_dm_seed: f64,
    minus_dm_seed: f64,
    seed_count: usize,

    // Smoothed running values after seeding.
    tr_smooth: Option<f64>,
    plus_dm_smooth: Option<f64>,
    minus_dm_smooth: Option<f64>,

    // ADX seeding.
    dx_buf: Vec<f64>,
    adx_value: Option<f64>,
    last_plus_di: f64,
    last_minus_di: f64,
}

impl Adx {
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
            prev: None,
            tr_seed: 0.0,
            plus_dm_seed: 0.0,
            minus_dm_seed: 0.0,
            seed_count: 0,
            tr_smooth: None,
            plus_dm_smooth: None,
            minus_dm_smooth: None,
            dx_buf: Vec::with_capacity(period),
            adx_value: None,
            last_plus_di: 0.0,
            last_minus_di: 0.0,
        })
    }

    /// Configured period.
    pub const fn period(&self) -> usize {
        self.period
    }
}

pub(crate) fn directional_movement(prev: &Candle, current: &Candle) -> (f64, f64) {
    let up = current.high - prev.high;
    let down = prev.low - current.low;
    let plus_dm = if up > down && up > 0.0 { up } else { 0.0 };
    let minus_dm = if down > up && down > 0.0 { down } else { 0.0 };
    (plus_dm, minus_dm)
}

impl Indicator for Adx {
    type Input = Candle;
    type Output = AdxOutput;

    fn update(&mut self, candle: Candle) -> Option<AdxOutput> {
        let Some(prev) = self.prev else {
            self.prev = Some(candle);
            return None;
        };
        self.prev = Some(candle);

        let tr = candle.true_range(Some(prev.close));
        let (plus_dm, minus_dm) = directional_movement(&prev, &candle);
        let n = self.period as f64;

        let (tr_v, plus_v, minus_v) = if let (Some(t), Some(p), Some(m)) =
            (self.tr_smooth, self.plus_dm_smooth, self.minus_dm_smooth)
        {
            let t_new = t - t / n + tr;
            let p_new = p - p / n + plus_dm;
            let m_new = m - m / n + minus_dm;
            self.tr_smooth = Some(t_new);
            self.plus_dm_smooth = Some(p_new);
            self.minus_dm_smooth = Some(m_new);
            (t_new, p_new, m_new)
        } else {
            self.tr_seed += tr;
            self.plus_dm_seed += plus_dm;
            self.minus_dm_seed += minus_dm;
            self.seed_count += 1;
            if self.seed_count < self.period {
                return None;
            }
            self.tr_smooth = Some(self.tr_seed);
            self.plus_dm_smooth = Some(self.plus_dm_seed);
            self.minus_dm_smooth = Some(self.minus_dm_seed);
            (self.tr_seed, self.plus_dm_seed, self.minus_dm_seed)
        };

        let plus_di = if tr_v == 0.0 {
            0.0
        } else {
            100.0 * plus_v / tr_v
        };
        let minus_di = if tr_v == 0.0 {
            0.0
        } else {
            100.0 * minus_v / tr_v
        };
        self.last_plus_di = plus_di;
        self.last_minus_di = minus_di;

        let dx_den = plus_di + minus_di;
        let dx = if dx_den == 0.0 {
            0.0
        } else {
            100.0 * (plus_di - minus_di).abs() / dx_den
        };

        if let Some(prev_adx) = self.adx_value {
            let new_adx = (prev_adx * (n - 1.0) + dx) / n;
            self.adx_value = Some(new_adx);
            return Some(AdxOutput {
                plus_di,
                minus_di,
                adx: new_adx,
            });
        }

        self.dx_buf.push(dx);
        if self.dx_buf.len() == self.period {
            let seed = self.dx_buf.iter().sum::<f64>() / n;
            self.adx_value = Some(seed);
            return Some(AdxOutput {
                plus_di,
                minus_di,
                adx: seed,
            });
        }
        None
    }

    fn reset(&mut self) {
        self.prev = None;
        self.tr_seed = 0.0;
        self.plus_dm_seed = 0.0;
        self.minus_dm_seed = 0.0;
        self.seed_count = 0;
        self.tr_smooth = None;
        self.plus_dm_smooth = None;
        self.minus_dm_smooth = None;
        self.dx_buf.clear();
        self.adx_value = None;
        self.last_plus_di = 0.0;
        self.last_minus_di = 0.0;
    }

    #[inline]
    fn warmup_period(&self) -> usize {
        2 * self.period
    }

    #[inline]
    fn is_ready(&self) -> bool {
        self.adx_value.is_some()
    }

    #[inline]
    fn name(&self) -> &'static str {
        "ADX"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;
    use approx::assert_relative_eq;

    fn c(h: f64, l: f64, cl: f64) -> Candle {
        Candle::new(cl, h, l, cl, 1.0, 0).unwrap()
    }

    #[test]
    fn pure_uptrend_yields_plus_di_dominant() {
        // Strict uptrend: highs increase, lows increase, ADX should trend up,
        // +DI should dominate -DI.
        let candles: Vec<Candle> = (0..50)
            .map(|i| {
                let base = 100.0 + f64::from(i) * 2.0;
                c(base + 1.0, base - 0.5, base + 0.5)
            })
            .collect();
        let mut adx = Adx::new(14).unwrap();
        let last = adx
            .batch(&candles)
            .into_iter()
            .flatten()
            .last()
            .expect("emits");
        assert!(
            last.plus_di > last.minus_di,
            "+DI {} should exceed -DI {}",
            last.plus_di,
            last.minus_di
        );
        assert!(last.adx > 0.0);
    }

    #[test]
    fn pure_downtrend_yields_minus_di_dominant() {
        let candles: Vec<Candle> = (0..50)
            .rev()
            .map(|i| {
                let base = 100.0 + f64::from(i) * 2.0;
                c(base + 1.0, base - 0.5, base + 0.5)
            })
            .collect();
        let mut adx = Adx::new(14).unwrap();
        let last = adx
            .batch(&candles)
            .into_iter()
            .flatten()
            .last()
            .expect("emits");
        assert!(last.minus_di > last.plus_di);
    }

    #[test]
    fn rejects_zero_period() {
        assert!(Adx::new(0).is_err());
    }

    /// Cover the const accessor `period` (lines 89-91) and the Indicator-impl
    /// `warmup_period` (199-201) + `name` (207-209). None of the trend tests
    /// inspect these metadata methods.
    #[test]
    fn accessors_and_metadata() {
        let adx = Adx::new(14).unwrap();
        assert_eq!(adx.period(), 14);
        assert_eq!(adx.warmup_period(), 28);
        assert_eq!(adx.name(), "ADX");
    }

    /// Cover the `tr_v == 0.0` defensive branches in `update` (lines 142,
    /// 147) — feeding a stream of perfectly flat candles (H == L == close
    /// every bar) gives true-range 0 each step, so the smoothed `tr_smooth`
    /// stays at 0.0 and the `plus_di` / `minus_di` divisions would otherwise
    /// blow up. The indicator must emit zeros (DX denominator is also 0).
    #[test]
    fn zero_true_range_yields_zero_di_and_zero_adx() {
        let candles: Vec<Candle> = (0..30).map(|_| c(10.0, 10.0, 10.0)).collect();
        let mut adx = Adx::new(5).unwrap();
        let last = adx
            .batch(&candles)
            .into_iter()
            .flatten()
            .last()
            .expect("ADX emits after 2 * period candles");
        assert_eq!(last.plus_di, 0.0);
        assert_eq!(last.minus_di, 0.0);
        assert_eq!(last.adx, 0.0);
    }

    #[test]
    fn batch_equals_streaming() {
        let candles: Vec<Candle> = (0..60)
            .map(|i| {
                let base = 100.0 + (f64::from(i) * 0.3).sin() * 5.0;
                c(base + 1.0, base - 1.0, base)
            })
            .collect();
        let mut a = Adx::new(14).unwrap();
        let mut b = Adx::new(14).unwrap();
        assert_eq!(
            a.batch(&candles),
            candles.iter().map(|x| b.update(*x)).collect::<Vec<_>>()
        );
    }

    #[test]
    fn reset_clears_state() {
        let candles: Vec<Candle> = (0..40).map(|_| c(11.0, 9.0, 10.0)).collect();
        let mut adx = Adx::new(14).unwrap();
        adx.batch(&candles);
        adx.reset();
        assert!(!adx.is_ready());
    }

    #[test]
    fn outputs_remain_finite() {
        let candles: Vec<Candle> = (0..200)
            .map(|i| {
                let m = 100.0 + (f64::from(i) * 0.2).sin() * 5.0;
                c(m + 1.0, m - 1.0, m)
            })
            .collect();
        let mut adx = Adx::new(14).unwrap();
        for v in adx.batch(&candles).into_iter().flatten() {
            assert!(v.plus_di.is_finite() && v.minus_di.is_finite() && v.adx.is_finite());
        }
        // Sanity: ADX is bounded by 100.
        let last = adx.batch(&candles).into_iter().flatten().last().unwrap();
        assert!(last.adx <= 100.0 + 1e-6);
        assert_relative_eq!(0.0_f64.max(last.adx), last.adx, epsilon = 1e-9);
    }

    #[test]
    fn rejects_period_above_maximum() {
        assert!(matches!(
            Adx::new(crate::error::MAX_PERIOD + 1),
            Err(Error::InvalidPeriod { .. })
        ));
    }
}
