//! Hasbrouck Information Share — each venue's contribution to price discovery.

use std::collections::VecDeque;

use crate::error::{Error, Result};
use crate::traits::Indicator;

/// Number of regressors per VECM equation: constant, error-correction term and
/// one lag of each price change.
const K: usize = 4;

/// Hasbrouck Information Share — the share of price discovery attributable to the
/// **first** of two synchronised price series (e.g. the same asset on two venues),
/// estimated from a vector error-correction model (Hasbrouck, *"One Security,
/// Many Markets"*, Journal of Finance, 1995).
///
/// The two prices are cointegrated with vector `β = (1, −1)` (one asset, two
/// venues). Over the trailing window a VECM with one lag is fitted by OLS:
///
/// ```text
/// Δp_t = c + α · (x_{t−1} − y_{t−1}) + Γ · Δp_{t−1} + e_t,   p = (x, y),  Ω = Cov(e)
/// ψ    ∝ α⊥ = (α_y, −α_x)                 (the common long-run impact row)
/// F    = Cholesky factor of Ω (lower), x ordered first
/// IS_x(first) = ([ψ·F]_x)² / (ψ·Ω·ψᵀ)     (upper bound for x)
/// IS_x(last)  = 1 − IS_y(first)          (lower bound for x)
/// IS_x        = (IS_x(first) + IS_x(last)) / 2
/// ```
///
/// The bounds come from the two Cholesky orderings; the indicator reports their
/// midpoint, the usual point estimate. A venue that does not error-correct
/// (`α ≈ 0` for it) leads price discovery and holds most of the share. The output
/// is in `[0, 1]`; a degenerate window (flat prices, a singular regression or no
/// error correction at all) reports the neutral `0.5`. The first value lands
/// after `period + 2` inputs (two prices seed the lagged change); each `update`
/// refits the window (O(period)).
///
/// # Example
///
/// ```
/// use wickra_core::{Indicator, HasbrouckInformationShare};
///
/// let mut indicator = HasbrouckInformationShare::new(60).unwrap();
/// let mut last = None;
/// let mut x = 100.0;
/// let mut prev_x = 100.0;
/// for i in 0..200 {
///     // Venue x carries the news; venue y follows one step later.
///     prev_x = x;
///     x += (f64::from(i) * 1.7).sin() + 0.3 * (f64::from(i) * 0.37).cos();
///     let y = prev_x + 0.05 * (f64::from(i) * 2.3).sin();
///     last = indicator.update((x, y));
/// }
/// assert!(last.unwrap() > 0.8);
/// ```
#[derive(Debug, Clone)]
pub struct HasbrouckInformationShare {
    period: usize,
    /// The last two price pairs, oldest first.
    prices: VecDeque<(f64, f64)>,
    /// `(Δx_t, Δy_t, z_{t−1}, Δx_{t−1}, Δy_{t−1})` for the last `period` bars.
    window: VecDeque<[f64; 5]>,
}

impl HasbrouckInformationShare {
    /// Construct a Hasbrouck information share over `period` observations.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidPeriod`] if `period < 6` (the four-regressor
    /// VECM needs at least two residual degrees of freedom).
    pub fn new(period: usize) -> Result<Self> {
        if period < K + 2 {
            return Err(Error::InvalidPeriod {
                message: "information share needs period >= 6",
            });
        }
        if period > crate::error::MAX_PERIOD {
            return Err(Error::InvalidPeriod {
                message: crate::error::PERIOD_ABOVE_MAX,
            });
        }
        Ok(Self {
            period,
            prices: VecDeque::with_capacity(2),
            window: VecDeque::with_capacity(period),
        })
    }

    /// Configured window of observations.
    pub const fn period(&self) -> usize {
        self.period
    }

    /// Fit the VECM over the window and return `IS_x`, or `None` when the fit
    /// is degenerate.
    fn information_share(&self) -> Option<f64> {
        // Normal equations X'X b = X'y for both equations (same regressors).
        let mut xtx = [[0.0; K]; K];
        let mut xty = [[0.0; 2]; K];
        for row in &self.window {
            let reg = [1.0, row[2], row[3], row[4]];
            for i in 0..K {
                for j in 0..K {
                    xtx[i][j] += reg[i] * reg[j];
                }
                xty[i][0] += reg[i] * row[0];
                xty[i][1] += reg[i] * row[1];
            }
        }
        let bx = solve(xtx, [xty[0][0], xty[1][0], xty[2][0], xty[3][0]])?;
        let by = solve(xtx, [xty[0][1], xty[1][1], xty[2][1], xty[3][1]])?;
        // Residual covariance Ω.
        let (mut sxx, mut sxy, mut syy) = (0.0, 0.0, 0.0);
        for row in &self.window {
            let reg = [1.0, row[2], row[3], row[4]];
            let fit = |b: &[f64; K]| b.iter().zip(&reg).map(|(c, r)| c * r).sum::<f64>();
            let (ex, ey) = (row[0] - fit(&bx), row[1] - fit(&by));
            sxx += ex * ex;
            sxy += ex * ey;
            syy += ey * ey;
        }
        let n = self.window.len() as f64;
        let (oxx, oxy, oyy) = (sxx / n, sxy / n, syy / n);
        // ψ ∝ α⊥ = (α_y, −α_x).
        let (px, py) = (by[1], -bx[1]);
        let total = px * px * oxx + 2.0 * px * py * oxy + py * py * oyy;
        // `NaN`-safe: anything that is not a strictly positive number fails.
        if total.partial_cmp(&0.0) != Some(std::cmp::Ordering::Greater)
            || oxx.partial_cmp(&0.0) != Some(std::cmp::Ordering::Greater)
            || oyy.partial_cmp(&0.0) != Some(std::cmp::Ordering::Greater)
        {
            return None;
        }
        // x first: F = [[√oxx, 0], [oxy/√oxx, ·]] -> [ψF]_x = px·√oxx + py·oxy/√oxx.
        let fx = px * oxx.sqrt() + py * oxy / oxx.sqrt();
        // y first: [ψF]_y = py·√oyy + px·oxy/√oyy.
        let fy = py * oyy.sqrt() + px * oxy / oyy.sqrt();
        let upper = (fx * fx / total).clamp(0.0, 1.0);
        let lower = (1.0 - fy * fy / total).clamp(0.0, 1.0);
        let share = f64::midpoint(upper, lower);
        share.is_finite().then_some(share)
    }
}

/// Solve the 4×4 system `a · x = b` by Gaussian elimination with partial
/// pivoting; `None` if it is singular.
fn solve(mut a: [[f64; K]; K], mut b: [f64; K]) -> Option<[f64; K]> {
    for col in 0..K {
        let pivot = (col..K).max_by(|&i, &j| a[i][col].abs().total_cmp(&a[j][col].abs()))?;
        if a[pivot][col].abs() < 1e-12 {
            return None;
        }
        a.swap(col, pivot);
        b.swap(col, pivot);
        let pivot_row = a[col];
        for row in col + 1..K {
            let f = a[row][col] / pivot_row[col];
            for (cell, &p) in a[row][col..].iter_mut().zip(&pivot_row[col..]) {
                *cell -= f * p;
            }
            b[row] -= f * b[col];
        }
    }
    let mut x = [0.0; K];
    for row in (0..K).rev() {
        let tail: f64 = (row + 1..K).map(|k| a[row][k] * x[k]).sum();
        x[row] = (b[row] - tail) / a[row][row];
    }
    Some(x)
}

impl Indicator for HasbrouckInformationShare {
    type Input = (f64, f64);
    type Output = f64;

    #[inline]
    fn update(&mut self, input: (f64, f64)) -> Option<f64> {
        let (x, y) = input;
        if !x.is_finite() || !y.is_finite() {
            return None;
        }
        if self.prices.len() == 2 {
            let (x2, y2) = self.prices[0];
            let (x1, y1) = self.prices[1];
            if self.window.len() == self.period {
                self.window.pop_front();
            }
            self.window
                .push_back([x - x1, y - y1, x1 - y1, x1 - x2, y1 - y2]);
            self.prices.pop_front();
        }
        self.prices.push_back((x, y));
        if self.window.len() < self.period {
            return None;
        }
        Some(self.information_share().unwrap_or(0.5))
    }

    fn reset(&mut self) {
        self.prices.clear();
        self.window.clear();
    }

    #[inline]
    fn warmup_period(&self) -> usize {
        self.period + 2
    }

    #[inline]
    fn is_ready(&self) -> bool {
        self.window.len() == self.period
    }

    #[inline]
    fn name(&self) -> &'static str {
        "HasbrouckInformationShare"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;
    use approx::assert_relative_eq;

    /// Venue `x` carries the news; `y` follows it with a one-step lag.
    fn leader_follower(n: i32) -> Vec<(f64, f64)> {
        let mut x = 100.0;
        let mut out = Vec::new();
        for i in 0..n {
            let prev_x = x;
            x += (f64::from(i) * 1.7).sin() + 0.3 * (f64::from(i) * 0.37).cos();
            out.push((x, prev_x + 0.05 * (f64::from(i) * 2.3).sin()));
        }
        out
    }

    #[test]
    fn rejects_period_below_six() {
        assert!(matches!(
            HasbrouckInformationShare::new(5),
            Err(Error::InvalidPeriod { .. })
        ));
        assert!(HasbrouckInformationShare::new(6).is_ok());
    }

    #[test]
    fn accessors_and_metadata() {
        let h = HasbrouckInformationShare::new(20).unwrap();
        assert_eq!(h.period(), 20);
        assert_eq!(h.warmup_period(), 22);
        assert_eq!(h.name(), "HasbrouckInformationShare");
        assert!(!h.is_ready());
    }

    #[test]
    fn warmup_needs_period_plus_two() {
        let mut h = HasbrouckInformationShare::new(6).unwrap();
        let pairs = leader_follower(8);
        for p in &pairs[..7] {
            assert_eq!(h.update(*p), None);
        }
        assert!(h.update(pairs[7]).is_some());
    }

    #[test]
    fn leading_venue_holds_the_share() {
        let last = HasbrouckInformationShare::new(60)
            .unwrap()
            .batch(&leader_follower(200))
            .into_iter()
            .flatten()
            .last()
            .unwrap();
        assert!(last > 0.8, "the leading venue should dominate");
        // Swapping the venues swaps the share.
        let swapped: Vec<(f64, f64)> = leader_follower(200)
            .into_iter()
            .map(|(a, b)| (b, a))
            .collect();
        let last = HasbrouckInformationShare::new(60)
            .unwrap()
            .batch(&swapped)
            .into_iter()
            .flatten()
            .last()
            .unwrap();
        assert!(last < 0.2, "the following venue should hold little");
    }

    #[test]
    fn output_is_a_share() {
        let pairs: Vec<(f64, f64)> = (0..200)
            .map(|i| {
                (
                    100.0 + (f64::from(i) * 0.5).sin() * 5.0,
                    100.0 + (f64::from(i) * 0.5).cos() * 5.0,
                )
            })
            .collect();
        for v in HasbrouckInformationShare::new(40)
            .unwrap()
            .batch(&pairs)
            .into_iter()
            .flatten()
        {
            assert!((0.0..=1.0).contains(&v));
        }
    }

    #[test]
    fn flat_series_is_half() {
        let pairs: Vec<(f64, f64)> = (0..20).map(|_| (7.0, 9.0)).collect();
        let last = HasbrouckInformationShare::new(6)
            .unwrap()
            .batch(&pairs)
            .into_iter()
            .flatten()
            .last()
            .unwrap();
        assert_relative_eq!(last, 0.5, epsilon = 1e-12);
    }

    #[test]
    fn reset_clears_state() {
        let mut h = HasbrouckInformationShare::new(6).unwrap();
        h.batch(&leader_follower(10));
        assert!(h.is_ready());
        h.reset();
        assert!(!h.is_ready());
        assert_eq!(h.update((1.0, 1.0)), None);
    }

    #[test]
    fn batch_equals_streaming() {
        let pairs: Vec<(f64, f64)> = (0..120)
            .map(|i| {
                let t = f64::from(i);
                (t.sin() * 5.0, (t * 0.5).cos() * 3.0)
            })
            .collect();
        let batch = HasbrouckInformationShare::new(20).unwrap().batch(&pairs);
        let mut h = HasbrouckInformationShare::new(20).unwrap();
        let streamed: Vec<_> = pairs.iter().map(|p| h.update(*p)).collect();
        assert_eq!(batch, streamed);
    }

    #[test]
    fn non_finite_input_returns_none() {
        let mut h = HasbrouckInformationShare::new(6).unwrap();
        assert_eq!(h.update((f64::NAN, 1.0)), None);
        assert_eq!(h.update((1.0, f64::INFINITY)), None);
        // Non-finite ticks are skipped: the window still needs period + 2 pairs.
        let pairs = leader_follower(8);
        for p in &pairs[..7] {
            assert_eq!(h.update(*p), None);
        }
        assert!(h.update(pairs[7]).is_some());
    }

    /// Load a crafted window `[dx, dy, z, dx_lag, dy_lag]` straight into a
    /// fresh period-6 instance.
    fn with_window(rows: &[[f64; 5]; 6]) -> HasbrouckInformationShare {
        let mut h = HasbrouckInformationShare::new(6).unwrap();
        h.window.extend(rows.iter().copied());
        h
    }

    /// Regressor columns `1, z, dx_lag, dy_lag` that are mutually orthogonal:
    /// `X'X = diag(6, 2, 2, 2)`. Any residual vector of the form
    /// `(p, p, q, q, r, r)` with `p + q + r = 0` is orthogonal to all four.
    const REGRESSORS: [[f64; 3]; 6] = [
        [1.0, 0.0, 0.0],
        [-1.0, 0.0, 0.0],
        [0.0, 1.0, 0.0],
        [0.0, -1.0, 0.0],
        [0.0, 0.0, 1.0],
        [0.0, 0.0, -1.0],
    ];

    /// Build rows `dx = alpha_x * z + ex`, `dy = alpha_y * z + ey`.
    fn crafted_rows(alpha_x: f64, alpha_y: f64, ex: [f64; 6], ey: [f64; 6]) -> [[f64; 5]; 6] {
        let mut rows = [[0.0; 5]; 6];
        for (i, row) in rows.iter_mut().enumerate() {
            let [z, ax, ay] = REGRESSORS[i];
            *row = [alpha_x * z + ex[i], alpha_y * z + ey[i], z, ax, ay];
        }
        rows
    }

    const RESID_U: [f64; 6] = [1.0, 1.0, -1.0, -1.0, 0.0, 0.0];
    const RESID_V: [f64; 6] = [1.0, 1.0, 0.0, 0.0, -1.0, -1.0];

    #[test]
    fn rejects_every_invalid_period() {
        for period in [0, 1, 5] {
            assert!(matches!(
                HasbrouckInformationShare::new(period),
                Err(Error::InvalidPeriod { .. })
            ));
        }
        let too_big = HasbrouckInformationShare::new(crate::error::MAX_PERIOD + 1);
        assert!(matches!(too_big, Err(Error::InvalidPeriod { .. })));
        assert!(HasbrouckInformationShare::new(crate::error::MAX_PERIOD).is_ok());
    }

    #[test]
    fn hand_computed_vecm_share() {
        // dx = -0.5 z + u, dy = 1.0 z + v with u = (1,1,-1,-1,0,0) and
        // v = (1,1,0,0,-1,-1), both orthogonal to every regressor, so OLS
        // recovers alpha_x = -0.5, alpha_y = 1.0 and residuals u, v exactly.
        // Omega = (u'u, u'v, v'v) / 6 = (4, 2, 4) / 6 = (2/3, 1/3, 2/3).
        // psi = (alpha_y, -alpha_x) = (1, 0.5).
        // total = 1 * 2/3 + 2 * 1 * 0.5 * 1/3 + 0.25 * 2/3 = 7/6.
        // fx = (1 * 2/3 + 0.5 * 1/3) / sqrt(2/3) -> fx^2 = (25/36) / (2/3) = 25/24.
        // upper = (25/24) / (7/6) = 25/28.
        // fy = (0.5 * 2/3 + 1 * 1/3) / sqrt(2/3) -> fy^2 = (4/9) / (2/3) = 2/3.
        // lower = 1 - (2/3) / (7/6) = 3/7 = 12/28.
        // share = (25/28 + 12/28) / 2 = 37/56.
        let h = with_window(&crafted_rows(-0.5, 1.0, RESID_U, RESID_V));
        let share = h.information_share().unwrap();
        assert_relative_eq!(share, 37.0 / 56.0, epsilon = 1e-12);
        // Swapping the venues swaps alpha_x / alpha_y (with a sign flip of z)
        // and the residuals; the share of the new first venue is 1 - 37/56.
        let swapped: [[f64; 5]; 6] =
            crafted_rows(-0.5, 1.0, RESID_U, RESID_V).map(|r| [r[1], r[0], -r[2], r[4], r[3]]);
        let share = with_window(&swapped).information_share().unwrap();
        assert_relative_eq!(share, 19.0 / 56.0, epsilon = 1e-12);
    }

    #[test]
    fn hand_computed_symmetric_vecm_is_half() {
        // alpha_x = -0.5, alpha_y = 0.5: psi = (0.5, 0.5), Omega = (2/3, 1/3, 2/3).
        // total = 0.25 * (2/3 + 2/3 + 2/3) = 0.5; fx^2 = fy^2 = 0.25 * 1.5 = 0.375.
        // upper = 0.75, lower = 0.25, share = 0.5.
        let h = with_window(&crafted_rows(-0.5, 0.5, RESID_U, RESID_V));
        assert_relative_eq!(h.information_share().unwrap(), 0.5, epsilon = 1e-12);
    }

    #[test]
    fn zero_price_changes_give_no_share() {
        // All dx and dy rows are exactly 0.0 while the regressors vary, so X'X
        // is non-singular, both coefficient vectors solve to exactly 0 and
        // total = oxx = oyy = 0 -> the "not strictly positive" guard fires.
        let h = with_window(&crafted_rows(0.0, 0.0, [0.0; 6], [0.0; 6]));
        assert_eq!(h.information_share(), None);
    }

    #[test]
    fn zero_residual_variance_on_one_side_gives_no_share() {
        // dx fits exactly (oxx = 0) but dy carries noise: total = 0.25 * oyy > 0,
        // so the oxx check is the one that rejects.
        let h = with_window(&crafted_rows(-0.5, 1.0, [0.0; 6], RESID_V));
        assert_eq!(h.information_share(), None);
        // dy fits exactly (oyy = 0) but dx carries noise: total = oxx > 0,
        // so the oyy check is the one that rejects.
        let h = with_window(&crafted_rows(-0.5, 1.0, RESID_U, [0.0; 6]));
        assert_eq!(h.information_share(), None);
    }

    #[test]
    fn solve_hand_computed_and_singular() {
        // diag(6, 2, 2, 2) x = (6, -1, 4, 0) -> x = (1, -0.5, 2, 0).
        let mut a = [[0.0; K]; K];
        for (i, d) in [6.0, 2.0, 2.0, 2.0].into_iter().enumerate() {
            a[i][i] = d;
        }
        let x = solve(a, [6.0, -1.0, 4.0, 0.0]).unwrap();
        assert_eq!(x, [1.0, -0.5, 2.0, 0.0]);
        // A system that needs a row swap: [[0,1],[1,0]] block.
        let mut b = a;
        b[0] = [0.0, 1.0, 0.0, 0.0];
        b[1] = [1.0, 0.0, 0.0, 0.0];
        let x = solve(b, [3.0, 5.0, 4.0, 2.0]).unwrap();
        assert_eq!(x, [5.0, 3.0, 2.0, 1.0]);
        // The zero matrix is singular.
        assert_eq!(solve([[0.0; K]; K], [1.0; K]), None);
        // Two identical rows: singular after elimination.
        let mut c = a;
        c[1] = c[0];
        assert_eq!(solve(c, [1.0; K]), None);
    }

    #[test]
    fn singular_regression_via_update_reports_half() {
        // A constant spread keeps z constant (collinear with the intercept) ->
        // solve fails -> the neutral 0.5 is reported.
        let pairs: Vec<(f64, f64)> = (0..12)
            .map(|i| (f64::from(i), f64::from(i) + 1.0))
            .collect();
        let out = HasbrouckInformationShare::new(6).unwrap().batch(&pairs);
        assert_eq!(out[7], Some(0.5));
        assert_eq!(out[11], Some(0.5));
    }

    #[test]
    fn first_value_lands_exactly_at_warmup_index() {
        let pairs = leader_follower(40);
        let mut h = HasbrouckInformationShare::new(10).unwrap();
        let out = h.batch(&pairs);
        let warm = h.warmup_period();
        assert!(out[..warm - 1].iter().all(Option::is_none));
        assert!(out[warm - 1..].iter().all(Option::is_some));
    }

    #[test]
    fn reset_replays_identically() {
        let pairs = leader_follower(80);
        let fresh = HasbrouckInformationShare::new(12).unwrap().batch(&pairs);
        let mut h = HasbrouckInformationShare::new(12).unwrap();
        let _ = h.batch(&pairs);
        h.reset();
        assert_eq!(h.batch(&pairs), fresh);
    }

    #[test]
    fn batch_nan_into_matches_streaming_bits() {
        let pairs = leader_follower(80);
        let mut h = HasbrouckInformationShare::new(12).unwrap();
        let streamed: Vec<f64> = pairs
            .iter()
            .map(|p| h.update(*p).unwrap_or(f64::NAN))
            .collect();
        let mut out = vec![0.0; pairs.len()];
        HasbrouckInformationShare::new(12)
            .unwrap()
            .batch_nan_into(&pairs, &mut out);
        assert!(streamed
            .iter()
            .zip(&out)
            .all(|(a, b)| a.to_bits() == b.to_bits()));
        let mut fast = vec![0.0; pairs.len()];
        HasbrouckInformationShare::new(12)
            .unwrap()
            .batch_fast_into(&pairs, &mut fast);
        assert!(streamed
            .iter()
            .zip(&fast)
            .all(|(a, b)| a.to_bits() == b.to_bits()));
    }
}
