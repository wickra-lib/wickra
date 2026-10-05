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
/// R[period]    = 0.2·power[period]² + 0.8·R[period]_{t−1}       (EMA of SqSum²)
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
    /// [`Error::InvalidPeriod`] if `min_period < AvgLength + 1`,
    /// `max_period <= min_period`, or `max_period <= 10` (the roofing
    /// pre-filter's 10-bar low-pass cutoff must sit below `max_period`).
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
            // Ehlers smooths the *square* of the summed power (SqSum²), which
            // sharpens the dominant peak against the side lobes.
            let sq_sum = cosine * cosine + sine * sine;
            self.r[period] = 0.2 * sq_sum * sq_sum + 0.8 * self.r[period];
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
    use approx::assert_relative_eq;

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
        assert!((last - 20.0).abs() < 6.0, "expected a ~20-bar cycle");
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

    #[test]
    fn rejects_zero_max_period_and_min_period_equal_to_max() {
        assert!(matches!(
            AutocorrelationPeriodogram::new(10, 0),
            Err(Error::PeriodZero)
        ));
        assert!(matches!(
            AutocorrelationPeriodogram::new(10, 10),
            Err(Error::InvalidPeriod { .. })
        ));
        // The internal RoofingFilter(10, max_period) also needs `max_period > 10`.
        assert!(matches!(
            AutocorrelationPeriodogram::new(4, 10),
            Err(Error::InvalidPeriod { .. })
        ));
        // `min_period = AvgLength + 1` is the smallest accepted value.
        assert!(AutocorrelationPeriodogram::new(4, 11).is_ok());
    }

    fn noisy_cycle(len: i32) -> Vec<f64> {
        (0..len)
            .map(|i| {
                let t = f64::from(i);
                100.0 + (TAU * t / 17.0).sin() * 4.0 + (t * 0.91).cos() * 0.7
            })
            .collect()
    }

    #[test]
    fn reset_replays_identically_to_fresh_instance() {
        let xs = noisy_cycle(150);
        let mut used = AutocorrelationPeriodogram::new(8, 30).unwrap();
        used.batch(&xs);
        used.reset();
        let replay = used.batch(&xs);
        assert_eq!(
            replay,
            AutocorrelationPeriodogram::new(8, 30).unwrap().batch(&xs)
        );
    }

    #[test]
    fn batch_nan_into_matches_streaming_bits() {
        let xs = noisy_cycle(160);
        let mut nan_out = vec![0.0; xs.len()];
        AutocorrelationPeriodogram::new(8, 30)
            .unwrap()
            .batch_nan_into(&xs, &mut nan_out);
        let mut streamer = AutocorrelationPeriodogram::new(8, 30).unwrap();
        let identical = xs
            .iter()
            .zip(&nan_out)
            .all(|(x, v)| streamer.update(*x).unwrap_or(f64::NAN).to_bits() == v.to_bits());
        assert!(identical);
    }

    /// Hand-computed Pearson correlation on a crafted buffer (oldest first)
    /// `[3, 1, 2, 1, 2, 3]`, so `filt(0..6) = 3, 2, 1, 2, 1, 3`.
    /// Lag 3: `x = (3, 2, 1)`, `y = (2, 1, 3)`; `Σx = Σy = 6`, `Σx² = Σy² = 14`,
    /// `Σxy = 6 + 2 + 3 = 11`. Numerator `3·11 − 6·6 = −3`; denominator
    /// `sqrt((3·14 − 36)·(3·14 − 36)) = 6`; correlation `−0.5`.
    /// Lag 0 is the series against itself: `1`.
    #[test]
    fn correlation_reference_value() {
        let mut p = AutocorrelationPeriodogram::new(4, 11).unwrap();
        p.buffer.extend([3.0, 1.0, 2.0, 1.0, 2.0, 3.0]);
        assert_relative_eq!(p.correlation(3), -0.5, epsilon = 1e-12);
        assert_relative_eq!(p.correlation(0), 1.0, epsilon = 1e-12);
        // A flat lagged slice has zero variance: denominator 0 -> correlation 0.
        let mut flat = AutocorrelationPeriodogram::new(4, 11).unwrap();
        flat.buffer.extend([1.0, 1.0, 1.0, 4.0, 5.0, 7.0]);
        assert_relative_eq!(flat.correlation(3), 0.0, epsilon = 1e-12);
    }

    /// The periodogram smooths the *square* of the summed power:
    /// `R_t = 0.2 · SqSum² + 0.8 · R_{t−1}`, with `SqSum = cos² + sin²` of the
    /// correlation-weighted sums. Replay that recurrence from the correlations the
    /// indicator itself computed (left in its scratch buffer) and compare.
    #[test]
    fn r_is_ema_of_squared_sq_sum() {
        let (min_period, max_period) = (6, 14);
        let mut p = AutocorrelationPeriodogram::new(min_period, max_period).unwrap();
        let mut r_prev = vec![0.0_f64; max_period + 1];
        let mut steps = 0;
        for x in noisy_cycle(60) {
            if p.update(x).is_none() {
                continue;
            }
            steps += 1;
            for (period, prev) in r_prev.iter_mut().enumerate().skip(min_period) {
                let (mut cosine, mut sine) = (0.0, 0.0);
                for (n, corr) in p.corr.iter().enumerate().skip(AVG_LENGTH) {
                    let (cos, sin) = trig_term(n, period);
                    cosine += corr * cos;
                    sine += corr * sin;
                }
                let sq_sum = cosine * cosine + sine * sine;
                let expected = 0.2 * sq_sum * sq_sum + 0.8 * *prev;
                assert_relative_eq!(p.r[period], expected, epsilon = 1e-12, max_relative = 1e-12);
                *prev = p.r[period];
            }
        }
        // The first emission starts from `R_{t−1} = 0`, i.e. `R = 0.2 · SqSum²`.
        assert_ne!(steps, 0);
        assert!(p.max_pwr > 0.0);
    }

    #[test]
    fn dominant_cycle_is_power_weighted_centre_of_gravity() {
        // Feed a real series, then recompute the centre of gravity of the periods
        // with normalised power >= 0.5 from the indicator's own `r` / `max_pwr`.
        let mut p = AutocorrelationPeriodogram::new(8, 30).unwrap();
        let last = p
            .batch(&noisy_cycle(200))
            .into_iter()
            .flatten()
            .last()
            .unwrap();
        let (mut spx, mut sp) = (0.0, 0.0);
        for period in 8..=30_u32 {
            let pwr = p.r[usize::try_from(period).unwrap()] / p.max_pwr;
            if pwr >= 0.5 {
                spx += f64::from(period) * pwr;
                sp += pwr;
            }
        }
        assert_relative_eq!(last, spx / sp, epsilon = 1e-9);
    }
}
