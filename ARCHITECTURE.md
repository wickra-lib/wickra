# Architecture

A walkthrough of how Wickra is organised internally — written for new
contributors who want to know **where the code lives, why it's split that
way, and which invariants they must not break**. Pair it with [`CONTRIBUTING.md`](CONTRIBUTING.md)
for the day-to-day workflow.

## Workspace layout

Wickra is a Cargo workspace of three Rust crates plus four binding crates.
The split is deliberate: every concern that one user might want to disable
or replace lives behind a separate crate boundary.

```
┌────────────────────────────────────────────────────────────────────┐
│                            wickra (facade)                         │
│              re-exports wickra-core::* + wickra-data::*            │
└──────────────┬──────────────────────────────────┬──────────────────┘
               │                                  │
   ┌───────────▼──────────┐            ┌──────────▼─────────┐
   │    wickra-core       │            │    wickra-data     │
   │   indicator engine   │            │  i/o + aggregation │
   │   • 515 indicators   │            │  • CSV reader      │
   │   • Indicator trait  │            │  • Tick aggregator │
   │   • BatchExt impl    │            │  • Resampler       │
   │   • OHLCV / Candle   │            │  • Live feeds      │
   │   no I/O, no deps    │            │  optional features │
   └──────────────────────┘            └────────────────────┘
                ▲
                │  every binding wraps the same core
   ┌────────────┼────────────┬────────────────┐
   │            │            │                │
┌──▼───────┐ ┌──▼───────┐ ┌──▼───────────┐ ┌──▼──────────────────┐
│  Python  │ │ Node.js  │ │     WASM     │ │  C ABI (cbindgen)   │
│  (PyO3)  │ │ (napi-rs)│ │(wasm-bindgen)│ │   cdylib + header   │
└──────────┘ └──────────┘ └──────────────┘ └─────────┬───────────┘
                                                     │  linked by
                                          ┌──────────▼──────────┐
                                          │  C · C++ · C# · Go  │
                                          │     · Java · R      │
                                          └─────────────────────┘
```

Python, Node.js and WASM are *native* Rust bindings (PyO3 / napi-rs /
wasm-bindgen). The C ABI is the *hub* every other C-capable language links
against: it builds to a `cdylib`/`staticlib` plus a generated `wickra.h`, and
downstream languages link that one artifact rather than each re-wrapping the
core. C and C++ link it directly; the **C#** binding (`bindings/csharp`,
on NuGet), the **Go** binding (`bindings/go`, cgo), the **R** binding
(`bindings/r`, `.Call`) and the **Java** binding (`bindings/java`, the Java FFM
API / Panama, on Maven Central) are all generated from `wickra.h`.

R is the one binding `release.yml` does not publish: r-universe builds it from
`main`, and `bindings/r/configure` resolves the C ABI by downloading the GitHub
release that matches the version in `DESCRIPTION`.

| Crate | Path | What it owns | Public deps |
|---|---|---|---|
| `wickra-core` | `crates/wickra-core` | every indicator, the `Indicator` trait, `BatchExt`, `Candle`/`Tick` types, `Error` | `thiserror`, `rayon` (parallel batch) |
| `wickra` | `crates/wickra` | thin facade — re-exports everything user-facing from `wickra-core` and `wickra-data` | both internal crates |
| `wickra-data` | `crates/wickra-data` | CSV reader, tick aggregator, resampler, live exchange feeds (feature-gated) | `tokio`, `tokio-tungstenite` (live), `serde_json` |
| `wickra-python` | `bindings/python` | `_wickra` PyO3 module + Python package | `pyo3`, `numpy`, depends on `wickra-core` |
| `wickra-node` | `bindings/node` | NAPI-RS native binding | `napi`, depends on `wickra-core` |
| `wickra-wasm` | `bindings/wasm` | WASM binding | `wasm-bindgen`, depends on `wickra-core` |
| `wickra-c` | `bindings/c` | C ABI hub — `cdylib`/`staticlib` + generated `wickra.h` (cbindgen) | depends on `wickra-core` |
| `wickra-examples` | `examples/rust` | runnable binary examples | depends on `wickra`, `wickra-data` |

The `fuzz/` directory is **excluded** from the workspace (it has its own
`Cargo.toml`) because the libfuzzer-sys harness requires a nightly
toolchain, which would otherwise infect the stable workspace lints.

## The `Indicator` trait

Every indicator in Wickra implements one trait, defined in
`crates/wickra-core/src/traits.rs`:

```rust
pub trait Indicator {
    type Input;
    type Output;

    fn update(&mut self, input: Self::Input) -> Option<Self::Output>;
    fn reset(&mut self);
    fn warmup_period(&self) -> usize;
    fn is_ready(&self) -> bool;
    fn name(&self) -> &'static str;
}
```

Four design choices that are non-negotiable:

1. **Streaming-first.** `update` is the only computation entry point. Each
   call must not replay history — no passes over the series behind the tick, no `clone`s of
   the input window unless absolutely necessary.
2. **`Option<Output>` warmup.** A new indicator returns `None` until it has
   ingested `warmup_period()` inputs. After that it returns `Some(value)`
   on every call. The `None` → `Some` transition happens exactly once per
   `reset()`.
3. **Reset is mandatory.** Calling `reset()` returns the indicator to the
   state of a newly constructed one. Tests verify this for every indicator.
4. **No interior mutability across `update` calls.** Indicators may hold
   `VecDeque` / array state, but no `Cell`/`RefCell`/`Mutex` should be
   needed — `&mut self` is the only mutation channel.

### Batch is free

`BatchExt` is a blanket impl over `Indicator`:

```rust
impl<I: Indicator> BatchExt for I {
    fn batch<'a>(&mut self, input: &'a [I::Input]) -> Vec<Option<I::Output>>
    where I::Input: Copy
    {
        input.iter().map(|x| self.update(*x)).collect()
    }
    fn batch_parallel(...)  // rayon-based for multi-asset processing
}
```

Consequence: **every indicator gets batch and parallel-batch for free** as
soon as `Indicator` is implemented. Tests verify `batch == streaming`
equivalence on every indicator — this is the `batch_equals_streaming` test
that appears in every indicator module.

## Indicator-module convention

Each indicator lives in its own file under
`crates/wickra-core/src/indicators/`. Naming: snake-case of the struct,
e.g. `Sma` → `sma.rs`, `MacdIndicator` → `macd.rs`.

Layout inside an indicator file is uniform:

```rust
//! Doc-comment with the formula and one-line summary.

use std::collections::VecDeque;
use crate::error::{Error, Result};
use crate::traits::Indicator;

/// Public struct + rustdoc with mathematical definition + a runnable example.
#[derive(Debug, Clone)]
pub struct Foo { /* state fields */ }

impl Foo {
    /// Constructor with parameter validation.
    pub fn new(period: usize, ...) -> Result<Self> { ... }
    /// Const accessors for configured params.
    pub const fn period(&self) -> usize { ... }
}

impl Indicator for Foo {
    type Input = f64;        // or (f64, f64), or Candle
    type Output = f64;       // or FooOutput { ... }
    fn update(...) -> ... { ... }
    fn reset(...) { ... }
    fn warmup_period(...) -> usize { ... }
    fn is_ready(...) -> bool { ... }
    fn name(...) -> &'static str { "Foo" }
}

#[cfg(test)]
mod tests {
    // mandatory tests (every indicator):
    // - rejects_invalid_params
    // - accessors_and_metadata
    // - reference_value (vs TA-Lib / pandas-ta / hand-calculated)
    // - ignores_non_finite_input
    // - reset_clears_state
    // - batch_equals_streaming
    // plus indicator-specific edge cases
}
```

The `FAMILIES` constant in `mod.rs` (introduced in PR #60) is the
machine-readable index of which family every indicator belongs to. It is
the canonical taxonomy; README and Wiki tables should be derived from it.

## Input types

| Input | Used for | Examples |
|---|---|---|
| `f64` | Scalar inputs — usually a price or a return | SMA, EMA, RSI, ROC |
| `Candle` | OHLCV bar — `{open, high, low, close, volume, timestamp}` | ATR, Bollinger, Ichimoku, all candlestick patterns |
| `(f64, f64)` | Two-series indicators — `(asset, benchmark)` or `(x, y)` | PearsonCorrelation, Beta, Alpha, TreynorRatio |

The `Candle` type lives in `wickra-core::ohlcv` and is the binding
contract across bindings — Python's `Candle` namedtuple, Node's
`Candle` object, and WASM's `Candle` JS class all map 1:1.

## Output types

Most indicators emit `f64`. Multi-output indicators emit a dedicated
struct in the same module, named `FooOutput`:

```rust
pub struct BollingerOutput {
    pub upper: f64,
    pub middle: f64,
    pub lower: f64,
}
```

Bindings flatten these into matrix outputs (NumPy 2-D array for Python,
typed object arrays for Node/WASM).

## Numerical-stability notes

A handful of indicators need care beyond naive accumulation:

- **Rolling variance is running-sum, not Welford.** The sliding-window
  variance family — `StdDev`, `Variance`, `ZScore`, `Bollinger` — keeps
  running `Σx` and `Σx²` over the window and reports `var = Σx²/n − mean²`,
  clamping to zero the tiny negative values floating-point cancellation can
  produce. `Bollinger` periodically reseeds its `Σx²` from the live window
  so error cannot accumulate over a long stream. Welford's online algorithm
  (an incremental `M2` accumulator) does **not** transfer cleanly to a
  sliding window — removing the oldest point from `M2` is numerically
  unstable — so it is used only where the statistic is *not* a fixed
  window: `IntradayVolatilityProfile` and `SeasonalZScore` accumulate
  per-bucket variance that way.
- **Logarithm bases** matter for some indicators (Hurst, MFI). Wickra
  uses natural log everywhere unless the reference math explicitly
  requires `log10` or `log2` — and then it documents the choice in the
  rustdoc.
- **NaN / infinity guards.** Every indicator's `update` rejects
  non-finite input early (returns `None` without state mutation). Tests
  cover this with `ignores_non_finite_input`.

## Cross-crate flow

A typical full-stack call sequence for a Python live-trading example:

```
[ Python: live_binance.py ]
        │
        ▼
[ binance.AsyncClient WebSocket ] ──── wickra_data live feed ───┐
                                                                 │
                                              ┌──────────────────┘
                                              ▼
                                  [ Candle struct conversion ]
                                              │
                                              ▼
                                  [ PyRsi.update(close)       ]
                                              │
              wraps                           │
                                              ▼
                              [ wickra_core::Rsi::update(f64) ]   <-- the only place math runs
                                              │
                                              ▼
                                  [ Option<f64> -> Py<PyFloat> ]
                                              │
                                              ▼
                                       [ Python user code ]
```

The same call sequence happens identically for Node (via NAPI),
WASM (via wasm-bindgen → JS), and Rust (no FFI overhead, just direct
calls).

## What lives where — the navigation cheat sheet

| You want to … | Look in |
|---|---|
| add a new indicator | `crates/wickra-core/src/indicators/<name>.rs` + add to `mod.rs` + add to `FAMILIES` + re-export in `lib.rs` |
| change the `Indicator` trait surface | `crates/wickra-core/src/traits.rs` — this affects every indicator, treat as breaking |
| add a new Candle field | `crates/wickra-core/src/ohlcv.rs` — also propagates to every binding's `Candle` mapping |
| add a new exchange / data source | `crates/wickra-data/src/live/<exchange>.rs`, feature-gated under `live-<exchange>` |
| expose a new binding | new crate under `bindings/` + macro-driven boilerplate in `bindings/<lang>/src/lib.rs` |
| change benchmark coverage | `crates/wickra/benches/indicators.rs` |
| add a new fuzz target | `fuzz/fuzz_targets/<name>.rs` + register in `fuzz/Cargo.toml` |
| change CI matrix | `.github/workflows/ci.yml` |
| change release pipeline | `.github/workflows/release.yml` (irreversible on `v*` tag — test on a throwaway tag first; the publish jobs are independent, so one failing leaves the rest published) |

## What is **deliberately** not in this repo

- **Backtest framework.** Wickra is an indicator library, not a backtester.
  Strategy + PnL + fills logic is for the user (see `examples/` for
  illustrative scripts).
- **Multi-exchange aggregation.** Binance is the demo feed; full
  exchange-agnostic aggregation is `ccxt`'s job. Wickra's
  `wickra-data::live` is intentionally minimal.
- **Order-book / L2 data.** Wickra works on OHLCV bars and ticks, not
  full depth. Tick-data variants (cumulative delta, single print) are on
  the roadmap but require new input types.
- **Charting / visualization.** Out of scope for the Rust core. The
  WASM examples include a `lightweight-charts` integration as a
  starting point, but no charting code lives in the published packages.
- **GPU / SIMD optimisation.** An update touches buffered state only — the
  bottleneck is not vector throughput. SIMD would only help large-batch
  workloads, which already saturate memory bandwidth via the cache-
  friendly `VecDeque` window.

## Performance characteristics

Every `update` is bounded by the configured window, never by the history
behind it. The constant factor
varies:

| Class | Indicators | Per-`update` cost (approx) |
|---|---|---|
| Simple rolling | SMA, EMA, WMA, Mom | 1-2 floating-point ops |
| Recursive smoothers | KAMA, FRAMA, VIDYA, JMA | 5-15 ops |
| Window-sort | OmegaRatio, percentile-based VaR | O(period · log period) per update |
| Multi-buffer DSP | MAMA, HilbertDominantCycle, EmpiricalModeDecomposition | 30-80 ops |
| Multi-component | MacdIndicator, TtmSqueeze, Alligator | sum of components |

Benchmarks against real BTCUSDT 1-minute data live in
`crates/wickra/benches/indicators.rs`. Cross-library comparison vs
TA-Lib / pandas-ta / talipp / finta lives in
`bindings/python/benchmarks/compare_libraries.py`.

## Stability commitments

- **MSRV.** Workspace: Rust 1.86. Node binding: 1.88 (NAPI-RS pins it).
- **`Indicator` trait surface.** Breaking changes here are major-version
  events. Adding a new method with a default impl is minor.
- **Indicator removal.** Once an indicator ships in a release, it stays
  callable. Renames go through a deprecation period of at least one
  minor version.
- **Output structs.** Adding a field to a `FooOutput` is non-breaking
  because the binding contracts go through serde and accept extra keys.

## Open questions / known sharp edges

These are documented for contributors so you don't waste time
re-discovering them.

- **`Rvi`** (Relative Vigor Index) and `RviVolatility` (Relative
  Volatility Index) are different indicators with the same short
  acronym — make sure you import the right one.
- **Fuzz coverage of pair indicators** uses `indicator_update_pair.rs`,
  which is small because pair indicators are simpler — but coverage
  should grow as more pair indicators land.
- **`FAMILIES` (from PR #60) is hand-maintained.** Adding a new
  indicator requires a separate entry in `FAMILIES`. The
  `total_count_matches_expected` test will fail if you forget.
- **WASM is covered by `wasm-bindgen-test`.** `bindings/wasm/src/lib.rs`
  carries 21 in-crate tests (run under `wasm-pack test` in CI), in
  addition to the manual browser examples.

For the high-level project goals see [`ROADMAP.md`](ROADMAP.md); for
day-to-day contribution mechanics see [`CONTRIBUTING.md`](CONTRIBUTING.md).
