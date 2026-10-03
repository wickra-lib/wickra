//! Internal swing-pivot tracker shared by the chart-pattern and harmonic-pattern
//! detectors. Not a public indicator — it carries no `Indicator` impl and is
//! re-exported nowhere, so it is excluded from the public catalogue counter.
//!
//! The tracker mirrors [`crate::indicators::ZigZag`]'s non-repainting
//! percent-threshold confirmation logic, but differs in two ways that make it a
//! reusable building block rather than a standalone indicator:
//!
//! * It is **parameter-free at the call site** — the reversal threshold is baked
//!   in by each detector as a compile-time constant, so construction is
//!   infallible (`const fn new`) and there is no user-facing validation branch.
//! * It **accumulates a bounded history** of the most recently confirmed pivots
//!   (capped at `cap`), so a detector can inspect the last few swings to match a
//!   geometric template (double top, head-and-shoulders, the XABCD legs of a
//!   harmonic pattern, …).

use crate::ohlcv::Candle;

/// Default fractional reversal threshold for pattern swing detection (5%). A
/// pivot is confirmed once price reverses by this fraction away from the running
/// extreme. Baked in so the pattern detectors stay parameter-free, mirroring the
/// candlestick-pattern family's fixed geometric thresholds.
pub(crate) const SWING_THRESHOLD: f64 = 0.05;

/// Default relative tolerance for two swing levels to count as "equal" (3%) —
/// the twin tops of a double top, the shoulders of a head-and-shoulders, the
/// flat boundary of a rectangle.
pub(crate) const LEVEL_TOLERANCE: f64 = 0.03;

/// A confirmed swing pivot: the extreme price the swing turned from, its
/// direction (`+1.0` for a swing high, `-1.0` for a swing low) and the bar index
/// at which that extreme occurred.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Pivot {
    /// Price of the confirmed swing extreme.
    pub price: f64,
    /// `+1.0` if the pivot is a swing high, `-1.0` if it is a swing low.
    pub direction: f64,
    /// Zero-based index of the candle at which the swing extreme occurred (not
    /// the later bar that confirmed it). Used by the geometric Fibonacci tools
    /// (fan, arcs, channel, time zones) to place trendlines and time offsets.
    pub bar: usize,
}

/// Non-repainting percent-threshold swing tracker with a bounded pivot history.
///
/// Feeding a candle returns `true` exactly on the bar where a new pivot is
/// confirmed (price has reversed by the configured fraction away from the
/// running extreme); the newly confirmed pivot is appended to [`pivots`] and the
/// oldest is dropped once the cap is exceeded. Bars that merely extend the
/// running extreme, or that move less than the threshold, return `false`.
///
/// [`pivots`]: SwingTracker::pivots
#[derive(Debug, Clone)]
pub(crate) struct SwingTracker {
    threshold: f64,
    cap: usize,
    /// Number of candles fed so far; the current bar index is `bars_seen - 1`.
    bars_seen: usize,
    state: Option<State>,
    pivots: Vec<Pivot>,
}

#[derive(Debug, Clone, Copy)]
struct State {
    /// `+1.0` while tracking a candidate high (uptrend), `-1.0` while tracking a
    /// candidate low (downtrend).
    direction: f64,
    /// The running candidate extreme price.
    extreme: f64,
    /// Bar index at which the running extreme was last set.
    extreme_bar: usize,
}

impl SwingTracker {
    /// Construct a tracker with a fractional reversal `threshold` (e.g. `0.05`
    /// for 5%) and a pivot history capped at `cap` entries.
    ///
    /// The threshold is supplied by the detectors as a compile-time constant in
    /// `(0, 1)`, so no runtime validation is performed — an out-of-range
    /// constant would be a library bug caught by the unit tests, not invalid
    /// caller input.
    pub(crate) const fn new(threshold: f64, cap: usize) -> Self {
        Self {
            threshold,
            cap,
            bars_seen: 0,
            state: None,
            pivots: Vec::new(),
        }
    }

    /// Feed one candle. Returns `true` when a new pivot was confirmed this bar.
    pub(crate) fn update(&mut self, candle: Candle) -> bool {
        let bar = self.bars_seen;
        self.bars_seen += 1;
        let Some(s) = self.state else {
            // Bootstrap: seed an uptrend tracking the first candle's high.
            self.state = Some(State {
                direction: 1.0,
                extreme: candle.high,
                extreme_bar: bar,
            });
            return false;
        };

        if s.direction > 0.0 {
            if candle.high > s.extreme {
                // Extend the candidate high.
                self.state = Some(State {
                    direction: 1.0,
                    extreme: candle.high,
                    extreme_bar: bar,
                });
                return false;
            }
            if candle.low <= s.extreme * (1.0 - self.threshold) {
                // Confirm the swing high; flip to tracking this bar's low.
                self.push(Pivot {
                    price: s.extreme,
                    direction: 1.0,
                    bar: s.extreme_bar,
                });
                self.state = Some(State {
                    direction: -1.0,
                    extreme: candle.low,
                    extreme_bar: bar,
                });
                return true;
            }
            false
        } else {
            if candle.low < s.extreme {
                // Extend the candidate low.
                self.state = Some(State {
                    direction: -1.0,
                    extreme: candle.low,
                    extreme_bar: bar,
                });
                return false;
            }
            if candle.high >= s.extreme * (1.0 + self.threshold) {
                // Confirm the swing low; flip to tracking this bar's high.
                self.push(Pivot {
                    price: s.extreme,
                    direction: -1.0,
                    bar: s.extreme_bar,
                });
                self.state = Some(State {
                    direction: 1.0,
                    extreme: candle.high,
                    extreme_bar: bar,
                });
                return true;
            }
            false
        }
    }

    /// Zero-based index of the most recently fed candle. Saturates at `0` before
    /// any candle has been seen.
    pub(crate) fn current_bar(&self) -> usize {
        self.bars_seen.saturating_sub(1)
    }

    fn push(&mut self, pivot: Pivot) {
        self.pivots.push(pivot);
        if self.pivots.len() > self.cap {
            self.pivots.remove(0);
        }
    }

    /// The confirmed pivots in chronological order (oldest first, newest last).
    pub(crate) fn pivots(&self) -> &[Pivot] {
        &self.pivots
    }

    /// Clear all state, returning the tracker to its just-constructed condition.
    pub(crate) fn reset(&mut self) {
        self.bars_seen = 0;
        self.state = None;
        self.pivots.clear();
    }
}

/// The two most recent swing highs and lows from the last four (strictly
/// alternating) pivots, returned as `(high_old, high_new, low_old, low_new)`.
/// Used by the converging/diverging trendline patterns (triangle, wedge,
/// rectangle). The slice must hold at least four pivots.
pub(crate) fn recent_legs(pivots: &[Pivot]) -> (f64, f64, f64, f64) {
    let n = pivots.len();
    if pivots[n - 1].direction > 0.0 {
        // … low_old, high_old, low_new, high_new (newest is a high)
        (
            pivots[n - 3].price,
            pivots[n - 1].price,
            pivots[n - 4].price,
            pivots[n - 2].price,
        )
    } else {
        // … high_old, low_old, high_new, low_new (newest is a low)
        (
            pivots[n - 4].price,
            pivots[n - 2].price,
            pivots[n - 3].price,
            pivots[n - 1].price,
        )
    }
}

/// Relative-tolerance equality: `true` when `a` and `b` are within `tol`
/// (a fraction) of the larger magnitude. Used to decide whether two swing
/// levels (the twin highs of a double top, the shoulders of a head-and-shoulders,
/// a harmonic Fibonacci ratio) count as "the same".
pub(crate) fn approx_equal(a: f64, b: f64, tol: f64) -> bool {
    let scale = a.abs().max(b.abs()).max(f64::MIN_POSITIVE);
    (a - b).abs() <= tol * scale
}

/// The five most recent pivots interpreted as the X-A-B-C-D points of a harmonic
/// pattern, with the terminal direction. The slice must hold at least five
/// pivots. Each detector derives the leg lengths and Fibonacci ratios it needs
/// from these five prices.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Xabcd {
    pub x: f64,
    pub a: f64,
    pub b: f64,
    pub c: f64,
    pub d: f64,
    /// `true` when the terminal point D is a swing low (a bullish, buy-side
    /// completion); `false` when D is a swing high (bearish).
    pub bullish: bool,
}

/// Read the last five pivots as an [`Xabcd`]. Pivots are guaranteed nonzero-leg
/// (the swing tracker only confirms moves of at least the threshold), so the
/// leg-ratio divisions in the detectors never divide by zero.
pub(crate) fn xabcd(pivots: &[Pivot]) -> Xabcd {
    let n = pivots.len();
    Xabcd {
        x: pivots[n - 5].price,
        a: pivots[n - 4].price,
        b: pivots[n - 3].price,
        c: pivots[n - 2].price,
        d: pivots[n - 1].price,
        bullish: pivots[n - 1].direction < 0.0,
    }
}

/// The six legs spanned by the seven most recent pivots, in chronological order,
/// together with the terminal direction. Alternating shapes that repeat a
/// `retracement, drive` pair three times need one pair more than the five-point
/// [`Xabcd`] detectors, so they read this instead.
#[derive(Debug, Clone, Copy)]
pub(crate) struct DriveLegs {
    /// Leg magnitudes from oldest to newest: `retracement, drive` three times
    /// over.
    pub legs: [f64; 6],
    /// `true` when the terminal pivot is a swing low (a bullish completion —
    /// the drives ran down); `false` when it is a swing high.
    pub bullish: bool,
}

/// Read the last seven pivots as a [`DriveLegs`]. Pivots are guaranteed
/// nonzero-leg (the swing tracker only confirms moves of at least the
/// threshold), so the leg-ratio divisions in the detectors never divide by zero.
pub(crate) fn drive_legs(pivots: &[Pivot]) -> DriveLegs {
    let n = pivots.len();
    let mut legs = [0.0; 6];
    for (leg, pair) in legs.iter_mut().zip(pivots[n - 7..].windows(2)) {
        *leg = (pair[1].price - pair[0].price).abs();
    }
    DriveLegs {
        legs,
        bullish: pivots[n - 1].direction < 0.0,
    }
}

/// `true` when every `(value, low, high)` triple satisfies `low <= value <= high`.
/// Harmonic detectors express their Fibonacci windows as a list of these triples;
/// evaluating them in one expression keeps the per-triple comparison on a single
/// line (no multi-line `&&` coverage gaps).
pub(crate) fn ratios_in(checks: &[(f64, f64, f64)]) -> bool {
    checks
        .iter()
        .all(|&(value, low, high)| value >= low && value <= high)
}

/// Build a candle sequence that drives a `SwingTracker` (or any detector built
/// on one) to confirm exactly the given alternating pivot prices, in order.
///
/// `pivots` must start with a **high** and strictly alternate high/low, with
/// each consecutive pair differing by at least the swing threshold (5%) in the
/// correct direction (`high > adjacent low * 1.05`). The returned vector has one
/// seed candle plus one confirming candle per pivot; pivot `k` is confirmed by
/// candle `k + 1`. Only the high/low of each candle is meaningful — the pattern
/// detectors read swings, not bodies.
#[cfg(test)]
pub(crate) fn candles_for_pivots(pivots: &[f64]) -> Vec<Candle> {
    fn bar(high: f64, low: f64, ts: i64) -> Candle {
        Candle::new(low, high, low, low, 1.0, ts).unwrap()
    }
    let mut out = vec![bar(pivots[0], pivots[0] * 0.999, 0)];
    let mut ts: i64 = 0;
    for (k, &price) in pivots.iter().enumerate() {
        ts += 1;
        let is_high = k % 2 == 0;
        let next = if k + 1 < pivots.len() {
            pivots[k + 1]
        } else if is_high {
            price * 0.90
        } else {
            price * 1.10
        };
        let candle = if is_high {
            // Reverse down from the candidate high `price` to confirm it.
            bar(price * 0.99, next, ts)
        } else {
            // Reverse up from the candidate low `price` to confirm it.
            bar(next, price * 1.01, ts)
        };
        out.push(candle);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn c_hl(high: f64, low: f64, ts: i64) -> Candle {
        Candle::new(low, high, low, low, 1.0, ts).unwrap()
    }

    #[test]
    fn first_bar_only_bootstraps_no_pivot() {
        let mut t = SwingTracker::new(0.05, 6);
        assert!(!t.update(c_hl(100.0, 99.5, 0)));
        assert_eq!(t.pivots(), Vec::new());
    }

    #[test]
    fn extends_candidate_high_without_confirming() {
        let mut t = SwingTracker::new(0.10, 6);
        assert!(!t.update(c_hl(100.0, 99.5, 0)));
        // A higher high merely raises the candidate — no pivot yet.
        assert!(!t.update(c_hl(110.0, 109.0, 1)));
        assert_eq!(t.pivots(), Vec::new());
    }

    #[test]
    fn uptrend_small_move_does_not_confirm() {
        let mut t = SwingTracker::new(0.10, 6);
        let _ = t.update(c_hl(100.0, 99.5, 0));
        // A 1% dip is below the 10% threshold — neither extends nor confirms.
        assert!(!t.update(c_hl(99.8, 99.0, 1)));
        assert_eq!(t.pivots(), Vec::new());
    }

    #[test]
    fn confirms_high_then_low_alternating() {
        let mut t = SwingTracker::new(0.10, 6);
        let _ = t.update(c_hl(100.0, 99.5, 0)); // seed uptrend
        let _ = t.update(c_hl(120.0, 119.5, 1)); // raise candidate high to 120
                                                 // Drop ≥10% below 120 → confirm the high at 120, flip to downtrend.
        assert!(t.update(c_hl(101.0, 100.0, 2)));
        assert_eq!(
            t.pivots().last().copied(),
            // The high extreme was set at bar 1, confirmed at bar 2.
            Some(Pivot {
                price: 120.0,
                direction: 1.0,
                bar: 1,
            })
        );
        // Now in a downtrend: a lower low extends the candidate low.
        assert!(!t.update(c_hl(100.5, 90.0, 3)));
        // Rise ≥10% above 90 → confirm the low at 90.
        assert!(t.update(c_hl(100.0, 99.0, 4)));
        assert_eq!(
            t.pivots().last().copied(),
            // The low extreme was set at bar 3, confirmed at bar 4.
            Some(Pivot {
                price: 90.0,
                direction: -1.0,
                bar: 3,
            })
        );
    }

    #[test]
    fn downtrend_tiny_rise_does_not_confirm() {
        let mut t = SwingTracker::new(0.10, 6);
        let _ = t.update(c_hl(100.0, 99.5, 0));
        let _ = t.update(c_hl(120.0, 119.5, 1));
        let _ = t.update(c_hl(101.0, 90.0, 2)); // confirm high, now downtrend at 90
                                                // A 1% bounce is below threshold — no confirmation, no new candidate low.
        assert!(!t.update(c_hl(91.0, 90.5, 3)));
        assert_eq!(t.pivots().len(), 1);
    }

    #[test]
    fn history_is_capped() {
        let mut t = SwingTracker::new(0.10, 2);
        // Drive an oscillation that confirms several pivots; only the last 2 stay.
        let path = [
            (100.0, 99.5),
            (120.0, 119.5),
            (101.0, 90.0), // confirm 120 (high)
            (91.0, 90.5),
            (110.0, 109.0), // confirm 90 (low)
            (109.0, 95.0),  // confirm 110 (high)
        ];
        for (i, (h, l)) in path.iter().enumerate() {
            let _ = t.update(c_hl(*h, *l, i64::try_from(i).unwrap()));
        }
        assert_eq!(t.pivots().len(), 2);
        // The two most recent confirmations: low 90 then high 110.
        assert_eq!(t.pivots()[0].price, 90.0);
        assert_eq!(t.pivots()[1].price, 110.0);
    }

    #[test]
    fn reset_clears_state_and_history() {
        let mut t = SwingTracker::new(0.10, 6);
        let _ = t.update(c_hl(100.0, 99.5, 0));
        let _ = t.update(c_hl(120.0, 119.5, 1));
        let _ = t.update(c_hl(101.0, 90.0, 2));
        assert_eq!(t.pivots().len(), 1);
        t.reset();
        assert_eq!(t.pivots(), Vec::new());
        // After reset the next bar bootstraps again (returns false).
        assert!(!t.update(c_hl(100.0, 99.5, 0)));
    }

    #[test]
    fn recent_legs_extracts_highs_and_lows_either_ending() {
        // Newest pivot a high: [low_old, high_old, low_new, high_new].
        let ending_high = [
            Pivot {
                price: 100.0,
                direction: -1.0,
                bar: 0,
            },
            Pivot {
                price: 120.0,
                direction: 1.0,
                bar: 0,
            },
            Pivot {
                price: 110.0,
                direction: -1.0,
                bar: 0,
            },
            Pivot {
                price: 121.0,
                direction: 1.0,
                bar: 0,
            },
        ];
        assert_eq!(recent_legs(&ending_high), (120.0, 121.0, 100.0, 110.0));
        // Newest pivot a low: [high_old, low_old, high_new, low_new].
        let ending_low = [
            Pivot {
                price: 120.0,
                direction: 1.0,
                bar: 0,
            },
            Pivot {
                price: 100.0,
                direction: -1.0,
                bar: 0,
            },
            Pivot {
                price: 110.0,
                direction: 1.0,
                bar: 0,
            },
            Pivot {
                price: 99.0,
                direction: -1.0,
                bar: 0,
            },
        ];
        assert_eq!(recent_legs(&ending_low), (120.0, 110.0, 100.0, 99.0));
    }

    #[test]
    fn xabcd_reads_last_five_pivots_and_direction() {
        let pivots = [
            Pivot {
                price: 50.0,
                direction: 1.0,
                bar: 0,
            },
            Pivot {
                price: 100.0,
                direction: -1.0,
                bar: 0,
            }, // X
            Pivot {
                price: 140.0,
                direction: 1.0,
                bar: 0,
            }, // A
            Pivot {
                price: 115.0,
                direction: -1.0,
                bar: 0,
            }, // B
            Pivot {
                price: 128.0,
                direction: 1.0,
                bar: 0,
            }, // C
            Pivot {
                price: 108.0,
                direction: -1.0,
                bar: 0,
            }, // D (low → bullish)
        ];
        let p = xabcd(&pivots);
        assert_eq!(
            (p.x, p.a, p.b, p.c, p.d),
            (100.0, 140.0, 115.0, 128.0, 108.0)
        );
        assert!(p.bullish);
    }

    #[test]
    fn ratios_in_checks_every_window() {
        assert!(ratios_in(&[(0.6, 0.5, 0.7), (1.5, 1.0, 2.0)]));
        assert!(!ratios_in(&[(0.6, 0.5, 0.7), (3.0, 1.0, 2.0)])); // second out of range
        assert!(!ratios_in(&[(0.4, 0.5, 0.7)])); // below the window
    }

    #[test]
    fn candles_for_pivots_realizes_the_requested_swings() {
        let want = [120.0, 100.0, 125.0, 95.0];
        let mut t = SwingTracker::new(0.05, 6);
        for candle in candles_for_pivots(&want) {
            let _ = t.update(candle);
        }
        let got: Vec<f64> = t.pivots().iter().map(|p| p.price).collect();
        assert_eq!(got, want);
        // Directions alternate starting from a high.
        assert_eq!(t.pivots()[0].direction, 1.0);
        assert_eq!(t.pivots()[1].direction, -1.0);
    }

    #[test]
    fn approx_equal_relative_tolerance() {
        assert!(approx_equal(100.0, 102.0, 0.03)); // 2% apart, within 3%
        assert!(!approx_equal(100.0, 110.0, 0.03)); // 10% apart, outside 3%
        assert!(approx_equal(0.0, 0.0, 0.01)); // both zero
        assert!(approx_equal(-50.0, -49.0, 0.05)); // negative magnitudes
    }

    #[test]
    fn tracks_extreme_bar_and_current_bar() {
        let mut t = SwingTracker::new(0.10, 6);
        // Before any candle, the current bar saturates at 0.
        assert_eq!(t.current_bar(), 0);
        let _ = t.update(c_hl(100.0, 99.5, 0));
        let _ = t.update(c_hl(120.0, 119.5, 1)); // candidate high at bar 1
        let _ = t.update(c_hl(101.0, 90.0, 2)); // confirm high @120 (extreme bar 1)
        assert_eq!(t.current_bar(), 2);
        assert_eq!(t.pivots().last().unwrap().bar, 1);
    }
}
