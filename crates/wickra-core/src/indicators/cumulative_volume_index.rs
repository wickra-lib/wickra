//! Cumulative Volume Index — running total of net advancing volume.

use crate::cross_section::CrossSection;
use crate::traits::Indicator;

/// Cumulative Volume Index (CVI) — the running total of net advancing volume
/// across a universe.
///
/// On each [`CrossSection`] tick the increment is `advancing volume - declining
/// volume`, the standard definition (`StockCharts`, `MetaStock`):
/// `CVI_t = CVI_{t-1} + (advancing volume - declining volume)`. The index is
/// path-dependent, so only its slope and divergences against a price index carry
/// meaning, not its absolute level. Unchanged issues contribute to neither side.
///
/// `Input = CrossSection`, `Output = f64`, `warmup_period == 1`.
///
/// # Example
///
/// ```
/// use wickra_core::{CrossSection, CumulativeVolumeIndex, Indicator, Member};
///
/// let mut cvi = CumulativeVolumeIndex::new();
/// // adv vol 150, dec vol 50 -> 150 - 50 = 100.
/// let tick = CrossSection::new(
///     vec![
///         Member::new(1.0, 150.0, false, false),
///         Member::new(-1.0, 50.0, false, false),
///     ],
///     0,
/// )
/// .unwrap();
/// assert_eq!(cvi.update(tick), Some(100.0));
/// ```
#[derive(Debug, Clone, Default)]
pub struct CumulativeVolumeIndex {
    index: f64,
    has_emitted: bool,
}

impl CumulativeVolumeIndex {
    /// Construct a new Cumulative Volume Index indicator.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            index: 0.0,
            has_emitted: false,
        }
    }
}

impl Indicator for CumulativeVolumeIndex {
    type Input = CrossSection;
    type Output = f64;

    #[inline]
    fn update(&mut self, section: CrossSection) -> Option<f64> {
        let net = section.advancing_volume() - section.declining_volume();
        self.index += net;
        self.has_emitted = true;
        Some(self.index)
    }

    fn reset(&mut self) {
        self.index = 0.0;
        self.has_emitted = false;
    }

    #[inline]
    fn warmup_period(&self) -> usize {
        1
    }

    #[inline]
    fn is_ready(&self) -> bool {
        self.has_emitted
    }

    #[inline]
    fn name(&self) -> &'static str {
        "CumulativeVolumeIndex"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cross_section::Member;
    use crate::traits::BatchExt;

    fn tick(items: &[(f64, f64)]) -> CrossSection {
        CrossSection::new(
            items
                .iter()
                .map(|&(change, volume)| Member::new(change, volume, false, false))
                .collect(),
            0,
        )
        .unwrap()
    }

    #[test]
    fn accessors_and_metadata() {
        let cvi = CumulativeVolumeIndex::new();
        assert_eq!(cvi.name(), "CumulativeVolumeIndex");
        assert_eq!(cvi.warmup_period(), 1);
        assert!(!cvi.is_ready());
    }

    #[test]
    fn first_tick_emits_net_volume() {
        let mut cvi = CumulativeVolumeIndex::new();
        assert_eq!(cvi.update(tick(&[(1.0, 150.0), (-1.0, 50.0)])), Some(100.0));
        assert!(cvi.is_ready());
    }

    #[test]
    fn index_accumulates_net_volume() {
        let mut cvi = CumulativeVolumeIndex::new();
        assert_eq!(cvi.update(tick(&[(1.0, 150.0), (-1.0, 50.0)])), Some(100.0));
        // adv 60, dec 60 -> net 0 -> index unchanged.
        assert_eq!(cvi.update(tick(&[(1.0, 60.0), (-1.0, 60.0)])), Some(100.0));
    }

    #[test]
    fn zero_volume_leaves_index_unchanged() {
        let mut cvi = CumulativeVolumeIndex::new();
        cvi.update(tick(&[(1.0, 150.0), (-1.0, 50.0)]));
        // A tick with no volume at all: net 0 -> 0 increment.
        assert_eq!(cvi.update(tick(&[(0.0, 0.0)])), Some(100.0));
    }

    #[test]
    fn reset_clears_state() {
        let mut cvi = CumulativeVolumeIndex::new();
        cvi.update(tick(&[(1.0, 150.0), (-1.0, 50.0)]));
        assert!(cvi.is_ready());
        cvi.reset();
        assert!(!cvi.is_ready());
        assert_eq!(cvi.update(tick(&[(1.0, 100.0)])), Some(100.0));
    }

    #[test]
    fn batch_equals_streaming() {
        let sections = vec![
            tick(&[(1.0, 150.0), (-1.0, 50.0)]),
            tick(&[(1.0, 60.0), (-1.0, 60.0)]),
            tick(&[(0.0, 0.0)]),
        ];
        let mut a = CumulativeVolumeIndex::new();
        let mut b = CumulativeVolumeIndex::new();
        assert_eq!(
            a.batch(&sections),
            sections
                .iter()
                .map(|s| b.update(s.clone()))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn warmup_first_value_at_index_zero() {
        let mut cvi = CumulativeVolumeIndex::new();
        assert_eq!(cvi.warmup_period(), 1);
        // warmup_period() - 1 == 0: the very first tick already emits.
        assert!(cvi.update(tick(&[(1.0, 10.0)])).is_some());
    }

    #[test]
    fn hand_computed_multi_member_series_ignores_unchanged() {
        let mut cvi = CumulativeVolumeIndex::new();
        // Tick 1: adv 100 + 20 = 120, dec 30, unchanged 500 ignored -> 120 - 30 = 90.
        assert_eq!(
            cvi.update(tick(&[
                (2.0, 100.0),
                (0.5, 20.0),
                (-1.0, 30.0),
                (0.0, 500.0)
            ])),
            Some(90.0)
        );
        // Tick 2: adv 10, dec 70 + 40 = 110 -> net -100 -> 90 - 100 = -10.
        assert_eq!(
            cvi.update(tick(&[(1.0, 10.0), (-0.5, 70.0), (-3.0, 40.0)])),
            Some(-10.0)
        );
        // Tick 3: only unchanged issues -> net 0 -> -10.
        assert_eq!(cvi.update(tick(&[(0.0, 1_000.0)])), Some(-10.0));
        // Tick 4: adv 25, no decliners -> -10 + 25 = 15.
        assert_eq!(cvi.update(tick(&[(1.0, 25.0)])), Some(15.0));
    }

    #[test]
    fn reset_replays_identically_to_fresh_instance() {
        let sections = vec![
            tick(&[(1.0, 150.0), (-1.0, 50.0)]),
            tick(&[(-1.0, 80.0), (0.0, 5.0)]),
            tick(&[(1.0, 7.5), (-2.0, 2.5)]),
        ];
        let mut cvi = CumulativeVolumeIndex::new();
        let first = cvi.batch(&sections);
        cvi.reset();
        let second = cvi.batch(&sections);
        let fresh = CumulativeVolumeIndex::default().batch(&sections);
        assert_eq!(first, second);
        assert_eq!(second, fresh);
    }
}
