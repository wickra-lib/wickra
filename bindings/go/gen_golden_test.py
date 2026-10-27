"""Generate golden_all_test.go: a value-parity test that replays the shared
golden input through every one of the 515 Go indicators and checks output
against the Rust-generated g_<Canonical>.csv fixtures.
The comparison is a relative tolerance, not bit equality. Every binding calls
into the same Rust core, so 462 of the 515 indicators are bit-identical by
construction; the other 53 reach a transcendental in the platform math library
(`ln`, `sin`, `cos`, `atan`, `exp`), which no mainstream libm rounds correctly
and which differs in the last bit between implementations. Tightening this to an
exact comparison would make the suite fail on a machine whose libm rounds
differently, which is not a defect in Wickra.


Run from repo root:  python bindings/go/gen_golden_test.py
"""
import glob
import json
import os
import re

ROOT = os.path.normpath(os.path.join(os.path.dirname(__file__), "..", ".."))
G = os.path.join(ROOT, "testdata", "golden")
GEN = open(os.path.join(ROOT, "bindings", "go", "indicators_gen.go"), encoding="utf-8").read()

# Canonical core Indicator::name() per indicator, shared across every binding.
NAMES = json.load(open(os.path.join(G, "names.json")))

# Go constructor parameter types, keyed by canonical (== Go type name).
ctor_types = {}
for m in re.finditer(r"func New(\w+)\(([^)]*)\)\s*\(\*\w+, error\)", GEN):
    name, ps = m.group(1), m.group(2).strip()
    types = []
    if ps:
        for p in ps.split(","):
            p = p.strip()
            _, _, ty = p.partition(" ")
            types.append(ty.strip())
    ctor_types[name] = types

# Unified archetype + params, keyed by canonical.
spec = {}  # canon -> dict(arch, params, width?, n?)
scal = json.load(open(os.path.join(G, "scalar_manifest.json")))
for e in scal:
    inp = e["input"]
    arch = {"f64": "scalar_f64", "Candle": "scalar_candle", "(f64, f64)": "pairwise"}[inp]
    spec[e["canonical"]] = {"arch": arch, "params": e["params"]}
for e in json.load(open(os.path.join(G, "multi_manifest.json"))):
    inp = e["input"]
    arch = {"f64": "multi_f64", "Candle": "multi_candle", "(f64, f64)": "multi_pairwise"}[inp]
    spec[e["canonical"]] = {"arch": arch, "params": e["params"], "n": e["n"]}
ex = json.load(open(os.path.join(G, "exotic_manifest.json")))
for e in ex["deriv"]:
    spec[e["canonical"]] = {"arch": "deriv_multi" if "n" in e else "deriv", "params": e["params"], "n": e.get("n")}
for e in ex["cross"]:
    spec[e["canonical"]] = {"arch": "cross", "params": e["params"]}
for e in ex["trade"]:
    spec[e["canonical"]] = {"arch": "trade", "params": e["params"]}
for e in ex["trademid"]:
    spec[e["canonical"]] = {"arch": "trademid", "params": e["params"]}
for e in ex["ob"]:
    spec[e["canonical"]] = {"arch": "ob", "params": e["params"]}
for e in json.load(open(os.path.join(G, "profile_manifest.json"))):
    spec[e["canonical"]] = {"arch": "profile_" + e["kind"], "params": e["params"], "width": e["width"]}
for e in json.load(open(os.path.join(G, "bars_manifest.json"))):
    arch = "footprint" if e["canonical"] == "Footprint" else "bars_" + e["feed"]
    spec[e["canonical"]] = {"arch": arch, "params": e["params"]}

canons = sorted(os.path.basename(f)[2:-4] for f in glob.glob(os.path.join(G, "g_*.csv")))


def go_param(value, gotype):
    intlike = gotype in ("int", "int32", "int64", "uint", "uintptr", "usize")
    if intlike:
        return str(int(round(value)))
    # float64
    return repr(float(value)) if "." in repr(float(value)) or "e" in repr(float(value)) else f"{float(value)}"


def ctor_call(canon):
    types = ctor_types.get(canon, [])
    vals = spec[canon]["params"]
    args = ", ".join(go_param(v, t) for v, t in zip(vals, types))
    return f"New{canon}({args})"


# Update-call expression + output handling per archetype.
def row_lines(canon, pad):
    """The loop body that fills `got[i]` for one archetype, indented by `pad`.

    Shared by the value pass and the lifecycle pass so the two cannot drift.
    """
    s = spec[canon]
    a = s["arch"]
    lines = []
    if a == "scalar_f64":
        upd = "ind.Update(r[3])"
        lines.append(f"\t\t\tgot[i] = []float64{{{upd}}}")
    elif a == "pairwise":
        lines.append("\t\t\tgot[i] = []float64{ind.Update(r[3], r[0])}")
    elif a == "scalar_candle":
        lines.append("\t\t\tgot[i] = []float64{ind.Update(r[0], r[1], r[2], r[3], r[4], int64(i))}")
    elif a == "trade":
        lines.append("\t\t\tgot[i] = []float64{ind.Update(r[3], r[4], r[3] >= r[0], int64(i))}")
    elif a == "trademid":
        lines.append("\t\t\tgot[i] = []float64{ind.Update(r[3], r[4], r[3] >= r[0], int64(i), (r[1]+r[2])/2)}")
    elif a == "ob":
        lines.append("\t\t\tbp, bs, ap, as_ := obLists(r)")
        lines.append("\t\t\tgot[i] = []float64{ind.Update(bp, bs, ap, as_)}")
    elif a == "deriv":
        lines.append("\t\t\td := derivFields(r)")
        lines.append("\t\t\tgot[i] = []float64{ind.Update(d[0], d[1], d[2], d[3], d[4], d[5], d[6], d[7], d[8], d[9], d[10], int64(i))}")
    elif a == "deriv_multi":
        lines.append("\t\t\td := derivFields(r)")
        lines.append("\t\t\tout, ok := ind.Update(d[0], d[1], d[2], d[3], d[4], d[5], d[6], d[7], d[8], d[9], d[10], int64(i))")
        lines.append(f"\t\t\tgot[i] = reflectRow(out, ok, {s['n']})")
    elif a == "cross":
        lines.append("\t\t\tch, vo, nh, nl, am, ob_ := crossLists(r)")
        lines.append("\t\t\tgot[i] = []float64{ind.Update(ch, vo, nh, nl, am, ob_, int64(i))}")
    elif a in ("multi_f64",):
        lines.append("\t\t\tout, ok := ind.Update(r[3])")
        lines.append(f"\t\t\tgot[i] = reflectRow(out, ok, {s['n']})")
    elif a == "multi_pairwise":
        lines.append("\t\t\tout, ok := ind.Update(r[3], r[0])")
        lines.append(f"\t\t\tgot[i] = reflectRow(out, ok, {s['n']})")
    elif a == "multi_candle":
        lines.append("\t\t\tout, ok := ind.Update(r[0], r[1], r[2], r[3], r[4], int64(i))")
        lines.append(f"\t\t\tgot[i] = reflectRow(out, ok, {s['n']})")
    elif a == "profile_bins":
        lines.append("\t\t\tbins, ok := ind.Update(r[0], r[1], r[2], r[3], r[4], int64(i))")
        # Emitted in the expanded form gofmt produces, so regenerating the
        # file is idempotent and does not fail a format check.
        lines.append("\t\t\tif ok {")
        lines.append("\t\t\t\tgot[i] = bins")
        lines.append("\t\t\t} else {")
        lines.append(f"\t\t\t\tgot[i] = nanRow({s['width']})")
        lines.append("\t\t\t}")
    elif a == "profile_pricebins":
        lines.append("\t\t\tout, ok := ind.Update(r[0], r[1], r[2], r[3], r[4], int64(i))")
        lines.append(f"\t\t\tgot[i] = reflectRow(out, ok, {s['width']})")
    elif a == "bars_close":
        lines.append("\t\t\tgot[i] = flattenBars(ind.Update(r[3], r[3], r[3], r[3], 1.0, 0))")
    elif a == "bars_candle4":
        lines.append("\t\t\tgot[i] = flattenBars(ind.Update(r[0], r[1], r[2], r[3], 1.0, 0))")
    elif a == "bars_candle5":
        lines.append("\t\t\tgot[i] = flattenBars(ind.Update(r[0], r[1], r[2], r[3], r[4], 0))")
    elif a == "footprint":
        lines.append("\t\t\tgot[i] = flattenBars(ind.Update(r[3], r[4], r[3] >= r[0], int64(i)))")
    else:
        raise SystemExit("unknown arch " + a)
    return [pad + line[3:] for line in lines]


def block(canon):
    ctor = ctor_call(canon)
    lines = [f'\tt.Run("{canon}", func(t *testing.T) {{']
    lines.append(f"\t\tind, err := {ctor}")
    lines.append('\t\tif err != nil {')
    lines.append(f'\t\t\tt.Fatalf("new {canon}: %v", err)')
    lines.append("\t\t}")
    lines.append(f'\t\tif n := ind.Name(); n != {json.dumps(NAMES[canon])} {{')
    lines.append(f'\t\t\tt.Errorf("name: got %q want %q", n, {json.dumps(NAMES[canon])})')
    lines.append("\t\t}")
    lines.append("\t\tgot := make([][]float64, len(rows))")
    lines.append("\t\tfor i, r := range rows {")
    lines.extend(row_lines(canon, "\t\t\t"))
    lines.append("\t\t}")
    lines.append(f'\t\tcompareGolden(t, "{canon}", got)')
    lines.append("\t})")
    return "\n".join(lines)


def emits(canon):
    """Whether the reference fixture holds a single finite value.

    A few indicators never emit over this input, so asserting "ready once the
    series is done" would be wrong for them. Deciding it here, from the fixture,
    beats guessing at runtime from a row that might legitimately be NaN.
    """
    with open(os.path.join(G, f"g_{canon}.csv"), encoding="utf-8") as f:
        next(f, None)
        for line in f:
            for cell in line.strip().split(","):
                try:
                    value = float(cell)
                except ValueError:
                    continue
                if value == value and abs(value) != float("inf"):
                    return True
    return False


def lifecycle_block(canon):
    """One subtest for the contract around the values: a fresh indicator is not
    ready, a driven one is, and `Reset` really does return it to the start."""
    a = spec[canon]["arch"]
    # The ten bar builders implement BarBuilder, not Indicator: one candle can
    # complete any number of bars, so they carry no warmup or ready state.
    stateful = not a.startswith("bars_")
    lines = [f'\tt.Run("{canon}", func(t *testing.T) {{']
    lines.append(f"\t\tind, err := {ctor_call(canon)}")
    lines.append('\t\tif err != nil {')
    lines.append(f'\t\t\tt.Fatalf("new {canon}: %v", err)')
    lines.append("\t\t}")
    lines.append("\t\tdrive := func() [][]float64 {")
    lines.append("\t\t\tgot := make([][]float64, len(rows))")
    lines.append("\t\t\tfor i, r := range rows {")
    lines.extend(row_lines(canon, "\t\t\t\t"))
    lines.append("\t\t\t}")
    lines.append("\t\t\treturn got")
    lines.append("\t\t}")
    if stateful:
        lines.append("\t\tif ind.IsReady() {")
        lines.append(f'\t\t\tt.Errorf("{canon}: ready before any input")')
        lines.append("\t\t}")
        lines.append("\t\tif w := ind.WarmupPeriod(); w < 1 {")
        lines.append(f'\t\t\tt.Errorf("{canon}: warmup period %d, want >= 1", w)')
        lines.append("\t\t}")
    lines.append("\t\tfirst := drive()")
    if stateful and emits(canon):
        lines.append("\t\tif !ind.IsReady() {")
        lines.append(f'\t\t\tt.Errorf("{canon}: not ready after the whole series, but the fixture has values")')
        lines.append("\t\t}")
    lines.append("\t\tind.Reset()")
    if stateful:
        lines.append("\t\tif ind.IsReady() {")
        lines.append(f'\t\t\tt.Errorf("{canon}: still ready after Reset")')
        lines.append("\t\t}")
    lines.append(f'\t\tcompareRuns(t, "{canon}", first, drive())')
    lines.append("\t})")
    return "\n".join(lines)


def batch_block(canon: str) -> str:
    """One subtest driving `Batch` over the whole series and comparing it with
    the same fixture the streaming pass uses."""
    s = spec[canon]
    a = s["arch"]
    ctor = f'{canon}({", ".join(go_param(v, "") for v in s["params"])})'
    lines = [f'\tt.Run("{canon}", func(t *testing.T) {{']
    lines.append(f"\t\tind, err := New{ctor}")
    lines.append("\t\tif err != nil {")
    lines.append(f'\t\t\tt.Fatalf("construct {canon}: %v", err)')
    lines.append("\t\t}")
    lines.append("\t\tdefer ind.Close()")
    # The order-book batch takes only its own flattened columns.
    if a != "ob":
        lines.append("\t\tcols := goldenColumns(rows)")

    if a == "scalar_f64":
        lines.append("\t\tgot := scalarRows(ind.Batch(cols.close))")
    elif a == "pairwise":
        lines.append("\t\tgot := scalarRows(ind.Batch(cols.close, cols.open))")
    elif a == "scalar_candle":
        lines.append("\t\tgot := scalarRows(ind.Batch(cols.open, cols.high, cols.low, cols.close, cols.volume, cols.index))")
    elif a == "trade":
        lines.append("\t\tgot := scalarRows(ind.Batch(cols.close, cols.volume, cols.isBuy, cols.index))")
    elif a == "trademid":
        lines.append("\t\tgot := scalarRows(ind.Batch(cols.close, cols.volume, cols.isBuy, cols.index, cols.mid))")
    elif a == "ob":
        lines.append("\t\tob := obColumns(rows)")
        lines.append("\t\tgot := scalarRows(ind.Batch(ob.bidPx, ob.bidSz, obDepth, ob.askPx, ob.askSz, obDepth))")
    elif a == "cross":
        lines.append("\t\tcs := crossColumns(rows)")
        lines.append("\t\tgot := scalarRows(ind.Batch(cs.change, cs.volume, cs.newHigh, cs.newLow, cs.aboveMa, cs.onBuy, crossMembers, cols.index))")
    elif a == "deriv":
        lines.append("\t\td := derivColumns(rows)")
        lines.append("\t\tgot := scalarRows(ind.Batch(d[0], d[1], d[2], d[3], d[4], d[5], d[6], d[7], d[8], d[9], d[10], cols.index))")
    elif a == "deriv_multi":
        lines.append("\t\td := derivColumns(rows)")
        lines.append(f"\t\tgot := structRows(ind.Batch(d[0], d[1], d[2], d[3], d[4], d[5], d[6], d[7], d[8], d[9], d[10], cols.index), {s['n']})")
    elif a == "multi_f64":
        lines.append(f"\t\tgot := structRows(ind.Batch(cols.close), {s['n']})")
    elif a == "multi_pairwise":
        lines.append(f"\t\tgot := structRows(ind.Batch(cols.close, cols.open), {s['n']})")
    elif a == "multi_candle":
        lines.append(f"\t\tgot := structRows(ind.Batch(cols.open, cols.high, cols.low, cols.close, cols.volume, cols.index), {s['n']})")
    elif a == "profile_bins":
        lines.append("\t\tgot := ind.Batch(cols.open, cols.high, cols.low, cols.close, cols.volume, cols.index)")
    elif a == "profile_pricebins":
        lines.append(f"\t\tgot := structRows(ind.Batch(cols.open, cols.high, cols.low, cols.close, cols.volume, cols.index), {s['width']})")
    elif a == "bars_close":
        lines.append("\t\tgot := flattenBars(ind.Batch(cols.close, cols.close, cols.close, cols.close, cols.ones, cols.zeroTs))")
    elif a == "bars_candle4":
        lines.append("\t\tgot := flattenBars(ind.Batch(cols.open, cols.high, cols.low, cols.close, cols.ones, cols.zeroTs))")
    elif a == "bars_candle5":
        lines.append("\t\tgot := flattenBars(ind.Batch(cols.open, cols.high, cols.low, cols.close, cols.volume, cols.zeroTs))")
    elif a == "footprint":
        lines.append("\t\tgot := flattenBars(ind.Batch(cols.close, cols.volume, cols.isBuy, cols.index))")
    else:
        raise SystemExit("unknown arch " + a)

    if a.startswith("bars_"):
        # A bar builder completes an unpredictable number of bars per candle, so
        # the batch is the concatenation of what streaming emits row by row.
        lines.append(f'\t\tcompareGoldenFlat(t, "{canon}", got)')
    elif a == "footprint":
        # Footprint reports the whole book after each trade, so the batch holds
        # the final snapshot rather than every intermediate one.
        lines.append(f'\t\tcompareGoldenLastRow(t, "{canon}", got)')
    else:
        lines.append(f'\t\tcompareGolden(t, "{canon}", got)')
    lines.append("\t})")
    return "\n".join(lines)


HEADER = '''// Code generated by gen_golden_test.py. DO NOT EDIT.
//
// Value-parity for every one of the 515 Go indicators: the shared golden input
// is replayed through each one and checked against the Rust
// reference fixtures testdata/golden/g_<Canonical>.csv. Multi-output, profile
// and bar shapes are flattened by reflection so a single comparator covers all
// archetypes. Regenerate with: python bindings/go/gen_golden_test.py

package wickra

import (
\t"bufio"
\t"math"
\t"os"
\t"reflect"
\t"strings"
\t"testing"
)

// readGoldenRaw keeps blank lines (a candle on which no bar closed) so bar rows
// stay aligned to the input; non-bar fixtures contain no blank lines.
// Two bounds, not one. The loose one exists for indicators that reach a
// transcendental in the platform math library, whose last bit is not portable.
// Everything else is built from IEEE-754 arithmetic and is bit-identical
// everywhere, so it is held to a much tighter bound: 1e-6 is ten orders looser
// than the 1-ulp difference it exists for, loose enough to hide a real defect.
const (
\tlibmTol  = 1e-6
\texactTol = 1e-12
)

var libmDependent = loadLibmDependent()

func loadLibmDependent() map[string]bool {
\tset := map[string]bool{}
\tdata, err := os.ReadFile("../../testdata/golden/libm_dependent.txt")
\tif err != nil {
\t\tpanic("cannot read libm_dependent.txt: " + err.Error())
\t}
\tfor _, line := range strings.Split(string(data), "\\n") {
\t\tif name := strings.TrimSpace(line); name != "" {
\t\t\tset[name] = true
\t\t}
\t}
\treturn set
}

func goldenTolFor(name string) float64 {
\tif libmDependent[name] {
\t\treturn libmTol
\t}
\treturn exactTol
}

func readGoldenRaw(t *testing.T, name string) [][]string {
\tt.Helper()
\tf, err := os.Open("../../testdata/golden/" + name + ".csv")
\tif err != nil {
\t\tt.Fatalf("open %s: %v", name, err)
\t}
\tdefer f.Close()
\tvar rows [][]string
\tsc := bufio.NewScanner(f)
\tsc.Buffer(make([]byte, 0, 1024*1024), 1024*1024)
\tfirst := true
\tfor sc.Scan() {
\t\tline := sc.Text()
\t\tif first {
\t\t\tfirst = false
\t\t\tcontinue
\t\t}
\t\tif line == "" {
\t\t\trows = append(rows, []string{})
\t\t\tcontinue
\t\t}
\t\trows = append(rows, strings.Split(line, ","))
\t}
\treturn rows
}

func nanRow(n int) []float64 {
\tr := make([]float64, n)
\tfor i := range r {
\t\tr[i] = math.NaN()
\t}
\treturn r
}

func reflectRow(out any, ok bool, width int) []float64 {
\tif !ok {
\t\treturn nanRow(width)
\t}
\tv := reflect.ValueOf(out)
\trow := make([]float64, 0, width)
\tfor k := 0; k < v.NumField(); k++ {
\t\trow = appendField(row, v.Field(k))
\t}
\treturn row
}

func appendField(row []float64, f reflect.Value) []float64 {
\tswitch f.Kind() {
\tcase reflect.Float64, reflect.Float32:
\t\treturn append(row, f.Float())
\tcase reflect.Int, reflect.Int8, reflect.Int16, reflect.Int32, reflect.Int64:
\t\treturn append(row, float64(f.Int()))
\tcase reflect.Uint, reflect.Uint8, reflect.Uint16, reflect.Uint32, reflect.Uint64, reflect.Uintptr:
\t\treturn append(row, float64(f.Uint()))
\tcase reflect.Slice:
\t\tfor j := 0; j < f.Len(); j++ {
\t\t\trow = appendField(row, f.Index(j))
\t\t}
\t\treturn row
\tdefault:
\t\treturn row
\t}
}

func flattenBars(bars any) []float64 {
\tv := reflect.ValueOf(bars)
\trow := []float64{}
\tfor i := 0; i < v.Len(); i++ {
\t\tbar := v.Index(i)
\t\tfor k := 0; k < bar.NumField(); k++ {
\t\t\trow = appendField(row, bar.Field(k))
\t\t}
\t}
\treturn row
}

// Synthetic feeds derived from one OHLCV row, identical to gen_golden's Rust
// construction (DerivativesTick / CrossSection / OrderBook).
func derivFields(r []float64) [11]float64 {
\to, h, l, c, v := r[0], r[1], r[2], r[3], r[4]
\treturn [11]float64{
\t\t(c - o) / c * 0.01, // funding_rate
\t\tc,                  // mark_price
\t\tc - 0.5,            // index_price
\t\tc + 1.0,            // futures_price
\t\tv * 10.0,           // open_interest
\t\tv * 0.6,            // long_size
\t\tv * 0.4,            // short_size
\t\tv * 0.55,           // taker_buy_volume
\t\tv * 0.45,           // taker_sell_volume
\t\th - c,              // long_liquidation
\t\tc - l,              // short_liquidation
\t}
}

func crossLists(r []float64) ([]float64, []float64, []bool, []bool, []bool, []bool) {
\to, c, v := r[0], r[3], r[4]
\tchange := make([]float64, 5)
\tvolume := make([]float64, 5)
\tnewHigh := make([]bool, 5)
\tnewLow := make([]bool, 5)
\taboveMa := make([]bool, 5)
\tonBuy := make([]bool, 5)
\tfor j := 0; j < 5; j++ {
\t\tjf := float64(j)
\t\tchange[j] = (c - o) + jf
\t\tvolume[j] = v + jf*10.0
\t\tnewHigh[j] = j%2 == 0
\t\tnewLow[j] = j%3 == 0
\t\taboveMa[j] = j%2 == 0
\t\tonBuy[j] = j%3 == 0
\t}
\treturn change, volume, newHigh, newLow, aboveMa, onBuy
}

func obLists(r []float64) ([]float64, []float64, []float64, []float64) {
\tc, v := r[3], r[4]
\tbidPx := make([]float64, 5)
\tbidSz := make([]float64, 5)
\taskPx := make([]float64, 5)
\taskSz := make([]float64, 5)
\tfor k := 0; k < 5; k++ {
\t\tkf := float64(k + 1)
\t\tbidPx[k] = c - 0.1*kf
\t\tbidSz[k] = v / kf
\t\taskPx[k] = c + 0.1*kf
\t\taskSz[k] = v * 0.9 / kf
\t}
\treturn bidPx, bidSz, askPx, askSz
}

// The per-bar universe and book depth gen_golden uses; the batch entry points
// take them flat, so the stride has to be stated.
const (
	crossMembers = 5
	obDepth      = 5
)

type goldenCols struct {
	open, high, low, close, volume []float64
	mid, ones                      []float64
	isBuy                          []bool
	index, zeroTs                  []int64
}

// goldenColumns lifts the shared input to the column form the batch entry points
// take. The values match what the streaming pass feeds row by row -- including
// the constants the bar archetypes use (volume 1.0, timestamp 0).
func goldenColumns(rows [][]float64) goldenCols {
	n := len(rows)
	c := goldenCols{
		open: make([]float64, n), high: make([]float64, n), low: make([]float64, n),
		close: make([]float64, n), volume: make([]float64, n), mid: make([]float64, n),
		ones:  make([]float64, n),
		isBuy: make([]bool, n), index: make([]int64, n), zeroTs: make([]int64, n),
	}
	for i, r := range rows {
		c.open[i], c.high[i], c.low[i], c.close[i], c.volume[i] = r[0], r[1], r[2], r[3], r[4]
		c.mid[i] = (r[1] + r[2]) / 2
		c.ones[i] = 1.0
		c.isBuy[i] = r[3] >= r[0]
		c.index[i] = int64(i)
	}
	return c
}

func derivColumns(rows [][]float64) [11][]float64 {
	var cols [11][]float64
	for k := range cols {
		cols[k] = make([]float64, len(rows))
	}
	for i, r := range rows {
		d := derivFields(r)
		for k := range cols {
			cols[k][i] = d[k]
		}
	}
	return cols
}

type crossCols struct {
	change, volume                  []float64
	newHigh, newLow, aboveMa, onBuy []bool
}

// crossColumns flattens the per-bar member arrays: bar i occupies
// [i*crossMembers, (i+1)*crossMembers).
func crossColumns(rows [][]float64) crossCols {
	n := len(rows) * crossMembers
	c := crossCols{
		change: make([]float64, n), volume: make([]float64, n),
		newHigh: make([]bool, n), newLow: make([]bool, n),
		aboveMa: make([]bool, n), onBuy: make([]bool, n),
	}
	for i, r := range rows {
		ch, vo, nh, nl, am, ob := crossLists(r)
		copy(c.change[i*crossMembers:], ch)
		copy(c.volume[i*crossMembers:], vo)
		copy(c.newHigh[i*crossMembers:], nh)
		copy(c.newLow[i*crossMembers:], nl)
		copy(c.aboveMa[i*crossMembers:], am)
		copy(c.onBuy[i*crossMembers:], ob)
	}
	return c
}

type obCols struct {
	bidPx, bidSz, askPx, askSz []float64
}

func obColumns(rows [][]float64) obCols {
	n := len(rows) * obDepth
	c := obCols{
		bidPx: make([]float64, n), bidSz: make([]float64, n),
		askPx: make([]float64, n), askSz: make([]float64, n),
	}
	for i, r := range rows {
		bp, bs, ap, as := obLists(r)
		copy(c.bidPx[i*obDepth:], bp)
		copy(c.bidSz[i*obDepth:], bs)
		copy(c.askPx[i*obDepth:], ap)
		copy(c.askSz[i*obDepth:], as)
	}
	return c
}

func scalarRows(values []float64) [][]float64 {
	rows := make([][]float64, len(values))
	for i, v := range values {
		rows[i] = []float64{v}
	}
	return rows
}

// structRows flattens one output struct per bar. A row the indicator did not
// produce already carries NaN in every field, so there is no ok flag to consult.
func structRows(out any, width int) [][]float64 {
	v := reflect.ValueOf(out)
	rows := make([][]float64, v.Len())
	for i := 0; i < v.Len(); i++ {
		row := make([]float64, 0, width)
		elem := v.Index(i)
		for k := 0; k < elem.NumField(); k++ {
			row = appendField(row, elem.Field(k))
		}
		rows[i] = row
	}
	return rows
}

// compareGoldenFlat checks a single concatenated run against every fixture cell
// in order, for the bar builders whose batch cannot be split back into rows.
func compareGoldenFlat(t *testing.T, name string, got []float64) {
	t.Helper()
	exp := readGoldenRaw(t, "g_"+name)
	var want []float64
	for _, row := range exp {
		for _, cell := range row {
			want = append(want, goldenCell(cell))
		}
	}
	if len(want) != len(got) {
		t.Fatalf("%s: %d fixture values vs %d batched", name, len(want), len(got))
	}
	tol := goldenTolFor(name)
	for i := range want {
		if math.Abs(got[i]-want[i]) > tol*math.Max(1.0, math.Abs(want[i])) {
			t.Fatalf("%s value %d: got %v want %v", name, i, got[i], want[i])
		}
	}
}

// compareGoldenLastRow is for Footprint, which reports the whole book after each
// trade: the batch holds the final snapshot, which is the fixture's last row.
func compareGoldenLastRow(t *testing.T, name string, got []float64) {
	t.Helper()
	exp := readGoldenRaw(t, "g_"+name)
	last := exp[len(exp)-1]
	if len(last) != len(got) {
		t.Fatalf("%s: final row has %d values, batch produced %d", name, len(last), len(got))
	}
	tol := goldenTolFor(name)
	for i := range last {
		want := goldenCell(last[i])
		if math.Abs(got[i]-want) > tol*math.Max(1.0, math.Abs(want)) {
			t.Fatalf("%s final value %d: got %v want %v", name, i, got[i], want)
		}
	}
}

func compareGolden(t *testing.T, name string, got [][]float64) {
\tt.Helper()
\texp := readGoldenRaw(t, "g_"+name)
\tif len(exp) != len(got) {
\t\tt.Fatalf("%s: %d fixture rows vs %d computed", name, len(exp), len(got))
\t}
\tfor i := range exp {
\t\tif len(exp[i]) != len(got[i]) {
\t\t\tt.Fatalf("%s row %d: arity %d vs %d", name, i, len(got[i]), len(exp[i]))
\t\t}
\t\tfor k := range exp[i] {
\t\t\twant := goldenCell(exp[i][k])
\t\t\tg := got[i][k]
\t\t\tif math.IsNaN(want) {
\t\t\t\tif !math.IsNaN(g) {
\t\t\t\t\tt.Fatalf("%s row %d col %d: want NaN got %v", name, i, k, g)
\t\t\t\t}
\t\t\t\tcontinue
\t\t\t}
\t\t\tif math.IsInf(want, 0) {
\t\t\t\tif !math.IsInf(g, 0) || (g > 0) != (want > 0) {
\t\t\t\t\tt.Fatalf("%s row %d col %d: want %v got %v", name, i, k, want, g)
\t\t\t\t}
\t\t\t\tcontinue
\t\t\t}
\t\t\ttol := goldenTolFor(name) * math.Max(1.0, math.Abs(want))
\t\t\tif math.Abs(g-want) > tol {
\t\t\t\tt.Fatalf("%s row %d col %d: got %v want %v", name, i, k, g, want)
\t\t\t}
\t\t}
\t}
}

// compareRuns asserts that Reset really put the indicator back where it started:
// the second pass over the same input has to reproduce the first. Equality, not
// tolerance — the same code over the same input on the same machine has no
// reason to differ in a single bit, and a tolerance here would hide exactly the
// leftover state this is looking for.
func compareRuns(t *testing.T, name string, first, second [][]float64) {
\tt.Helper()
\tif len(first) != len(second) {
\t\tt.Fatalf("%s: %d rows before Reset, %d after", name, len(first), len(second))
\t}
\tfor i := range first {
\t\tif len(first[i]) != len(second[i]) {
\t\t\tt.Fatalf("%s row %d: %d values before Reset, %d after", name, i, len(first[i]), len(second[i]))
\t\t}
\t\tfor k := range first[i] {
\t\t\tbefore, after := first[i][k], second[i][k]
\t\t\tif math.IsNaN(before) && math.IsNaN(after) {
\t\t\t\tcontinue
\t\t\t}
\t\t\tif before != after {
\t\t\t\tt.Fatalf("%s row %d col %d: %v before Reset, %v after", name, i, k, before, after)
\t\t\t}
\t\t}
\t}
}

func TestGoldenAll(t *testing.T) {
\trows := goldenInput(t)
'''

# bars need blank-line-preserving fixture reads; reuse readGolden but it skips
# blanks. We need a raw reader for bars and input.
out = [HEADER]
for canon in canons:
    out.append(block(canon))
out.append("}")
out.append("")
out.append("// TestGoldenAllBatch drives the same series through Batch. The streaming pass")
out.append("// above is the only thing the golden suite used to exercise, so a batch that")
out.append("// disagreed with it went unnoticed.")
out.append("func TestGoldenAllBatch(t *testing.T) {")
out.append("\trows := goldenInput(t)")
for canon in canons:
    out.append(batch_block(canon))
out.append("}")
out.append("")
out.append("// TestGoldenAllLifecycle covers what the value passes do not: that a fresh")
out.append("// indicator is not ready, that a driven one is, and that Reset returns it to")
out.append("// the start rather than to something that merely looks like it.")
out.append("func TestGoldenAllLifecycle(t *testing.T) {")
out.append("\trows := goldenInput(t)")
for canon in canons:
    out.append(lifecycle_block(canon))
out.append("}")
open(os.path.join(ROOT, "bindings", "go", "golden_all_test.go"), "w", encoding="utf-8").write("\n".join(out) + "\n")
print("generated golden_all_test.go with", len(canons), "indicators")
