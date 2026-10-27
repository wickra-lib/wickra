//! Fractal Chaos Bands (Bill Williams Fractals).

use std::collections::VecDeque;

use crate::error::{Error, Result};
use crate::ohlcv::Candle;
use crate::traits::Indicator;

/// Fractal Chaos Bands output.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FractalChaosBandsOutput {
    /// Upper band: high of the most recent confirmed fractal high.
    pub upper: f64,
    /// Lower band: low of the most recent confirmed fractal low.
    pub lower: f64,
}

/// Fractal Chaos Bands: a step-function envelope of the most recent Bill
/// Williams fractal highs and lows.
///
/// A bar is a **fractal high** when its high is the maximum of the window
/// `[i − k, …, i + k]`. A **fractal low** is defined symmetrically on lows.
/// The bands hold the high (low) of the latest confirmed fractal high (low),
/// stepping outwards whenever a new fractal forms and otherwise staying flat:
///
/// ```text
/// confirmation_lag = k                     // the centre bar is known only k bars later
/// upper = high of the most recent confirmed fractal high
/// lower = low  of the most recent confirmed fractal low
/// ```
///
/// `k = 2` (5-bar fractals) is the canonical Williams setting and matches the
/// "Fractal Chaos Bands" oscillator shipped with several chart vendors. With
/// `k` bars of look-ahead, every band update reflects price `k` bars ago —
/// strict streaming preserves this lag rather than peeking into the future.
///
/// # Example
///
/// ```
/// use wickra_core::{Candle, FractalChaosBands, Indicator};
///
/// let mut indicator = FractalChaosBands::new(2).unwrap();
/// let mut last = None;
/// for i in 0..30 {
///     let base = 100.0 + (f64::from(i) * 0.5).sin() * 5.0;
///     let candle =
///         Candle::new(base, base + 1.0, base - 1.0, base, 10.0, i64::from(i)).unwrap();
///     last = indicator.update(candle);
/// }
/// // Confirmation requires `2k + 1` bars plus at least one fractal of each
/// // kind, so `last` may legitimately be `None` on a single sweep without
/// // both a peak and a trough in the window.
/// let _ = last;
/// ```
#[derive(Debug, Clone)]
pub struct FractalChaosBands {
    k: usize,
    window: VecDeque<Candle>,
    last_upper: Option<f64>,
    last_lower: Option<f64>,
}

impl FractalChaosBands {
    /// Construct a new Fractal Chaos Bands indicator with the given fractal
    /// half-width `k` (a bar is a fractal high if its high exceeds the highs
    /// of the `k` bars on either side; canonical `k = 2`).
    ///
    /// # Errors
    /// Returns [`Error::PeriodZero`] if `k == 0` (a single bar is always its
    /// own trivial fractal).
    pub fn new(k: usize) -> Result<Self> {
        if k == 0 {
            return Err(Error::PeriodZero);
        }
        if k > crate::error::MAX_PERIOD {
            return Err(Error::InvalidPeriod {
                message: crate::error::PERIOD_ABOVE_MAX,
            });
        }
        Ok(Self {
            k,
            window: VecDeque::with_capacity(2 * k + 1),
            last_upper: None,
            last_lower: None,
        })
    }

    /// Canonical Bill Williams configuration: `k = 2` (5-bar fractals).
    pub fn classic() -> Self {
        Self::new(2).expect("classic Fractal Chaos Bands parameters are valid")
    }

    /// Configured half-width `k`.
    pub const fn k(&self) -> usize {
        self.k
    }
}

impl Indicator for FractalChaosBands {
    type Input = Candle;
    type Output = FractalChaosBandsOutput;

    #[inline]
    fn update(&mut self, candle: Candle) -> Option<FractalChaosBandsOutput> {
        let window_len = 2 * self.k + 1;
        if self.window.len() == window_len {
            self.window.pop_front();
        }
        self.window.push_back(candle);
        if self.window.len() < window_len {
            return None;
        }
        // The centre bar is at index `k`. Strictly compare against the `k`
        // bars on either side: `>` for the high and `<` for the low (a ties-
        // included pattern would fire on flat tops/bottoms, against Williams'
        // intent).
        let center = &self.window[self.k];
        let mut is_high = true;
        let mut is_low = true;
        for (i, c) in self.window.iter().enumerate() {
            if i == self.k {
                continue;
            }
            if c.high >= center.high {
                is_high = false;
            }
            if c.low <= center.low {
                is_low = false;
            }
        }
        if is_high {
            self.last_upper = Some(center.high);
        }
        if is_low {
            self.last_lower = Some(center.low);
        }
        // Both bands must have been seen at least once before we can emit.
        match (self.last_upper, self.last_lower) {
            (Some(u), Some(l)) => Some(FractalChaosBandsOutput { upper: u, lower: l }),
            _ => None,
        }
    }

    fn reset(&mut self) {
        self.window.clear();
        self.last_upper = None;
        self.last_lower = None;
    }

    #[inline]
    fn warmup_period(&self) -> usize {
        2 * self.k + 1
    }

    #[inline]
    fn is_ready(&self) -> bool {
        self.last_upper.is_some() && self.last_lower.is_some()
    }

    #[inline]
    fn name(&self) -> &'static str {
        "FractalChaosBands"
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
    fn rejects_zero_k() {
        assert!(matches!(FractalChaosBands::new(0), Err(Error::PeriodZero)));
    }

    #[test]
    fn accessors_and_metadata() {
        let f = FractalChaosBands::classic();
        assert_eq!(f.k(), 2);
        assert_eq!(f.warmup_period(), 5);
        assert_eq!(f.name(), "FractalChaosBands");
    }

    /// Detect a single peak and a single trough with `k = 2`.
    /// Bars (high, low, close): (1,1,1), (2,2,2), (5,3,4), (3,1,2),
    /// (2,2,2), (1,1,1), (2,2,2), (5,3,4).
    /// Indices: 0..7. The peak at i=2 is `>` its 2 neighbours on each side
    /// (after index 4 lands). The trough at i=3 is `<` its 2 neighbours on
    /// each side (after index 5 lands). Both bands first emit on index 5.
    #[test]
    fn detects_simple_peak_and_trough() {
        let candles = vec![
            c(1.0, 1.0, 1.0),
            c(2.0, 2.0, 2.0),
            c(5.0, 3.0, 4.0), // peak: high 5 is the max of neighbouring 4
            c(3.0, 0.5, 1.0), // trough: low 0.5 is the min
            c(2.0, 2.0, 2.0),
            c(1.0, 1.0, 1.0),
            c(2.0, 2.0, 2.0),
        ];
        let mut f = FractalChaosBands::new(2).unwrap();
        let out = f.batch(&candles);
        // Bars 0..4 are warmup or single-band only — both bands haven't been
        // confirmed yet.
        for v in out.iter().take(5) {
            assert!(v.is_none());
        }
        // Bar 5 confirms the trough at i=3 (low 0.5); the peak at i=2 was
        // confirmed by bar 4 (centre 2, look-ahead 2 → index 4). So index 5
        // is the first bar with *both* upper and lower set.
        let v = out[5].unwrap();
        assert_relative_eq!(v.upper, 5.0, epsilon = 1e-12);
        assert_relative_eq!(v.lower, 0.5, epsilon = 1e-12);
    }

    /// In a flat market no bar is strictly higher (or lower) than its
    /// neighbours, so no fractal ever confirms and the indicator never emits.
    #[test]
    fn flat_market_never_emits() {
        let candles: Vec<Candle> = (0..30).map(|_| c(10.0, 10.0, 10.0)).collect();
        let mut f = FractalChaosBands::new(2).unwrap();
        for v in f.batch(&candles) {
            assert!(v.is_none());
        }
    }

    #[test]
    fn batch_equals_streaming() {
        let candles: Vec<Candle> = (0..40)
            .map(|i| {
                let m = 100.0 + (f64::from(i) * 0.5).sin() * 3.0;
                c(m + 1.0, m - 1.0, m)
            })
            .collect();
        let mut a = FractalChaosBands::new(2).unwrap();
        let mut b = FractalChaosBands::new(2).unwrap();
        assert_eq!(
            a.batch(&candles),
            candles.iter().map(|x| b.update(*x)).collect::<Vec<_>>()
        );
    }

    #[test]
    fn reset_clears_state() {
        let candles = vec![
            c(1.0, 1.0, 1.0),
            c(2.0, 2.0, 2.0),
            c(5.0, 3.0, 4.0),
            c(3.0, 0.5, 1.0),
            c(2.0, 2.0, 2.0),
            c(1.0, 1.0, 1.0),
            c(2.0, 2.0, 2.0),
        ];
        let mut f = FractalChaosBands::new(2).unwrap();
        f.batch(&candles);
        assert!(f.is_ready());
        f.reset();
        assert!(!f.is_ready());
        assert_eq!(f.update(candles[0]), None);
    }

    #[test]
    fn upper_above_lower_when_both_set() {
        let candles: Vec<Candle> = (0..60)
            .map(|i| {
                let m = 100.0 + (f64::from(i) * 0.4).sin() * 5.0;
                c(m + 1.0, m - 1.0, m)
            })
            .collect();
        let mut f = FractalChaosBands::new(2).unwrap();
        for o in f.batch(&candles).into_iter().flatten() {
            assert!(o.upper >= o.lower);
        }
    }

    #[test]
    fn rejects_k_above_maximum() {
        assert!(matches!(
            FractalChaosBands::new(crate::error::MAX_PERIOD + 1),
            Err(Error::InvalidPeriod { .. })
        ));
    }
}
