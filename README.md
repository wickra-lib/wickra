<p align="center">
  <a href="https://wickra.org"><img src="https://raw.githubusercontent.com/wickra-lib/.github/main/profile/wickra-banner.webp?v=514-7" alt="Wickra — streaming-first technical indicators" width="100%"></a>
</p>

[![CI](https://raw.githubusercontent.com/wickra-lib/.github/main/profile/badges/wickra/ci.svg)](https://github.com/wickra-lib/wickra/actions/workflows/ci.yml)
[![CodeQL](https://raw.githubusercontent.com/wickra-lib/.github/main/profile/badges/wickra/codeql.svg)](https://github.com/wickra-lib/wickra/actions/workflows/codeql.yml)
[![codecov](https://raw.githubusercontent.com/wickra-lib/.github/main/profile/badges/wickra/codecov.svg)](https://codecov.io/gh/wickra-lib/wickra)
[![GitHub release](https://raw.githubusercontent.com/wickra-lib/.github/main/profile/badges/wickra/release.svg)](https://github.com/wickra-lib/wickra/releases/latest)
[![crates.io](https://raw.githubusercontent.com/wickra-lib/.github/main/profile/badges/wickra/crates.svg)](https://crates.io/crates/wickra)
[![PyPI](https://raw.githubusercontent.com/wickra-lib/.github/main/profile/badges/wickra/pypi.svg)](https://pypi.org/project/wickra/)
[![npm](https://raw.githubusercontent.com/wickra-lib/.github/main/profile/badges/wickra/npm.svg)](https://www.npmjs.com/package/wickra)
[![NuGet](https://raw.githubusercontent.com/wickra-lib/.github/main/profile/badges/wickra/nuget.svg)](https://www.nuget.org/packages/Wickra)
[![Maven Central](https://raw.githubusercontent.com/wickra-lib/.github/main/profile/badges/wickra/maven.svg)](https://central.sonatype.com/artifact/org.wickra/wickra)
[![Go module](https://raw.githubusercontent.com/wickra-lib/.github/main/profile/badges/wickra/go.svg)](https://pkg.go.dev/github.com/wickra-lib/wickra-go)
[![R-universe](https://raw.githubusercontent.com/wickra-lib/.github/main/profile/badges/wickra/r-universe.svg)](https://wickra-lib.r-universe.dev)
[![License: MIT OR Apache-2.0](https://raw.githubusercontent.com/wickra-lib/.github/main/profile/badges/wickra/license.svg)](#license)
[![OpenSSF Scorecard](https://raw.githubusercontent.com/wickra-lib/.github/main/profile/badges/wickra/scorecard.svg)](https://scorecard.dev/viewer/?uri=github.com/wickra-lib/wickra)
[![OpenSSF Best Practices](https://raw.githubusercontent.com/wickra-lib/.github/main/profile/badges/wickra/best-practices.svg)](https://www.bestpractices.dev/projects/13094)
[![Build provenance](https://raw.githubusercontent.com/wickra-lib/.github/main/profile/badges/wickra/provenance.svg)](https://github.com/wickra-lib/wickra/attestations)
[![Docs](https://raw.githubusercontent.com/wickra-lib/.github/main/profile/badges/wickra/docs.svg)](https://docs.wickra.org)
[![Verified across 10 languages](https://raw.githubusercontent.com/wickra-lib/.github/main/profile/badges/wickra/verified.svg)](https://github.com/wickra-lib/wickra)
[![Live demo](https://raw.githubusercontent.com/wickra-lib/.github/main/profile/badges/wickra/live-demo.svg)](https://live.wickra.org)

---

**Streaming-first technical indicators. Install with `pip install wickra` — no system dependencies, zero third-party packages.**

> **▶ Live demos:** all 514 indicators of the core over a real Binance feed — **[live.wickra.org](https://live.wickra.org)**;
> the backtester compiled to WebAssembly, an equity curve building bar by bar — **[backtest-live.wickra.org](https://backtest-live.wickra.org)**;
> one StrategySpec side by side in Python, Rust, JS and Go — **[playground.wickra.org](https://playground.wickra.org)**. Zero backend, all of them.

**Part of the [Wickra ecosystem](#ecosystem):** the same data-driven core and ten-language binding surface also power [wickra-exchange](https://github.com/wickra-lib/wickra-exchange), [wickra-backtest](https://github.com/wickra-lib/wickra-backtest), [wickra-terminal](https://github.com/wickra-lib/wickra-terminal) and 21 more — see [the full list](https://github.com/wickra-lib).

Wickra is a multi-language technical-analysis library with a Rust core and
native bindings for Python, Node.js and WASM, plus a C ABI that C, C++,
C#, Go, Java, R and any other C-capable language links against. Every indicator is a
state machine: a tick touches buffered state and never the history behind it, so
live trading bots and historical backtests share the exact same implementation.

```python
import wickra as ta                     # zero third-party deps — not even NumPy

# Batch: classic TA-Lib-style usage
prices = [100.0 + i * 0.1 for i in range(1000)]
rsi = ta.RSI(14)
values = rsi.batch(prices)              # array.array('d'), NaN during warmup
                                        # np.asarray(values) wraps it zero-copy if you use NumPy

# Streaming: same indicator, fed tick by tick
rsi = ta.RSI(14)
for price in live_feed:
    value = rsi.update(price)           # no recomputation over history
    if value is not None and value > 70:
        print("overbought")
```

## Documentation

Full documentation lives at **[docs.wickra.org](https://docs.wickra.org)**:

- **Quickstarts** — [Rust](https://docs.wickra.org/Quickstart-Rust),
  [Python](https://docs.wickra.org/Quickstart-Python),
  [Node](https://docs.wickra.org/Quickstart-Node),
  [WASM](https://docs.wickra.org/Quickstart-WASM),
  [C](https://docs.wickra.org/Quickstart-C),
  [C++](https://docs.wickra.org/Quickstart-C),
  [C#](https://docs.wickra.org/Quickstart-CSharp),
  [Go](https://docs.wickra.org/Quickstart-Go),
  [Java](https://docs.wickra.org/Quickstart-Java),
  [R](https://docs.wickra.org/Quickstart-R).
- **Indicators** — a per-indicator deep dive (formula, parameters, warmup) for
  every one of the 514 indicators; start at the
  [indicators overview](https://docs.wickra.org/Indicators-Overview).
- **Reference** — [warmup periods](https://docs.wickra.org/Warmup-Periods),
  [streaming vs batch](https://docs.wickra.org/Streaming-vs-Batch),
  [indicator chaining](https://docs.wickra.org/Indicator-Chaining), the
  [data layer](https://docs.wickra.org/Data-Layer).
- **Guides** — [Cookbook](https://docs.wickra.org/Cookbook),
  [TA-Lib migration](https://docs.wickra.org/TA-Lib-Migration),
  [FAQ](https://docs.wickra.org/FAQ).

## Why Wickra

Most TA libraries are fast, *or* multi-language, *or* broad. Wickra refuses to
pick. It's the streaming-first engine built for the workload the others treat as
an afterthought — **live, tick-by-tick data** — without giving up the breadth of
a full batch library, and without making you reimplement your indicators four
times to get there.

- **The biggest streaming-native catalogue, period.** 514 indicators across 24
  families — candlesticks, harmonic & chart patterns, market profile, market
  breadth, Renko/Kagi/Point&Figure bars, Ehlers DSP cycles, risk/performance
  metrics — **every single one updating incrementally**, with no pass over the
  history behind the tick. TA-Lib ships ~150 and none of them stream.
- **One Rust core, five first-class targets.** Native **Rust · Python · Node.js ·
  WASM** plus a **C ABI** for C, C++, C#, Go, Java, R and any other C-capable language —
  identical math, identical results, zero per-language reimplementation and zero
  GIL bottleneck.
- **Correct by construction, not by hope.** Every `update` validates its input,
  runs a real warmup, and returns an `Option` so a single bad tick can't silently
  poison state. `batch == streaming` is **bit-exact, fuzzed and 100 %-line-covered
  for all 514 indicators**.
- **Identical across every language — proven, not promised.** All 514 indicators
  are replayed through **all 10 languages** (Rust · Python · Node.js · WASM · C ·
  C++ · C# · Go · Java · R) and checked **against the Rust reference** via shared
  golden fixtures in CI. The math is verifiably the same everywhere — this very
  check caught and fixed two real cross-language marshalling bugs. (Every binding
  calls into the same Rust core, so 461 of the 514 are bit-identical by
  construction; the other 53 call a libm transcendental, whose last bit belongs
  to the platform's math library rather than to Wickra, so the runners compare to
  a relative tolerance. See [Verification](#verification).)
- **Orders of magnitude faster where it counts.** In streaming Wickra is **11–56×**
  faster than the only other incremental peer and **thousands of times** faster
  than recompute-on-every-tick libraries. On batch it wins several rows outright
  and trades the simple recurrences (SMA, EMA, MACD) for its guarantees — and
  the losses are shown, not hidden.
- **Install in one line, anywhere.** `pip install wickra` / `npm install wickra` —
  precompiled wheels and binaries, **no C toolchain, none of TA-Lib's setup pain**.
  macOS · Linux · Windows.
- **Batteries included — zero third-party deps, in every language.** A full native
  data layer ships in the box: a CSV candle reader, a tick-to-candle aggregator, a
  timeframe resampler, a live Binance WebSocket feed and a historical Binance REST
  fetcher — in **all 10 languages**. Loading a CSV, rolling ticks into candles,
  resampling and streaming live data needs **no foreign package** — no pandas, no
  `csv-parse`, no `ws`/`websockets`, no `jackson`, no `jsonlite`, not even NumPy.
  `pip install wickra` / `npm install wickra` / `go get` / … pulls **nothing else**.
- **Truly permissive.** **MIT OR Apache-2.0** — drop it straight into commercial
  and closed-source work.

Every other library forces one of those compromises. Wickra doesn't:

| Library          | Install     | Streaming   | Languages                   | Indicators | Active |
|------------------|-------------|-------------|-----------------------------|-----------:|--------|
| **★&nbsp;Wickra**| **clean**   | **yes, all 514** | **Rust · Python · Node.js · WASM · C · C++ · C# · Go · Java · R** | **514** | **yes** |
| kand             | clean       | yes         | Python · WASM · Rust        |       ~60  | yes    |
| ta-rs            | clean       | yes         | Rust only                   |       ~30  | stale  |
| yata             | clean       | partial     | Rust only                   |       ~35  | yes    |
| TA-Lib           | yes (C deps)| no          | many bindings               |      ~150  | barely |
| pandas-ta        | clean       | no          | Python                      |      ~130  | slow   |
| finta            | clean       | no          | Python                      |       ~80  | stale  |
| talipp           | clean       | yes         | Python                      |       ~40  | yes    |

Broad, multi-language, streaming-native **and** honest about its trade-offs — at
the same time. That's the combination no one else ships.

## Why Wickra exists

Wickra started as a personal itch. The existing TA libraries never quite fit the
projects I was building, so I decided to build one from the ground up — partly to
learn, partly because I genuinely enjoy taking something that already exists and
trying to do it differently (and, ideally, better). It's open source because the
useful version of that itch is the one other people can build on too.

## Benchmarks

Wickra updates every indicator incrementally: the cost of a tick is bounded by
the window you configure, never by how much history has gone before it. Most
indicators do constant work; the ones that need an order statistic or a full-window
pass — a rolling quantile sorts, CCI averages absolute deviations — scale with the
period, not the series. In **streaming** — the workload it is built for — it is
**11–56× faster** than the only other incremental peer and **thousands of times**
faster than recompute-on-every-tick libraries.
**Batch** is competitive: it wins several rows outright and trades a few µs
elsewhere for `None`-warmup, NaN-safety and bit-exact `batch == streaming`.

Full tables (Rust + Python, streaming + batch) and how to reproduce them live in
**[BENCHMARKS.md](BENCHMARKS.md)**.

### Pick your language with eyes open — per-binding throughput

Every binding calls the **same** Rust core, so this is **not** a speed claim — it
is the raw cost of crossing each language's FFI boundary (`SMA(20)`, 200 000 bars,
Ryzen 9 9950X, million updates/sec). **Batch stays high for most bindings;
streaming is where the boundary shows** — so if you stream tick-by-tick, the table
tells you which binding keeps up and which to avoid for hot loops.

| Language        | streaming (Mupd/s) | batch (Mupd/s) |
|-----------------|-------------------:|---------------:|
| Rust (no FFI)   |                380 |            498 |
| C / C++         |                365 |            358 |
| C#              |                348 |            259 |
| Python          |                 31 |             46 |
| Java            |                 38 |            173 |
| Go              |                 23 |            394 |
| WASM            |                 21 |            169 |
| Node.js         |                 16 |              9 |
| R               |                0.1 |            279 |

All ten share one verified implementation (see the verification badge above), so
the *numbers* differ but the *values* do not. Methodology and
the per-indicator breakdown are in [BENCHMARKS.md](BENCHMARKS.md#3-per-binding-throughput--the-cost-of-the-boundary).

## Indicators

514 streaming-first indicators across twenty-four families. Every one passes the
`batch == streaming` equivalence test, reference-value tests, and reset
semantics tests — and is replayed through **all 10 languages** and checked
against the Rust reference (golden fixtures, in CI). Each has a
per-indicator deep dive (formula, parameters, warmup) at
[docs.wickra.org](https://docs.wickra.org/Indicators-Overview).

| Family | Indicators |
|--------|-----------|
| Moving Averages      | SMA, EMA, WMA, DEMA, TEMA, HMA, KAMA, SMMA, TRIMA, ZLEMA, T3, VWMA, ALMA, McGinley Dynamic, FRAMA, VIDYA, JMA, Alligator, EVWMA, SWMA, GMA, EHMA, Median MA, Adaptive Laguerre, GD, Holt-Winters |
| Momentum Oscillators | RSI (Wilder), Anchored RSI, Stochastic, CCI, ROC, Williams %R, MFI, Awesome Oscillator, MOM, CMO, TSI, PMO, StochRSI, Ultimate Oscillator, RVI, PGO, KST, SMI, Laguerre RSI, Connors RSI, Inertia, ROC Percentage (ROCP), ROC Ratio (ROCR), ROC Ratio 100 (ROCR100), Disparity Index, Fisher RSI, RSX, Dynamic Momentum Index, Stochastic CCI, RMI, Derivative Oscillator, Elder Ray, Intraday Momentum Index, QQE |
| Trend & Directional  | MACD, MACD Fixed (MACDFIX), MACD Extended (MACDEXT), ADX (+DI/-DI), ADXR, Aroon, TRIX, Aroon Oscillator, Vortex, Random Walk Index, Trend Intensity Index, Wave Trend Oscillator, Mass Index, Choppiness Index, Vertical Horizontal Filter, Plus DM, Minus DM, Plus DI, Minus DI, DX, TTM Trend, Trend Strength Index, Qstick, Polarized Fractal Efficiency, Wave PM, Gator Oscillator, Kase Permission Stochastic |
| Price Oscillators    | PPO, DPO, Coppock, Accelerator Oscillator, Balance of Power, APO, AO Histogram, CFO, Zero-Lag MACD, Elder Impulse, STC, TSF Oscillator, MACD Histogram, PPO Histogram |
| Volatility & Bands   | ATR, Bollinger Bands, Keltner Channels, Donchian Channels, NATR, StdDev, Ulcer Index, Historical Volatility, Bollinger Bandwidth, %B, True Range, Chaikin Volatility, RVI (Relative Volatility Index), Parkinson Volatility, Garman-Klass Volatility, Rogers-Satchell Volatility, Yang-Zhang Volatility, Volatility Cone |
| Bands & Channels     | MA Envelope, Acceleration Bands, STARC Bands, ATR Bands, Hurst Channel, LinReg Channel, Standard Error Bands, Double Bollinger Bands, TTM Squeeze, Fractal Chaos Bands, VWAP StdDev Bands, Quartile Bands, Bomar Bands, Median Channel, Projection Bands, Projection Oscillator |
| Trailing Stops       | Parabolic SAR, Parabolic SAR Extended (SAREXT), SuperTrend, Chandelier Exit, Chande Kroll Stop, ATR Trailing Stop, HiLo Activator, Volty Stop, Yo-Yo Exit, Donchian Channel Stop, Percentage Trailing Stop, Step Trailing Stop, Renko Trailing Stop, Kase DevStop, Elder SafeZone, ATR Ratchet, NRTR, Time-Based Stop, Modified MA Stop |
| Volume               | OBV, VWAP (cumulative + rolling), ADL, Volume-Price Trend, Chaikin Money Flow, Chaikin Oscillator, Force Index, Ease of Movement, Klinger Volume Oscillator, Volume Oscillator, NVI, PVI, Williams A/D Oscillator, Anchored VWAP, Demand Index, TSV, VZO, Market Facilitation Index, Volume RSI, Williams Accumulation/Distribution, Twiggs Money Flow, Trade Volume Index, Intraday Intensity, Better Volume, Volume-Weighted MACD |
| Price Statistics     | Typical Price, Median Price, Weighted Close, Linear Regression, Linear Regression Slope, Z-Score, Linear Regression Angle, Variance, Coefficient of Variation, Skewness, Kurtosis, Standard Error, Detrended StdDev, R², Median Absolute Deviation, Autocorrelation, Hurst Exponent, Pearson Correlation, Beta, Pairwise Beta, Pair Spread Z-Score, Lead-Lag Cross-Correlation, Cointegration, Relative Strength A-vs-B, Spearman Correlation, Mid Price, Mid Point, Average Price, Linear Regression Intercept, Time Series Forecast, Rolling Correlation, Rolling Covariance, OU Half-Life, Spread Hurst, Distance SSD, Beta-Neutral Spread, Variance Ratio, Granger Causality, Kalman Hedge Ratio, Spread Bollinger Bands, Spread AR(1) Coefficient, Jarque-Bera, Rolling Min-Max Scaler, Shannon Entropy, Sample Entropy, Kendall Tau |
| Ehlers / Cycle (DSP) | MAMA, FAMA, Fisher Transform, Inverse Fisher Transform, SuperSmoother, Hilbert Dominant Cycle, Hilbert Phasor, Hilbert DC Phase, Hilbert Trend Mode, Sine Wave, Decycler, Decycler Oscillator, Roofing Filter, Center of Gravity, Cybernetic Cycle, Adaptive Cycle, Empirical Mode Decomposition, Ehlers Stochastic, Instantaneous Trendline, Highpass Filter, Reflex, Trendflex, Correlation Trend Indicator, Adaptive RSI, Universal Oscillator, Adaptive CCI, Bandpass Filter, Even Better Sinewave, Autocorrelation Periodogram |
| Pivots & S/R         | Classic Pivots, Fibonacci Pivots, Camarilla, Woodie Pivots, DeMark Pivots, Williams Fractals, ZigZag, Central Pivot Range, Murrey Math Lines, Andrews Pitchfork, Volume-Weighted Support/Resistance, Pivot Reversal |
| DeMark               | TD Setup, TD Sequential, TD DeMarker, TD REI, TD Pressure, TD Combo, TD Countdown, TD Lines, TD Range Projection, TD Differential, TD Open, TD Risk Level, TD Camouflage, TD Clop, TD Clopwin, TD Propulsion, TD Trap, TD D-Wave, TD Moving Averages |
| Ichimoku & Charts    | Ichimoku Kinko Hyo (Tenkan, Kijun, Senkou A/B, Chikou), Heikin-Ashi, Heikin-Ashi Oscillator, Three Line Break, Smoothed Heikin-Ashi, Equivolume, CandleVolume |
| Alt-Chart Bars       | Renko (box-size bricks), Kagi (reversal-amount lines), Point & Figure (X/O columns), Range, Tick, Volume, Dollar, Imbalance, Run, Three-Line Break |
| Candlestick Patterns | Doji, Hammer, Inverted Hammer, Hanging Man, Shooting Star, Engulfing, Harami, Morning/Evening Star, Three White Soldiers/Black Crows, Piercing Line/Dark Cloud Cover, Marubozu, Tweezer, Spinning Top, Three Inside Up/Down, Three Outside Up/Down, Two Crows, Upside Gap Two Crows, Identical Three Crows, Three Line Strike, Three Stars in the South, Abandoned Baby, Advance Block, Belt-hold, Breakaway, Counterattack, Doji Star, Dragonfly Doji, Gravestone Doji, Long-Legged Doji, Rickshaw Man, Evening Doji Star, Morning Doji Star, Gap Side-by-Side White, High-Wave, Hikkake, Modified Hikkake, Homing Pigeon, On-Neck, In-Neck, Thrusting, Separating Lines, Kicking, Kicking by Length, Ladder Bottom, Mat Hold, Matching Low, Long Line, Short Line, Rising Three Methods, Falling Three Methods, Upside Gap Three Methods, Downside Gap Three Methods, Stalled Pattern, Stick Sandwich, Takuri, Closing Marubozu, Opening Marubozu, Tasuki Gap, Unique Three River, Concealing Baby Swallow, Tristar, Harami Cross, Tower Top/Bottom, Dumpling Top, New Price Lines, Frying Pan Bottom |
| Chart Patterns       | Double Top / Bottom, Triple Top / Bottom, Head and Shoulders, Triangle (asc/desc/sym), Wedge (rising/falling), Flag / Pennant, Rectangle / Range, Cup and Handle |
| Harmonic Patterns    | AB=CD, Gartley, Butterfly, Bat, Crab, Shark, Cypher, Three Drives |
| Fibonacci            | Fibonacci Retracement, Fibonacci Extension, Fibonacci Projection, Auto-Fibonacci, Golden Pocket, Fibonacci Confluence, Fibonacci Fan, Fibonacci Arcs, Fibonacci Channel, Fibonacci Time Zones |
| Microstructure       | Order-Book Imbalance (Top-1 / Top-N / Full), Microprice, Quoted Spread, Depth Slope, Signed Volume, Cumulative Volume Delta, Trade Imbalance, Effective Spread, Realized Spread, Kyle's Lambda, Footprint, Order Flow Imbalance, VPIN, Amihud Illiquidity, Roll Measure, Trade-Sign Autocorrelation, Hasbrouck Information Share |
| Derivatives          | Funding Rate, Funding Rate Mean, Funding Rate Z-Score, Funding Basis, Open-Interest Delta, OI / Price Divergence, OI-Weighted Price, Long/Short Ratio, Taker Buy/Sell Ratio, Liquidation Features, Term-Structure Basis, Calendar Spread, Estimated Leverage Ratio, OI-to-Volume Ratio, Perpetual Premium Index, Funding-Implied APR, Open-Interest Momentum |
| Market Profile       | Value Area (POC / VAH / VAL), Volume Profile (histogram), TPO Profile, Initial Balance, Opening Range, Naked POC, Single Prints, Profile Shape, High/Low Volume Nodes, Composite Profile |
| Market Breadth       | Advance/Decline Line, Advance/Decline Ratio, Advance/Decline Volume Line, McClellan Oscillator, McClellan Summation Index, TRIN / Arms Index, Breadth Thrust, New Highs - New Lows, High-Low Index, Percent Above Moving Average, Up/Down Volume Ratio, Bullish Percent Index, Cumulative Volume Index, Absolute Breadth Index, TICK Index |
| Risk / Performance   | Sharpe Ratio, Sortino Ratio, Calmar Ratio, Omega Ratio, Max Drawdown, Average Drawdown, Drawdown Duration, Pain Index, Value at Risk, Conditional Value at Risk (CVaR), Profit Factor, Gain/Loss Ratio, Recovery Factor, Kelly Criterion, Treynor Ratio, Information Ratio, Alpha (Jensen) |
| Seasonality & Session | Session VWAP, Session High/Low, Session Range, Average Daily Range, Overnight Gap, Overnight/Intraday Return, Turn-of-Month, Seasonal Z-Score, Time-of-Day Return Profile, Day-of-Week Profile, Intraday Volatility Profile, Volume-by-Time Profile |

Every candlestick pattern emits a signed per-bar value — `+1.0` bullish,
`−1.0` bearish, `0.0` none — so the family drops straight into a feature matrix
as one column each. `Doji` is direction-less by default (`+1.0` / `0.0`);
construct it in signed mode (`Doji::new().signed()`, `Doji(signed=True)`,
`new Doji(true)`) for a dragonfly / gravestone `±1` reading.

Adding a new indicator means implementing one trait in Rust; every binding
inherits it automatically (the C ABI — and the C#, Go, Java and R bindings generated from
it — regenerate from the core).

## Languages

| Binding           | Install                                       | Example |
|-------------------|-----------------------------------------------|---------|
| Python (PyO3)     | `pip install wickra`                          | `examples/python/backtest.py` |
| Node.js (napi-rs) | `npm install wickra`                          | `examples/node/backtest.js` |
| Browser / WASM    | `npm install wickra-wasm`                     | `examples/wasm/index.html` |
| Rust              | `cargo add wickra`                            | `examples/rust/src/bin/backtest.rs` |
| C / C++ (C ABI)   | header + library, see [`bindings/c`](bindings/c) | `examples/c/streaming.c` |
| C# (C ABI) | `dotnet add package Wickra`, see [`bindings/csharp`](bindings/csharp) | `examples/csharp/streaming` |
| Go (cgo, C ABI)   | `go get github.com/wickra-lib/wickra/bindings/go`, see [`bindings/go`](bindings/go) | `examples/go/streaming` |
| Java (FFM, C ABI)  | Maven Central `org.wickra:wickra`, see [`bindings/java`](bindings/java) | `examples/java` (`Streaming`) |
| R (`.Call`, C ABI) | `R CMD INSTALL bindings/r`, see [`bindings/r`](bindings/r) | `examples/r/streaming.R` |

Each binding ships several runnable examples (streaming, backtest, live feed);
[`examples/README.md`](examples/README.md) is the full cross-language index.

The wickra-core crate is `unsafe`-forbidden, so the native bindings are
memory-safe end to end. The C ABI runs the same safe core; only its thin FFI
boundary uses `unsafe`, and the caller owns handle lifetimes (`_new` / `_free`).

## Requirements

The minimum supported version per language. Prebuilt packages (Rust, Python,
Node.js, WASM, C#) need only the runtime; the C-ABI bindings that compile on
install — Go (cgo) and R (`.Call`) — also need a C compiler, and Java runs with
`--enable-native-access=ALL-UNNAMED`.

| Language    | Package                              | Minimum supported          |
|-------------|--------------------------------------|----------------------------|
| Rust        | crates.io · `wickra`                 | 1.86 (MSRV)                |
| Python      | PyPI · `wickra` (abi3 wheel)         | 3.9 (tested through 3.13)  |
| Node.js     | npm · `wickra` (N-API 8)             | 20 (tested on 22 · 24 LTS) |
| WASM        | npm · `wickra-wasm`                  | any modern JS engine       |
| C           | `wickra.h` + library (releases)      | C99 compiler               |
| C++         | `wickra.hpp` over the C ABI          | C++14 compiler             |
| C#          | NuGet · `Wickra`                     | .NET 8 (`net8.0`)          |
| Go          | module · `wickra-lib/wickra-go`      | Go 1.23 (cgo)              |
| Java        | Maven Central · `org.wickra:wickra`  | Java 22 (FFM / Panama)     |
| R           | source package                       | R ≥ 4.1 (Rtools on Win.)   |

Full per-language detail (runtime vs. build-from-source) is on the
[Requirements page](https://docs.wickra.org/Requirements) in the docs.

## Rust API

```rust
use wickra::{Indicator, BatchExt, Chain, Ema, Rsi, Sma};

// Streaming or batch — same trait, same code.
let mut sma = Sma::new(14)?;
let out: Vec<Option<f64>> = sma.batch(&[1.0, 2.0, 3.0, 4.0, 5.0]);

let mut rsi = Rsi::new(14)?;
for price in live_feed {
    if let Some(v) = rsi.update(price) {
        println!("RSI = {v}");
    }
}

// Compose indicators: RSI(7) on top of EMA(14).
let mut chain = Chain::new(Ema::new(14)?, Rsi::new(7)?);
chain.update(price);
```

## Live data sources

Wickra ships a complete, **native data layer** — exposed in **all 10 languages**,
pulling **zero third-party packages** (no pandas / `csv-parse` / `ws` / `jackson`
/ `jsonlite`). In Rust it lives in the `wickra-data` crate; every binding exposes
the same building blocks:

- A streaming OHLCV **CSV reader** (`CandleReader`).
- A **tick-to-candle aggregator** with arbitrary timeframes (`TickAggregator`).
- A **candle resampler** for multi-timeframe analysis (1m → 5m → 1h on the fly, `Resampler`).
- A live **Binance Spot WebSocket** kline feed (`BinanceFeed`, feature `live-binance`).
- A historical **Binance REST** kline fetcher (`fetch_binance_klines`) — native
  HTTP + JSON, no third-party client.

```rust
use wickra::{Indicator, Rsi};
use wickra_data::live::binance::{BinanceKlineStream, Interval};

let mut stream = BinanceKlineStream::connect(&["BTCUSDT".into()], Interval::OneMinute).await?;
let mut rsi = Rsi::new(14)?;
while let Some(event) = stream.next_event().await? {
    if event.is_closed {
        if let Some(v) = rsi.update(event.candle.close) {
            println!("RSI = {v:.2}");
        }
    }
}
```

Native live-feed and historical-fetch examples — using `wickra.BinanceFeed` and
`wickra.fetch_binance_klines`, **with no third-party HTTP/WebSocket client** — live
under `examples/python/` (and the matching directory for every other language).

## Project layout

```
wickra/
├── crates/
│   ├── wickra-core/         core engine + all 514 indicators
│   ├── wickra/              top-level facade crate (publishes on crates.io) + benches/
│   ├── wickra-data/         CSV reader, tick aggregator, live exchange feeds
│   └── wickra-bench/        internal cross-library benchmark harness (not published)
├── bindings/
│   ├── python/              PyO3 + maturin (publishes on PyPI)
│   ├── node/                napi-rs (publishes on npm)
│   ├── wasm/                wasm-bindgen (browsers, bundlers, Node)
│   ├── c/                   C ABI (cdylib + staticlib) + generated include/wickra.h
│   ├── csharp/              C# binding over the C ABI (publishes on NuGet)
│   ├── go/                  Go binding over the C ABI via cgo (module tag)
│   ├── r/                   R binding over the C ABI via .Call (R package)
│   └── java/                Java binding over the C ABI via the FFM API (Maven Central)
├── examples/                examples/README.md indexes every language
│   ├── data/                real BTCUSDT OHLCV datasets, one per timeframe
│   ├── rust/                Rust workspace member (`wickra-examples`)
│   ├── python/              backtest, live Binance feed, parallel assets, multi-tf
│   ├── node/                streaming, backtest, live Binance feed (load `wickra`)
│   ├── wasm/                browser demo for `wickra-wasm`
│   ├── c/                   C smoke + streaming, C++ RAII wrapper
│   ├── csharp/              streaming, backtest, strategies (load `Wickra`)
│   ├── go/                  streaming, backtest, strategies (cgo binding)
│   ├── r/                   streaming, backtest, strategies (.Call binding)
│   └── java/                streaming, backtest, strategies (FFM binding)
└── .github/workflows/       CI and release pipelines
```

Wickra's own regression benchmarks live in `crates/wickra/benches/`; the
cross-library comparison against kand, ta-rs and yata lives in the internal
`crates/wickra-bench/` crate. Runnable Rust examples live in the workspace member
crate at `examples/rust/`. There is no top-level `benches/` directory.

## Building everything from source

```bash
# Rust core + tests
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo bench -p wickra           # Wickra's own regression benchmarks
cargo bench -p wickra-bench     # cross-library comparison (kand, ta-rs, yata)

# Python binding (requires Rust toolchain + maturin)
cd bindings/python
maturin develop --release
pytest

# WASM binding (requires wasm-pack + wasm32-unknown-unknown target)
wasm-pack build bindings/wasm --target web --release --features panic-hook

# Node binding (requires @napi-rs/cli)
cd bindings/node && npm install && npm run build && npm test

# C ABI (cdylib + staticlib + generated header)
cargo build -p wickra-c --release
cmake -S examples/c -B examples/c/build -DWICKRA_LIB_DIR="$PWD/target/release"
cmake --build examples/c/build && ctest --test-dir examples/c/build --output-on-failure

# C# binding (requires the .NET 8 SDK; links the C ABI above)
dotnet test bindings/csharp/Wickra.Tests/Wickra.Tests.csproj

# Go binding (requires a C compiler for cgo; links the C ABI above)
cp target/release/libwickra.so bindings/go/lib/   # .dylib on macOS, wickra.dll on Windows
cd bindings/go && go test ./...

# R binding (requires a C toolchain / Rtools; links the C ABI above)
WICKRA_INCLUDE_DIR="$PWD/bindings/c/include" WICKRA_LIB_DIR="$PWD/target/release" \
  R CMD INSTALL bindings/r

# Java binding (requires JDK 22+ and Maven; links the C ABI above)
mvn -f bindings/java test
```

## Testing

Every layer is covered; run the suites with the commands in
[Building everything from source](#building-everything-from-source).

- `wickra-core`: unit tests per indicator — textbook reference values
  (Wilder RSI, Bollinger Bands, MACD, ATR, Stochastic), `batch == streaming`
  equivalence, `reset` semantics, NaN/Inf handling, and property tests. A
  catalogue-wide property harness (`tests/invariants.rs`) additionally asserts
  `batch == streaming`, `reset == fresh`, and non-finite-input rejection for
  **every** indicator and bar-builder.
- `wickra-data`: unit tests for CSV decoding, the tick aggregator, the
  resampler, and the Binance payload parser.
- `bindings/python`: pytest covering smoke checks, streaming/batch
  equivalence, reference values, lifecycle, input validation, and
  dict/tuple candle inputs.
- `bindings/node`: `node --test` cases for batch, streaming, and reference
  values across all indicators.
- `bindings/wasm`: `wasm-bindgen-test` cases for constructors, equivalence,
  and reference values.
- `bindings/c`: Rust unit tests over the FFI boundary, plus C and C++ smoke
  tests and offline example `ctest`s run on the three OSes.
- `bindings/csharp`: `dotnet test` cases covering one indicator per FFI archetype
  (scalar/batch, multi-output, bars, profile, array input) plus SMA reference values.
- `bindings/go`: `go test` cases covering one indicator per FFI archetype
  (scalar/batch, multi-output, bars, profile, array input), reset, and lifecycle.
- `bindings/r`: `testthat` cases covering one indicator per FFI archetype
  (scalar/batch, multi-output, bars, profile, array input), reset, and validation.
- `bindings/java`: JUnit cases covering one indicator per FFI archetype
  (scalar/batch, multi-output, bars, profile, array input) plus batch equivalence.

On top of those per-binding tests, **all 10 languages** (Rust, Python, Node.js,
WASM, C, C++, C#, Go, Java, R) replay a shared, language-neutral golden fixture
(`testdata/golden/*.csv`, generated by
`cargo run -p wickra-examples --bin gen_golden`) and assert **parity with the
Rust reference for every one of the 514 indicators** across every
archetype (scalar, multi-output, pairwise, derivatives-tick, cross-section,
order-book, trade, profile, alt-chart bars, footprint). This catches FFI wiring
bugs the math-only core tests cannot see — it has already found and fixed real
cross-language marshalling bugs in the Java and R bindings.

> **What "parity" means here, precisely.** Every binding calls into the same Rust
> core, so the arithmetic is not reimplemented anywhere and the results are
> identical by construction — with one exception worth stating rather than
> glossing over. 53 of the 514 indicators call a transcendental from the
> platform's math library (`ln`, `sin`, `cos`, `atan`, `exp` and friends, either
> directly or through an indicator they compose). No mainstream libm rounds those
> correctly, and implementations differ in the last bit: measured on
> `LinRegAngle` over the golden input, Rust's `atan(x).to_degrees()` differs from
> the correctly rounded result on 24 of 67 bars, by one ulp. That last bit is a
> property of the machine, not of Wickra, and it is why one golden fixture
> stopped reproducing after a toolchain change. The golden runners therefore
> compare to a relative tolerance rather than by bit equality. The remaining 461
> indicators use only IEEE-754 arithmetic (`+ − × ÷ √`), which every conforming
> platform rounds identically.


## Ecosystem

Wickra is the core library. The same data-driven core + ten-language binding
pattern powers a family of products built on it:

- [**wickra**](https://github.com/wickra-lib/wickra) — main library (Rust core + Python / Node.js / WASM bindings + a C ABI for C / C++ / C# / Go / Java / R)
- [**wickra-playground**](https://github.com/wickra-lib/wickra-playground) — a polyglot strategy playground: one StrategySpec live side by side in Python, Rust, JS and Go, entirely in the browser
- [**wickra-exchange**](https://github.com/wickra-lib/wickra-exchange) — unified market-data + execution across ten crypto exchanges
- [**wickra-backtest**](https://github.com/wickra-lib/wickra-backtest) — event-driven backtester over the Wickra core
- [**wickra-terminal**](https://github.com/wickra-lib/wickra-terminal) — the trading terminal: a TUI and a browser renderer over the stack
- [**wickra-screener**](https://github.com/wickra-lib/wickra-screener) — parallel multi-symbol screening over 514 streaming indicators
- [**wickra-xray**](https://github.com/wickra-lib/wickra-xray) — market-microstructure explorer: footprint, order-book heatmap, liquidation map, funding/OI divergence
- [**wickra-radar**](https://github.com/wickra-lib/wickra-radar) — perp-universe alert radar: OI delta, funding flip, book imbalance, liquidation clusters, OI/price divergence
- [**wickra-copilot**](https://github.com/wickra-lib/wickra-copilot) — local market copilot grounded in real order-book, liquidation and funding microstructure
- [**wickra-shazam**](https://github.com/wickra-lib/wickra-shazam) — match an asset's current microstructure fingerprint against its entire history
- [**wickra-benchmark**](https://github.com/wickra-lib/wickra-benchmark) — reproducible, golden-verified benchmark suite — recompute any (strategy, dataset, report) in ten languages and confirm it byte-for-byte
- [**wickra-strategy-ci**](https://github.com/wickra-lib/wickra-strategy-ci) — Jest for trading strategies: golden-pin the report, catch regressions in CI, property-test against fuzzed data
- [**wickra-verify**](https://github.com/wickra-lib/wickra-verify) — confirm or refute a claimed backtest report against its strategy and data, in ten languages
- [**wickra-proof**](https://github.com/wickra-lib/wickra-proof) — Proof-of-Backtest: deterministic (spec, data) → report + blake3 hash, recomputable byte-for-byte in ten languages
- [**wickra-zk**](https://github.com/wickra-lib/wickra-zk) — prove a backtest zero-knowledge — on-chain-verifiable performance without revealing the data or the strategy
- [**wickra-impact**](https://github.com/wickra-lib/wickra-impact) — the backtester that knows you would have moved the market: agent-based fills on the real historical L2 order book
- [**wickra-darwin**](https://github.com/wickra-lib/wickra-darwin) — evolutionary strategy search at millions of backtests per second, mutating and crossing JSON specs across the 514-indicator space
- [**wickra-gym**](https://github.com/wickra-lib/wickra-gym) — a Gymnasium-compatible, microstructure-aware backtest environment with O(1) steps for deterministic RL rollouts
- [**wickra-feature-store**](https://github.com/wickra-lib/wickra-feature-store) — OHLCV and microstructure streams into ML-ready feature matrices over 514 O(1) streaming indicators
- [**wickra-genome**](https://github.com/wickra-lib/wickra-genome) — a vector database of the whole market: every asset a 514-dim live vector, for similarity search, clustering and anomaly detection
- [**wickra-timemachine**](https://github.com/wickra-lib/wickra-timemachine) — scrub the whole market like a video — every symbol, full order book, rewound to any moment via deterministic re-fold
- [**wickra-synth**](https://github.com/wickra-lib/wickra-synth) — deterministic synthetic market microstructure: OHLCV, order book, trades and funding from a single seed
- [**wickra-compile**](https://github.com/wickra-lib/wickra-compile) — compile a strategy spec into a standalone deployable: a WASM module, a self-contained binary, or a `no_std` artifact
- [**wickra-embed**](https://github.com/wickra-lib/wickra-embed) — allocation-free, `no_std` streaming indicators for bare-metal and HFT, byte-for-byte identical to the core
- [**wickra-pico**](https://github.com/wickra-lib/wickra-pico) — the O(1) indicator core running bare-metal on a $5 Raspberry Pi Pico — the LED blinks on the EMA cross

Docs live at [docs.wickra.org](https://docs.wickra.org) ([wickra-docs](https://github.com/wickra-lib/wickra-docs)); the marketing site and in-browser demo at [wickra.org](https://wickra.org) ([webpage](https://github.com/wickra-lib/webpage)).

## Contributing

Contributions are very welcome — issues, bug reports, ideas, and pull requests
all land in the same place: <https://github.com/wickra-lib/wickra>.

A short orientation for first-time contributors:

- **Adding an indicator.** Implement the `Indicator` trait in
  `crates/wickra-core/src/indicators/<name>.rs`, wire it into
  `indicators/mod.rs` and the crate root, and add reference-value tests,
  a `batch == streaming` equivalence test, and (where it makes sense) a
  proptest. The four bindings inherit your indicator automatically once
  you expose it in the language wrappers.
- **Fixing a numeric bug.** Add a failing test that pins the textbook value
  first, then fix the math. Property tests in `crates/wickra-core` catch
  most regressions; please don't disable them.
- **Improving a binding.** Each binding lives under `bindings/<lang>` with
  its own tests; please keep the `batch == streaming` invariant.
- **Style.** `cargo fmt --all` + `cargo clippy --workspace --all-targets -- -D warnings`
  are CI gates; running them locally before pushing keeps reviews short.

For larger architectural changes, open an issue first so we can sketch the
shape together before you invest the time.

Pull requests open with a short template. For a change that touches the release
process, the bindings' shared surface, or the public API, there is a longer one
that asks the questions such a change needs to answer — append
`?template=detailed.md` to the pull request URL to use it. GitHub offers no
picker for a second template, so it is unreachable unless you know it is there.

## Security

Please do not open a public issue for a security vulnerability. Report it
privately through GitHub's [private vulnerability
reporting](https://github.com/wickra-lib/wickra/security/advisories/new), or by
email to **support@wickra.org** with a subject line starting with
`[wickra security]`.

Security fixes are applied to the latest released version only.
[SECURITY.md](SECURITY.md) has the full policy: what to include in a report,
what response times to expect, and which versions are supported.

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or
  <http://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or <http://opensource.org/licenses/MIT>)

at your option. Use it, fork it, modify it, redistribute it — commercially or
not — file issues, send pull requests; all welcome.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.

## Disclaimer

Wickra is an indicator toolkit, not a trading system. Values it computes are
deterministic transforms of the input data — they are not financial advice and
they do not predict the market. Any use of this library in a production
trading context is at your own risk.

The library is provided **as is**, without warranty of any kind; see
[LICENSE](LICENSE) for the full terms.

---

<p align="center">
  <a href="https://github.com/wickra-lib/wickra">
    <img alt="GitHub stars" src="https://raw.githubusercontent.com/wickra-lib/.github/main/profile/badges/wickra/stars.svg">
  </a>
  <a href="https://github.com/wickra-lib/wickra/network/members">
    <img alt="GitHub forks" src="https://raw.githubusercontent.com/wickra-lib/.github/main/profile/badges/wickra/forks.svg">
  </a>
  <a href="https://github.com/wickra-lib/wickra/issues">
    <img alt="GitHub issues" src="https://raw.githubusercontent.com/wickra-lib/.github/main/profile/badges/wickra/issues.svg">
  </a>
</p>

<p align="center">
  If Wickra saved you time, the cheapest way to say thanks is to ⭐ the repo.
</p>

<p align="center">
  <img alt="Wickra star history" width="640"
       src="https://raw.githubusercontent.com/wickra-lib/.github/main/profile/badges/wickra/star-history.svg">
</p>
