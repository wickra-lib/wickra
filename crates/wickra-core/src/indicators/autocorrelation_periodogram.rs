//! Ehlers Autocorrelation Periodogram — estimates the dominant market cycle.
#![allow(clippy::doc_markdown)]

use std::collections::VecDeque;
use std::f64::consts::TAU;

use crate::error::{Error, Result};
use crate::indicators::roofing_filter::RoofingFilter;
use crate::traits::Indicator;

/// Number of bars averaged into each lagged correlation (Ehlers' `AvgLength`).
const AVG_LENGTH: usize = 3;

/// Largest cosine/sine table kept, in `(period, lag)` pairs (1 MiB). Beyond it
/// the terms are computed where they are used, as before the table existed.
const TABLE_LIMIT: usize = 1 << 16;

/// Ehlers' **Autocorrelation Periodogram** — measures the **dominant cycle
/// period** of the market by correlating a roofing-filtered price with lagged
/// copies of itself and reading off the spectral peak.
///
/// From John Ehlers' *Cycle Analytics for Traders* (2013, ch. 8):
///
/// ```text
/// Filt = RoofingFilter(price)                                   (detrend + denoise)
/// Corr[lag] = Pearson( Filt[0..AvgLength], Filt[lag..lag+AvgLength] )   for lag = 0..max_period
/// for each candidate period:
///   power[period] = (Σ Corr[N]·cos(2πN/period))² + (Σ Corr[N]·sin(2πN/period))²
/// R[period]    = 0.2·power[period] + 0.8·R[period]_{t−1}        (EMA across time)
/// normalise by a decaying max, then
/// DominantCycle = centre-of-gravity of periods whose normalised power ≥ 0.5
/// ```
///
/// The autocorrelation function emphasises whatever cycle is actually present and
/// suppresses noise; transforming it into a periodogram and taking the
/// power-weighted centre of gravity gives a smooth, robust estimate of the
/// dominant cycle length. That cycle is the key input for every *adaptive*
/// indicator (adaptive RSI/CCI/stochastic) — set their lookback from it. The
/// output is a period in bars within `[min_period, max_period]`.
///
/// The first value lands after `max_period + AvgLength` inputs. Each `update` is
/// O(`max_period²`); the cosines and sines it weighs the correlations by depend
/// only on the configuration, so they are computed once, at construction.
///
/// # Example
///
/// ```
/// use wickra_core::{Indicator, AutocorrelationPeriodogram};
/// use std::f64::consts::TAU;
///
/// let mut indicator = AutocorrelationPeriodogram::new(10, 48).unwrap();
/// let mut last = None;
/// for i in 0..200 {
///     last = indicator.update(100.0 + (TAU * f64::from(i) / 20.0).sin() * 5.0);
/// }
/// assert!(last.is_some());
/// ```
#[derive(Debug, Clone)]
pub struct AutocorrelationPeriodogram {
    min_period: usize,
    max_period: usize,
    roof: RoofingFilter,
    buffer: VecDeque<f64>,
    r: Vec<f64>,
    max_pwr: f64,
    last: Option<f64>,
    /// `(cos, sin)` of `2π·n/period` for every candidate period (rows) and lag
    /// `n` in `AvgLength..=max_period` (columns); `None` above [`TABLE_LIMIT`].
    trig: Option<Box<[(f64, f64)]>>,
    /// Scratch for the lagged correlations, reused across updates.
    corr: Vec<f64>,
}

impl AutocorrelationPeriodogram {
    /// Construct an autocorrelation periodogram searching cycles in
    /// `[min_period, max_period]`.
    ///
    /// # Errors
    ///
    /// Returns [`Error::PeriodZero`] if either period is `0`, or
    /// [`Error::InvalidPeriod`] if `min_period < AvgLength + 1` or
    /// `max_period <= min_period`.
    pub fn new(min_period: usize, max_period: usize) -> Result<Self> {
        if min_period == 0 || max_period == 0 {
            return Err(Error::PeriodZero);
        }
        if min_period < AVG_LENGTH + 1 || max_period <= min_period {
            return Err(Error::InvalidPeriod {
                message: "autocorrelation periodogram needs AvgLength < min_period < max_period",
            });
        }
        Ok(Self {
            min_period,
            max_period,
            roof: RoofingFilter::new(10, max_period)?,
            buffer: VecDeque::with_capacity(max_period + AVG_LENGTH),
            r: vec![0.0; max_period + 1],
            max_pwr: 0.0,
            last: None,
            trig: trig_table(min_period, max_period),
            corr: vec![0.0; max_period + 1],
        })
    }

    /// Configured `(min_period, max_period)`.
    pub const fn periods(&self) -> (usize, usize) {
        (self.min_period, self.max_period)
    }

    /// Current dominant-cycle estimate if available.
    pub const fn value(&self) -> Option<f64> {
        self.last
    }

    /// Pearson correlation of the `AvgLength`-deep slices offset by `lag`.
    /// `buffer` is newest-last; `filt(k)` is the value `k` bars back.
    fn correlation(&self, lag: usize) -> f64 {
        let len = self.buffer.len();
        let filt = |k: usize| self.buffer[len - 1 - k];
        let m = AVG_LENGTH as f64;
        let (mut sx, mut sy, mut sxx, mut syy, mut sxy) = (0.0, 0.0, 0.0, 0.0, 0.0);
        for count in 0..AVG_LENGTH {
            let x = filt(count);
            let y = filt(lag + count);
            sx += x;
            sy += y;
            sxx += x * x;
            syy += y * y;
            sxy += x * y;
        }
        let denom = (m * sxx - sx * sx) * (m * syy - sy * sy);
        if denom > 0.0 {
            (m * sxy - sx * sy) / denom.sqrt()
        } else {
            0.0
        }
    }
}

/// `(cos, sin)` of `2π·n/period`, the periodogram's weight for lag `n`.
fn trig_term(n: usize, period: usize) -> (f64, f64) {
    let angle = TAU * n as f64 / period as f64;
    (angle.cos(), angle.sin())
}

/// Every [`trig_term`] the periodogram uses, period-major, unless that is more
/// than [`TABLE_LIMIT`] pairs. The same function on the same arguments, so the
/// table holds exactly the values computed in place.
fn trig_table(min_period: usize, max_period: usize) -> Option<Box<[(f64, f64)]>> {
    let pairs = (max_period + 1 - min_period) * (max_period + 1 - AVG_LENGTH);
    (pairs <= TABLE_LIMIT).then(|| {
        (min_period..=max_period)
            .flat_map(|period| (AVG_LENGTH..=max_period).map(move |n| trig_term(n, period)))
            .collect()
    })
}

impl Indicator for AutocorrelationPeriodogram {
    type Input = f64;
    type Output = f64;

    fn update(&mut self, price: f64) -> Option<f64> {
        if !price.is_finite() {
            return None;
        }
        let filt = self.roof.update(price)?;
        if self.buffer.len() == self.max_period + AVG_LENGTH {
            self.buffer.pop_front();
        }
        self.buffer.push_back(filt);
        if self.buffer.len() < self.max_period + AVG_LENGTH {
            return None;
        }

        // Autocorrelation across lags.
        let mut corr = std::mem::take(&mut self.corr);
        for (lag, c) in corr.iter_mut().enumerate() {
            *c = self.correlation(lag);
        }

        // Periodogram: spectral power for each candidate period, EMA'd over time.
        self.max_pwr *= 0.995;
        let lags = self.max_period + 1 - AVG_LENGTH;
        for (row, period) in (self.min_period..=self.max_period).enumerate() {
            let mut cosine = 0.0;
            let mut sine = 0.0;
            if let Some(table) = &self.trig {
                let weights = &table[row * lags..(row + 1) * lags];
                for (&cn, &(cos, sin)) in corr[AVG_LENGTH..].iter().zip(weights) {
                    cosine += cn * cos;
                    sine += cn * sin;
                }
            } else {
                for (n, &cn) in corr.iter().enumerate().skip(AVG_LENGTH) {
                    let (cos, sin) = trig_term(n, period);
                    cosine += cn * cos;
                    sine += cn * sin;
                }
            }
            let power = cosine * cosine + sine * sine;
            self.r[period] = 0.2 * power + 0.8 * self.r[period];
            if self.r[period] > self.max_pwr {
                self.max_pwr = self.r[period];
            }
        }

        // Power-weighted centre of gravity of the strong periods.
        let mut spx = 0.0;
        let mut sp = 0.0;
        for period in self.min_period..=self.max_period {
            let pwr = if self.max_pwr > 0.0 {
                self.r[period] / self.max_pwr
            } else {
                0.0
            };
            if pwr >= 0.5 {
                spx += period as f64 * pwr;
                sp += pwr;
            }
        }
        let dominant = if sp > 0.0 {
            (spx / sp).clamp(self.min_period as f64, self.max_period as f64)
        } else {
            self.min_period as f64
        };
        self.corr = corr;
        self.last = Some(dominant);
        Some(dominant)
    }

    fn reset(&mut self) {
        self.roof.reset();
        self.buffer.clear();
        self.r.iter_mut().for_each(|x| *x = 0.0);
        self.max_pwr = 0.0;
        self.last = None;
    }

    #[inline]
    fn warmup_period(&self) -> usize {
        self.max_period + AVG_LENGTH
    }

    #[inline]
    fn is_ready(&self) -> bool {
        self.last.is_some()
    }

    #[inline]
    fn name(&self) -> &'static str {
        "AutocorrelationPeriodogram"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;

    #[test]
    fn the_trig_table_gives_the_bits_of_the_terms_computed_in_place() {
        let prices: Vec<f64> = (0..600)
            .map(|i| {
                let t = f64::from(i);
                100.0 + (TAU * t / 23.0).sin() * 5.0 + (t * 0.37).cos()
            })
            .collect();
        let mut table = AutocorrelationPeriodogram::new(10, 48).unwrap();
        let mut in_place = table.clone();
        assert!(table.trig.is_some());
        in_place.trig = None;
        for &price in &prices {
            let (a, b) = (table.update(price), in_place.update(price));
            assert_eq!(a.map(f64::to_bits), b.map(f64::to_bits));
        }
        // A range whose table would pass the limit computes its terms in place.
        assert!(AutocorrelationPeriodogram::new(10, 300)
            .unwrap()
            .trig
            .is_none());
    }

    #[test]
    fn rejects_invalid_periods() {
        assert!(matches!(
            AutocorrelationPeriodogram::new(0, 48),
            Err(Error::PeriodZero)
        ));
        assert!(matches!(
            AutocorrelationPeriodogram::new(3, 48),
            Err(Error::InvalidPeriod { .. })
        ));
        assert!(matches!(
            AutocorrelationPeriodogram::new(48, 10),
            Err(Error::InvalidPeriod { .. })
        ));
    }

    #[test]
    fn accessors_and_metadata() {
        let p = AutocorrelationPeriodogram::new(10, 48).unwrap();
        assert_eq!(p.periods(), (10, 48));
        assert_eq!(p.warmup_period(), 51);
        assert_eq!(p.name(), "AutocorrelationPeriodogram");
        assert!(!p.is_ready());
        assert_eq!(p.value(), None);
    }

    #[test]
    fn first_emission_at_warmup_period() {
        let mut p = AutocorrelationPeriodogram::new(8, 20).unwrap();
        let xs: Vec<f64> = (0..40)
            .map(|i| 100.0 + (TAU * f64::from(i) / 12.0).sin() * 5.0)
            .collect();
        let out = p.batch(&xs);
        let warmup = p.warmup_period(); // 23
        assert_eq!(warmup, 23);
        for v in out.iter().take(warmup - 1) {
            assert!(v.is_none());
        }
        assert!(out[warmup - 1].is_some());
    }

    #[test]
    fn output_within_period_band() {
        let mut p = AutocorrelationPeriodogram::new(10, 48).unwrap();
        let xs: Vec<f64> = (0..400)
            .map(|i| 100.0 + (TAU * f64::from(i) / 20.0).sin() * 5.0)
            .collect();
        for v in p.batch(&xs).into_iter().flatten() {
            assert!((10.0..=48.0).contains(&v), "cycle out of band: {v}");
        }
    }

    #[test]
    fn detects_injected_cycle() {
        // A clean 20-bar sine: the dominant cycle estimate should settle near 20.
        let mut p = AutocorrelationPeriodogram::new(10, 48).unwrap();
        let xs: Vec<f64> = (0..600)
            .map(|i| 100.0 + (TAU * f64::from(i) / 20.0).sin() * 5.0)
            .collect();
        let last = p.batch(&xs).into_iter().flatten().last().unwrap();
        assert!(
            (last - 20.0).abs() < 6.0,
            "expected ~20-bar cycle, got {last}"
        );
    }

    #[test]
    fn ignores_non_finite() {
        let mut p = AutocorrelationPeriodogram::new(10, 48).unwrap();
        p.batch(
            &(0..80)
                .map(|i| 100.0 + (TAU * f64::from(i) / 20.0).sin() * 5.0)
                .collect::<Vec<_>>(),
        );
        let before = p.value();
        assert_eq!(p.update(f64::NAN), None);
        // The rejected input must not have disturbed the state.
        assert_eq!(p.value(), before);
    }

    #[test]
    fn reset_clears_state() {
        let mut p = AutocorrelationPeriodogram::new(10, 48).unwrap();
        p.batch(
            &(0..120)
                .map(|i| 100.0 + (TAU * f64::from(i) / 20.0).sin() * 5.0)
                .collect::<Vec<_>>(),
        );
        assert!(p.is_ready());
        p.reset();
        assert!(!p.is_ready());
        assert_eq!(p.value(), None);
    }

    #[test]
    fn batch_equals_streaming() {
        let xs: Vec<f64> = (0..200)
            .map(|i| 100.0 + (TAU * f64::from(i) / 20.0).sin() * 5.0)
            .collect();
        let batch = AutocorrelationPeriodogram::new(10, 48).unwrap().batch(&xs);
        let mut b = AutocorrelationPeriodogram::new(10, 48).unwrap();
        let streamed: Vec<_> = xs.iter().map(|x| b.update(*x)).collect();
        assert_eq!(batch, streamed);
    }

    #[test]
    fn flat_input_falls_back_to_min_period() {
        // Constant input has zero variance, so every lag correlation is
        // degenerate (denom <= 0), the max power is zero and no period clears
        // the 0.5 threshold -> the dominant cycle defaults to `min_period`.
        let flat = [100.0_f64; 200];
        let last = AutocorrelationPeriodogram::new(10, 48)
            .unwrap()
            .batch(&flat)
            .into_iter()
            .flatten()
            .last()
            .unwrap();
        assert_eq!(last, 10.0);
    }
}
