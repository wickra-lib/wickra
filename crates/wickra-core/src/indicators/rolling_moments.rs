//! Numerically stable rolling moments.
//!
//! The textbook incremental variance `E[x²] − E[x]²` is cheap and O(1), but it
//! cancels catastrophically once the values are large relative to their spread —
//! which is exactly the shape of a price series. Measured against a two-pass
//! reference over a 20-bar window of `level + sin(i · 0.7) · spread`:
//!
//! | level | spread | `E[x²] − E[x]²` | shifted |
//! |-------|--------|-----------------|---------|
//! | 1e2   | 1      | 3.9e-12         | 3.3e-16 |
//! | 1e2   | 0.01   | 9.0e-08         | 5.1e-16 |
//! | 1e5   | 1      | 4.3e-06         | 1.6e-16 |
//! | 1e5   | 0.01   | **9.4e-02**     | 1.3e-16 |
//! | 1e8   | 1      | **1.0**         | 4.9e-16 |
//!
//! At 1e8 the naive form does not merely lose digits: `sum_sq / n` and
//! `mean · mean` agree to every bit, the difference is exactly zero, and the
//! `.max(0.0)` clamp that was meant to absorb rounding noise hides the collapse
//! completely. A squeeze indicator then reports a permanent squeeze and a
//! z-score divides by zero dispersion. Bitcoin at 100 000 on one-second bars sits
//! in the 1e5 / 0.01 row.
//!
//! `ShiftedMoments` fixes this by accumulating the moments of `x − offset`
//! instead of `x`, where `offset` is a value from inside the window. The
//! subtraction is exact in the common case and near-exact otherwise, so the
//! cancellation that remains is between quantities of order `spread²` rather
//! than `level²`. Cost is unchanged: one add and one subtract per bar, plus an
//! amortised reseed.
//!
//! It deliberately does **not** own the window. Every indicator that needs this
//! already keeps its own values (most in a `VecDeque`, some in a ring buffer),
//! often for other purposes too, so the accumulator attaches to whatever the
//! caller already has and is driven by `push` / `evict`.

/// Rolling `Σ(x − offset)` and `Σ(x − offset)²` for a fixed-length window.
///
/// The caller owns the window and is responsible for calling [`Self::evict`]
/// with a value that was previously [`pushed`](Self::push), and for passing a
/// consistent `n` to the query methods.
#[derive(Debug, Clone)]
pub(crate) struct ShiftedMoments {
    /// Reference point the accumulated moments are relative to. Chosen from
    /// inside the window so `x − offset` stays on the order of the spread.
    offset: f64,
    /// Whether `offset` has been chosen yet. The first pushed value seeds it.
    seeded: bool,
    /// `Σ(x − offset)` over the live window.
    sum: f64,
    /// `Σ(x − offset)²` over the live window.
    sum_sq: f64,
    /// Pushes since the last reseed, used to bound both accumulated drift and
    /// how far `offset` may have wandered from the live window.
    pushes_since_reseed: usize,
}

impl ShiftedMoments {
    /// A fresh accumulator with no reference point yet.
    pub(crate) const fn new() -> Self {
        Self {
            offset: 0.0,
            seeded: false,
            sum: 0.0,
            sum_sq: 0.0,
            pushes_since_reseed: 0,
        }
    }

    /// Add `value` to the accumulated moments.
    ///
    /// The first value seeds the reference point, which makes its own
    /// contribution exactly zero.
    pub(crate) fn push(&mut self, value: f64) {
        if !self.seeded {
            self.offset = value;
            self.seeded = true;
        }
        let d = value - self.offset;
        self.sum += d;
        self.sum_sq += d * d;
        self.pushes_since_reseed += 1;
    }

    /// Remove `value` from the accumulated moments.
    ///
    /// `value` must be one previously passed to [`Self::push`] and not yet
    /// evicted; passing anything else silently corrupts the moments, exactly as
    /// a hand-rolled `sum -= old` would.
    pub(crate) fn evict(&mut self, value: f64) {
        let d = value - self.offset;
        self.sum -= d;
        self.sum_sq -= d * d;
    }

    /// Mean of the `n` values currently in the window.
    pub(crate) fn mean(&self, n: usize) -> f64 {
        self.offset + self.sum / n as f64
    }

    /// Population variance (divisor `n`) of the `n` values in the window.
    ///
    /// Clamped at zero: the residual cancellation is now on the order of
    /// `spread² · ε`, so a negative result is pure rounding noise rather than
    /// the sign of a collapsed computation.
    pub(crate) fn variance(&self, n: usize) -> f64 {
        let n = n as f64;
        let mean_shifted = self.sum / n;
        (self.sum_sq / n - mean_shifted * mean_shifted).max(0.0)
    }

    /// Sample variance (divisor `n − 1`, Bessel's correction) of the `n` values.
    ///
    /// Returns `0.0` for `n < 2`, where the sample variance is undefined.
    pub(crate) fn sample_variance(&self, n: usize) -> f64 {
        if n < 2 {
            return 0.0;
        }
        let nf = n as f64;
        let mean_shifted = self.sum / nf;
        (nf.mul_add(-(mean_shifted * mean_shifted), self.sum_sq) / (nf - 1.0)).max(0.0)
    }

    /// Population standard deviation of the `n` values in the window.
    pub(crate) fn std_dev(&self, n: usize) -> f64 {
        self.variance(n).sqrt()
    }

    /// Whether enough pushes have accumulated to justify a reseed.
    ///
    /// Reseeding once per window keeps the reference point at most one window
    /// old, so `offset` can never drift further from the live values than the
    /// price moved across that window — the same order as the spread the
    /// accumulator is measuring. The cost is `O(period)` once every `period`
    /// pushes, i.e. amortised `O(1)`.
    pub(crate) const fn needs_reseed(&self, period: usize) -> bool {
        self.pushes_since_reseed >= period
    }

    /// Recompute both moments from the live window, re-centring `offset` on its
    /// mean.
    ///
    /// This is the only place either accumulator is derived from scratch, so it
    /// simultaneously bounds the drift that repeated add/subtract accumulates
    /// and re-anchors the reference point. `values` must yield exactly the live
    /// window.
    pub(crate) fn reseed<I>(&mut self, values: I)
    where
        I: IntoIterator<Item = f64> + Clone,
    {
        let mut count = 0_usize;
        let mut total = 0.0;
        for v in values.clone() {
            total += v;
            count += 1;
        }
        if count == 0 {
            self.reset();
            return;
        }
        let mean = total / count as f64;
        self.offset = mean;
        self.seeded = true;
        self.sum = 0.0;
        self.sum_sq = 0.0;
        for v in values {
            let d = v - mean;
            self.sum += d;
            self.sum_sq += d * d;
        }
        self.pushes_since_reseed = 0;
    }

    /// Drop every accumulated value and the reference point.
    pub(crate) fn reset(&mut self) {
        self.offset = 0.0;
        self.seeded = false;
        self.sum = 0.0;
        self.sum_sq = 0.0;
        self.pushes_since_reseed = 0;
    }
}

/// Rolling first and second moments of a *pair* of series, plus their
/// cross-moment.
///
/// Covariance shares the variance defect exactly: `E[xy] - E[x]E[y]` on raw
/// levels cancels once the values are large relative to how they vary, and
/// correlation inherits it through both the covariance and the two variances.
/// Measured on `PearsonCorrelation` over a 20-bar window of two sine series,
/// against a two-pass reference: 8.6e-12 relative error at a level of 100,
/// 1.1e-05 at 1e5, 9.6e-02 at 1e5 with a 0.01 spread, and a complete collapse
/// to a meaningless value at 1e8.
///
/// Both channels are accumulated relative to their own reference point, so the
/// residual cancellation is on the order of the spreads rather than the levels.
/// Cost is unchanged, plus an amortised rebuild.
#[derive(Debug, Clone)]
pub(crate) struct ShiftedPairMoments {
    /// Reference point for the first channel.
    offset_a: f64,
    /// Reference point for the second channel.
    offset_b: f64,
    /// Whether the reference points have been chosen yet.
    seeded: bool,
    /// `Σ(a − offset_a)`.
    sum_a: f64,
    /// `Σ(b − offset_b)`.
    sum_b: f64,
    /// `Σ(a − offset_a)²`.
    sum_aa: f64,
    /// `Σ(b − offset_b)²`.
    sum_bb: f64,
    /// `Σ(a − offset_a)(b − offset_b)`.
    sum_ab: f64,
    /// Pushes since the last rebuild.
    pushes_since_reseed: usize,
}

impl ShiftedPairMoments {
    /// A fresh accumulator with no reference points yet.
    pub(crate) const fn new() -> Self {
        Self {
            offset_a: 0.0,
            offset_b: 0.0,
            seeded: false,
            sum_a: 0.0,
            sum_b: 0.0,
            sum_aa: 0.0,
            sum_bb: 0.0,
            sum_ab: 0.0,
            pushes_since_reseed: 0,
        }
    }

    /// Add one observation.
    pub(crate) fn push(&mut self, a: f64, b: f64) {
        if !self.seeded {
            self.offset_a = a;
            self.offset_b = b;
            self.seeded = true;
        }
        let da = a - self.offset_a;
        let db = b - self.offset_b;
        self.sum_a += da;
        self.sum_b += db;
        self.sum_aa += da * da;
        self.sum_bb += db * db;
        self.sum_ab += da * db;
        self.pushes_since_reseed += 1;
    }

    /// Remove one observation. It must be one previously pushed and not yet
    /// removed.
    pub(crate) fn evict(&mut self, a: f64, b: f64) {
        let da = a - self.offset_a;
        let db = b - self.offset_b;
        self.sum_a -= da;
        self.sum_b -= db;
        self.sum_aa -= da * da;
        self.sum_bb -= db * db;
        self.sum_ab -= da * db;
    }

    /// Mean of the first channel over `n` observations.
    pub(crate) fn mean_a(&self, n: usize) -> f64 {
        self.offset_a + self.sum_a / n as f64
    }

    /// Mean of the second channel over `n` observations.
    pub(crate) fn mean_b(&self, n: usize) -> f64 {
        self.offset_b + self.sum_b / n as f64
    }

    /// Population variance of the first channel, clamped at zero.
    pub(crate) fn var_a(&self, n: usize) -> f64 {
        let nf = n as f64;
        let m = self.sum_a / nf;
        (self.sum_aa / nf - m * m).max(0.0)
    }

    /// Population variance of the second channel, clamped at zero.
    pub(crate) fn var_b(&self, n: usize) -> f64 {
        let nf = n as f64;
        let m = self.sum_b / nf;
        (self.sum_bb / nf - m * m).max(0.0)
    }

    /// Population covariance of the two channels.
    ///
    /// Not clamped: a covariance is legitimately negative.
    pub(crate) fn cov(&self, n: usize) -> f64 {
        let nf = n as f64;
        self.sum_ab / nf - (self.sum_a / nf) * (self.sum_b / nf)
    }

    /// Whether enough pushes have accumulated to justify a rebuild.
    pub(crate) const fn needs_reseed(&self, period: usize) -> bool {
        self.pushes_since_reseed >= period
    }

    /// Rebuild every moment from the live window, re-centring both reference
    /// points on their channel means. `values` must yield exactly the live
    /// window.
    pub(crate) fn reseed<I>(&mut self, values: I)
    where
        I: IntoIterator<Item = (f64, f64)> + Clone,
    {
        let mut count = 0_usize;
        let (mut ta, mut tb) = (0.0, 0.0);
        for (a, b) in values.clone() {
            ta += a;
            tb += b;
            count += 1;
        }
        if count == 0 {
            self.reset();
            return;
        }
        let nf = count as f64;
        self.offset_a = ta / nf;
        self.offset_b = tb / nf;
        self.seeded = true;
        self.sum_a = 0.0;
        self.sum_b = 0.0;
        self.sum_aa = 0.0;
        self.sum_bb = 0.0;
        self.sum_ab = 0.0;
        for (a, b) in values {
            let da = a - self.offset_a;
            let db = b - self.offset_b;
            self.sum_a += da;
            self.sum_b += db;
            self.sum_aa += da * da;
            self.sum_bb += db * db;
            self.sum_ab += da * db;
        }
        self.pushes_since_reseed = 0;
    }

    /// Drop every accumulated value and both reference points.
    pub(crate) fn reset(&mut self) {
        self.offset_a = 0.0;
        self.offset_b = 0.0;
        self.seeded = false;
        self.sum_a = 0.0;
        self.sum_b = 0.0;
        self.sum_aa = 0.0;
        self.sum_bb = 0.0;
        self.sum_ab = 0.0;
        self.pushes_since_reseed = 0;
    }
}

/// A rolling sum that is periodically rebuilt from its window.
///
/// `sum += new; sum -= old` is O(1) but never forgets a rounding error, so the
/// deviation from a from-scratch sum grows without bound over a long stream —
/// and long streams are the case this library is built for. Measured over three
/// million updates: `Vwma` drifts 6e-14 relative, `Cci` 5e-09, `Dpo` 2e-07.
/// Small, but unbounded, and `Sma` — which already reseeded — came out exactly
/// equal to a from-scratch pass.
///
/// Rebuilding once per window costs `O(period)` every `period` pushes, i.e.
/// amortised `O(1)`, and takes the deviation back to zero. The window itself
/// stays with the caller: these accumulators sit alongside deques and ring
/// buffers the indicator already keeps for other reasons, and duplicating that
/// storage here would cost more cache than the reseed saves.
#[derive(Debug, Clone, Default)]
pub(crate) struct RollingSum {
    /// Running total over the live window.
    total: f64,
    /// Pushes since the last rebuild.
    pushes_since_reseed: usize,
}

impl RollingSum {
    /// A fresh, empty accumulator.
    pub(crate) const fn new() -> Self {
        Self {
            total: 0.0,
            pushes_since_reseed: 0,
        }
    }

    /// Add `value` to the total.
    pub(crate) fn push(&mut self, value: f64) {
        self.total += value;
        self.pushes_since_reseed += 1;
    }

    /// Remove `value` from the total. It must be one previously pushed and not
    /// yet removed.
    pub(crate) fn evict(&mut self, value: f64) {
        self.total -= value;
    }

    /// The current total.
    pub(crate) const fn value(&self) -> f64 {
        self.total
    }

    /// Whether enough pushes have accumulated to justify a rebuild.
    pub(crate) const fn needs_reseed(&self, period: usize) -> bool {
        self.pushes_since_reseed >= period
    }

    /// Rebuild the total from the live window. `values` must yield exactly the
    /// values currently included.
    pub(crate) fn reseed<I>(&mut self, values: I)
    where
        I: IntoIterator<Item = f64>,
    {
        self.total = values.into_iter().sum();
        self.pushes_since_reseed = 0;
    }

    /// Drop the total and the rebuild counter.
    pub(crate) fn reset(&mut self) {
        self.total = 0.0;
        self.pushes_since_reseed = 0;
    }
}

/// Rolling central moments up to the fourth, for the shape statistics.
///
/// Skewness and kurtosis reconstruct `m3` and `m4` from raw power sums by
/// binomial expansion (`m4 = E[x⁴] − 4·mean·E[x³] + 6·mean²·E[x²] − 3·mean⁴`).
/// Every term there is of order `level⁴`, while the result is of order
/// `spread⁴`, so the cancellation is worse than for the variance by two further
/// powers of the level — a 1e5 price leaves nothing at all of a 1e-2 spread.
///
/// Accumulating the same power sums for `x − offset` keeps every term on the
/// scale of the spread, so the expansions stay meaningful. The reference point
/// is maintained exactly as in `ShiftedMoments`.
#[derive(Debug, Clone)]
pub(crate) struct ShiftedHigherMoments {
    /// Reference point the accumulated power sums are relative to.
    offset: f64,
    /// Whether `offset` has been chosen yet.
    seeded: bool,
    /// `Σ(x − offset)`.
    s1: f64,
    /// `Σ(x − offset)²`.
    s2: f64,
    /// `Σ(x − offset)³`.
    s3: f64,
    /// `Σ(x − offset)⁴`.
    s4: f64,
    /// Pushes since the last reseed.
    pushes_since_reseed: usize,
}

impl ShiftedHigherMoments {
    /// A fresh accumulator with no reference point yet.
    pub(crate) const fn new() -> Self {
        Self {
            offset: 0.0,
            seeded: false,
            s1: 0.0,
            s2: 0.0,
            s3: 0.0,
            s4: 0.0,
            pushes_since_reseed: 0,
        }
    }

    /// Add `value` to the accumulated power sums.
    pub(crate) fn push(&mut self, value: f64) {
        if !self.seeded {
            self.offset = value;
            self.seeded = true;
        }
        let d = value - self.offset;
        let d2 = d * d;
        self.s1 += d;
        self.s2 += d2;
        self.s3 += d * d2;
        self.s4 += d2 * d2;
        self.pushes_since_reseed += 1;
    }

    /// Remove `value` from the accumulated power sums.
    pub(crate) fn evict(&mut self, value: f64) {
        let d = value - self.offset;
        let d2 = d * d;
        self.s1 -= d;
        self.s2 -= d2;
        self.s3 -= d * d2;
        self.s4 -= d2 * d2;
    }

    /// Second central moment (population variance) of the `n` values.
    pub(crate) fn m2(&self, n: usize) -> f64 {
        let nf = n as f64;
        let a = self.s1 / nf;
        (self.s2 / nf - a * a).max(0.0)
    }

    /// Third central moment of the `n` values.
    pub(crate) fn m3(&self, n: usize) -> f64 {
        let nf = n as f64;
        let a = self.s1 / nf;
        self.s3 / nf - 3.0 * a * (self.s2 / nf) + 2.0 * a * a * a
    }

    /// Fourth central moment of the `n` values.
    pub(crate) fn m4(&self, n: usize) -> f64 {
        let nf = n as f64;
        let a = self.s1 / nf;
        let a2 = a * a;
        self.s4 / nf - 4.0 * a * (self.s3 / nf) + 6.0 * a2 * (self.s2 / nf) - 3.0 * a2 * a2
    }

    /// Whether enough pushes have accumulated to justify a reseed.
    pub(crate) const fn needs_reseed(&self, period: usize) -> bool {
        self.pushes_since_reseed >= period
    }

    /// Recompute every power sum from the live window, re-centring `offset` on
    /// its mean. `values` must yield exactly the live window.
    pub(crate) fn reseed<I>(&mut self, values: I)
    where
        I: IntoIterator<Item = f64> + Clone,
    {
        let mut count = 0_usize;
        let mut total = 0.0;
        for v in values.clone() {
            total += v;
            count += 1;
        }
        if count == 0 {
            self.reset();
            return;
        }
        let mean = total / count as f64;
        self.offset = mean;
        self.seeded = true;
        self.s1 = 0.0;
        self.s2 = 0.0;
        self.s3 = 0.0;
        self.s4 = 0.0;
        for v in values {
            let d = v - mean;
            let d2 = d * d;
            self.s1 += d;
            self.s2 += d2;
            self.s3 += d * d2;
            self.s4 += d2 * d2;
        }
        self.pushes_since_reseed = 0;
    }

    /// Drop every accumulated value and the reference point.
    pub(crate) fn reset(&mut self) {
        self.offset = 0.0;
        self.seeded = false;
        self.s1 = 0.0;
        self.s2 = 0.0;
        self.s3 = 0.0;
        self.s4 = 0.0;
        self.pushes_since_reseed = 0;
    }
}

/// Population variance of each channel and their covariance, over a paired
/// window, computed about that window's own means.
///
/// This is the same defect [`ShiftedPairMoments`] exists for, in the case where
/// no accumulator is needed. Some indicators recompute their statistics from
/// the live window on every update instead of maintaining running sums, and
/// they were still doing it in the one-pass form `E[xy] − E[x]E[y]`, which
/// cancels exactly as badly whether the power sums were carried across updates
/// or built a microsecond ago. Recomputing does bound the *drift*; it does
/// nothing at all about the cancellation.
///
/// Making a second pass costs one extra traversal of a window these callers
/// already traverse, and there is no reference point to maintain because
/// nothing survives the call.
///
/// The window must be non-empty; an empty one divides by zero, exactly as the
/// hand-rolled accumulation it replaces did.
pub(crate) fn centred_moments<I>(pairs: I) -> CentredMoments
where
    I: IntoIterator<Item = (f64, f64)> + Clone,
{
    let (mut sum_x, mut sum_y) = (0.0, 0.0);
    let mut count = 0_usize;
    for (x, y) in pairs.clone() {
        sum_x += x;
        sum_y += y;
        count += 1;
    }
    let n = count as f64;
    let (mean_x, mean_y) = (sum_x / n, sum_y / n);
    let (mut var_x, mut var_y, mut cov) = (0.0, 0.0, 0.0);
    for (x, y) in pairs {
        let dx = x - mean_x;
        let dy = y - mean_y;
        var_x += dx * dx;
        var_y += dy * dy;
        cov += dx * dy;
    }
    CentredMoments {
        // Clamped for the same reason [`ShiftedMoments::variance`] clamps: what
        // is left is rounding noise, not a negative variance.
        var_x: (var_x / n).max(0.0),
        var_y: (var_y / n).max(0.0),
        // Not clamped: a covariance is legitimately negative.
        cov: cov / n,
    }
}

/// The result of [`centred_moments`].
pub(crate) struct CentredMoments {
    /// Population variance of the first channel.
    pub(crate) var_x: f64,
    /// Population variance of the second channel.
    pub(crate) var_y: f64,
    /// Population covariance of the two channels.
    pub(crate) cov: f64,
}

/// Rolling sums for a least-squares fit of a window against its own index,
/// centred on a reference point.
///
/// The OLS slope of `y` on the position index is mathematically invariant when
/// a constant is subtracted from `y`, and so is everything derived from the
/// residuals -- the coefficient of determination, the standard error, the
/// detrended deviation. Only quantities that name an absolute price level (the
/// intercept, the endpoint, a forecast) need the reference point added back.
///
/// That invariance is what makes the fix cheap, and the defect it fixes is
/// severe. Measured on a 20-bar window carrying a one-unit wobble on top of a
/// price level, against a two-pass reference: the raw form
/// `(n·Σxy − Σx·Σy)/denom` put the slope 8.8e-09 out at a level of 100, 7.9e-06
/// at 1e5 and 2.2e-02 at 1e8. `RSquared` divides one cancelled quantity by
/// another and reached 8.3e+04 -- on a value defined to lie in `[0, 1]` -- and
/// the standard error collapsed to exactly zero, reporting a perfect fit for a
/// series that was not fitted at all.
///
/// The caller owns the window and drives the accumulator with it: [`push`] for
/// an arrival at the end, [`slide`] for the departure from the front, which
/// also shifts every remaining index down by one.
///
/// [`push`]: Self::push
/// [`slide`]: Self::slide
#[derive(Debug, Clone)]
pub(crate) struct ShiftedTrend {
    /// Reference point the accumulated sums are relative to.
    offset: f64,
    /// Whether `offset` has been chosen yet. The first pushed value seeds it.
    seeded: bool,
    /// `Σ(y − offset)` over the live window.
    sum_y: f64,
    /// `Σ i·(yᵢ − offset)`, with `i` the position within the live window.
    sum_xy: f64,
    /// `Σ(y − offset)²` over the live window.
    sum_y_sq: f64,
    /// Pushes since the last reseed.
    pushes_since_reseed: usize,
}

impl ShiftedTrend {
    /// A fresh accumulator with no reference point yet.
    pub(crate) const fn new() -> Self {
        Self {
            offset: 0.0,
            seeded: false,
            sum_y: 0.0,
            sum_xy: 0.0,
            sum_y_sq: 0.0,
            pushes_since_reseed: 0,
        }
    }

    /// Add `value`, arriving at position `index` of the window.
    ///
    /// The first value seeds the reference point, which makes its own
    /// contribution exactly zero.
    pub(crate) fn push(&mut self, value: f64, index: usize) {
        if !self.seeded {
            self.offset = value;
            self.seeded = true;
        }
        let d = value - self.offset;
        self.sum_y += d;
        self.sum_xy += index as f64 * d;
        self.sum_y_sq += d * d;
        self.pushes_since_reseed += 1;
    }

    /// Drop the value at position 0 and shift every remaining index down by one.
    ///
    /// `front` is the raw value leaving the window. Shifting the indices uses
    /// the identity `Σ((i−1)·yᵢ) = Σ(i·yᵢ) − Σ(yᵢ) + y₀`, which is linear in
    /// `y` and therefore holds just as well for the shifted values.
    pub(crate) fn slide(&mut self, front: f64) {
        let d = front - self.offset;
        self.sum_xy = self.sum_xy - self.sum_y + d;
        self.sum_y -= d;
        self.sum_y_sq -= d * d;
    }

    /// The reference point, to be added back to any absolute level derived from
    /// these sums.
    pub(crate) const fn offset(&self) -> f64 {
        self.offset
    }

    /// `Σ(y − offset)` over the live window.
    pub(crate) const fn sum_y(&self) -> f64 {
        self.sum_y
    }

    /// `Σ i·(yᵢ − offset)` over the live window.
    pub(crate) const fn sum_xy(&self) -> f64 {
        self.sum_xy
    }

    /// `Σ(y − offset)²` over the live window.
    pub(crate) const fn sum_y_sq(&self) -> f64 {
        self.sum_y_sq
    }

    /// Whether enough pushes have accumulated to justify a reseed.
    pub(crate) const fn needs_reseed(&self, period: usize) -> bool {
        self.pushes_since_reseed >= period
    }

    /// Rebuild every sum from the live window, re-centring `offset` on its mean.
    /// `values` must yield exactly the live window, in position order.
    pub(crate) fn reseed<I>(&mut self, values: I)
    where
        I: IntoIterator<Item = f64> + Clone,
    {
        let mut total = 0.0;
        let mut count = 0_usize;
        for v in values.clone() {
            total += v;
            count += 1;
        }
        if count == 0 {
            self.reset();
            return;
        }
        self.offset = total / count as f64;
        self.seeded = true;
        self.sum_y = 0.0;
        self.sum_xy = 0.0;
        self.sum_y_sq = 0.0;
        for (index, v) in values.into_iter().enumerate() {
            let d = v - self.offset;
            self.sum_y += d;
            self.sum_xy += index as f64 * d;
            self.sum_y_sq += d * d;
        }
        self.pushes_since_reseed = 0;
    }

    /// Drop every accumulated value and the reference point.
    pub(crate) fn reset(&mut self) {
        self.offset = 0.0;
        self.seeded = false;
        self.sum_y = 0.0;
        self.sum_xy = 0.0;
        self.sum_y_sq = 0.0;
        self.pushes_since_reseed = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_relative_eq;

    /// Two-pass population variance — the numerically stable reference.
    fn reference_variance(window: &[f64]) -> f64 {
        let n = window.len() as f64;
        let mean = window.iter().sum::<f64>() / n;
        window.iter().map(|x| (x - mean) * (x - mean)).sum::<f64>() / n
    }

    /// Drive a rolling window of `period` through `values`, returning the final
    /// accumulator and the live window.
    fn roll(values: &[f64], period: usize, reseed: bool) -> (ShiftedMoments, Vec<f64>) {
        let mut moments = ShiftedMoments::new();
        let mut window: Vec<f64> = Vec::with_capacity(period);
        for &v in values {
            if window.len() == period {
                moments.evict(window.remove(0));
            }
            window.push(v);
            moments.push(v);
            if reseed && moments.needs_reseed(period) {
                moments.reseed(window.iter().copied());
            }
        }
        (moments, window)
    }

    #[test]
    fn matches_the_two_pass_reference_at_extreme_price_levels() {
        for level in [1.0e2_f64, 1.0e5, 1.0e8] {
            for spread in [1.0_f64, 0.01] {
                let values: Vec<f64> = (0..60)
                    .map(|i| level + (f64::from(i) * 0.7).sin() * spread)
                    .collect();
                let (moments, window) = roll(&values, 20, true);
                let want = reference_variance(&window);
                assert_relative_eq!(moments.variance(20), want, max_relative = 1e-9);
            }
        }
    }

    #[test]
    fn mean_matches_the_window_mean() {
        let values: Vec<f64> = (0..40).map(|i| 1.0e6 + f64::from(i) * 0.25).collect();
        let (moments, window) = roll(&values, 12, true);
        let want = window.iter().sum::<f64>() / 12.0;
        assert_relative_eq!(moments.mean(12), want, max_relative = 1e-12);
    }

    #[test]
    fn variance_matches_a_known_data_set() {
        let values = [2.0_f64, 4.0, 4.0, 4.0, 5.0, 5.0, 7.0, 9.0];
        let (moments, _) = roll(&values, 8, false);
        // Population variance of this classic set is exactly 4.
        assert_relative_eq!(moments.variance(8), 4.0, epsilon = 1e-12);
    }

    #[test]
    fn sample_variance_applies_bessels_correction() {
        let values = [2.0_f64, 4.0, 4.0, 4.0, 5.0, 5.0, 7.0, 9.0];
        let (moments, _) = roll(&values, 8, false);
        // Population variance of this classic set is 4, sample variance 32/7.
        assert_relative_eq!(moments.variance(8), 4.0, epsilon = 1e-12);
        assert_relative_eq!(moments.sample_variance(8), 32.0 / 7.0, epsilon = 1e-12);
    }

    #[test]
    fn sample_variance_is_zero_below_two_values() {
        let (moments, _) = roll(&[3.0], 1, false);
        assert_relative_eq!(moments.sample_variance(1), 0.0, epsilon = 1e-12);
    }

    #[test]
    fn sample_variance_stays_accurate_at_extreme_levels() {
        for level in [1.0e2_f64, 1.0e5, 1.0e8] {
            let values: Vec<f64> = (0..60)
                .map(|i| level + (f64::from(i) * 0.7).sin())
                .collect();
            let (moments, window) = roll(&values, 20, true);
            let n = window.len() as f64;
            let mean = window.iter().sum::<f64>() / n;
            let want = window.iter().map(|x| (x - mean) * (x - mean)).sum::<f64>() / (n - 1.0);
            assert_relative_eq!(moments.sample_variance(20), want, max_relative = 1e-9);
        }
    }

    #[test]
    fn std_dev_is_the_root_of_the_variance() {
        let values: Vec<f64> = (0..30)
            .map(|i| 100.0 + (f64::from(i) * 0.5).sin())
            .collect();
        let (moments, _) = roll(&values, 10, true);
        assert_relative_eq!(
            moments.std_dev(10),
            moments.variance(10).sqrt(),
            epsilon = 1e-15
        );
    }

    #[test]
    fn a_constant_window_has_zero_variance() {
        let (moments, _) = roll(&[1.0e8_f64; 40], 16, true);
        assert_relative_eq!(moments.variance(16), 0.0, epsilon = 1e-12);
    }

    #[test]
    fn reseeding_bounds_drift_over_a_long_stream() {
        // Without the periodic reseed the incremental add/subtract accumulates
        // rounding noise; with it the result must still track the reference.
        let values: Vec<f64> = (0..200_000)
            .map(|i| 1.0e5 + (f64::from(i % 1000) * 0.01).sin())
            .collect();
        let (moments, window) = roll(&values, 50, true);
        assert_relative_eq!(
            moments.variance(50),
            reference_variance(&window),
            max_relative = 1e-9
        );
    }

    #[test]
    fn reseed_on_an_empty_window_clears_the_accumulator() {
        let mut moments = ShiftedMoments::new();
        moments.push(5.0);
        moments.reseed(std::iter::empty());
        assert_relative_eq!(moments.variance(1), 0.0, epsilon = 1e-15);
        assert_relative_eq!(moments.mean(1), 0.0, epsilon = 1e-15);
    }

    #[test]
    fn reset_clears_the_reference_point() {
        let mut moments = ShiftedMoments::new();
        moments.push(1.0e8);
        moments.reset();
        // A fresh reference point must be taken from the next value, not kept
        // from before the reset.
        moments.push(1.0);
        moments.push(3.0);
        assert_relative_eq!(moments.mean(2), 2.0, epsilon = 1e-15);
        assert_relative_eq!(moments.variance(2), 1.0, epsilon = 1e-15);
    }

    /// Two-pass central moment of order `k` — the stable reference.
    fn reference_central_moment(window: &[f64], k: i32) -> f64 {
        let n = window.len() as f64;
        let mean = window.iter().sum::<f64>() / n;
        window.iter().map(|x| (x - mean).powi(k)).sum::<f64>() / n
    }

    /// Drive a rolling window through the four-moment accumulator.
    fn roll4(values: &[f64], period: usize) -> (ShiftedHigherMoments, Vec<f64>) {
        let mut moments = ShiftedHigherMoments::new();
        let mut window: Vec<f64> = Vec::with_capacity(period);
        for &v in values {
            if window.len() == period {
                moments.evict(window.remove(0));
            }
            window.push(v);
            moments.push(v);
            if moments.needs_reseed(period) {
                moments.reseed(window.iter().copied());
            }
        }
        (moments, window)
    }

    #[test]
    fn higher_moments_match_the_two_pass_reference_at_extreme_levels() {
        for level in [1.0e2_f64, 1.0e5, 1.0e8] {
            let values: Vec<f64> = (0..60)
                .map(|i| level + (f64::from(i) * 0.7).sin() + (f64::from(i) * 0.13).cos() * 0.4)
                .collect();
            let (moments, window) = roll4(&values, 20);
            assert_relative_eq!(
                moments.m2(20),
                reference_central_moment(&window, 2),
                max_relative = 1e-9
            );
            assert_relative_eq!(
                moments.m3(20),
                reference_central_moment(&window, 3),
                max_relative = 1e-7
            );
            assert_relative_eq!(
                moments.m4(20),
                reference_central_moment(&window, 4),
                max_relative = 1e-8
            );
        }
    }

    #[test]
    fn higher_moments_of_a_constant_window_are_zero() {
        let (moments, _) = roll4(&[1.0e8_f64; 40], 16);
        assert_relative_eq!(moments.m2(16), 0.0, epsilon = 1e-12);
        assert_relative_eq!(moments.m3(16), 0.0, epsilon = 1e-12);
        assert_relative_eq!(moments.m4(16), 0.0, epsilon = 1e-12);
    }

    #[test]
    fn higher_moments_reset_clears_the_reference_point() {
        let mut moments = ShiftedHigherMoments::new();
        moments.push(1.0e8);
        moments.reset();
        moments.push(-1.0);
        moments.push(1.0);
        // Symmetric pair about zero: variance 1, zero skew, fourth moment 1.
        assert_relative_eq!(moments.m2(2), 1.0, epsilon = 1e-15);
        assert_relative_eq!(moments.m3(2), 0.0, epsilon = 1e-15);
        assert_relative_eq!(moments.m4(2), 1.0, epsilon = 1e-15);
    }

    #[test]
    fn higher_moments_reseed_on_an_empty_window_clears_the_accumulator() {
        let mut moments = ShiftedHigherMoments::new();
        moments.push(5.0);
        moments.reseed(std::iter::empty());
        assert_relative_eq!(moments.m2(1), 0.0, epsilon = 1e-15);
        assert_relative_eq!(moments.m4(1), 0.0, epsilon = 1e-15);
    }

    #[test]
    fn higher_moments_needs_reseed_triggers_once_per_window() {
        let mut moments = ShiftedHigherMoments::new();
        for _ in 0..4 {
            moments.push(1.0);
        }
        assert!(!moments.needs_reseed(5));
        moments.push(1.0);
        assert!(moments.needs_reseed(5));
    }

    #[test]
    fn rolling_sum_reseed_removes_accumulated_drift() {
        // `sum += new; sum -= old` never forgets a rounding error, so over a
        // long stream it wanders away from a from-scratch sum. Rebuilding once
        // per window brings it back exactly.
        let period = 20_usize;
        let values: Vec<f64> = (0..200_000)
            .map(|i| 1.0e5 + (f64::from(i % 1000) * 0.017).sin())
            .collect();

        let mut drifting = RollingSum::new();
        let mut reseeding = RollingSum::new();
        let mut window: Vec<f64> = Vec::with_capacity(period);
        for &v in &values {
            if window.len() == period {
                let old = window.remove(0);
                drifting.evict(old);
                reseeding.evict(old);
            }
            window.push(v);
            drifting.push(v);
            reseeding.push(v);
            if reseeding.needs_reseed(period) {
                reseeding.reseed(window.iter().copied());
            }
        }
        let exact: f64 = window.iter().sum();
        assert_relative_eq!(reseeding.value(), exact, max_relative = 0.0);
        assert!(
            (drifting.value() - exact).abs() > 0.0,
            "the un-reseeded accumulator is expected to have drifted"
        );
    }

    #[test]
    fn needs_reseed_triggers_once_per_window() {
        let mut moments = ShiftedMoments::new();
        for _ in 0..4 {
            moments.push(1.0);
        }
        assert!(!moments.needs_reseed(5));
        moments.push(1.0);
        assert!(moments.needs_reseed(5));
    }

    #[test]
    fn pair_reseed_on_an_empty_window_clears_the_accumulator() {
        let mut moments = ShiftedPairMoments::new();
        moments.push(5.0, 7.0);
        moments.reseed(std::iter::empty());
        assert!(!moments.seeded);
        assert_eq!(moments.pushes_since_reseed, 0);
        assert_eq!(
            [
                moments.sum_a,
                moments.sum_b,
                moments.sum_aa,
                moments.sum_bb,
                moments.sum_ab
            ],
            [0.0; 5]
        );
        // The next value takes the reference points afresh.
        moments.push(1.0, 2.0);
        assert_eq!((moments.offset_a, moments.offset_b), (1.0, 2.0));
    }

    #[test]
    fn trend_reseed_on_an_empty_window_clears_the_accumulator() {
        let mut trend = ShiftedTrend::new();
        trend.push(5.0, 0);
        trend.reseed(std::iter::empty());
        assert!(!trend.seeded);
        assert_eq!(trend.pushes_since_reseed, 0);
        assert_eq!([trend.sum_y, trend.sum_xy, trend.sum_y_sq], [0.0; 3]);
        // The next value takes the reference point afresh.
        trend.push(3.0, 0);
        assert_eq!(trend.offset, 3.0);
    }
}
