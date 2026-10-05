"""Generate wasm_manifest.json for the WASM golden replay: for every one of the
514 indicators, record the JS class name, constructor params, the ordered update
argument names (parsed from pkg/wickra_wasm.d.ts, each flagged array/bigint) and
the output archetype (from golden_manifest.json). Run from repo root after
`wasm-pack build --target nodejs`:  python bindings/wasm/gen_golden_test.py
"""
import json
import os
import re

ROOT = os.path.normpath(os.path.join(os.path.dirname(__file__), "..", ".."))
G = os.path.join(ROOT, "testdata", "golden")
DTS = os.path.join(ROOT, "bindings", "wasm", "pkg", "wickra_wasm.d.ts")

# canonical -> native (JS class) and canonical -> (arch, n/width) from the manifests.
native = {}
arch = {}
extra = {}
for e in json.load(open(os.path.join(G, "scalar_manifest.json"))):
    native[e["canonical"]] = e["native"]
    arch[e["canonical"]] = {"f64": "scalar_f64", "Candle": "scalar_candle", "(f64, f64)": "pairwise"}[e["input"]]
for e in json.load(open(os.path.join(G, "multi_manifest.json"))):
    native[e["canonical"]] = e["native"]
    arch[e["canonical"]] = {"f64": "multi_f64", "Candle": "multi_candle", "(f64, f64)": "multi_pairwise"}[e["input"]]
    extra[e["canonical"]] = {"n": e["n"]}
ex = json.load(open(os.path.join(G, "exotic_manifest.json")))
for e in ex["deriv"]:
    native[e["canonical"]] = e["native"]
    arch[e["canonical"]] = "deriv_multi" if "n" in e else "deriv"
    if "n" in e:
        extra[e["canonical"]] = {"n": e["n"]}
for fam, a in (("cross", "cross"), ("trade", "trade"), ("trademid", "trademid"), ("ob", "ob")):
    for e in ex[fam]:
        native[e["canonical"]] = e["native"]
        arch[e["canonical"]] = a
for e in json.load(open(os.path.join(G, "profile_manifest.json"))):
    native[e["canonical"]] = e["native"]
    arch[e["canonical"]] = "profile_" + e["kind"]
    extra[e["canonical"]] = {"width": e["width"], **({"arrayField": "counts" if e["canonical"] == "TpoProfile" else "bins"} if e["kind"] == "pricebins" else {})}
for e in json.load(open(os.path.join(G, "bars_manifest.json"))):
    native[e["canonical"]] = e["native"]
    arch[e["canonical"]] = "footprint" if e["canonical"] == "Footprint" else "bars"

# params per canonical (constructor values) — same as the other bindings.
params = {}
for fn in ("scalar_manifest", "multi_manifest"):
    for e in json.load(open(os.path.join(G, fn + ".json"))):
        params[e["canonical"]] = e["params"]
for fam in json.load(open(os.path.join(G, "exotic_manifest.json"))).values():
    for e in fam:
        params[e["canonical"]] = e["params"]
for e in json.load(open(os.path.join(G, "profile_manifest.json"))):
    params[e["canonical"]] = e["params"]
for e in json.load(open(os.path.join(G, "bars_manifest.json"))):
    params[e["canonical"]] = e["params"]

# parse update args per JS class from the wasm .d.ts
dts = open(DTS, encoding="utf-8").read()
cls_args = {}
for m in re.finditer(r"export class (\w+) \{(.*?)\n\}", dts, re.S):
    name, body = m.group(1), m.group(2)
    um = re.search(r"\bupdate\(([^)]*)\)", body)
    args = []
    if um and um.group(1).strip():
        for p in um.group(1).split(","):
            p = p.strip()
            nm = p.split(":")[0].strip()
            typ = p.split(":", 1)[1].strip() if ":" in p else ""
            args.append({"name": nm, "array": "Array" in typ, "bigint": "bigint" in typ})
    cls_args[name] = args

out = []
for canon in sorted(native):
    js = native[canon]
    if js not in cls_args:
        raise SystemExit(f"WASM class {js} (for {canon}) not in d.ts")
    ctor = params.get(canon, [])
    # EaseOfMovement's volume divisor is an optional Rust constructor argument
    # (default 1e8) but a required WASM constructor parameter; pass it explicitly.
    if canon == "EaseOfMovement":
        ctor = [ctor[0], 100000000.0]
    args = cls_args[js]
    # A pair whose update arguments carry domain names instead of x / y gets a
    # `role` per argument: the golden feeds (close, open) as (x, y).
    if arch[canon] == "pairwise" and [a["name"] for a in args] != ["x", "y"]:
        args = [dict(a, role=r) for a, r in zip(args, ("x", "y"))]
    e = {"canonical": canon, "js": js, "ctor": ctor,
         "args": args, "out": arch[canon]}
    e.update(extra.get(canon, {}))
    out.append(e)

json.dump(out, open(os.path.join(G, "wasm_manifest.json"), "w"), indent=1)
from collections import Counter
print("wasm_manifest:", len(out), dict(Counter(e["out"] for e in out)))
