"""Every accepted input container must give the same batch result, bit for bit.

The batch helpers read a one-dimensional buffer (NumPy arrays, ``array.array``,
``memoryview``) with a single copy of its bytes and widen non-``float64`` dtypes
element by element; lists, tuples and exotic buffers take the sequence path.
These tests pin that every one of those routes produces exactly what a plain
Python list of floats produces, and that the error a caller got before is the
error they still get.
"""

from __future__ import annotations

import array
import math

import numpy as np
import pytest

import wickra as ta

HOUR_MS = 3_600_000


def _bits(values) -> list[int]:
    """Bit patterns of a batch result, so ``NaN`` warmups compare equal."""
    return list(np.asarray(values, dtype=np.float64).view(np.int64))


def _sma(prices):
    return ta.SMA(5).batch(prices)


@pytest.fixture(scope="module")
def prices() -> list[float]:
    return [100.0 + math.sin(i * 0.37) * 4.0 + (i % 7) * 0.25 for i in range(64)]


@pytest.fixture(scope="module")
def reference(prices) -> list[int]:
    return _bits(_sma(list(prices)))


@pytest.mark.parametrize(
    "convert",
    [
        pytest.param(lambda p: np.asarray(p, dtype=np.float64), id="numpy-float64"),
        pytest.param(lambda p: array.array("d", p), id="array-d"),
        pytest.param(lambda p: memoryview(array.array("d", p)), id="memoryview-d"),
        pytest.param(lambda p: tuple(p), id="tuple"),
        pytest.param(lambda p: np.asarray(p, dtype=">f8"), id="numpy-float64-other-byte-order"),
        pytest.param(
            lambda p: np.repeat(np.asarray(p, dtype=np.float64), 2)[::2], id="numpy-strided"
        ),
        pytest.param(
            lambda p: np.frombuffer(
                b"\0" + np.asarray(p, dtype=np.float64).tobytes(), dtype=np.float64, offset=1
            ),
            id="numpy-float64-unaligned",
        ),
    ],
)
def test_float64_containers_match_a_list(prices, reference, convert):
    assert _bits(_sma(convert(prices))) == reference


def test_float32_widens_exactly_like_float_conversion(prices):
    f32 = np.asarray(prices, dtype=np.float32)
    want = _bits(_sma([float(x) for x in f32]))
    assert _bits(_sma(f32)) == want
    assert _bits(_sma(array.array("f", f32.tolist()))) == want


@pytest.mark.parametrize(
    "dtype", [np.int8, np.int16, np.int32, np.int64, np.uint8, np.uint16, np.uint32, np.uint64]
)
def test_integer_buffers_widen_like_python_float(dtype):
    values = np.arange(40, dtype=dtype) * 3
    want = _bits(_sma([float(x) for x in values]))
    assert _bits(_sma(values)) == want


def test_large_integers_round_like_python_float():
    # Above 2**53 an int64 has no exact float; the binding must round the way
    # Python's float(int) does (to nearest, ties to even).
    values = np.array([2**62 + i * 7 for i in range(10)], dtype=np.int64)
    want = _bits(ta.SMA(1).batch([float(int(x)) for x in values]))
    assert _bits(ta.SMA(1).batch(values)) == want
    big = np.array([2**64 - 1 - i for i in range(5)], dtype=np.uint64)
    want = _bits(ta.SMA(1).batch([float(int(x)) for x in big]))
    assert _bits(ta.SMA(1).batch(big)) == want


def test_bool_buffer_takes_the_sequence_path():
    flags = np.array([True, False, True, True, False, True, False, False], dtype=bool)
    assert _bits(ta.SMA(2).batch(flags)) == _bits(ta.SMA(2).batch([float(x) for x in flags]))


def test_empty_buffer_gives_an_empty_result():
    assert len(ta.SMA(3).batch(np.array([], dtype=np.float64))) == 0
    assert len(ta.SMA(3).batch(array.array("d"))) == 0


def test_two_dimensional_buffer_is_rejected_as_before():
    with pytest.raises(TypeError):
        ta.SMA(3).batch(np.ones((4, 2)))


def test_non_numeric_input_is_rejected_as_before():
    with pytest.raises(TypeError):
        ta.SMA(3).batch("not a series")


def test_result_is_a_float64_array(prices):
    out = _sma(np.asarray(prices))
    assert isinstance(out, array.array)
    assert out.typecode == "d"
    assert len(out) == len(prices)


@pytest.fixture(scope="module")
def candle_columns():
    n = 96
    t = np.arange(n, dtype=np.float64)
    close = 100.0 + np.sin(t * 0.3) * 5.0
    open_ = close + np.sin(t * 0.5) * 0.5
    high = np.maximum(open_, close) + 1.0
    low = np.minimum(open_, close) - 1.0
    volume = 1000.0 + (t % 24) * 50.0
    timestamp = np.arange(n, dtype=np.int64) * HOUR_MS
    return open_, high, low, close, volume, timestamp


@pytest.mark.parametrize(
    "stamps",
    [
        pytest.param(lambda ts: ts, id="numpy-int64"),
        pytest.param(lambda ts: ts.astype(np.uint64), id="numpy-uint64"),
        pytest.param(lambda ts: array.array("q", ts.tolist()), id="array-q"),
        pytest.param(lambda ts: ts.tolist(), id="list"),
        pytest.param(lambda ts: ts.astype(">i8"), id="numpy-int64-other-byte-order"),
    ],
)
def test_timestamp_containers_match_a_list(candle_columns, stamps):
    *ohlcv, ts = candle_columns
    want = _bits(ta.SessionVwap(0).batch(*ohlcv, ts.tolist()))
    assert _bits(ta.SessionVwap(0).batch(*ohlcv, stamps(ts))) == want


def test_uint64_timestamp_above_i64_max_still_overflows(candle_columns):
    *ohlcv, ts = candle_columns
    too_big = ts.astype(np.uint64)
    too_big[3] = np.uint64(2**63)
    with pytest.raises(OverflowError):
        ta.SessionVwap(0).batch(*ohlcv, too_big)


def test_float_timestamps_are_rejected_as_before(candle_columns):
    *ohlcv, ts = candle_columns
    with pytest.raises(TypeError):
        ta.SessionVwap(0).batch(*ohlcv, ts.astype(np.float64))
