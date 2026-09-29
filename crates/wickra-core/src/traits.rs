//! Core traits: the [`Indicator`] state machine and the [`BatchExt`] blanket extension.

use crate::ohlcv::Candle;

/// A streaming technical indicator.
///
/// Every indicator in Wickra implements this trait. The contract is:
///
/// - [`update`](Indicator::update) is called once per input point and must be O(1) in
///   the input length. Pre-existing buffered state may be touched, but no full
///   recomputation over the entire series is permitted.
/// - The returned `Option<Output>` is `None` while the indicator is still in its
///   *warmup* phase (insufficient inputs to produce a defined value), and `Some`
///   once it is ready.
/// - [`reset`](Indicator::reset) clears all state, returning the indicator to the
///   exact configuration it had immediately after construction.
///
/// Implementors that consume scalar prices use `Input = f64` so they automatically
/// gain access to chaining via [`Chain`].
pub trait Indicator {
    /// Type of one input data point (typically `f64` for a price, or `Candle` / `Tick`).
    type Input;
    /// Type of one output value.
    type Output;

    /// Feed one new data point into the indicator and return the freshly
    /// computed output, or `None` if there is no value for this input.
    ///
    /// `None` covers exactly two cases:
    ///
    /// * the indicator is still warming up, and
    /// * the input was rejected as non-finite.
    ///
    /// A rejected input is *skipped*: it does not enter the indicator's state,
    /// so a single bad tick cannot corrupt the values that follow it. The
    /// alternative — repeating the last computed value — was rejected because
    /// it hands the caller a stale number that looks exactly like a fresh one.
    fn update(&mut self, input: Self::Input) -> Option<Self::Output>;

    /// Reset all internal state, leaving the indicator equivalent to a freshly
    /// constructed instance with the same parameters.
    fn reset(&mut self);

    /// Number of inputs required before the first non-`None` output can be produced.
    fn warmup_period(&self) -> usize;

    /// Whether the indicator has emitted at least one value since the last reset.
    fn is_ready(&self) -> bool;

    /// Stable, human-readable indicator name. Used by chaining and diagnostics.
    fn name(&self) -> &'static str;

    /// Run a scalar indicator over `inputs`, writing one output per input into
    /// the caller-owned `out` buffer (`NaN` where [`update`](Indicator::update)
    /// returns `None`).
    ///
    /// This is the exact batch: every value is *bit-for-bit* the one a replay of
    /// `update` produces, and the indicator is left in the state that replay
    /// leaves it in. The default replays `update`; indicators with a faster exact
    /// formulation override it, and every binding and extension method routes
    /// through here, so the override reaches all of them. Writing into a buffer
    /// the caller owns skips the output allocation, which on a large series costs
    /// more than the arithmetic of a simple indicator.
    ///
    /// # Panics
    ///
    /// Panics if `out.len() != inputs.len()`.
    fn batch_nan_into(&mut self, inputs: &[f64], out: &mut [f64])
    where
        Self: Indicator<Input = f64, Output = f64>,
    {
        assert_eq!(
            inputs.len(),
            out.len(),
            "batch output length must equal input length"
        );
        for (slot, &x) in out.iter_mut().zip(inputs) {
            *slot = self.update(x).unwrap_or(f64::NAN);
        }
    }

    /// Opt-in fast batch: like [`batch_nan_into`](Indicator::batch_nan_into), but
    /// an indicator with a vectorised kernel may reassociate its arithmetic to
    /// run it in SIMD lanes. Each value then agrees with the exact batch to within
    /// the tolerance the indicator documents (a few units in the last place), not
    /// bit for bit; warmup positions, `NaN` placement and the output length are
    /// identical. The kernels are deterministic: the same input produces the same
    /// bits on every platform, with or without SIMD hardware.
    ///
    /// The kernel only runs from a fresh (just constructed or reset) indicator
    /// over an all-finite slice; any other call is served by the exact batch.
    /// Afterwards the indicator continues streaming from the kernel's final
    /// state. The default is the exact batch, so every scalar indicator offers
    /// this method and the ones without a kernel simply return exact values.
    ///
    /// # Panics
    ///
    /// Panics if `out.len() != inputs.len()`.
    fn batch_fast_into(&mut self, inputs: &[f64], out: &mut [f64])
    where
        Self: Indicator<Input = f64, Output = f64>,
    {
        self.batch_nan_into(inputs, out);
    }
}

/// Blanket extension that adds batch evaluation to every [`Indicator`].
///
/// The naive `batch` simply replays `update` over a slice, which is always correct
/// because `update` is the only state transition. Concrete indicators may override
/// `batch` if they have a faster vectorized path; the default keeps the contract
/// `batch == repeated update`.
pub trait BatchExt: Indicator {
    /// Run the indicator over a slice of inputs in order, returning one output (or
    /// `None` during warmup) per input.
    fn batch(&mut self, inputs: &[Self::Input]) -> Vec<Option<Self::Output>>
    where
        Self::Input: Clone,
    {
        let mut out = Vec::with_capacity(inputs.len());
        for x in inputs {
            out.push(self.update(x.clone()));
        }
        out
    }

    /// Run an independent copy of the indicator over each input series in parallel.
    ///
    /// Each asset is processed by its own fresh instance built via `make`, so state
    /// never leaks across assets. Requires the `parallel` feature (enabled by
    /// default), which pulls in `rayon`.
    #[cfg(feature = "parallel")]
    fn batch_parallel<F>(
        inputs_per_asset: &[Vec<Self::Input>],
        make: F,
    ) -> Vec<Vec<Option<Self::Output>>>
    where
        Self: Sized + Send,
        Self::Input: Sync + Clone,
        Self::Output: Send,
        F: Fn() -> Self + Sync + Send,
    {
        use rayon::prelude::*;
        inputs_per_asset
            .par_iter()
            .map(|series| {
                let mut ind = make();
                ind.batch(series)
            })
            .collect()
    }
}

impl<T: Indicator> BatchExt for T {}

/// Fast batch for scalar `f64 -> f64` indicators.
///
/// The generic [`BatchExt::batch`] returns `Vec<Option<f64>>` — 16 bytes per
/// element (no niche fits an arbitrary `f64`), which a caller wanting a dense
/// `f64` series then has to walk a second time to map warmup `None`s to `NaN`.
/// This skips both the wide intermediate and the second pass: one allocation,
/// one pass, warmup encoded as `NaN`. Both methods allocate the result and fill
/// it through [`Indicator::batch_nan_into`] / [`Indicator::batch_fast_into`],
/// so an indicator's faster formulation reaches them too.
pub trait BatchNanExt: Indicator<Input = f64, Output = f64> {
    /// One `f64` per input, warmup positions filled with `NaN`, bit-for-bit equal
    /// to replaying `update`.
    fn batch_nan(&mut self, inputs: &[f64]) -> Vec<f64> {
        let mut out = vec![0.0; inputs.len()];
        self.batch_nan_into(inputs, &mut out);
        out
    }

    /// The opt-in fast batch ([`Indicator::batch_fast_into`]) into a fresh
    /// vector: within the indicator's documented tolerance of
    /// [`batch_nan`](BatchNanExt::batch_nan), deterministic across platforms.
    fn batch_fast(&mut self, inputs: &[f64]) -> Vec<f64> {
        let mut out = vec![0.0; inputs.len()];
        self.batch_fast_into(inputs, &mut out);
        out
    }
}

impl<T: Indicator<Input = f64, Output = f64>> BatchNanExt for T {}

/// A streaming *bar builder* — an alternative-chart constructor (Renko, Kagi,
/// Point-and-Figure) that turns a candle stream into a stream of price-driven
/// bars.
///
/// Bar builders are deliberately **not** [`Indicator`]s: a single input candle
/// may complete zero, one, or many bars (a large move can print several Renko
/// bricks at once), which breaks the `update -> Option<Output>` one-in-one-out
/// contract and the `batch == repeated update` length invariant. They get their
/// own trait instead, returning a `Vec` of freshly completed bars per candle.
///
/// The contract is:
///
/// - [`update`](BarBuilder::update) ingests one candle and returns every bar it
///   *completed* on that candle, in chronological order. An empty vector means
///   the move was not large enough to finish a bar yet.
/// - [`reset`](BarBuilder::reset) clears all state, returning the builder to the
///   configuration it had immediately after construction.
/// - [`batch`](BarBuilder::batch) concatenates the bars from replaying `update`
///   over a slice; the flattened length is data-dependent, not the input length.
///
/// Bar builders cannot participate in [`Chain`] (which requires
/// `Indicator<Input = f64, Output = f64>`); feed a downstream indicator from the
/// bars' close prices manually if you need to chain off them.
///
/// ```text
/// let mut renko = RenkoBars::new(1.0).unwrap();
/// let bricks = renko.update(candle); // Vec<RenkoBrick>: 0..n completed bricks
/// ```
pub trait BarBuilder {
    /// Type of one completed bar.
    type Bar;

    /// Feed one candle and return every bar completed on it (possibly none).
    fn update(&mut self, candle: Candle) -> Vec<Self::Bar>;

    /// Reset all internal state to the freshly-constructed configuration.
    fn reset(&mut self);

    /// Stable, human-readable builder name.
    fn name(&self) -> &'static str;

    /// Replay `update` over a slice, concatenating all completed bars. The
    /// result length is data-dependent (not the input length).
    fn batch(&mut self, candles: &[Candle]) -> Vec<Self::Bar> {
        let mut out = Vec::new();
        for candle in candles {
            out.extend(self.update(*candle));
        }
        out
    }
}

/// Chain two indicators so the output of the first becomes the input of the second.
///
/// Both indicators must agree on `f64` as the bridging type, which is the common
/// case for price-in/value-out indicators. The chain itself is an indicator, so
/// chains can be nested arbitrarily.
///
/// # Example
///
/// ```
/// use wickra_core::{Chain, Ema, Indicator, Rsi};
///
/// // RSI(7) on top of EMA(14). EMA seeds at input 14, then RSI needs 7+1 more
/// // valid inputs to emit, so the chain becomes ready at input 21.
/// let mut chain = Chain::new(Ema::new(14).unwrap(), Rsi::new(7).unwrap());
/// for i in 1..=21 {
///     chain.update(f64::from(i));
/// }
/// assert!(chain.is_ready());
/// ```
#[derive(Debug, Clone)]
pub struct Chain<A, B>
where
    A: Indicator<Input = f64, Output = f64>,
    B: Indicator<Input = f64>,
{
    first: A,
    second: B,
}

impl<A, B> Chain<A, B>
where
    A: Indicator<Input = f64, Output = f64>,
    B: Indicator<Input = f64>,
{
    /// Construct a chain whose inputs flow through `first` and then `second`.
    pub const fn new(first: A, second: B) -> Self {
        Self { first, second }
    }

    /// Add a third stage on top.
    pub fn then<C>(self, third: C) -> Chain<Self, C>
    where
        C: Indicator<Input = f64>,
        Self: Indicator<Input = f64, Output = f64>,
    {
        Chain::new(self, third)
    }

    /// Borrow the upstream indicator.
    pub const fn first(&self) -> &A {
        &self.first
    }

    /// Borrow the downstream indicator.
    pub const fn second(&self) -> &B {
        &self.second
    }
}

impl<A, B> Indicator for Chain<A, B>
where
    A: Indicator<Input = f64, Output = f64>,
    B: Indicator<Input = f64>,
{
    type Input = f64;
    type Output = B::Output;

    fn update(&mut self, input: f64) -> Option<Self::Output> {
        self.first.update(input).and_then(|v| self.second.update(v))
    }

    fn reset(&mut self) {
        self.first.reset();
        self.second.reset();
    }

    fn warmup_period(&self) -> usize {
        // Not an upper bound: this method promises the input count before the
        // first value, so over-declaring it is as wrong as under-declaring it.
        // The second stage receives its first input on the bar the first stage
        // emits, so the two warmups overlap by exactly one.
        // A stage declaring 0 still needs its first input to produce anything,
        // so each side counts as at least one bar before the overlap is taken
        // off -- otherwise two pass-through stages underflow.
        self.first.warmup_period().max(1) + self.second.warmup_period().max(1) - 1
    }

    fn is_ready(&self) -> bool {
        self.first.is_ready() && self.second.is_ready()
    }

    fn name(&self) -> &'static str {
        "Chain"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A trivial test indicator: identity (passes input through).
    #[derive(Debug, Default)]
    struct Identity {
        seen: bool,
    }

    impl Indicator for Identity {
        type Input = f64;
        type Output = f64;
        fn update(&mut self, input: f64) -> Option<f64> {
            self.seen = true;
            Some(input)
        }
        fn reset(&mut self) {
            self.seen = false;
        }
        fn warmup_period(&self) -> usize {
            0
        }
        fn is_ready(&self) -> bool {
            self.seen
        }
        fn name(&self) -> &'static str {
            "Identity"
        }
    }

    /// Another trivial test indicator: scales input by 2.
    #[derive(Debug, Default)]
    struct Doubler {
        seen: bool,
    }

    impl Indicator for Doubler {
        type Input = f64;
        type Output = f64;
        fn update(&mut self, input: f64) -> Option<f64> {
            self.seen = true;
            Some(input * 2.0)
        }
        fn reset(&mut self) {
            self.seen = false;
        }
        fn warmup_period(&self) -> usize {
            0
        }
        fn is_ready(&self) -> bool {
            self.seen
        }
        fn name(&self) -> &'static str {
            "Doubler"
        }
    }

    #[test]
    fn batch_replays_update() {
        let mut id = Identity::default();
        let out = id.batch(&[1.0, 2.0, 3.0]);
        assert_eq!(out, vec![Some(1.0), Some(2.0), Some(3.0)]);
    }

    /// The blanket [`BatchNanExt::batch_nan`] default (used by every scalar
    /// indicator without an inherent fast path) maps `update` outputs to a dense
    /// `f64` series, warmup `None` becoming `NaN`. `Identity` is always ready, so
    /// the result is just the inputs back.
    #[test]
    fn batch_nan_default_maps_none_to_nan() {
        let mut id = Identity::default();
        let out = id.batch_nan(&[1.0, 2.0, 3.0]);
        assert_eq!(out, vec![1.0, 2.0, 3.0]);
    }

    /// The default `batch_nan_into` writes one value per input into the
    /// caller's buffer, overwriting whatever was there.
    #[test]
    fn batch_nan_into_default_fills_caller_buffer() {
        let mut id = Identity::default();
        let mut out = [f64::INFINITY; 3];
        id.batch_nan_into(&[4.0, 5.0, 6.0], &mut out);
        assert_eq!(out, [4.0, 5.0, 6.0]);
    }

    /// A length mismatch is a caller bug, not something to truncate silently.
    #[test]
    #[should_panic(expected = "batch output length must equal input length")]
    fn batch_nan_into_rejects_mismatched_lengths() {
        let mut id = Identity::default();
        let mut out = [0.0; 2];
        id.batch_nan_into(&[1.0, 2.0, 3.0], &mut out);
    }

    /// An indicator without a vectorised kernel serves `batch_fast` from the
    /// exact batch, so the two agree bit for bit.
    #[test]
    fn batch_fast_default_is_the_exact_batch() {
        let exact = Doubler::default().batch_nan(&[1.5, -2.0, 3.25]);
        let fast = Doubler::default().batch_fast(&[1.5, -2.0, 3.25]);
        assert_eq!(exact, fast);
        let mut out = [0.0; 3];
        Doubler::default().batch_fast_into(&[1.5, -2.0, 3.25], &mut out);
        assert_eq!(out.to_vec(), exact);
    }

    #[test]
    fn chain_pipes_first_into_second() {
        let mut c = Chain::new(Doubler::default(), Doubler::default());
        // 5 -> 10 -> 20
        assert_eq!(c.update(5.0), Some(20.0));
    }

    #[test]
    fn chain_is_ready_only_after_both_stages_emit() {
        let mut c = Chain::new(Doubler::default(), Doubler::default());
        assert!(!c.is_ready());
        c.update(1.0);
        assert!(c.is_ready());
    }

    #[test]
    fn chain_reset_propagates() {
        let mut c = Chain::new(Doubler::default(), Doubler::default());
        c.update(1.0);
        assert!(c.is_ready());
        c.reset();
        assert!(!c.is_ready());
    }

    #[test]
    fn chain_three_levels_via_then() {
        let c = Chain::new(Doubler::default(), Doubler::default()).then(Doubler::default());
        let mut c = c;
        // 1 -> 2 -> 4 -> 8
        assert_eq!(c.update(1.0), Some(8.0));
    }

    /// Cover the `Chain::first` / `Chain::second` borrow accessors and the
    /// `Chain::warmup_period` + `Chain::name` Indicator-impl bodies.
    ///
    /// Existing chain tests only invoked the Indicator surface (`update`,
    /// `reset`, `is_ready`) on the wrapped `Chain`. The const borrow accessors
    /// and the `warmup_period` / `name` impls were never traversed, so Codecov
    /// flagged traits.rs lines 140-142, 145-147, 167-170, 176-178 as missed.
    /// `chain.warmup_period()` also reaches `Doubler::warmup_period`
    /// (228-230), and `chain.first().name()` reaches `Doubler::name`
    /// (234-236) — both helper methods were uncovered for the same reason.
    #[test]
    fn chain_accessors_and_metadata() {
        let chain = Chain::new(Doubler::default(), Doubler::default());
        // Borrow accessors return the wrapped stages; query each via .name()
        // so Doubler::name (lines 234-236) is also exercised.
        assert_eq!(chain.first().name(), "Doubler");
        assert_eq!(chain.second().name(), "Doubler");
        // Doubler::warmup_period (lines 228-230) is 0, meaning it emits on its
        // first input; chaining two of them still emits on the first input.
        assert_eq!(chain.first().warmup_period(), 0);
        assert_eq!(chain.second().warmup_period(), 0);
        assert_eq!(chain.warmup_period(), 1);
        // Chain::name returns the literal "Chain" (line 177).
        assert_eq!(chain.name(), "Chain");
    }

    /// Cover the full Indicator surface of the `Identity` test helper:
    /// `reset` (198-200), `warmup_period` (201-203), `is_ready` (204-206),
    /// and `name` (207-209). The only other test using `Identity`
    /// (`batch_replays_update`) calls `batch`, which exercises `update`
    /// alone, leaving the remaining four trait methods uncovered.
    #[test]
    fn identity_helper_full_indicator_surface() {
        let mut id = Identity::default();
        // warmup_period is the literal 0; name is the literal "Identity".
        assert_eq!(id.warmup_period(), 0);
        assert_eq!(id.name(), "Identity");
        // is_ready exercises the `self.seen` return with seen=false first…
        assert!(!id.is_ready());
        // …then with seen=true after a single update.
        let out = id.update(42.0);
        assert_eq!(out, Some(42.0));
        assert!(id.is_ready());
        // reset() flips seen back to false; is_ready reflects it.
        id.reset();
        assert!(!id.is_ready());
    }

    #[cfg(feature = "parallel")]
    #[test]
    fn batch_parallel_runs_independent_instances() {
        let series: Vec<Vec<f64>> = vec![vec![1.0, 2.0, 3.0], vec![4.0, 5.0, 6.0]];
        let out = Doubler::batch_parallel(&series, Doubler::default);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0], vec![Some(2.0), Some(4.0), Some(6.0)]);
        assert_eq!(out[1], vec![Some(8.0), Some(10.0), Some(12.0)]);
    }
}
