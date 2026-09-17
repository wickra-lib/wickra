<p align="center">
  <a href="https://wickra.org"><img src="https://raw.githubusercontent.com/wickra-lib/.github/main/profile/wickra-banner.webp?v=514-7" alt="Wickra — streaming-first technical indicators" width="100%"></a>
</p>

[![CI](https://raw.githubusercontent.com/wickra-lib/.github/main/profile/badges/wickra/ci.svg)](https://github.com/wickra-lib/wickra/actions/workflows/ci.yml)
[![codecov](https://raw.githubusercontent.com/wickra-lib/.github/main/profile/badges/wickra/codecov.svg)](https://codecov.io/gh/wickra-lib/wickra)
[![NuGet](https://raw.githubusercontent.com/wickra-lib/.github/main/profile/badges/wickra/nuget.svg)](https://www.nuget.org/packages/Wickra)
[![License: MIT OR Apache-2.0](https://raw.githubusercontent.com/wickra-lib/.github/main/profile/badges/wickra/license.svg)](https://github.com/wickra-lib/wickra#license)

# Wickra — C#

---

> **▶ Live demo:** all 514 indicators over real Binance market data, computed live in your browser — **[live.wickra.org](https://live.wickra.org)** · zero backend, powered by `wickra-wasm`.

**Streaming-first technical indicators for C#. `dotnet add package Wickra` —
prebuilt native library, no system dependencies.**

Wickra is a multi-language technical-analysis library with a Rust core and
bindings for Python, Node.js and WASM, plus a C ABI for C, C++, C#, Go, Java, R and any
other C-capable language. Every indicator is an incremental
streaming state machine, so live trading bots and historical backtests share
the exact same implementation. This package is the C# binding; it consumes the
C ABI hub through `[LibraryImport]` P/Invoke and exposes all 514 streaming-first
indicators as idiomatic `IDisposable` classes.

## Install

```bash
dotnet add package Wickra
```

The native library ships prebuilt per platform (Linux, macOS, Windows — x64 and
arm64) under `runtimes/<rid>/native/`, selected automatically. There is nothing
to compile. Targets .NET 8 and later.

## Quick start

```csharp
using Wickra;

// Batch: run an indicator over a whole series (NaN at warmup positions).
var prices = Enumerable.Range(0, 1000).Select(i => 100.0 + i * 0.1).ToArray();
using var sma = new Sma(20);
double[] values = sma.Batch(prices);

// Streaming: the same indicator, fed tick by tick.
using var rsi = new Rsi(14);
foreach (var price in liveFeed)
{
    var value = rsi.Update(price); // NaN during warmup, no recomputation
    if (double.IsFinite(value) && value > 70)
    {
        Console.WriteLine("overbought");
    }
}
```

`Batch(prices)` and feeding the same prices through `Update()` produce identical
values — the equivalence is enforced by the test suite. Multi-output indicators
(MACD, Bollinger, ADX, …) return a nullable `record struct`, `null` while warming up.

## Benchmark

`benchmarks/` reports streaming and batch updates-per-second for `SMA`, `ATR`
and `MACD`. It measures this binding's FFI overhead, not a cross-library ratio
(the same Rust core runs under every binding) — see the repository
[BENCHMARKS.md](https://github.com/wickra-lib/wickra/blob/main/BENCHMARKS.md) §3.

```bash
cargo build -p wickra-c --release
dotnet run -c Release --project benchmarks
```

## Documentation

The full indicator catalogue, guides, quickstarts, and API reference live in
the main repository and documentation site:

- **Repository & full indicator list:** <https://github.com/wickra-lib/wickra>
- **Docs** (quickstarts, cookbook, TA-Lib migration): <https://docs.wickra.org>
- **Runnable examples:** [`examples/csharp/`](https://github.com/wickra-lib/wickra/tree/main/examples/csharp)

Wickra ships native bindings for Python, Node.js, WASM and Rust, plus a
C ABI hub that any C-capable language (C, C++, C#, Go, Java, R) links against —
all exposing the same indicators from the shared, `unsafe`-forbidden Rust core.

## Security

Found a security issue? **Please don't open a public issue.** Report it privately
via the affected repository's *Security* tab (*"Report a vulnerability"*) or email
**support@wickra.org** with a subject line starting `[wickra security]`. Full
policy: <https://github.com/wickra-lib/wickra/blob/main/SECURITY.md>.

## Disclaimer

Wickra is an indicator toolkit, not a trading system. The values it computes
are deterministic transforms of the input data — they are not financial advice
and do not predict the market. Any use in a live trading context is at your own
risk. The library is provided **as is**, without warranty of any kind.

## License

Licensed under either of [Apache-2.0](https://github.com/wickra-lib/wickra/blob/main/LICENSE-APACHE)
or [MIT](https://github.com/wickra-lib/wickra/blob/main/LICENSE-MIT) at your option.
