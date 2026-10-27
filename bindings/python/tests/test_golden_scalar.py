"""Generic golden replay: every scalar indicator in scalar_manifest.json is
constructed by its native name with the recorded params, fed the shared golden
input, and checked against the Rust-generated g_<Canonical>.csv.
The comparison is a relative tolerance, not bit equality. Every binding calls
into the same Rust core, so 462 of the 515 indicators are bit-identical by
construction; the other 53 reach a transcendental in the platform math library
(`ln`, `sin`, `cos`, `atan`, `exp`), which no mainstream libm rounds correctly
and which differs in the last bit between implementations. Tightening this to an
exact comparison would make the suite fail on a machine whose libm rounds
differently, which is not a defect in Wickra.


This ties the Python binding to the Rust reference for the whole scalar-output
tranche (not just the seven archetype representatives). Fixtures + manifest are
produced by `cargo run -p wickra-examples --bin gen_golden`.
"""
import json
import math
import os

import numpy as np
import pytest
import wickra as ta

HERE = os.path.dirname(__file__)
GOLDEN = os.path.normpath(os.path.join(HERE, "..", "..", "..", "testdata", "golden"))


# Two bounds, not one. The loose bound exists for indicators that reach a transcendental in the platform math library, whose last bit is not portable; everything else is IEEE-754 arithmetic and is bit-identical everywhere. A blanket 1e-6 is ten orders looser than the 1-ulp difference it exists for, loose enough to hide a real defect.
def _load_libm_dependent():
    with open(os.path.join(GOLDEN, "libm_dependent.txt")) as f:
        return frozenset(line.strip() for line in f if line.strip())


_LIBM_DEPENDENT = _load_libm_dependent()


def golden_tol(canonical):
    """Relative tolerance for one indicator against its golden fixture."""
    return 1e-6 if canonical in _LIBM_DEPENDENT else 1e-12


def _cell(s):
    return math.nan if s == "nan" else float(s)


def _load(name):
    with open(os.path.join(GOLDEN, name + ".csv")) as f:
        return [_cell(line.strip()) for line in f.read().splitlines()[1:] if line.strip()]


def _input():
    rows = []
    with open(os.path.join(GOLDEN, "input.csv")) as f:
        for line in f.read().splitlines()[1:]:
            if line.strip():
                rows.append([float(x) for x in line.split(",")])
    return rows


with open(os.path.join(GOLDEN, "scalar_manifest.json")) as _mf:
    MANIFEST = json.load(_mf)
ROWS = _input()


@pytest.mark.parametrize("spec", MANIFEST, ids=[m["canonical"] for m in MANIFEST])
def test_scalar_matches_golden(spec):
    cls = getattr(ta, spec["native"])
    ind = cls(*spec["params"])
    expected = _load("g_" + spec["canonical"])
    inp = spec["input"]
    for i, (o, h, l, c, v) in enumerate(ROWS):
        if inp == "f64":
            got = ind.update(c)
        elif inp == "Candle":
            got = ind.update((o, h, l, c, v, i))
        else:  # pairwise (f64, f64): generator fed (close, open)
            got = ind.update(c, o)
        want = expected[i]
        got = math.nan if got is None else got
        if math.isnan(want):
            assert math.isnan(got), f"{spec['canonical']} row {i}: want NaN got {got}"
        elif math.isinf(want):
            assert math.isinf(got) and (got > 0) == (want > 0), (
                f"{spec['canonical']} row {i}: got {got} want {want}"
            )
        else:
            tol = golden_tol(spec["canonical"]) * max(1.0, abs(want))
            assert abs(got - want) <= tol, f"{spec['canonical']} row {i}: got {got} want {want}"
