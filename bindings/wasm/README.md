<p align="center">
  <a href="https://wickra.org"><img src="https://raw.githubusercontent.com/wickra-lib/.github/main/profile/wickra-banner.svg?v=514-8" alt="Wickra — streaming-first technical indicators" width="100%"></a>
</p>

[![CI](https://raw.githubusercontent.com/wickra-lib/.github/main/profile/badges/wickra/ci.svg)](https://github.com/wickra-lib/wickra/actions/workflows/ci.yml)
[![codecov](https://raw.githubusercontent.com/wickra-lib/.github/main/profile/badges/wickra/codecov.svg)](https://codecov.io/gh/wickra-lib/wickra)
[![npm](https://img.shields.io/npm/v/wickra-wasm.svg?logo=npm&color=red)](https://www.npmjs.com/package/wickra-wasm)
[![License: MIT OR Apache-2.0](https://raw.githubusercontent.com/wickra-lib/.github/main/profile/badges/wickra/license.svg)](https://github.com/wickra-lib/wickra#license)

# Wickra — WASM

---

> **▶ Live demo:** all 514 indicators over real Binance market data, computed live in your browser — **[live.wickra.org](https://live.wickra.org)** · zero backend, powered by `wickra-wasm`.

**Streaming-first technical indicators in the browser. `npm install
wickra-wasm` — pure WebAssembly, runs anywhere a modern JS engine does.**

Wickra is a multi-language technical-analysis library with a Rust core and
bindings for Python, Node.js and WASM, plus a C ABI for C, C++, C#, Go, Java, R and any
other C-capable language. Every indicator is an incremental
streaming state machine, so live trading dashboards and historical backtests
share the exact same implementation. This package is the WASM binding
(wasm-bindgen, built for the `web` target); it exposes all 514 streaming-first
indicators across twenty-four families.

## Install

```bash
npm install wickra-wasm
```

## Quick start

The module ships a default `init` export that loads the `.wasm` payload; await
it once before constructing indicators.

```js
import init, { RSI } from 'wickra-wasm';

await init(); // load the WebAssembly module once

// Streaming: feed prices tick by tick.
const rsi = new RSI(14);
for (const price of liveFeed) {
  const value = rsi.update(price); // null during warmup
  if (value !== null && value > 70) {
    console.log('overbought');
  }
}
```

Constructors mirror the other bindings (`new SMA(20)`, `new MACD(12, 26, 9)`,
`new BollingerBands(20, 2.0)`, …); `update()` returns the latest value or
`null` while the indicator is still warming up.

### Batch, caller buffers, and the opt-in fast batch

`batch(prices)` runs a whole `Float64Array` in one call and returns a
`Float64Array`, `NaN` during warmup — bit for bit what `update()` gives. The
single-output indicators (and MACD, Bollinger Bands, ATR, the Chaikin
oscillator and Pearson correlation) add three methods:

```js
const out = new Float64Array(prices.length);
new SMA(20).batchInto(prices, out);              // batch() into a reused buffer
const fast = new EMA(20).batchFast(prices);      // the opt-in fast batch
new EMA(20).batchFastInto(prices, out);
```

`batchFast` runs a kernel where the indicator has one (moving averages, RSI,
ATR, MACD, Bollinger, Chaikin, skewness, Pearson and more). The kernel
reassociates the arithmetic, so each value agrees with `batch` to within a few
units in the last place rather than bit for bit; NaN placement and length are
identical, and the result is the same on every platform. Where there is no
kernel, `batchFast` is `batch` exactly. MACD and Bollinger Bands write flat rows
(`n * 3` and `n * 4` values), as their `batch` does, and a buffer of the wrong
length is refused before anything is consumed.

## Benchmark

`benchmarks/throughput.mjs` reports streaming and batch updates-per-second for
`SMA`, `ATR` and `MACD`. It measures this binding's FFI overhead, not a
cross-library ratio (the same Rust core runs under every binding) — see the
repository [BENCHMARKS.md](https://github.com/wickra-lib/wickra/blob/main/BENCHMARKS.md) §3.

```bash
wasm-pack build --target nodejs --out-dir pkg-node --release
node benchmarks/throughput.mjs
```

## Documentation

The full indicator catalogue, guides, quickstarts, and API reference live in
the main repository and documentation site:

- **Repository & full indicator list:** <https://github.com/wickra-lib/wickra>
- **Docs** (quickstarts, cookbook, TA-Lib migration): <https://docs.wickra.org>
- **Runnable browser examples:** [`examples/wasm/`](https://github.com/wickra-lib/wickra/tree/main/examples/wasm)

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
