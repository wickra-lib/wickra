"""Throughput benchmark for the Wickra Python binding.

Measures how many indicator updates per second the binding sustains, per-tick
(streaming ``update``), bulk (``batch``) and bulk through the opt-in SIMD
kernels (``batch_fast``), over a synthetic OHLCV series. It is the Python
counterpart of the Node ``throughput.js`` and the Rust criterion benches: it
benchmarks Wickra's own O(1) streaming engine across the Python<->Rust boundary,
so the headline number is raw per-binding throughput / FFI overhead, not a
cross-library ratio.

For the cross-library comparison against TA-Lib, pandas-ta, tulipy and finta,
see ``benchmarks/compare_libraries.py`` instead.

Three indicators are timed, chosen by call-signature archetype rather than
algorithm: SMA (1-in -> 1-out), ATR (multi-in -> 1-out) and MACD (1-in ->
multi-out). All three are timed streaming, batch and fast batch.

Install the binding first (``maturin develop --release`` in bindings/python),
then run from bindings/python::

    python -m benchmarks.throughput                # 200k bars (default)
    python -m benchmarks.throughput --bars 1000000
"""

from __future__ import annotations

import argparse
import time

import numpy as np

import wickra as ta


def _time_ns(fn, reps: int = 3) -> float:
    """Median elapsed-ns over a few repetitions, after one warmup pass."""
    fn()  # warmup
    samples = []
    for _ in range(reps):
        t0 = time.perf_counter_ns()
        fn()
        samples.append(time.perf_counter_ns() - t0)
    samples.sort()
    return samples[len(samples) // 2]


def main() -> None:
    parser = argparse.ArgumentParser(description="Wickra Python throughput benchmark")
    parser.add_argument("--bars", type=int, default=200_000, help="number of synthetic bars")
    args = parser.parse_args()
    bars = args.bars if args.bars >= 1000 else 200_000

    # Deterministic synthetic OHLCV (no RNG, so runs are comparable).
    idx = np.arange(bars, dtype=np.float64)
    mid = 100 + np.sin(idx * 0.001) * 20 + idx * 1e-4
    close = mid + np.sin(idx * 0.05) * 2
    high = np.maximum(close, mid) + 1.5
    low = np.minimum(close, mid) - 1.5
    open_ = mid
    volume = 1000 + (idx % 97) * 13

    close_list = close.tolist()
    # ATR streams a 6-tuple (open, high, low, close, volume, timestamp) per tick.
    candles = list(
        zip(open_.tolist(), high.tolist(), low.tolist(), close_list, volume.tolist(), range(bars))
    )

    def mups(ns: float) -> float:
        return bars / (ns / 1e9) / 1e6

    def sma_stream() -> None:
        ind = ta.SMA(20)
        for value in close_list:
            ind.update(value)

    def sma_batch() -> None:
        ta.SMA(20).batch(close)

    def sma_fast() -> None:
        ta.SMA(20).batch_fast(close)

    def atr_stream() -> None:
        ind = ta.ATR(14)
        for candle in candles:
            ind.update(candle)

    def atr_batch() -> None:
        ta.ATR(14).batch(high, low, close)

    def atr_fast() -> None:
        ta.ATR(14).batch_fast(high, low, close)

    def macd_stream() -> None:
        ind = ta.MACD(12, 26, 9)
        for value in close_list:
            ind.update(value)

    def macd_batch() -> None:
        ta.MACD(12, 26, 9).batch(close)

    def macd_fast() -> None:
        ta.MACD(12, 26, 9).batch_fast(close)

    # SMA (scalar 1-in/1-out), ATR (multi-in/1-out), MACD (1-in/multi-out).
    indicators = [
        ("SMA(20)", sma_stream, sma_batch, sma_fast),
        ("ATR(14)", atr_stream, atr_batch, atr_fast),
        ("MACD(12,26,9)", macd_stream, macd_batch, macd_fast),
    ]

    print(f"Wickra Python throughput - {bars:,} bars (median of 3 runs)\n")
    header = f"{'Indicator':<22}{'streaming (Mupd/s)':>20}{'batch (Mupd/s)':>18}{'fast (Mupd/s)':>18}"
    print(header)
    print("-" * len(header))
    for name, stream, batch, fast in indicators:
        cells = [f"{mups(_time_ns(run)):.1f}" for run in (stream, batch, fast)]
        print(f"{name:<22}{cells[0]:>20}{cells[1]:>18}{cells[2]:>18}")

    print(
        "\nMupd/s = million indicator updates per second. Streaming is the per-tick\n"
        "`update` path crossing the Python<->Rust boundary once per value; batch is\n"
        "the bulk path (one boundary crossing); fast is the opt-in batch_fast, SIMD\n"
        "kernels within a few units in the last place of batch. Higher is better.\n"
        "Numbers are machine-dependent - use them for relative comparison, not as a\n"
        "speed claim."
    )


if __name__ == "__main__":
    main()
