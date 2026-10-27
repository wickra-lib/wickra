"""Canonical-name parity: every one of the 515 indicators must report the same
``Indicator::name()`` the Rust core defines — the identical string every other
binding returns. Names are recorded in ``testdata/golden/names.json`` (keyed by
the Rust canonical); the construction recipe (native class + ctor args) is the
shared ``node_manifest.json``. The core name may differ from the registered
class name (e.g. ``ChaikinMoneyFlow`` -> ``"CMF"``), which is exactly what this
pins across languages.
"""
import json
import os

import pytest
import wickra as ta

HERE = os.path.dirname(__file__)
GOLDEN = os.path.normpath(os.path.join(HERE, "..", "..", "..", "testdata", "golden"))

with open(os.path.join(GOLDEN, "node_manifest.json")) as _mf:
    MANIFEST = json.load(_mf)
with open(os.path.join(GOLDEN, "names.json")) as _nf:
    NAMES = json.load(_nf)


@pytest.mark.parametrize("spec", MANIFEST, ids=[m["canonical"] for m in MANIFEST])
def test_name_matches_core(spec):
    cls = getattr(ta, spec["native"])
    ind = cls(*spec["ctor"])
    assert ind.name() == NAMES[spec["canonical"]]
