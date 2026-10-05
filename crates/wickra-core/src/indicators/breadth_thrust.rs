//! Breadth Thrust (Zweig) — an exponential moving average of the advancing-issues share.

use crate::cross_section::CrossSection;
use crate::error::Result;
use crate::traits::Indicator;
use crate::Ema;

/// Breadth Thrust (Zweig) — an exponential moving average of the advancing-issues
/// share, `advancers / (advancers + decliners)`.
///
/// Martin Zweig's breadth thrust smooths the fraction of participating issues
/// that are advancing with an EMA (`α = 2 / (period + 1)`, seeded with the simple
/// mean of the first `period` shares; Zweig's period is 10). A "thrust"
/// fires when this average climbs from below ~0.40 (oversold, washed-out breadth)
/// to above ~0.615 within about ten sessions — historically a rare, reliable
/// signal that a powerful new advance has begun with broad participation.
///
/// Each tick's share floors the participating count to one, so a tick with no
/// advancing or declining issues contributes a defined `0.0` instead of dividing
/// by zero. The reading is `None` until `period` ticks have been seen.
///
/// `Input = CrossSection`, `Output = f64` (a share in `0..=1`),
/// `warmup_period == period`.
///
/// # Example
///
/// ```
/// use wickra_core::{BreadthThrust, CrossSection, Indicator, Member};
///
/// let mut bt = BreadthThrust::new(2).unwrap();
/// let up = CrossSection::new(vec![Member::new(1.0, 1.0, false, false)], 0).unwrap();
/// assert_eq!(bt.update(up.clone()), None); // warming up
/// assert_eq!(bt.update(up), Some(1.0)); // both ticks 100% advancing
/// ```
#[derive(Debug, Clone)]
pub struct BreadthThrust {
    ema: Ema,
}

impl BreadthThrust {
    /// Construct a new Breadth Thrust over the given window length.
    ///
    /// # Errors
    ///
    /// Returns [`Error::PeriodZero`](crate::Error::PeriodZero) if `period == 0`.
    pub fn new(period: usize) -> Result<Self> {
        Ok(Self {
            ema: Ema::new(period)?,
        })
    }

    /// Configured window length.
    #[must_use]
    pub const fn period(&self) -> usize {
        self.ema.period()
    }
}

impl Indicator for BreadthThrust {
    type Input = CrossSection;
    type Output = f64;

    #[inline]
    fn update(&mut self, section: CrossSection) -> Option<f64> {
        let advancers = section.advancers();
        let decliners = section.decliners();
        let participating = (advancers + decliners).max(1) as f64;
        let share = advancers as f64 / participating;
        self.ema.update(share)
    }

    fn reset(&mut self) {
        self.ema.reset();
    }

    #[inline]
    fn warmup_period(&self) -> usize {
        self.ema.period()
    }

    #[inline]
    fn is_ready(&self) -> bool {
        self.ema.value().is_some()
    }

    #[inline]
    fn name(&self) -> &'static str {
        "BreadthThrust"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cross_section::Member;
    use crate::error::Error;
    use crate::traits::BatchExt;
    use approx::assert_relative_eq;

    fn section(up: usize, down: usize) -> CrossSection {
        let mut members = Vec::new();
        for _ in 0..up {
            members.push(Member::new(1.0, 10.0, false, false));
        }
        for _ in 0..down {
            members.push(Member::new(-1.0, 10.0, false, false));
        }
        members.push(Member::new(0.0, 10.0, false, false));
        CrossSection::new(members, 0).unwrap()
    }

    #[test]
    fn accessors_and_metadata() {
        let bt = BreadthThrust::new(10).unwrap();
        assert_eq!(bt.name(), "BreadthThrust");
        assert_eq!(bt.warmup_period(), 10);
        assert_eq!(bt.period(), 10);
        assert!(!bt.is_ready());
    }

    #[test]
    fn rejects_zero_period() {
        assert!(matches!(BreadthThrust::new(0), Err(Error::PeriodZero)));
    }

    #[test]
    fn smooths_the_advancing_share() {
        let mut bt = BreadthThrust::new(2).unwrap();
        // share = 8 / 10 = 0.8 ; window not full yet.
        assert_eq!(bt.update(section(8, 2)), None);
        // share = 6 / 10 = 0.6 ; EMA(2) seeds with the mean (0.8 + 0.6) / 2 = 0.7.
        let value = bt.update(section(6, 4)).unwrap();
        assert!((value - 0.7).abs() < 1e-9);
        assert!(bt.is_ready());
        // share = 5 / 10 = 0.5 ; EMA(2), α = 2/3: 0.7 + 2/3 · (0.5 − 0.7).
        let value = bt.update(section(5, 5)).unwrap();
        assert!((value - (0.7 + 2.0 / 3.0 * (0.5 - 0.7))).abs() < 1e-9);
    }

    #[test]
    fn empty_participation_floors_to_zero_share() {
        let mut bt = BreadthThrust::new(1).unwrap();
        // No advancers or decliners -> 0 / max(0, 1) = 0.0.
        assert_eq!(bt.update(section(0, 0)), Some(0.0));
    }

    #[test]
    fn reset_clears_state() {
        let mut bt = BreadthThrust::new(2).unwrap();
        bt.update(section(8, 2));
        bt.update(section(6, 4));
        assert!(bt.is_ready());
        bt.reset();
        assert!(!bt.is_ready());
        assert_eq!(bt.update(section(8, 2)), None);
    }

    #[test]
    fn batch_equals_streaming() {
        let sections = vec![section(8, 2), section(6, 4), section(5, 5), section(0, 0)];
        let mut a = BreadthThrust::new(2).unwrap();
        let mut b = BreadthThrust::new(2).unwrap();
        assert_eq!(
            a.batch(&sections),
            sections
                .iter()
                .map(|s| b.update(s.clone()))
                .collect::<Vec<_>>()
        );
    }

    fn mixed_sections() -> Vec<CrossSection> {
        (0..30_usize)
            .map(|i| section((i * 7) % 11, (i * 3) % 8))
            .collect()
    }

    #[test]
    fn rejects_oversized_period() {
        let too_long = crate::error::MAX_PERIOD + 1;
        assert!(matches!(
            BreadthThrust::new(too_long),
            Err(Error::InvalidPeriod { .. })
        ));
    }

    #[test]
    fn first_value_lands_exactly_at_warmup_index() {
        let sections = mixed_sections();
        let mut bt = BreadthThrust::new(10).unwrap();
        let warmup = bt.warmup_period();
        let out = bt.batch(&sections);
        assert!(out.iter().take(warmup - 1).all(Option::is_none));
        assert!(out.iter().skip(warmup - 1).all(Option::is_some));
    }

    #[test]
    fn reset_replays_identically_to_fresh_instance() {
        let sections = mixed_sections();
        let mut used = BreadthThrust::new(10).unwrap();
        used.batch(&sections);
        used.reset();
        let replay = used.batch(&sections);
        assert_eq!(replay, BreadthThrust::new(10).unwrap().batch(&sections));
    }

    #[test]
    fn batch_equals_streaming_bit_identical() {
        let sections = mixed_sections();
        let batch = BreadthThrust::new(10).unwrap().batch(&sections);
        let mut streamer = BreadthThrust::new(10).unwrap();
        let identical = sections
            .iter()
            .zip(&batch)
            .all(|(s, b)| streamer.update(s.clone()).map(f64::to_bits) == b.map(f64::to_bits));
        assert!(identical);
    }

    /// Hand-computed Zweig EMA(10). Ticks 1–5 are 3 up / 7 down (share 0.3),
    /// ticks 6–10 are 4 up / 6 down (share 0.4): the seed is their simple mean
    /// `(5 · 0.3 + 5 · 0.4) / 10 = 0.35`. Tick 11 is 9 up / 1 down (share 0.9):
    /// `α = 2 / 11`, `EMA = 0.35 + (2/11) · (0.9 − 0.35) = 0.35 + 0.1 = 0.45`.
    /// Tick 12 has no advancers or decliners (share 0):
    /// `EMA = 0.45 + (2/11) · (0 − 0.45) = 0.45 · 9 / 11`.
    #[test]
    fn reference_values_ema_ten() {
        let mut bt = BreadthThrust::new(10).unwrap();
        for _ in 0..5 {
            assert_eq!(bt.update(section(3, 7)), None);
        }
        for _ in 0..4 {
            assert_eq!(bt.update(section(4, 6)), None);
        }
        assert_relative_eq!(bt.update(section(4, 6)).unwrap(), 0.35, epsilon = 1e-12);
        assert_relative_eq!(bt.update(section(9, 1)).unwrap(), 0.45, epsilon = 1e-12);
        assert_relative_eq!(
            bt.update(section(0, 0)).unwrap(),
            0.45 * 9.0 / 11.0,
            epsilon = 1e-12
        );
    }

    #[test]
    fn unanimous_breadth_pins_the_extremes() {
        let mut up = BreadthThrust::new(3).unwrap();
        let ups = up.batch(&[section(5, 0), section(7, 0), section(1, 0), section(2, 0)]);
        assert_eq!(ups, vec![None, None, Some(1.0), Some(1.0)]);
        let mut down = BreadthThrust::new(3).unwrap();
        let downs = down.batch(&[section(0, 5), section(0, 7), section(0, 1)]);
        assert_eq!(downs, vec![None, None, Some(0.0)]);
    }
}
