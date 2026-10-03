"""Cross-library benchmark: Wickra vs TA-Lib vs pandas-ta vs talipp vs finta.

Runs each library through identical batch and streaming workloads, then prints a
table of timings. Libraries that are not installed are skipped automatically, so
the script always produces output regardless of the local environment.

Usage::

    python -m benchmarks.compare_libraries
    python -m benchmarks.compare_libraries --size 50000 --streaming-window 5000

Notes:

- "Batch" means computing the indicator over the whole price series in one call,
  which is what classic libraries support.
- "Streaming" simulates live trading: after seeding with ``streaming_window``
  historical bars, we keep appending one new price and recomputing the latest
  indicator value. Libraries without an incremental API have to recompute the
  whole indicator on every tick; Wickra updates in O(1). This is the gap the
  library was built to expose.
"""

from __future__ import annotations

import argparse
import contextlib
import importlib
import statistics
import time
from dataclasses import dataclass
from typing import Callable, Dict, List, Optional

import numpy as np


# --------------------------------------------------------------------------- #
# Library availability detection
# --------------------------------------------------------------------------- #


def _try_import(name: str):
    try:
        return importlib.import_module(name)
    except Exception:
        return None


TALIB = _try_import("talib")
PANDAS_TA = _try_import("pandas_ta")
TALIPP = _try_import("talipp.indicators") or _try_import("talipp")
FINTA = _try_import("finta")
TULIPY = _try_import("tulipy")
PD = _try_import("pandas")
import wickra as WICKRA  # noqa: E402  -- the library under test must be importable


# --------------------------------------------------------------------------- #
# Timing helpers
# --------------------------------------------------------------------------- #


@dataclass
class Sample:
    library: str
    indicator: str
    mode: str
    seconds: float
    iterations: int

    @property
    def per_iter_us(self) -> float:
        return (self.seconds / self.iterations) * 1_000_000


def time_call(fn: Callable[[], None], iterations: int, rounds: int = 5) -> float:
    """Time ``fn`` over ``iterations`` calls per round, across ``rounds`` rounds.

    Returns the *median* round's wall seconds for one round of ``iterations``
    calls. Taking the median across several rounds damps the OS scheduling and
    GC jitter that a single timing pass would otherwise bake into the result,
    so the per-iteration figure is stable run-to-run. Callers keep dividing the
    return value by ``iterations``.
    """
    fn()  # one warmup call to populate caches
    rounds_s: List[float] = []
    for _ in range(rounds):
        start = time.perf_counter()
        for _ in range(iterations):
            fn()
        rounds_s.append(time.perf_counter() - start)
    return statistics.median(rounds_s)


def gen_prices(n: int, seed: int = 0xC0FFEE) -> np.ndarray:
    rng = np.random.default_rng(seed)
    walk = rng.standard_normal(n) * 0.4
    return 100.0 + np.cumsum(walk)


def gen_ohlc(n: int, seed: int = 0xC0FFEE) -> tuple:
    close = gen_prices(n, seed)
    spread = 0.5 + np.abs(np.sin(np.arange(n) * 0.07))
    high = close + spread
    low = close - spread
    volume = np.full(n, 1_000.0)
    return high, low, close, volume


# --------------------------------------------------------------------------- #
# Per-library indicator runners. Each returns ``None`` to skip when unavailable.
# --------------------------------------------------------------------------- #


def wickra_sma_batch(prices: np.ndarray) -> Callable[[], None]:
    return lambda: WICKRA.SMA(20).batch(prices)


def talib_sma_batch(prices: np.ndarray) -> Optional[Callable[[], None]]:
    return None if TALIB is None else (lambda: TALIB.SMA(prices, timeperiod=20))


def pandas_ta_sma_batch(prices: np.ndarray) -> Optional[Callable[[], None]]:
    if PANDAS_TA is None or PD is None:
        return None
    s = PD.Series(prices)
    return lambda: PANDAS_TA.sma(s, length=20)


def finta_sma_batch(prices: np.ndarray) -> Optional[Callable[[], None]]:
    if FINTA is None or PD is None:
        return None
    df = PD.DataFrame({"open": prices, "high": prices, "low": prices, "close": prices, "volume": np.ones_like(prices)})
    return lambda: FINTA.TA.SMA(df, period=20)


def talipp_sma_batch(prices: np.ndarray) -> Optional[Callable[[], None]]:
    if TALIPP is None:
        return None
    # talipp's SMA accepts an initial list of values
    from talipp.indicators import SMA  # type: ignore

    return lambda: SMA(period=20, input_values=list(prices))


def wickra_rsi_batch(prices: np.ndarray) -> Callable[[], None]:
    return lambda: WICKRA.RSI(14).batch(prices)


def talib_rsi_batch(prices: np.ndarray) -> Optional[Callable[[], None]]:
    return None if TALIB is None else (lambda: TALIB.RSI(prices, timeperiod=14))


def pandas_ta_rsi_batch(prices: np.ndarray) -> Optional[Callable[[], None]]:
    if PANDAS_TA is None or PD is None:
        return None
    s = PD.Series(prices)
    return lambda: PANDAS_TA.rsi(s, length=14)


def finta_rsi_batch(prices: np.ndarray) -> Optional[Callable[[], None]]:
    if FINTA is None or PD is None:
        return None
    df = PD.DataFrame({"open": prices, "high": prices, "low": prices, "close": prices, "volume": np.ones_like(prices)})
    return lambda: FINTA.TA.RSI(df, period=14)


def talipp_rsi_batch(prices: np.ndarray) -> Optional[Callable[[], None]]:
    if TALIPP is None:
        return None
    from talipp.indicators import RSI  # type: ignore

    return lambda: RSI(period=14, input_values=list(prices))


def wickra_bollinger_batch(prices: np.ndarray) -> Callable[[], None]:
    return lambda: WICKRA.BollingerBands(20, 2.0).batch(prices)


def wickra_ema_batch(prices: np.ndarray) -> Callable[[], None]:
    return lambda: WICKRA.EMA(20).batch(prices)


def talib_ema_batch(prices: np.ndarray) -> Optional[Callable[[], None]]:
    return None if TALIB is None else (lambda: TALIB.EMA(prices, timeperiod=20))


def pandas_ta_ema_batch(prices: np.ndarray) -> Optional[Callable[[], None]]:
    if PANDAS_TA is None or PD is None:
        return None
    s = PD.Series(prices)
    return lambda: PANDAS_TA.ema(s, length=20)


def finta_ema_batch(prices: np.ndarray) -> Optional[Callable[[], None]]:
    if FINTA is None or PD is None:
        return None
    df = PD.DataFrame({"open": prices, "high": prices, "low": prices, "close": prices, "volume": np.ones_like(prices)})
    return lambda: FINTA.TA.EMA(df, period=20)


def talipp_ema_batch(prices: np.ndarray) -> Optional[Callable[[], None]]:
    if TALIPP is None:
        return None
    from talipp.indicators import EMA  # type: ignore
    return lambda: EMA(period=20, input_values=list(prices))


def wickra_macd_batch(prices: np.ndarray) -> Callable[[], None]:
    return lambda: WICKRA.MACD().batch(prices)


def talib_macd_batch(prices: np.ndarray) -> Optional[Callable[[], None]]:
    return None if TALIB is None else (lambda: TALIB.MACD(prices))


def pandas_ta_macd_batch(prices: np.ndarray) -> Optional[Callable[[], None]]:
    if PANDAS_TA is None or PD is None:
        return None
    s = PD.Series(prices)
    return lambda: PANDAS_TA.macd(s)


def finta_macd_batch(prices: np.ndarray) -> Optional[Callable[[], None]]:
    if FINTA is None or PD is None:
        return None
    df = PD.DataFrame({"open": prices, "high": prices, "low": prices, "close": prices, "volume": np.ones_like(prices)})
    return lambda: FINTA.TA.MACD(df)


def talipp_macd_batch(prices: np.ndarray) -> Optional[Callable[[], None]]:
    if TALIPP is None:
        return None
    from talipp.indicators import MACD  # type: ignore
    return lambda: MACD(fast_period=12, slow_period=26, signal_period=9, input_values=list(prices))


def wickra_atr_batch(high: np.ndarray, low: np.ndarray, close: np.ndarray) -> Callable[[], None]:
    return lambda: WICKRA.ATR(14).batch(high, low, close)


def talib_atr_batch(high: np.ndarray, low: np.ndarray, close: np.ndarray) -> Optional[Callable[[], None]]:
    return None if TALIB is None else (lambda: TALIB.ATR(high, low, close, timeperiod=14))


def finta_atr_batch(_high: np.ndarray, _low: np.ndarray, _close: np.ndarray) -> Optional[Callable[[], None]]:
    if FINTA is None or PD is None:
        return None
    df = PD.DataFrame({"open": _close, "high": _high, "low": _low, "close": _close, "volume": np.ones_like(_close)})
    return lambda: FINTA.TA.ATR(df, period=14)


def talipp_atr_batch(high: np.ndarray, low: np.ndarray, close: np.ndarray) -> Optional[Callable[[], None]]:
    if TALIPP is None:
        return None
    from talipp.indicators import ATR  # type: ignore
    from talipp.ohlcv import OHLCV
    bars = [OHLCV(open=c, high=h, low=l, close=c, volume=1.0, time=i) for i, (h, l, c) in enumerate(zip(high, low, close))]
    return lambda: ATR(period=14, input_values=bars)


def talib_bollinger_batch(prices: np.ndarray) -> Optional[Callable[[], None]]:
    if TALIB is None:
        return None
    return lambda: TALIB.BBANDS(prices, timeperiod=20, nbdevup=2, nbdevdn=2)


def pandas_ta_bollinger_batch(prices: np.ndarray) -> Optional[Callable[[], None]]:
    if PANDAS_TA is None or PD is None:
        return None
    s = PD.Series(prices)
    return lambda: PANDAS_TA.bbands(s, length=20, std=2.0)


def finta_bollinger_batch(prices: np.ndarray) -> Optional[Callable[[], None]]:
    if FINTA is None or PD is None:
        return None
    df = PD.DataFrame({"open": prices, "high": prices, "low": prices, "close": prices, "volume": np.ones_like(prices)})
    return lambda: FINTA.TA.BBANDS(df, period=20, std_multiplier=2.0)


def talipp_bollinger_batch(prices: np.ndarray) -> Optional[Callable[[], None]]:
    if TALIPP is None:
        return None
    from talipp.indicators import BB  # type: ignore

    return lambda: BB(period=20, std_dev_mult=2.0, input_values=list(prices))


# tulipy wraps the C "Tulip Indicators" library; it takes contiguous float64
# arrays and indicator options as positional arguments.


def tulipy_sma_batch(prices: np.ndarray) -> Optional[Callable[[], None]]:
    return None if TULIPY is None else (lambda: TULIPY.sma(prices, 20))


def tulipy_ema_batch(prices: np.ndarray) -> Optional[Callable[[], None]]:
    return None if TULIPY is None else (lambda: TULIPY.ema(prices, 20))


def tulipy_rsi_batch(prices: np.ndarray) -> Optional[Callable[[], None]]:
    return None if TULIPY is None else (lambda: TULIPY.rsi(prices, 14))


def tulipy_macd_batch(prices: np.ndarray) -> Optional[Callable[[], None]]:
    return None if TULIPY is None else (lambda: TULIPY.macd(prices, 12, 26, 9))


def tulipy_bollinger_batch(prices: np.ndarray) -> Optional[Callable[[], None]]:
    return None if TULIPY is None else (lambda: TULIPY.bbands(prices, 20, 2.0))


def tulipy_atr_batch(high: np.ndarray, low: np.ndarray, close: np.ndarray) -> Optional[Callable[[], None]]:
    return None if TULIPY is None else (lambda: TULIPY.atr(high, low, close, 14))


# --------------------------------------------------------------------------- #
# Streaming scenario: per-tick latency
# --------------------------------------------------------------------------- #


def wickra_rsi_streaming(seed: np.ndarray, live: np.ndarray) -> Callable[[], None]:
    def run() -> None:
        rsi = WICKRA.RSI(14)
        rsi.batch(seed)  # warm up
        for p in live:
            rsi.update(float(p))

    return run


def talib_rsi_streaming(seed: np.ndarray, live: np.ndarray) -> Optional[Callable[[], None]]:
    if TALIB is None:
        return None

    def run() -> None:
        history = list(seed)
        for p in live:
            history.append(float(p))
            TALIB.RSI(np.asarray(history), timeperiod=14)

    return run


def pandas_ta_rsi_streaming(seed: np.ndarray, live: np.ndarray) -> Optional[Callable[[], None]]:
    if PANDAS_TA is None or PD is None:
        return None

    def run() -> None:
        history = list(seed)
        for p in live:
            history.append(float(p))
            PANDAS_TA.rsi(PD.Series(history), length=14)

    return run


def talipp_rsi_streaming(seed: np.ndarray, live: np.ndarray) -> Optional[Callable[[], None]]:
    if TALIPP is None:
        return None
    from talipp.indicators import RSI  # type: ignore

    def run() -> None:
        rsi = RSI(period=14, input_values=list(seed))
        for p in live:
            rsi.add(float(p))

    return run


# Scalar streaming peers: Wickra and talipp both update incrementally in O(1),
# so this is the like-for-like per-tick comparison (batch-only libs are covered
# by the batch tables and the recompute contrast on RSI above).


def wickra_sma_streaming(seed: np.ndarray, live: np.ndarray) -> Callable[[], None]:
    def run() -> None:
        sma = WICKRA.SMA(20)
        sma.batch(seed)
        for p in live:
            sma.update(float(p))

    return run


def talipp_sma_streaming(seed: np.ndarray, live: np.ndarray) -> Optional[Callable[[], None]]:
    if TALIPP is None:
        return None
    from talipp.indicators import SMA  # type: ignore

    def run() -> None:
        sma = SMA(period=20, input_values=list(seed))
        for p in live:
            sma.add(float(p))

    return run


def wickra_ema_streaming(seed: np.ndarray, live: np.ndarray) -> Callable[[], None]:
    def run() -> None:
        ema = WICKRA.EMA(20)
        ema.batch(seed)
        for p in live:
            ema.update(float(p))

    return run


def talipp_ema_streaming(seed: np.ndarray, live: np.ndarray) -> Optional[Callable[[], None]]:
    if TALIPP is None:
        return None
    from talipp.indicators import EMA  # type: ignore

    def run() -> None:
        ema = EMA(period=20, input_values=list(seed))
        for p in live:
            ema.add(float(p))

    return run


def wickra_macd_streaming(seed: np.ndarray, live: np.ndarray) -> Callable[[], None]:
    def run() -> None:
        macd = WICKRA.MACD()
        macd.batch(seed)
        for p in live:
            macd.update(float(p))

    return run


def talipp_macd_streaming(seed: np.ndarray, live: np.ndarray) -> Optional[Callable[[], None]]:
    if TALIPP is None:
        return None
    from talipp.indicators import MACD  # type: ignore

    def run() -> None:
        macd = MACD(
            fast_period=12, slow_period=26, signal_period=9, input_values=list(seed)
        )
        for p in live:
            macd.add(float(p))

    return run


def wickra_bollinger_streaming(seed: np.ndarray, live: np.ndarray) -> Callable[[], None]:
    def run() -> None:
        bb = WICKRA.BollingerBands(20, 2.0)
        bb.batch(seed)
        for p in live:
            bb.update(float(p))

    return run


def talipp_bollinger_streaming(seed: np.ndarray, live: np.ndarray) -> Optional[Callable[[], None]]:
    if TALIPP is None:
        return None
    from talipp.indicators import BB  # type: ignore

    def run() -> None:
        bb = BB(period=20, std_dev_mult=2.0, input_values=list(seed))
        for p in live:
            bb.add(float(p))

    return run


# Recompute streaming peers: batch-only libraries have no incremental API, so
# the only honest way to drive them tick-by-tick is to re-run the full batch
# over the grown history on every new price. These runners expose exactly that
# cost — the gap Wickra's O(1) update closes.


def _talib_recompute_streaming(seed, live, fn):
    def run() -> None:
        history = list(seed)
        for p in live:
            history.append(float(p))
            fn(np.asarray(history))

    return run


def _pandas_ta_recompute_streaming(seed, live, fn):
    def run() -> None:
        history = list(seed)
        for p in live:
            history.append(float(p))
            fn(PD.Series(history))

    return run


def _tulipy_recompute_streaming(seed, live, fn):
    def run() -> None:
        history = list(seed)
        for p in live:
            history.append(float(p))
            fn(np.asarray(history, dtype=np.float64))

    return run


def _finta_recompute_streaming(seed, live, fn):
    def run() -> None:
        history = list(seed)
        for p in live:
            history.append(float(p))
            arr = np.asarray(history)
            fn(PD.DataFrame({"open": arr, "high": arr, "low": arr, "close": arr, "volume": np.ones_like(arr)}))

    return run


def talib_sma_streaming(seed, live):
    if TALIB is None:
        return None
    return _talib_recompute_streaming(seed, live, lambda a: TALIB.SMA(a, timeperiod=20))


def pandas_ta_sma_streaming(seed, live):
    if PANDAS_TA is None or PD is None:
        return None
    return _pandas_ta_recompute_streaming(seed, live, lambda s: PANDAS_TA.sma(s, length=20))


def tulipy_sma_streaming(seed, live):
    if TULIPY is None:
        return None
    return _tulipy_recompute_streaming(seed, live, lambda a: TULIPY.sma(a, 20))


def finta_sma_streaming(seed, live):
    if FINTA is None or PD is None:
        return None
    return _finta_recompute_streaming(seed, live, lambda df: FINTA.TA.SMA(df, period=20))


def talib_ema_streaming(seed, live):
    if TALIB is None:
        return None
    return _talib_recompute_streaming(seed, live, lambda a: TALIB.EMA(a, timeperiod=20))


def pandas_ta_ema_streaming(seed, live):
    if PANDAS_TA is None or PD is None:
        return None
    return _pandas_ta_recompute_streaming(seed, live, lambda s: PANDAS_TA.ema(s, length=20))


def tulipy_ema_streaming(seed, live):
    if TULIPY is None:
        return None
    return _tulipy_recompute_streaming(seed, live, lambda a: TULIPY.ema(a, 20))


def finta_ema_streaming(seed, live):
    if FINTA is None or PD is None:
        return None
    return _finta_recompute_streaming(seed, live, lambda df: FINTA.TA.EMA(df, period=20))


def tulipy_rsi_streaming(seed, live):
    if TULIPY is None:
        return None
    return _tulipy_recompute_streaming(seed, live, lambda a: TULIPY.rsi(a, 14))


def finta_rsi_streaming(seed, live):
    if FINTA is None or PD is None:
        return None
    return _finta_recompute_streaming(seed, live, lambda df: FINTA.TA.RSI(df, period=14))


def talib_macd_streaming(seed, live):
    if TALIB is None:
        return None
    return _talib_recompute_streaming(seed, live, lambda a: TALIB.MACD(a))


def pandas_ta_macd_streaming(seed, live):
    if PANDAS_TA is None or PD is None:
        return None
    return _pandas_ta_recompute_streaming(seed, live, lambda s: PANDAS_TA.macd(s))


def tulipy_macd_streaming(seed, live):
    if TULIPY is None:
        return None
    return _tulipy_recompute_streaming(seed, live, lambda a: TULIPY.macd(a, 12, 26, 9))


def finta_macd_streaming(seed, live):
    if FINTA is None or PD is None:
        return None
    return _finta_recompute_streaming(seed, live, lambda df: FINTA.TA.MACD(df))


def talib_bollinger_streaming(seed, live):
    if TALIB is None:
        return None
    return _talib_recompute_streaming(seed, live, lambda a: TALIB.BBANDS(a, timeperiod=20, nbdevup=2, nbdevdn=2))


def pandas_ta_bollinger_streaming(seed, live):
    if PANDAS_TA is None or PD is None:
        return None
    return _pandas_ta_recompute_streaming(seed, live, lambda s: PANDAS_TA.bbands(s, length=20, std=2.0))


def tulipy_bollinger_streaming(seed, live):
    if TULIPY is None:
        return None
    return _tulipy_recompute_streaming(seed, live, lambda a: TULIPY.bbands(a, 20, 2.0))


def finta_bollinger_streaming(seed, live):
    if FINTA is None or PD is None:
        return None
    return _finta_recompute_streaming(seed, live, lambda df: FINTA.TA.BBANDS(df, period=20, std_multiplier=2.0))


# The opt-in fast batch: SIMD kernels that reassociate the arithmetic, so the
# values agree with `batch` to a few units in the last place, not bit for bit.


def wickra_fast_sma_batch(prices: np.ndarray) -> Callable[[], None]:
    return lambda: WICKRA.SMA(20).batch_fast(prices)


def wickra_fast_ema_batch(prices: np.ndarray) -> Callable[[], None]:
    return lambda: WICKRA.EMA(20).batch_fast(prices)


def wickra_fast_rsi_batch(prices: np.ndarray) -> Callable[[], None]:
    return lambda: WICKRA.RSI(14).batch_fast(prices)


def wickra_fast_macd_batch(prices: np.ndarray) -> Callable[[], None]:
    return lambda: WICKRA.MACD().batch_fast(prices)


def wickra_fast_bollinger_batch(prices: np.ndarray) -> Callable[[], None]:
    return lambda: WICKRA.BollingerBands(20, 2.0).batch_fast(prices)


def wickra_fast_atr_batch(high: np.ndarray, low: np.ndarray, close: np.ndarray) -> Callable[[], None]:
    return lambda: WICKRA.ATR(14).batch_fast(high, low, close)


# --------------------------------------------------------------------------- #
# Runner
# --------------------------------------------------------------------------- #


BATCH_INDICATORS = [
    ("SMA(20)", [
        ("Wickra", wickra_sma_batch),
        ("Wickra (fast)", wickra_fast_sma_batch),
        ("TA-Lib", talib_sma_batch),
        ("pandas-ta", pandas_ta_sma_batch),
        ("tulipy", tulipy_sma_batch),
        ("finta", finta_sma_batch),
        ("talipp", talipp_sma_batch),
    ]),
    ("EMA(20)", [
        ("Wickra", wickra_ema_batch),
        ("Wickra (fast)", wickra_fast_ema_batch),
        ("TA-Lib", talib_ema_batch),
        ("pandas-ta", pandas_ta_ema_batch),
        ("tulipy", tulipy_ema_batch),
        ("finta", finta_ema_batch),
        ("talipp", talipp_ema_batch),
    ]),
    ("RSI(14)", [
        ("Wickra", wickra_rsi_batch),
        ("Wickra (fast)", wickra_fast_rsi_batch),
        ("TA-Lib", talib_rsi_batch),
        ("pandas-ta", pandas_ta_rsi_batch),
        ("tulipy", tulipy_rsi_batch),
        ("finta", finta_rsi_batch),
        ("talipp", talipp_rsi_batch),
    ]),
    ("MACD(12, 26, 9)", [
        ("Wickra", wickra_macd_batch),
        ("Wickra (fast)", wickra_fast_macd_batch),
        ("TA-Lib", talib_macd_batch),
        ("pandas-ta", pandas_ta_macd_batch),
        ("tulipy", tulipy_macd_batch),
        ("finta", finta_macd_batch),
        ("talipp", talipp_macd_batch),
    ]),
    ("Bollinger(20, 2.0)", [
        ("Wickra", wickra_bollinger_batch),
        ("Wickra (fast)", wickra_fast_bollinger_batch),
        ("TA-Lib", talib_bollinger_batch),
        ("pandas-ta", pandas_ta_bollinger_batch),
        ("tulipy", tulipy_bollinger_batch),
        ("finta", finta_bollinger_batch),
        ("talipp", talipp_bollinger_batch),
    ]),
]

OHLC_INDICATORS = [
    ("ATR(14)", [
        ("Wickra", wickra_atr_batch),
        ("Wickra (fast)", wickra_fast_atr_batch),
        ("TA-Lib", talib_atr_batch),
        ("tulipy", tulipy_atr_batch),
        ("finta", finta_atr_batch),
        ("talipp", talipp_atr_batch),
    ]),
]

STREAMING_INDICATORS = [
    ("SMA(20)", [
        ("Wickra", wickra_sma_streaming),
        ("talipp", talipp_sma_streaming),
        ("TA-Lib", talib_sma_streaming),
        ("pandas-ta", pandas_ta_sma_streaming),
        ("tulipy", tulipy_sma_streaming),
        ("finta", finta_sma_streaming),
    ]),
    ("EMA(20)", [
        ("Wickra", wickra_ema_streaming),
        ("talipp", talipp_ema_streaming),
        ("TA-Lib", talib_ema_streaming),
        ("pandas-ta", pandas_ta_ema_streaming),
        ("tulipy", tulipy_ema_streaming),
        ("finta", finta_ema_streaming),
    ]),
    ("RSI(14)", [
        ("Wickra", wickra_rsi_streaming),
        ("talipp", talipp_rsi_streaming),
        ("TA-Lib", talib_rsi_streaming),
        ("pandas-ta", pandas_ta_rsi_streaming),
        ("tulipy", tulipy_rsi_streaming),
        ("finta", finta_rsi_streaming),
    ]),
    ("MACD(12, 26, 9)", [
        ("Wickra", wickra_macd_streaming),
        ("talipp", talipp_macd_streaming),
        ("TA-Lib", talib_macd_streaming),
        ("pandas-ta", pandas_ta_macd_streaming),
        ("tulipy", tulipy_macd_streaming),
        ("finta", finta_macd_streaming),
    ]),
    ("Bollinger(20, 2.0)", [
        ("Wickra", wickra_bollinger_streaming),
        ("talipp", talipp_bollinger_streaming),
        ("TA-Lib", talib_bollinger_streaming),
        ("pandas-ta", pandas_ta_bollinger_streaming),
        ("tulipy", tulipy_bollinger_streaming),
        ("finta", finta_bollinger_streaming),
    ]),
]


def run_batch(prices: np.ndarray, iterations: int, rounds: int) -> List[Sample]:
    out: List[Sample] = []
    for indicator_name, libs in BATCH_INDICATORS:
        for lib_name, factory in libs:
            runner = factory(prices)
            if runner is None:
                continue
            secs = time_call(runner, iterations, rounds)
            out.append(Sample(lib_name, indicator_name, "batch", secs, iterations))
    return out


def run_ohlc(
    high: np.ndarray,
    low: np.ndarray,
    close: np.ndarray,
    iterations: int,
    rounds: int,
) -> List[Sample]:
    out: List[Sample] = []
    for indicator_name, libs in OHLC_INDICATORS:
        for lib_name, factory in libs:
            runner = factory(high, low, close)
            if runner is None:
                continue
            secs = time_call(runner, iterations, rounds)
            out.append(Sample(lib_name, indicator_name, "batch", secs, iterations))
    return out


def run_streaming(prices: np.ndarray, streaming_window: int, iterations: int, rounds: int) -> List[Sample]:
    out: List[Sample] = []
    seed = prices[:streaming_window]
    live = prices[streaming_window:]
    if len(live) == 0:
        return out
    for indicator_name, libs in STREAMING_INDICATORS:
        for lib_name, factory in libs:
            runner = factory(seed, live)
            if runner is None:
                continue
            secs = time_call(runner, iterations, rounds)
            sample = Sample(lib_name, indicator_name, "streaming", secs, iterations)
            sample.iterations = iterations * len(live)  # per-tick normalization
            out.append(sample)
    return out


def render_table(rows: List[Sample]) -> str:
    if not rows:
        return "(no results)"
    grouped: Dict[str, List[Sample]] = {}
    for r in rows:
        key = f"{r.mode} | {r.indicator}"
        grouped.setdefault(key, []).append(r)

    lines: List[str] = []
    lines.append("")
    lines.append("Reading the tables: lower µs/op = faster. The 'vs Wickra' column says")
    lines.append("how many times slower (or faster) the other library is compared to Wickra.")
    for key, samples in grouped.items():
        baseline = next((s for s in samples if s.library == "Wickra"), samples[0])
        base = baseline.per_iter_us
        lines.append("")
        lines.append(key)
        lines.append("-" * len(key))
        lines.append(
            f"{'library':<14}  {'µs/op':>14}  {'vs Wickra':>22}  {'verdict':<10}"
        )
        winner = min(samples, key=lambda x: x.per_iter_us)
        for s in sorted(samples, key=lambda x: x.per_iter_us):
            ratio = s.per_iter_us / base if base > 0 else float("nan")
            if s.library == "Wickra":
                comparison = "(reference)"
            elif s.per_iter_us > base:
                comparison = f"{ratio:>5.2f}x slower"
            else:
                comparison = f"{base / s.per_iter_us:>5.2f}x faster"
            verdict = "★ winner" if s is winner else ""
            lines.append(
                f"{s.library:<14}  {s.per_iter_us:>14.3f}  {comparison:>22}  {verdict:<10}"
            )
    return "\n".join(lines)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0] if __doc__ else None)
    parser.add_argument("--size", type=int, default=20_000, help="number of prices")
    parser.add_argument("--iterations", type=int, default=20, help="batch repetitions per timing")
    parser.add_argument(
        "--rounds",
        type=int,
        default=5,
        help="batch timing rounds; the median round is reported to damp jitter",
    )
    parser.add_argument(
        "--streaming-window",
        type=int,
        default=5_000,
        help="number of historical prices to seed before the live ticks begin",
    )
    parser.add_argument(
        "--streaming-iterations",
        type=int,
        default=3,
        help="repetitions of the streaming workload (each iteration replays all live ticks)",
    )
    parser.add_argument(
        "--streaming-rounds",
        type=int,
        default=2,
        help="streaming timing rounds; the median round is reported",
    )
    parser.add_argument("--skip-batch", action="store_true", help="skip the batch tables")
    parser.add_argument("--skip-streaming", action="store_true", help="skip the streaming tables")
    return parser.parse_args()


def main() -> None:
    args = parse_args()
    prices = gen_prices(args.size)

    available = []
    if TALIB is not None: available.append("TA-Lib")
    if PANDAS_TA is not None: available.append("pandas-ta")
    if TULIPY is not None: available.append("tulipy")
    if FINTA is not None: available.append("finta")
    if TALIPP is not None: available.append("talipp")
    print(f"Wickra benchmark suite — wickra=v{WICKRA.__version__}")
    print(f"Comparing against: {', '.join(available) if available else '(no peer libraries installed; install [bench] extra)'}")
    print(f"Series length: {args.size}  •  batch iterations: {args.iterations}")
    print(f"Streaming window: {args.streaming_window} seed, {args.size - args.streaming_window} live")

    high, low, close, _ = gen_ohlc(args.size)
    rows: List[Sample] = []
    if not args.skip_batch:
        rows += run_batch(prices, args.iterations, args.rounds)
        rows += run_ohlc(high, low, close, args.iterations, args.rounds)
    if not args.skip_streaming:
        rows += run_streaming(prices, args.streaming_window, args.streaming_iterations, args.streaming_rounds)

    print(render_table(rows))


if __name__ == "__main__":
    main()
