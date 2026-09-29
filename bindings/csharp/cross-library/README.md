# Cross-library benchmark (.NET)

Wickra against [QuanTAlib](https://github.com/mihakralj/QuanTAlib), TA-Lib
(`TALib.NETCore`), `Skender.Stock.Indicators` and `OoplesFinance.StockIndicators`,
on QuanTAlib's own setup — the one its `perf/Benchmark.cs` times — so the numbers
can be set side by side with the ones it publishes:

| | |
|---|---|
| Data | 500,000 bars from QuanTAlib's geometric-Brownian-motion feed, seed 42 |
| Period | 220 (Chaikin oscillator 3, 10) |
| Harness | BenchmarkDotNet, `Job.ShortRun`, .NET 10 |
| Indicators | SMA, EMA, WMA, HMA, Chaikin oscillator (ADOSC), correlation, skewness |

What it adds to that setup:

- **Wickra measured the way the others' fast rows are.** QuanTAlib's `Span` rows
  and TA-Lib write into a buffer the caller keeps; this times Wickra's exact
  `Batch` into a `Span` and the opt-in `BatchFast` into a `Span` next to the
  allocating `Batch`, and its per-tick `Update`.
- **Wickra on correlation and skewness** (`PearsonCorrelation`, `Skewness`).
- **Skewness both ways.** Wickra's is the population skewness (divisor `n`);
  QuanTAlib's default is the sample skewness, so QuanTAlib is also timed with
  `isPopulation: true`.

## Run

```bash
cargo build -p wickra-c --release
dotnet run -c Release --project bindings/csharp/cross-library                     # every benchmark
dotnet run -c Release --project bindings/csharp/cross-library -- --filter '*Sma*'  # one group
dotnet run -c Release --project bindings/csharp/cross-library -- verify           # numerical cross-check
```

It references this checkout's binding, so it measures the Wickra you have built;
the native library is found in `target/release`. It needs the .NET 10 SDK, as
QuanTAlib 0.8.12 ships for .NET 10 only.

## Same results before same speed

`verify` checks that the libraries compute the same thing before their times are
compared, from bar 2,000 on (past every warmup and the decay of the different EMA
seeds):

- Wickra's exact batch against TA-Lib (SMA, EMA, WMA, ADOSC, correlation), its
  fast batch against its exact one, and its exact batch against naive two-pass
  references (mean, weighted mean, HMA, Pearson correlation, population
  skewness). A disagreement exits with status 1; CI runs it.
- QuanTAlib against the same, reported without a verdict.

Two definitional differences show up there. HMA smooths over `sqrt(period)`
periods: Wickra rounds that to the nearest integer (15 for 220), QuanTAlib — like
TradingView and pandas-ta — truncates it (14), so the two HMAs are different
series. And the skewness is compared against the population definition.
