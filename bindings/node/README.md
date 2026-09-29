<p align="center">
  <a href="https://wickra.org"><img src="https://raw.githubusercontent.com/wickra-lib/.github/main/profile/wickra-banner.svg?v=514-8" alt="Wickra — streaming-first technical indicators" width="100%"></a>
</p>

[![CI](https://raw.githubusercontent.com/wickra-lib/.github/main/profile/badges/wickra/ci.svg)](https://github.com/wickra-lib/wickra/actions/workflows/ci.yml)
[![codecov](https://raw.githubusercontent.com/wickra-lib/.github/main/profile/badges/wickra/codecov.svg)](https://codecov.io/gh/wickra-lib/wickra)
[![npm](https://raw.githubusercontent.com/wickra-lib/.github/main/profile/badges/wickra/npm.svg)](https://www.npmjs.com/package/wickra)
[![License: MIT OR Apache-2.0](https://raw.githubusercontent.com/wickra-lib/.github/main/profile/badges/wickra/license.svg)](https://github.com/wickra-lib/wickra#license)

# Wickra — Node.js

---

> **▶ Live demo:** all 514 indicators over real Binance market data, computed live in your browser — **[live.wickra.org](https://live.wickra.org)** · zero backend, powered by `wickra-wasm`.

**Streaming-first technical indicators for Node.js. `npm install wickra` —
prebuilt native binary, no system dependencies.**

Wickra is a multi-language technical-analysis library with a Rust core and
bindings for Python, Node.js and WASM, plus a C ABI for C, C++, C#, Go, Java, R and any
other C-capable language. Every indicator is an incremental
streaming state machine, so live trading bots and historical backtests share
the exact same implementation. This package is the Node.js binding (napi-rs);
it exposes all 514 streaming-first indicators across twenty-four families.

## Install

```bash
npm install wickra
```

The native addon ships as a prebuilt binary per platform (Linux, macOS,
Windows — x64 and arm64), selected automatically through optional
dependencies. There is nothing to compile.

## Quick start

```js
const wickra = require('wickra');

// Batch: run an indicator over a whole array.
const prices = Array.from({ length: 1000 }, (_, i) => 100 + i * 0.1);
const values = new wickra.RSI(14).batch(prices); // NaN during warmup

// Streaming: the same indicator, fed tick by tick.
const rsi = new wickra.RSI(14);
for (const price of liveFeed) {
  const value = rsi.update(price); // no recomputation over history
  if (value !== null && value > 70) {
    console.log('overbought');
  }
}
```

`batch(prices)` and feeding the same prices through `update()` produce
identical values — the equivalence is enforced by the test suite.

### Typed arrays, caller buffers, and the opt-in fast batch

Every batch accepts a `Float64Array` as well as a plain array; a `Float64Array`
is read in place, without a copy. `batch` keeps returning a plain `Array`. For
throughput, the single-output indicators (and MACD, Bollinger Bands, ATR, the
Chaikin oscillator and Pearson correlation) add three methods:

```js
const input = Float64Array.from(prices);
const out = new Float64Array(input.length);

new wickra.SMA(20).batchInto(input, out);           // the same bits as batch(), no allocation
const fast = new wickra.EMA(20).batchFast(input);   // a new Float64Array
new wickra.EMA(20).batchFastInto(input, out);       // fast, into the caller's buffer
```

`batchFast` runs a SIMD kernel where the indicator has one (moving averages,
RSI, ATR, MACD, Bollinger, Chaikin, skewness, Pearson and more). The kernel
reassociates the arithmetic, so each value agrees with `batch` to within a few
units in the last place rather than bit for bit; NaN placement and length are
identical, and the result is the same on every platform. Where there is no
kernel, `batchFast` is `batch` exactly. MACD and Bollinger Bands write flat rows
(`n * 3` and `n * 4` values), as their `batch` does.

The output of `batchInto` / `batchFastInto` must be exactly as long as the
result, must not share memory with an input, and must not be backed by a
`SharedArrayBuffer`; anything else throws before a value is written.

## Benchmark

`benchmarks/throughput.js` reports streaming and batch updates-per-second for
`SMA`, `ATR` and `MACD`. It measures this binding's FFI overhead, not a
cross-library ratio (the same Rust core runs under every binding) — see the
repository [BENCHMARKS.md](https://github.com/wickra-lib/wickra/blob/main/BENCHMARKS.md) §3.

```bash
npm run build
node benchmarks/throughput.js
```

## Documentation

The full indicator catalogue, guides, quickstarts, and API reference live in
the main repository and documentation site:

- **Repository & full indicator list:** <https://github.com/wickra-lib/wickra>
- **Docs** (quickstarts, cookbook, TA-Lib migration): <https://docs.wickra.org>
- **Runnable examples:** [`examples/node/`](https://github.com/wickra-lib/wickra/tree/main/examples/node)

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
