<p align="center">
  <a href="https://wickra.org"><img src="https://raw.githubusercontent.com/wickra-lib/.github/main/profile/wickra-banner.webp?v=514-7" alt="Wickra — streaming-first technical indicators" width="100%"></a>
</p>

[![CI](https://raw.githubusercontent.com/wickra-lib/.github/main/profile/badges/wickra/ci.svg)](https://github.com/wickra-lib/wickra/actions/workflows/ci.yml)
[![codecov](https://raw.githubusercontent.com/wickra-lib/.github/main/profile/badges/wickra/codecov.svg)](https://codecov.io/gh/wickra-lib/wickra)
[![PyPI](https://raw.githubusercontent.com/wickra-lib/.github/main/profile/badges/wickra/pypi.svg)](https://pypi.org/project/wickra/)
[![License: MIT OR Apache-2.0](https://raw.githubusercontent.com/wickra-lib/.github/main/profile/badges/wickra/license.svg)](https://github.com/wickra-lib/wickra#license)

# Wickra — Python

---

> **▶ Live demo:** all 514 indicators over real Binance market data, computed live in your browser — **[live.wickra.org](https://live.wickra.org)** · zero backend, powered by `wickra-wasm`.

**Streaming-first technical indicators for Python. `pip install wickra` — zero
third-party dependencies (not even NumPy), no system dependencies, no C build tooling.**

Wickra is a multi-language technical-analysis library with a Rust core and
bindings for Python, Node.js and WASM, plus a C ABI for C, C++, C#, Go, Java, R and any
other C-capable language. Every indicator is an incremental
streaming state machine, so live trading bots and historical backtests share
the exact same implementation. This package is the Python binding (PyO3); it
exposes all 514 streaming-first indicators across twenty-four families.

## Install

```bash
pip install wickra
```

Pre-built wheels ship for Linux, macOS, and Windows — there is nothing to
compile and no C library to track down. `pip install wickra` pulls **zero**
third-party packages; NumPy is an optional extra (`pip install wickra[numpy]`)
for zero-copy interop.

## Quick start

```python
import wickra as ta                     # zero third-party deps — not even NumPy

# Batch: classic TA-Lib-style usage over a whole series.
prices = [100.0 + i * 0.1 for i in range(1000)]   # list, array.array or NumPy all work
rsi = ta.RSI(14)
values = rsi.batch(prices)              # array.array('d'), NaN during warmup
                                        # np.asarray(values) wraps it zero-copy if you use NumPy

# Streaming: the same indicator, fed tick by tick.
rsi = ta.RSI(14)
for price in live_feed:
    value = rsi.update(price)           # no recomputation over history
    if value is not None and value > 70:
        print("overbought")
```

`batch(prices)` and feeding the same prices through `update()` produce
identical values — the equivalence is enforced by the test suite.

## Benchmark

Two benchmarks ship with the binding:

- `benchmarks/throughput.py` — streaming and batch updates-per-second for `SMA`,
  `ATR` and `MACD`. This is per-binding FFI overhead (the same Rust core runs
  under every binding), not a cross-library ratio.
- `benchmarks/compare_libraries.py` — the cross-library comparison against
  TA-Lib, pandas-ta, tulipy and finta that backs the headline speedups.

```bash
maturin develop --release
python -m benchmarks.throughput
python -m benchmarks.compare_libraries   # cross-library; auto-detects installed peers
```

See the repository [BENCHMARKS.md](https://github.com/wickra-lib/wickra/blob/main/BENCHMARKS.md).

## Documentation

The full indicator catalogue, guides, quickstarts, and API reference live in
the main repository and documentation site:

- **Repository & full indicator list:** <https://github.com/wickra-lib/wickra>
- **Docs** (quickstarts, cookbook, TA-Lib migration): <https://docs.wickra.org>
- **Runnable examples:** [`examples/python/`](https://github.com/wickra-lib/wickra/tree/main/examples/python)

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
