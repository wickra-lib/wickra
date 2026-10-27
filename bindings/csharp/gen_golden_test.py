"""Generate Wickra.Tests/GoldenAllTests.g.cs: a value-parity test that replays
the shared golden input through every one of the 515 C# indicators and checks
output against the Rust reference fixtures g_<Canonical>.csv.
The comparison is a relative tolerance, not bit equality. Every binding calls
into the same Rust core, so 462 of the 515 indicators are bit-identical by
construction; the other 53 reach a transcendental in the platform math library
(`ln`, `sin`, `cos`, `atan`, `exp`), which no mainstream libm rounds correctly
and which differs in the last bit between implementations. Tightening this to an
exact comparison would make the suite fail on a machine whose libm rounds
differently, which is not a defect in Wickra.


Run from repo root:  python bindings/csharp/gen_golden_test.py
"""
import glob
import json
import os
import re

ROOT = os.path.normpath(os.path.join(os.path.dirname(__file__), "..", ".."))
G = os.path.join(ROOT, "testdata", "golden")
GEN = open(os.path.join(ROOT, "bindings", "csharp", "Wickra", "Generated", "Indicators.g.cs"), encoding="utf-8").read()

# Canonical core Indicator::name() per indicator, shared across every binding.
NAMES = json.load(open(os.path.join(G, "names.json")))

# C# constructor parameter types per class.
ctor_types = {}
cur = None
for line in GEN.splitlines():
    m = re.match(r"public sealed class (\w+)", line)
    if m:
        cur = m.group(1)
        continue
    if cur:
        cm = re.match(r"\s*public %s\(([^)]*)\)" % re.escape(cur), line)
        if cm:
            ps = cm.group(1).strip()
            types = [p.strip().rsplit(" ", 1)[0].strip() for p in ps.split(",")] if ps else []
            ctor_types[cur] = types
            cur = None

# Unified archetype + params, keyed by canonical (== C# class name).
spec = {}
for e in json.load(open(os.path.join(G, "scalar_manifest.json"))):
    arch = {"f64": "scalar_f64", "Candle": "scalar_candle", "(f64, f64)": "pairwise"}[e["input"]]
    spec[e["canonical"]] = {"arch": arch, "params": e["params"]}
for e in json.load(open(os.path.join(G, "multi_manifest.json"))):
    arch = {"f64": "multi_f64", "Candle": "multi_candle", "(f64, f64)": "multi_pairwise"}[e["input"]]
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


def lit(value, cstype):
    if cstype == "int":
        return str(int(round(value)))
    if cstype == "uint":
        return f"{int(round(value))}u"
    if cstype == "byte":
        return f"(byte){int(round(value))}"
    f = float(value)
    s = repr(f)
    return s if ("." in s or "e" in s or "E" in s) else s + ".0"


def ctor_call(canon):
    types = ctor_types.get(canon, [])
    vals = spec[canon]["params"]
    args = ", ".join(lit(v, t) for v, t in zip(vals, types))
    return f"new Wickra.{canon}({args})"


def row_lines(canon):
    """The loop body that appends one row for this archetype.

    Emitted once, into a `Drive_<Canonical>` helper both passes call, so the
    value pass and the lifecycle pass cannot drift apart.
    """
    s = spec[canon]
    a = s["arch"]
    L = []
    if a == "scalar_f64":
        L.append("            got.Add(new[] { ind.Update(r[3]) });")
    elif a == "pairwise":
        L.append("            got.Add(new[] { ind.Update(r[3], r[0]) });")
    elif a == "scalar_candle":
        L.append("            got.Add(new[] { ind.Update(r[0], r[1], r[2], r[3], r[4], i) });")
    elif a == "trade":
        L.append("            got.Add(new[] { ind.Update(r[3], r[4], r[3] >= r[0], i) });")
    elif a == "trademid":
        L.append("            got.Add(new[] { ind.Update(r[3], r[4], r[3] >= r[0], i, (r[1] + r[2]) / 2) });")
    elif a == "ob":
        L.append("            var (bp, bs, ap, asz) = ObLists(r);")
        L.append("            got.Add(new[] { ind.Update(bp, bs, ap, asz) });")
    elif a == "deriv":
        L.append("            var d = DerivFields(r);")
        L.append("            got.Add(new[] { ind.Update(d[0], d[1], d[2], d[3], d[4], d[5], d[6], d[7], d[8], d[9], d[10], i) });")
    elif a == "deriv_multi":
        L.append("            var d = DerivFields(r);")
        L.append("            got.Add(FlattenNullable(ind.Update(d[0], d[1], d[2], d[3], d[4], d[5], d[6], d[7], d[8], d[9], d[10], i), %d));" % s["n"])
    elif a == "cross":
        L.append("            var (ch, vo, nh, nl, am, ob) = CrossLists(r);")
        L.append("            got.Add(new[] { ind.Update(ch, vo, nh, nl, am, ob, i) });")
    elif a == "multi_f64":
        L.append("            got.Add(FlattenNullable(ind.Update(r[3]), %d));" % s["n"])
    elif a == "multi_pairwise":
        L.append("            got.Add(FlattenNullable(ind.Update(r[3], r[0]), %d));" % s["n"])
    elif a == "multi_candle":
        L.append("            got.Add(FlattenNullable(ind.Update(r[0], r[1], r[2], r[3], r[4], i), %d));" % s["n"])
    elif a == "profile_bins":
        L.append("            var bins = ind.Update(r[0], r[1], r[2], r[3], r[4], i);")
        L.append("            got.Add(bins ?? NanRow(%d));" % s["width"])
    elif a == "profile_pricebins":
        L.append("            got.Add(FlattenNullable(ind.Update(r[0], r[1], r[2], r[3], r[4], i), %d));" % s["width"])
    elif a == "bars_close":
        L.append("            got.Add(FlattenBars(ind.Update(r[3], r[3], r[3], r[3], 1.0, 0)));")
    elif a == "bars_candle4":
        L.append("            got.Add(FlattenBars(ind.Update(r[0], r[1], r[2], r[3], 1.0, 0)));")
    elif a == "bars_candle5":
        L.append("            got.Add(FlattenBars(ind.Update(r[0], r[1], r[2], r[3], r[4], 0)));")
    elif a == "footprint":
        L.append("            got.Add(FlattenBars(ind.Update(r[3], r[4], r[3] >= r[0], i)));")
    else:
        raise SystemExit("arch " + a)
    return L


def drive_helper(canon):
    """One `Drive_<Canonical>` per indicator: the whole golden input through
    `Update`, rows flattened the way the fixture stores them."""
    L = [f"    private static List<double[]> Drive_{canon}(Wickra.{canon} ind)", "    {"]
    L.append("        var got = new List<double[]>();")
    L.append("        for (var i = 0; i < Rows.Length; i++)")
    L.append("        {")
    L.append("            var r = Rows[i];")
    L.extend(row_lines(canon))
    L.append("        }")
    L.append("        return got;")
    L.append("    }")
    return "\n".join(L)


def block(canon):
    L = ["    [Fact]", f"    public void Golden_{canon}()", "    {"]
    L.append(f"        using var ind = {ctor_call(canon)};")
    L.append(f"        Assert.Equal({json.dumps(NAMES[canon])}, ind.Name());")
    L.append(f'        Compare("{canon}", Drive_{canon}(ind));')
    L.append("    }")
    return "\n".join(L)


def batch_call(canon):
    """The `Batch` call for one archetype, with the whole-series columns."""
    s = spec[canon]
    a = s["arch"]
    if a in ("scalar_f64", "multi_f64"):
        args = "Close"
    elif a in ("pairwise", "multi_pairwise"):
        args = "Close, Open"
    elif a in ("scalar_candle", "multi_candle", "profile_bins", "profile_pricebins", "bars_candle5"):
        args = "Open, High, Low, Close, Volume, Stamps"
    elif a in ("trade", "footprint"):
        args = "Close, Volume, IsBuys, Stamps"
    elif a == "trademid":
        args = "Close, Volume, IsBuys, Stamps, Mids"
    elif a == "ob":
        args = ("FlatOb(0), FlatOb(1), SnapshotWidth, FlatOb(2), FlatOb(3), SnapshotWidth")
    elif a == "cross":
        args = ("FlatCross(0), FlatCross(1), FlatFlags(2), FlatFlags(3), FlatFlags(4), "
                "FlatFlags(5), SnapshotWidth, Stamps")
    elif a in ("deriv", "deriv_multi"):
        args = ", ".join(f"DerivColumn({f})" for f in range(11)) + ", Stamps"
    elif a == "bars_close":
        args = "Close, Close, Close, Close, Ones, Zeros"
    elif a == "bars_candle4":
        args = "Open, High, Low, Close, Ones, Zeros"
    else:
        raise SystemExit("batch arch " + a)
    return f"ind.Batch({args})"


def batch_block(canon):
    """One fact driving `Batch` over the whole series and holding it to the same
    fixture the streaming pass uses."""
    s = spec[canon]
    a = s["arch"]
    L = ["    [Fact]", f"    public void Batch_{canon}()", "    {"]
    L.append(f"        using var ind = {ctor_call(canon)};")
    call = batch_call(canon)
    if a.startswith("bars_"):
        # A bar builder emits an unpredictable number of bars per candle, so its
        # batch is the concatenation of what streaming emits row by row.
        L.append(f'        CompareBatchFlat("{canon}", FlattenBars({call}));')
    elif a == "footprint":
        # Footprint reports the whole book after each trade, so the batch holds
        # the final snapshot rather than every intermediate one.
        L.append(f'        CompareBatchLastRow("{canon}", FlattenBars({call}));')
    elif a == "profile_bins":
        L.append(f'        CompareBatchRows("{canon}", new List<double[]>({call}));')
    elif a in ("multi_f64", "multi_candle", "multi_pairwise", "deriv_multi", "profile_pricebins"):
        L.append(f'        CompareBatchRows("{canon}", RecordRows({call}));')
    else:
        L.append(f'        CompareBatchRows("{canon}", ScalarRows({call}));')
    L.append("    }")
    return "\n".join(L)


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
    """The contract around the values: a fresh indicator is not ready, a driven
    one is, and `Reset` really does return it to the start."""
    a = spec[canon]["arch"]
    # The ten bar builders implement BarBuilder, not Indicator: one candle can
    # complete any number of bars, so they carry no warmup or ready state.
    stateful = not a.startswith("bars_")
    L = ["    [Fact]", f"    public void Lifecycle_{canon}()", "    {"]
    L.append(f"        using var ind = {ctor_call(canon)};")
    if stateful:
        L.append(f'        Assert.False(ind.IsReady(), "{canon}: ready before any input");')
        L.append(f'        Assert.True(ind.WarmupPeriod() >= 1, "{canon}: warmup period must be >= 1");')
    L.append(f"        var first = Drive_{canon}(ind);")
    if stateful and emits(canon):
        L.append(f'        Assert.True(ind.IsReady(), "{canon}: not ready after the whole series, but the fixture has values");')
    L.append("        ind.Reset();")
    if stateful:
        L.append(f'        Assert.False(ind.IsReady(), "{canon}: still ready after Reset");')
    L.append(f'        CompareRuns("{canon}", first, Drive_{canon}(ind));')
    L.append("    }")
    return "\n".join(L)


HEADER = '''// <auto-generated>
// Generated by gen_golden_test.py. DO NOT EDIT.
//
// Value-parity for every one of the 515 C# indicators: the shared golden input
// is replayed through each one and checked against the Rust
// reference fixtures testdata/golden/g_<Canonical>.csv. Multi-output, profile
// and bar shapes are flattened by reflection so one comparator covers all
// archetypes. Regenerate with: python bindings/csharp/gen_golden_test.py
// </auto-generated>
#nullable enable
using System;
using System.Collections.Generic;
using System.Globalization;
using System.IO;
using System.Linq;
using System.Reflection;
using Xunit;

namespace Wickra.Tests;

public class GoldenAllTests
{
    // Two bounds, not one: the loose tolerance exists for indicators that reach a transcendental in the platform math library, whose last bit is not portable. Everything else is built from IEEE-754 arithmetic and is bit-identical everywhere, so it is held to a much tighter bound -- 1e-6 is ten orders looser than the 1-ulp difference it exists for, loose enough to hide a real defect.
    private const double LibmTol = 1e-6;
    private const double ExactTol = 1e-12;

    private static readonly HashSet<string> LibmDependent = LoadLibmDependent();

    private static HashSet<string> LoadLibmDependent()
    {
        var path = Path.Combine(GoldenDir(), "libm_dependent.txt");
        return new HashSet<string>(File.ReadAllLines(path)
            .Select(l => l.Trim())
            .Where(l => l.Length > 0));
    }

    private static double TolFor(string name) =>
        LibmDependent.Contains(name) ? LibmTol : ExactTol;

    private static readonly double[][] Rows = LoadInput();

    private static string GoldenDir([System.Runtime.CompilerServices.CallerFilePath] string file = "") =>
        Path.GetFullPath(Path.Combine(Path.GetDirectoryName(file)!, "..", "..", "..", "testdata", "golden"));

    private static double Cell(string s) =>
        s == "nan" ? double.NaN
        : s == "inf" ? double.PositiveInfinity
        : s == "-inf" ? double.NegativeInfinity
        : double.Parse(s, CultureInfo.InvariantCulture);

    private static double[][] LoadInput()
    {
        var lines = File.ReadAllLines(Path.Combine(GoldenDir(), "input.csv"));
        return lines.Skip(1).Where(l => l.Length > 0)
            .Select(l => l.Split(',').Select(x => double.Parse(x, CultureInfo.InvariantCulture)).ToArray())
            .ToArray();
    }

    // Keep blank lines (a candle on which no bar closed) so rows stay aligned.
    private static double[]?[] ReadFixture(string name)
    {
        var lines = File.ReadAllLines(Path.Combine(GoldenDir(), "g_" + name + ".csv"));
        return lines.Skip(1).Select(l => l.Length == 0 ? Array.Empty<double>() : l.Split(',').Select(Cell).ToArray()).ToArray();
    }

    private static double[] NanRow(int n)
    {
        var r = new double[n];
        for (var i = 0; i < n; i++) r[i] = double.NaN;
        return r;
    }

    private static double[] FlattenStruct(object o)
    {
        var props = o.GetType()
            .GetProperties(BindingFlags.Public | BindingFlags.Instance)
            .OrderBy(p => p.MetadataToken);
        var list = new List<double>();
        foreach (var p in props)
        {
            var v = p.GetValue(o);
            switch (v)
            {
                case double d: list.Add(d); break;
                case float f: list.Add(f); break;
                case long l: list.Add(l); break;
                case int n: list.Add(n); break;
                case double[] arr: list.AddRange(arr); break;
            }
        }
        return list.ToArray();
    }

    private static double[] FlattenNullable<T>(T? value, int width) where T : struct =>
        value.HasValue ? FlattenStruct(value.Value) : NanRow(width);

    private static double[] FlattenBars<T>(T[] bars)
    {
        var list = new List<double>();
        foreach (var bar in bars) list.AddRange(FlattenStruct(bar!));
        return list.ToArray();
    }

    private static double[] DerivFields(double[] r)
    {
        double o = r[0], h = r[1], l = r[2], c = r[3], v = r[4];
        return new[]
        {
            (c - o) / c * 0.01, c, c - 0.5, c + 1.0, v * 10.0, v * 0.6, v * 0.4,
            v * 0.55, v * 0.45, h - c, c - l,
        };
    }

    private static (double[], double[], bool[], bool[], bool[], bool[]) CrossLists(double[] r)
    {
        double o = r[0], c = r[3], v = r[4];
        var change = new double[5];
        var volume = new double[5];
        var nh = new bool[5];
        var nl = new bool[5];
        var am = new bool[5];
        var ob = new bool[5];
        for (var j = 0; j < 5; j++)
        {
            change[j] = (c - o) + j;
            volume[j] = v + j * 10.0;
            nh[j] = j % 2 == 0;
            nl[j] = j % 3 == 0;
            am[j] = j % 2 == 0;
            ob[j] = j % 3 == 0;
        }
        return (change, volume, nh, nl, am, ob);
    }

    private static (double[], double[], double[], double[]) ObLists(double[] r)
    {
        double c = r[3], v = r[4];
        var bp = new double[5];
        var bs = new double[5];
        var ap = new double[5];
        var asz = new double[5];
        for (var k = 0; k < 5; k++)
        {
            var kf = k + 1;
            bp[k] = c - 0.1 * kf;
            bs[k] = v / kf;
            ap[k] = c + 0.1 * kf;
            asz[k] = v * 0.9 / kf;
        }
        return (bp, bs, ap, asz);
    }

    // The whole-series columns, built once from the same per-bar values the
    // streaming pass feeds `Update`.
    private static readonly double[] Open = Column(3, 0);
    private static readonly double[] High = Column(3, 1);
    private static readonly double[] Low = Column(3, 2);
    private static readonly double[] Close = Column(3, 3);
    private static readonly double[] Volume = Column(3, 4);
    private static readonly long[] Stamps = Enumerable.Range(0, Rows.Length).Select(i => (long)i).ToArray();
    private static readonly long[] Zeros = new long[Rows.Length];
    private static readonly double[] Ones = Enumerable.Repeat(1.0, Rows.Length).ToArray();
    private static readonly bool[] IsBuys = Rows.Select(r => r[3] >= r[0]).ToArray();
    private static readonly double[] Mids = Rows.Select(r => (r[1] + r[2]) / 2).ToArray();

    private static double[] Column(int _unused, int field) =>
        Rows.Select(r => r[field]).ToArray();

    private static double[] DerivColumn(int field) =>
        Rows.Select(r => DerivFields(r)[field]).ToArray();

    // The cross-section and order-book batches take one flat array covering the
    // whole series -- bar i occupies [i*width, (i+1)*width) -- plus the width as
    // a separate argument. Both families use a width of 5 here, the same
    // snapshot the streaming pass builds per bar.
    private const int SnapshotWidth = 5;

    private static double[] FlatOb(int which)
    {
        var out_ = new double[Rows.Length * SnapshotWidth];
        for (var i = 0; i < Rows.Length; i++)
        {
            var (bp, bs, ap, asz) = ObLists(Rows[i]);
            var snap = which switch { 0 => bp, 1 => bs, 2 => ap, _ => asz };
            Array.Copy(snap, 0, out_, i * SnapshotWidth, SnapshotWidth);
        }
        return out_;
    }

    private static double[] FlatCross(int which)
    {
        var out_ = new double[Rows.Length * SnapshotWidth];
        for (var i = 0; i < Rows.Length; i++)
        {
            var (ch, vo, _, _, _, _) = CrossLists(Rows[i]);
            Array.Copy(which == 0 ? ch : vo, 0, out_, i * SnapshotWidth, SnapshotWidth);
        }
        return out_;
    }

    private static bool[] FlatFlags(int which)
    {
        var out_ = new bool[Rows.Length * SnapshotWidth];
        for (var i = 0; i < Rows.Length; i++)
        {
            var (_, _, nh, nl, am, ob) = CrossLists(Rows[i]);
            var snap = which switch { 2 => nh, 3 => nl, 4 => am, _ => ob };
            Array.Copy(snap, 0, out_, i * SnapshotWidth, SnapshotWidth);
        }
        return out_;
    }

    private static List<double[]> ScalarRows(double[] flat) =>
        flat.Select(v => new[] { v }).ToList();

    // A multi-output batch returns one struct per bar with NaN fields during
    // warmup, not a nullable, so there is nothing to widen here.
    private static List<double[]> RecordRows<T>(T[] values) =>
        values.Select(v => FlattenStruct(v!)).ToList();

    private static void CompareBatchRows(string name, List<double[]> got) => Compare(name, got);

    // A bar builder's batch is one flat run, so the fixture's rows are
    // concatenated to match.
    private static void CompareBatchFlat(string name, double[] got)
    {
        var want = ReadFixture(name).SelectMany(r => r!).ToArray();
        Assert.True(want.Length == got.Length, $"{name}: batch produced {got.Length} values, fixture has {want.Length}");
        CompareValues(name, got, want);
    }

    // Footprint's batch is the final snapshot, so only the fixture's last
    // non-empty row applies.
    private static void CompareBatchLastRow(string name, double[] got)
    {
        var rows = ReadFixture(name);
        var want = Array.Empty<double>();
        foreach (var r in rows) { if (r!.Length > 0) { want = r; } }
        Assert.True(want.Length == got.Length, $"{name}: batch snapshot {got.Length} values, fixture {want.Length}");
        CompareValues(name, got, want);
    }

    private static void CompareValues(string name, double[] got, double[] want)
    {
        for (var k = 0; k < want.Length; k++)
        {
            var w = want[k];
            if (double.IsNaN(w)) { Assert.True(double.IsNaN(got[k]), $"{name} value {k}: want NaN got {got[k]}"); continue; }
            if (double.IsInfinity(w)) { Assert.True(double.IsInfinity(got[k]) && Math.Sign(got[k]) == Math.Sign(w), $"{name} value {k}: want {w} got {got[k]}"); continue; }
            var tol = TolFor(name) * Math.Max(1.0, Math.Abs(w));
            Assert.True(Math.Abs(got[k] - w) <= tol, $"{name} value {k}: got {got[k]} want {w}");
        }
    }

    // Equality, not tolerance: the same code over the same input in the same
    // process has no reason to differ in a single bit, and a tolerance here
    // would hide exactly the leftover state this is looking for.
    private static void CompareRuns(string name, List<double[]> first, List<double[]> second)
    {
        Assert.True(first.Count == second.Count, $"{name}: {first.Count} rows before Reset, {second.Count} after");
        for (var i = 0; i < first.Count; i++)
        {
            var before = first[i];
            var after = second[i];
            Assert.True(before.Length == after.Length, $"{name} row {i}: {before.Length} values before Reset, {after.Length} after");
            for (var k = 0; k < before.Length; k++)
            {
                if (double.IsNaN(before[k]) && double.IsNaN(after[k])) { continue; }
                Assert.True(before[k] == after[k], $"{name} row {i} col {k}: {before[k]} before Reset, {after[k]} after");
            }
        }
    }

    private static void Compare(string name, List<double[]> got)
    {
        var exp = ReadFixture(name);
        Assert.True(exp.Length == got.Count, $"{name}: {exp.Length} fixture rows vs {got.Count} computed");
        for (var i = 0; i < exp.Length; i++)
        {
            var want = exp[i]!;
            var g = got[i];
            Assert.True(want.Length == g.Length, $"{name} row {i}: arity {g.Length} vs {want.Length}");
            for (var k = 0; k < want.Length; k++)
            {
                var w = want[k];
                if (double.IsNaN(w)) { Assert.True(double.IsNaN(g[k]), $"{name} row {i} col {k}: want NaN got {g[k]}"); continue; }
                if (double.IsInfinity(w)) { Assert.True(double.IsInfinity(g[k]) && Math.Sign(g[k]) == Math.Sign(w), $"{name} row {i} col {k}: want {w} got {g[k]}"); continue; }
                var tol = TolFor(name) * Math.Max(1.0, Math.Abs(w));
                Assert.True(Math.Abs(g[k] - w) <= tol, $"{name} row {i} col {k}: got {g[k]} want {w}");
            }
        }
    }
'''

out = [HEADER]
for canon in canons:
    out.append(drive_helper(canon))
    out.append(block(canon))
    out.append(batch_block(canon))
    out.append(lifecycle_block(canon))
out.append("}")
open(os.path.join(ROOT, "bindings", "csharp", "Wickra.Tests", "GoldenAllTests.g.cs"), "w", encoding="utf-8").write("\n".join(out) + "\n")
print("generated GoldenAllTests.g.cs with", len(canons), "indicators")
