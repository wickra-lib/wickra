"""Build node_manifest.json: for every one of the 514 indicators, record the
native class name, constructor parameter values, the ordered update argument
names (parsed from index.d.ts, each flagged scalar/array) and how to read its
output against the shared g_<Canonical>.csv fixtures. Run from repo root."""
import json
import os
import re

ROOT = os.path.normpath(os.path.join(os.path.dirname(__file__), "..", "..", ".."))
DTS = os.path.join(ROOT, "bindings", "node", "index.d.ts")
GOLDEN = os.path.join(ROOT, "testdata", "golden")


def camel(snake):
    head, *rest = snake.split("_")
    return head + "".join(p[:1].upper() + p[1:] for p in rest)


def parse_args(sig):
    out = []
    for p in sig.split(","):
        p = p.strip()
        if not p:
            continue
        name = p.split(":")[0].strip().rstrip("?")
        typ = p.split(":", 1)[1].strip() if ":" in p else ""
        out.append({"name": name, "array": "Array" in typ})
    return out


dts = open(DTS, encoding="utf-8").read()
node_upd = {}
for name, body in re.findall(r"export declare class (\w+) \{(.*?)\n\}", dts, re.S):
    um = re.search(r"\bupdate\(([^)]*)\)", body)
    node_upd[name] = parse_args(um.group(1)) if um else []

# Two-series (pairwise) natives: the golden feeds (close, open) as (x, y). A pair
# whose update arguments carry domain names instead of x / y gets a `role` per
# argument so the harness knows which column each one is.
pair_natives = {
    e["native"]
    for e in json.load(open(os.path.join(GOLDEN, "scalar_manifest.json")))
    if e["input"] == "(f64, f64)"
}


def with_roles(native, args):
    if native in pair_natives and [a["name"] for a in args] != ["x", "y"]:
        return [dict(a, role=r) for a, r in zip(args, ("x", "y"))]
    return args


# constructor params: reuse the values pinned in the Python-side manifests.
params = {}
for fn in ("scalar_manifest", "multi_manifest"):
    for e in json.load(open(os.path.join(GOLDEN, fn + ".json"))):
        params[e["native"]] = e["params"]
for fam in json.load(open(os.path.join(GOLDEN, "exotic_manifest.json"))).values():
    for e in fam:
        params[e["native"]] = e["params"]
for e in json.load(open(os.path.join(GOLDEN, "profile_manifest.json"))):
    params[e["native"]] = e["params"]
for e in json.load(open(os.path.join(GOLDEN, "bars_manifest.json"))):
    params[e["native"]] = e["params"]

# the 11 hand-written archetypes (separate, non-g_ fixtures) keep their own test;
# everything else is g_<Canonical>.csv. Output shape per native:
multi_specs = {e["native"]: e for e in json.load(open(os.path.join(GOLDEN, "multi_manifest.json")))}
deriv_multi = {e["native"]: e for e in json.load(open(os.path.join(GOLDEN, "exotic_manifest.json")))["deriv"] if "n" in e}
profile_specs = {e["native"]: e for e in json.load(open(os.path.join(GOLDEN, "profile_manifest.json")))}
bars_specs = {e["native"]: e for e in json.load(open(os.path.join(GOLDEN, "bars_manifest.json")))}

# bar output field order (camelCase), mirroring gen_golden's per-bar tuple.
BAR_FIELDS = {
    "RenkoBars": ["open", "close", "direction"],
    "KagiBars": ["start", "end", "direction"],
    "PointAndFigureBars": ["direction", "high", "low"],
    "RangeBars": ["open", "close", "direction"],
    "ThreeLineBreakBars": ["open", "close", "direction"],
    "ImbalanceBars": ["open", "high", "low", "close", "imbalance", "direction"],
    "RunBars": ["open", "high", "low", "close", "length", "direction"],
    "DollarBars": ["open", "high", "low", "close", "volume", "dollar"],
    "TickBars": ["open", "high", "low", "close", "volume"],
    "VolumeBars": ["open", "high", "low", "close", "volume"],
    "Footprint": ["price", "bidVol", "askVol"],
}


def header_fields(canon):
    with open(os.path.join(GOLDEN, "g_" + canon + ".csv"), encoding="utf-8") as f:
        return f.readline().strip().split(",")


# canonical -> native, gathered from every Python-side manifest. The g_ fixture
# is named by canonical (the Rust struct); the Node class is the native alias.
canon_native = {}
for fn in ("scalar_manifest", "multi_manifest"):
    for e in json.load(open(os.path.join(GOLDEN, fn + ".json"))):
        canon_native[e["canonical"]] = e["native"]
for fam in json.load(open(os.path.join(GOLDEN, "exotic_manifest.json"))).values():
    for e in fam:
        canon_native[e["canonical"]] = e["native"]
for e in json.load(open(os.path.join(GOLDEN, "profile_manifest.json"))):
    canon_native[e["canonical"]] = e["native"]
for e in json.load(open(os.path.join(GOLDEN, "bars_manifest.json"))):
    canon_native[e["canonical"]] = e["native"]

out = []
for canon in sorted(canon_native):
    native = canon_native[canon]
    if native not in node_upd:
        raise SystemExit(f"node class for {canon} (native {native}) not found in index.d.ts")
    ctor = params.get(native, [])
    # EaseOfMovement's volume divisor is an optional Rust constructor argument
    # (default 1e8) but a required Node constructor parameter; pass it explicitly.
    if native == "EaseOfMovement":
        ctor = [ctor[0], 100000000.0]
    entry = {"canonical": canon, "native": native, "ctor": ctor, "args": with_roles(native, node_upd[native])}
    if native in bars_specs:
        entry["out"] = "footprint" if native == "Footprint" else "bars"
        entry["fields"] = BAR_FIELDS[native]
    elif native in profile_specs:
        ps = profile_specs[native]
        entry["out"] = "profile_" + ps["kind"]
        entry["width"] = ps["width"]
        if ps["kind"] == "pricebins":
            entry["arrayField"] = "counts" if native == "TpoProfile" else "bins"
    elif native in deriv_multi or native in multi_specs:
        entry["out"] = "multi"
        entry["fields"] = [camel(c) for c in header_fields(canon)]
    else:
        entry["out"] = "scalar"
    out.append(entry)

json.dump(out, open(os.path.join(GOLDEN, "node_manifest.json"), "w"), indent=1)
print("node_manifest entries:", len(out))
from collections import Counter
print(Counter(e["out"] for e in out))
