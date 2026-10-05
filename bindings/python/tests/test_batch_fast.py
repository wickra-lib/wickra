"""``batch_fast`` is the opt-in fast batch.

Where an indicator has a SIMD kernel, the kernel reassociates the arithmetic,
so each value agrees with ``batch`` to within a few units in the last place
rather than bit for bit; NaN placement and length are identical. Without a
kernel ``batch_fast`` is ``batch`` exactly. These tests hold every class that
exposes it to that contract.
"""

from __future__ import annotations

import array
import math

import numpy as np
import pytest

import wickra as ta

N = 1003
CLOSE = [100 + math.sin(i * 0.0137) * 5 + math.cos(i * 0.37) for i in range(N)]
OTHER = [c * 0.5 + 3 + math.sin(i * 0.11) for i, c in enumerate(reversed(CLOSE))]
COLUMNS = {
    "prices": CLOSE,
    "x": CLOSE,
    "y": OTHER,
    "high": [c + 1 for c in CLOSE],
    "low": [c - 1 for c in CLOSE],
    "close": CLOSE,
    "volume": [1000.0 + i % 7 for i in range(N)],
    "asset": CLOSE,
    "benchmark": OTHER,
}
# Constructor arguments where "14 for every required parameter" is not valid.
CONSTRUCT = {"DecyclerOscillator": (10, 20)}
# The batch columns of the indicators whose batch is not one price series.
BATCH_COLUMNS = {
    "ATR": ("high", "low", "close"),
    "ChaikinOscillator": ("high", "low", "close", "volume"),
    "PearsonCorrelation": ("x", "y"),
}

FAST_CLASSES = sorted(n for n in dir(ta) if hasattr(getattr(ta, n), "batch_fast"))


def _build(name: str):
    """The class with 14 for each required argument (they come first), found by
    trying: the abi3 wheels expose no constructor signature to `inspect` before
    Python 3.10."""
    cls = getattr(ta, name)
    if name in CONSTRUCT:
        return cls(*CONSTRUCT[name])
    for required in range(5):
        try:
            return cls(*([14] * required))
        except TypeError:
            continue
    raise AssertionError(f"no constructor arguments found for {name}")


def _flat(result) -> list[float]:
    if hasattr(result, "shape"):
        return [v for row in result.tolist() for v in row]
    return list(result)


def _assert_within(exact, fast, tol: float) -> None:
    exact, fast = _flat(exact), _flat(fast)
    assert len(exact) == len(fast)
    for i, (x, y) in enumerate(zip(exact, fast)):
        assert math.isnan(x) == math.isnan(y), f"NaN mismatch at {i}"
        if math.isinf(x):
            assert x == y, f"at {i}: {x} vs {y}"
        elif not math.isnan(x):
            assert abs(x - y) <= tol * max(1.0, abs(x)), f"at {i}: {x} vs {y}"


def test_every_scalar_batch_has_a_fast_twin():
    # The C ABI exports 156 `_batch_fast` entry points; Python mirrors them.
    assert len(FAST_CLASSES) == 156


@pytest.mark.parametrize("name", FAST_CLASSES)
def test_batch_fast_agrees_with_batch(name):
    exact, fast = _build(name), _build(name)
    cols = [COLUMNS[p] for p in BATCH_COLUMNS.get(name, ("prices",))]
    _assert_within(exact.batch(*cols), fast.batch_fast(*cols), 1e-11)


@pytest.mark.parametrize("name", ["SMA", "EMA", "WMA", "HMA", "SMMA", "DEMA", "TEMA", "RSI"])
def test_state_after_batch_fast_keeps_streaming(name):
    exact, fast = _build(name), _build(name)
    exact.batch(CLOSE)
    fast.batch_fast(CLOSE)
    for price in (101.0, 99.5, 100.25):
        a, b = exact.update(price), fast.update(price)
        assert a is not None and b is not None
        assert abs(a - b) <= 1e-11 * max(1.0, abs(a))


def test_without_a_kernel_batch_fast_is_the_exact_batch():
    want = ta.ROC(10).batch(CLOSE)
    got = ta.ROC(10).batch_fast(CLOSE)
    assert [x.hex() for x in want] == [x.hex() for x in got]


@pytest.mark.parametrize(
    "wrap",
    [
        pytest.param(list, id="list"),
        pytest.param(lambda p: array.array("d", p), id="array-d"),
        pytest.param(lambda p: np.asarray(p, dtype=np.float64), id="numpy"),
        pytest.param(lambda p: np.asarray(p, dtype=np.float32), id="numpy-f32"),
    ],
)
def test_batch_fast_accepts_every_series_type(wrap):
    series = wrap(CLOSE)
    want = ta.EMA(20).batch_fast(np.asarray(series, dtype=np.float64))
    assert [x.hex() for x in ta.EMA(20).batch_fast(series)] == [x.hex() for x in want]


def test_batch_fast_on_empty_input():
    assert len(ta.EMA(5).batch_fast([])) == 0
    assert ta.MACD().batch_fast([]).shape == (0, 3)
    assert ta.MACDFIX(9).batch_fast([]).shape == (0, 3)


def test_multi_output_batch_fast_has_the_batch_shape():
    assert ta.MACD().batch_fast(CLOSE).shape == ta.MACD().batch(CLOSE).shape == (N, 3)
    assert ta.MACDFIX(9).batch_fast(CLOSE).shape == ta.MACDFIX(9).batch(CLOSE).shape == (N, 3)
    assert ta.BollingerBands().batch_fast(CLOSE).shape == (N, 4)


def test_column_batches_reject_mismatched_lengths():
    with pytest.raises(ValueError):
        ta.PearsonCorrelation(5).batch_fast(CLOSE, CLOSE[:-1])
    with pytest.raises(ValueError):
        ta.ATR(14).batch_fast(COLUMNS["high"], COLUMNS["low"], CLOSE[:-1])
    with pytest.raises(ValueError):
        ta.ChaikinOscillator(3, 10).batch_fast(
            COLUMNS["high"], COLUMNS["low"], CLOSE, COLUMNS["volume"][:-1]
        )


@pytest.mark.parametrize("method", ["batch", "batch_fast"])
def test_chaikin_rejects_an_invalid_bar_before_touching_state(method):
    high = list(COLUMNS["high"])
    high[500] = COLUMNS["low"][500] - 1.0  # high below low
    ind = ta.ChaikinOscillator(3, 10)
    with pytest.raises(ValueError):
        getattr(ind, method)(high, COLUMNS["low"], CLOSE, COLUMNS["volume"])
    # Nothing was consumed: the indicator still matches a fresh one.
    fresh = ta.ChaikinOscillator(3, 10)
    cols = (COLUMNS["high"], COLUMNS["low"], CLOSE, COLUMNS["volume"])
    assert [x.hex() for x in ind.batch(*cols)] == [x.hex() for x in fresh.batch(*cols)]


def test_chaikin_and_pearson_exact_batch_match_streaming():
    ind = ta.ChaikinOscillator(3, 10)
    cols = (COLUMNS["high"], COLUMNS["low"], CLOSE, COLUMNS["volume"])
    batch = ind.batch(*cols)
    streamer = ta.ChaikinOscillator(3, 10)
    for i in range(N):
        v = streamer.update((CLOSE[i], cols[0][i], cols[1][i], CLOSE[i], cols[3][i], 0))
        assert (v is None and math.isnan(batch[i])) or v.hex() == batch[i].hex()

    pair = ta.PearsonCorrelation(20).batch(CLOSE, OTHER)
    streamer = ta.PearsonCorrelation(20)
    for i in range(N):
        v = streamer.update(CLOSE[i], OTHER[i])
        assert (v is None and math.isnan(pair[i])) or v.hex() == pair[i].hex()
