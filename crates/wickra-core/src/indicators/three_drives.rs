//! Three Drives harmonic pattern.

use crate::indicators::pattern_swing::{
    approx_equal, drive_legs, ratios_in, SwingTracker, SWING_THRESHOLD,
};
use crate::ohlcv::Candle;
use crate::traits::Indicator;

/// Three Drives — a symmetric harmonic pattern of three drives separated by two
/// retracements, read from the last seven pivots. Each drive extends the
/// retracement that precedes it, so the seven pivots span six alternating legs
/// `R1 D1 R2 D2 R3 D3`:
///
/// ```text
/// D1 / R1 ∈ [1.13, 1.75]        (each drive extends the leg before it)
/// D2 / R2 ∈ [1.13, 1.75]
/// D3 / R3 ∈ [1.13, 1.75]
/// D1 ≈ D2 ≈ D3 (within 20%)     (the three drives are similar in size)
/// R1 ≈ R2 ≈ R3 (within 30%)     (the retracements between them are similar)
/// ```
///
/// The third drive is what separates this from a plain two-push extension: a
/// structure that stops after two drives is not a match, it is an incomplete
/// pattern the detector keeps waiting on.
///
/// Output is `+1.0` (bullish, terminal D a swing low — drives down), `-1.0`
/// (bearish, drives up), or `0.0`; never `None`. See
/// `crates/wickra-core/src/indicators/three_drives.rs`.
#[derive(Debug, Clone)]
pub struct ThreeDrives {
    swing: SwingTracker,
    has_emitted: bool,
}

impl ThreeDrives {
    /// Construct a new Three Drives detector.
    pub const fn new() -> Self {
        Self {
            swing: SwingTracker::new(SWING_THRESHOLD, 7),
            has_emitted: false,
        }
    }
}

impl Default for ThreeDrives {
    fn default() -> Self {
        Self::new()
    }
}

impl Indicator for ThreeDrives {
    type Input = Candle;
    type Output = f64;

    #[inline]
    fn update(&mut self, candle: Candle) -> Option<f64> {
        let advanced = self.swing.update(candle);
        let pivots = self.swing.pivots();
        // Too few pivots to form the shape at all: the indicator cannot
        // judge yet, which is what `None` means.
        if pivots.len() < 7 {
            return None;
        }
        self.has_emitted = true;
        // Armed, but this bar did not close a new pivot, so there is
        // nothing new to match against.
        if !advanced {
            return Some(0.0);
        }
        let p = drive_legs(pivots);
        let [retr1, drive1, retr2, drive2, retr3, drive3] = p.legs;
        let extensions = ratios_in(&[
            (drive1 / retr1, 1.13, 1.75),
            (drive2 / retr2, 1.13, 1.75),
            (drive3 / retr3, 1.13, 1.75),
        ]);
        let drives_match = approx_equal(drive1, drive2, 0.20) && approx_equal(drive2, drive3, 0.20);
        let retracements_match =
            approx_equal(retr1, retr2, 0.30) && approx_equal(retr2, retr3, 0.30);
        if extensions && drives_match && retracements_match {
            return Some(if p.bullish { 1.0 } else { -1.0 });
        }
        Some(0.0)
    }

    fn reset(&mut self) {
        self.swing.reset();
        self.has_emitted = false;
    }

    #[inline]
    fn warmup_period(&self) -> usize {
        8
    }

    #[inline]
    fn is_ready(&self) -> bool {
        self.has_emitted
    }

    #[inline]
    fn name(&self) -> &'static str {
        "ThreeDrives"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::indicators::pattern_swing::candles_for_pivots;
    use crate::traits::BatchExt;

    fn run(pivots: &[f64]) -> Vec<f64> {
        let mut indicator = ThreeDrives::new();
        candles_for_pivots(pivots)
            .into_iter()
            .filter_map(|c| indicator.update(c))
            .collect()
    }

    #[test]
    fn accessors_and_metadata() {
        let indicator = ThreeDrives::new();
        assert_eq!(indicator.name(), "ThreeDrives");
        assert_eq!(indicator.warmup_period(), 8);
        assert!(!indicator.is_ready());
        assert!(!ThreeDrives::default().is_ready());
    }

    #[test]
    fn bearish_three_drives_is_minus_one() {
        // Seven pivots, three rising drives (124, 128, 132) each extending a
        // 10-point retracement by 14: every D/R is 1.4 and both symmetry bands
        // hold exactly.
        let out = run(&[120.0, 110.0, 124.0, 114.0, 128.0, 118.0, 132.0]);
        assert_eq!(*out.last().unwrap(), -1.0);
        assert!(out[..out.len() - 1].iter().all(|&x| x == 0.0));
    }

    #[test]
    fn two_drives_alone_do_not_complete_the_pattern() {
        // The five-pivot shape holds only two drive legs. A third drive is what
        // the pattern is named for, so the detector must still be waiting.
        let out = run(&[120.0, 100.0, 128.0, 108.0, 136.0]);
        assert_eq!(out, Vec::new());
    }

    #[test]
    fn bullish_three_drives_is_plus_one() {
        // Mirror image: three falling drives (114, 110, 106) off 10-point
        // upward retracements. The leading pivot only seeds the alternation;
        // the detector reads the last seven.
        let out = run(&[132.0, 118.0, 128.0, 114.0, 124.0, 110.0, 120.0, 106.0]);
        assert_eq!(*out.last().unwrap(), 1.0);
    }

    #[test]
    fn asymmetric_drives_do_not_trigger() {
        // Every D/R stays inside [1.13, 1.75] and the retracements stay within
        // 30% of each other, but the third drive is 20 against the first two at
        // 14 — outside the 20% band, so the shape is rejected on symmetry alone.
        let out = run(&[120.0, 110.0, 124.0, 114.0, 128.0, 114.0, 134.0]);
        assert_eq!(*out.last().unwrap(), 0.0);
    }

    #[test]
    fn reset_clears_state() {
        let mut indicator = ThreeDrives::new();
        for c in candles_for_pivots(&[120.0, 100.0, 128.0]) {
            let _ = indicator.update(c);
        }
        indicator.reset();
        assert!(!indicator.is_ready());
        let c = Candle::new(99.5, 100.0, 99.5, 99.5, 1.0, 0).unwrap();
        assert_eq!(indicator.update(c), None);
    }

    #[test]
    fn batch_equals_streaming() {
        let candles = candles_for_pivots(&[120.0, 110.0, 124.0, 114.0, 128.0, 118.0, 132.0]);
        let mut a = ThreeDrives::new();
        let mut b = ThreeDrives::new();
        assert_eq!(
            a.batch(&candles),
            candles.iter().map(|x| b.update(*x)).collect::<Vec<_>>()
        );
    }
}
