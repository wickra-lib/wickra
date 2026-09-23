<p align="center">
  <a href="https://wickra.org"><img src="https://raw.githubusercontent.com/wickra-lib/.github/main/profile/wickra-banner.webp?v=514-7" alt="Wickra — streaming-first technical indicators" width="100%"></a>
</p>

[![CI](https://raw.githubusercontent.com/wickra-lib/.github/main/profile/badges/wickra/ci.svg)](https://github.com/wickra-lib/wickra/actions/workflows/ci.yml)
[![codecov](https://raw.githubusercontent.com/wickra-lib/.github/main/profile/badges/wickra/codecov.svg)](https://codecov.io/gh/wickra-lib/wickra)
[![Maven Central](https://raw.githubusercontent.com/wickra-lib/.github/main/profile/badges/wickra/maven.svg)](https://central.sonatype.com/artifact/org.wickra/wickra)
[![License: MIT OR Apache-2.0](https://raw.githubusercontent.com/wickra-lib/.github/main/profile/badges/wickra/license.svg)](https://github.com/wickra-lib/wickra#license)

# Wickra — Java

---

> **▶ Live demo:** all 514 indicators over real Binance market data, computed live in your browser — **[live.wickra.org](https://live.wickra.org)** · zero backend, powered by `wickra-wasm`.

**Streaming-first technical indicators for the JVM, on the Java Foreign Function
& Memory API — prebuilt native library, no JNI, no system dependencies.**

Wickra is a multi-language technical-analysis library with a Rust core and
bindings for Python, Node.js and WASM, plus a C ABI for C, C++, C#, Go, Java, R
and any other C-capable language. Every indicator is an incremental streaming state
machine, so live trading bots and historical backtests share the exact same
implementation. This package is the Java binding; it consumes the C ABI hub
through the Panama FFM API (`java.lang.foreign`) and exposes all 514
streaming-first indicators as idiomatic `AutoCloseable` classes.

## Requirements

- **Java 22 or later** (the FFM API is final since Java 22; no preview flag).
- The FFM API is *restricted*: pass `--enable-native-access=ALL-UNNAMED` when you
  run your application to silence the native-access warning.

## Install

Maven:

```xml
<dependency>
  <groupId>org.wickra</groupId>
  <artifactId>wickra</artifactId>
  <version>1.0.6</version>
</dependency>
```

Gradle:

```kotlin
implementation("org.wickra:wickra:1.0.6")
```

The native library ships prebuilt per platform (Linux, macOS, Windows — x64 and
arm64) inside the jar and is extracted automatically on first use. There is
nothing to compile.

## Quick start

```java
import org.wickra.Ema;
import org.wickra.Rsi;

// Batch: run an indicator over a whole series (NaN at warmup positions).
double[] prices = new double[1000];
for (int i = 0; i < prices.length; i++) {
    prices[i] = 100.0 + i * 0.1;
}
try (Ema ema = new Ema(20)) {
    double[] values = ema.batch(prices);
}

// Streaming: the same indicator, fed tick by tick.
try (Rsi rsi = new Rsi(14)) {
    for (double price : liveFeed) {
        double value = rsi.update(price); // NaN during warmup, no recomputation
        if (Double.isFinite(value) && value > 70) {
            System.out.println("overbought");
        }
    }
}
```

`batch(prices)` and feeding the same prices through `update()` produce identical
values — the equivalence is enforced by the test suite. Multi-output indicators
(MACD, Bollinger, ADX, …) return a `record`, `null` while warming up. Each
indicator owns a native handle freed by a `Cleaner`; `close()` releases it
eagerly (use try-with-resources).

## Benchmark

`benchmarks/` reports streaming and batch updates-per-second for `SMA`, `ATR`
and `MACD`. It measures this binding's FFI overhead, not a cross-library ratio
(the same Rust core runs under every binding) — see the repository
[BENCHMARKS.md](https://github.com/wickra-lib/wickra/blob/main/BENCHMARKS.md) §3.

```bash
cargo build -p wickra-c --release
mvn -q install -DskipTests
mvn -q -f benchmarks exec:exec -Dexec.mainClass=org.wickra.benchmarks.Throughput
```

## Documentation

The full indicator catalogue, guides, quickstarts, and API reference live in
the main repository and documentation site:

- **Repository & full indicator list:** <https://github.com/wickra-lib/wickra>
- **Docs** (quickstarts, cookbook, TA-Lib migration): <https://docs.wickra.org>
- **Runnable examples:** [`examples/java/`](https://github.com/wickra-lib/wickra/tree/main/examples/java)

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
