"""Generate examples/c/golden_test.c: a value-parity test that replays the shared
golden input through every one of the 515 indicators via the C ABI (wickra.h)
and checks output bit-for-bit against the Rust reference fixtures
g_<Canonical>.csv. The same source compiles under both a C compiler (the C
binding) and a C++ compiler (the C++ binding) — wickra.h is `extern "C"`.

Run from repo root:  python examples/c/gen_golden_test.py
"""
import json
import os
import re

ROOT = os.path.normpath(os.path.join(os.path.dirname(__file__), "..", ".."))
G = os.path.join(ROOT, "testdata", "golden")
HDR = open(os.path.join(ROOT, "bindings", "c", "include", "wickra.h"), encoding="utf-8").read()

# Canonical core Indicator::name() per indicator, shared across every binding.
NAMES = json.load(open(os.path.join(G, "names.json")))

# canonical -> C prefix, from the R wrappers (.wk_obj first arg == C symbol prefix).
RSRC = open(os.path.join(ROOT, "bindings", "r", "R", "indicators.R"), encoding="utf-8").read()
PREFIX = {m.group(1): m.group(2)
          for m in re.finditer(r"^(\w+) <- function\([^)]*\) \{.*?\.wk_obj\(\"([^\"]+)\"", RSRC, re.S | re.M)}

# wickra_* function signatures (return type, args), multiline-collapsed.
SIG = {}
for m in re.finditer(r"([A-Za-z_][\w ]*\*?)\s*(wickra_\w+)\(([^;]*?)\);", HDR, re.S):
    SIG[m.group(2)] = (re.sub(r"\s+", " ", m.group(1)).strip(), re.sub(r"\s+", " ", m.group(3)).strip())

# archetype + n/width per canonical
spec = {}
for e in json.load(open(os.path.join(G, "scalar_manifest.json"))):
    spec[e["canonical"]] = {"arch": {"f64": "scalar_f64", "Candle": "scalar_candle", "(f64, f64)": "pairwise"}[e["input"]], "params": e["params"]}
for e in json.load(open(os.path.join(G, "multi_manifest.json"))):
    spec[e["canonical"]] = {"arch": {"f64": "multi_f64", "Candle": "multi_candle", "(f64, f64)": "multi_pairwise"}[e["input"]], "params": e["params"], "n": e["n"]}
ex = json.load(open(os.path.join(G, "exotic_manifest.json")))
for e in ex["deriv"]:
    spec[e["canonical"]] = {"arch": "deriv_multi" if "n" in e else "deriv", "params": e["params"], **({"n": e["n"]} if "n" in e else {})}
for fam, a in (("cross", "cross"), ("trade", "trade"), ("trademid", "trademid"), ("ob", "ob")):
    for e in ex[fam]:
        spec[e["canonical"]] = {"arch": a, "params": e["params"]}
for e in json.load(open(os.path.join(G, "profile_manifest.json"))):
    spec[e["canonical"]] = {"arch": "profile_" + e["kind"], "params": e["params"], "width": e["width"]}
for e in json.load(open(os.path.join(G, "bars_manifest.json"))):
    spec[e["canonical"]] = {"arch": "footprint" if e["canonical"] == "Footprint" else "bars_" + e["feed"], "params": e["params"]}

canons = sorted(os.path.basename(f)[2:-4] for f in __import__("glob").glob(os.path.join(G, "g_*.csv")))

BAR_FIELDS = {
    "RenkoBars": ["open", "close", "direction"], "KagiBars": ["start", "end", "direction"],
    "PointAndFigureBars": ["direction", "high", "low"], "RangeBars": ["open", "close", "direction"],
    "ThreeLineBreakBars": ["open", "close", "direction"],
    "ImbalanceBars": ["open", "high", "low", "close", "imbalance", "direction"],
    "RunBars": ["open", "high", "low", "close", "length", "direction"],
    "DollarBars": ["open", "high", "low", "close", "volume", "dollar"],
    "TickBars": ["open", "high", "low", "close", "volume"],
    "VolumeBars": ["open", "high", "low", "close", "volume"],
    "Footprint": ["price", "bid_vol", "ask_vol"],
}


# cbindgen appends '_' to struct fields that collide with C/C++ reserved words.
_C_RESERVED = {"long", "short", "int", "char", "float", "double", "new", "class",
               "this", "delete", "register", "auto", "const", "void"}


def c_field(name):
    return name + "_" if name in _C_RESERVED else name


def csv_header(canon):
    with open(os.path.join(G, "g_" + canon + ".csv"), encoding="utf-8") as f:
        return f.readline().strip().split(",")


def out_struct(prefix):
    """The pointer-to-struct out param type of wickra_<prefix>_update, if any."""
    _, args = SIG["wickra_" + prefix + "_update"]
    m = re.search(r"struct (\w+) \*out", args)
    if m:
        return "struct " + m.group(1)
    m = re.search(r"struct (\w+) \*scalars", args)
    if m:
        return "struct " + m.group(1)
    return None


def ctor_casts(prefix, params):
    _, args = SIG["wickra_" + prefix + "_new"]
    if not args:
        return ""
    types = [a.strip().rsplit(" ", 1)[0].strip() for a in args.split(",")]
    out = []
    for t, v in zip(types, params):
        if t in ("uintptr_t", "intptr_t", "size_t"):
            out.append(f"(uintptr_t){int(round(v))}")
        elif t in ("uint8_t",):
            out.append(f"(uint8_t){int(round(v))}")
        elif t in ("int32_t",):
            out.append(f"(int32_t){int(round(v))}")
        elif t in ("int64_t",):
            out.append(f"(int64_t){int(round(v))}")
        else:
            out.append(repr(float(v)))
    return ", ".join(out)


def gen_check(canon):
    s = spec[canon]
    p = PREFIX[canon]
    a = s["arch"]
    new = f"wickra_{p}_new({ctor_casts(p, s['params'])})"
    upd = f"wickra_{p}_update"
    L = [f"static int check_{canon}(void) {{",
         f"    struct {struct_name(p)} *h = {new};",
         f'    if (!h) {{ printf("FAIL {canon}: new returned NULL\\n"); return 1; }}',
         f"    double **exp; int rows = read_fixture(\"g_{canon}\", &exp);",
         "    int fails = 0;",
         f'    {{ const char *nm = wickra_{p}_name(h);',
         f'      if (!nm || strcmp(nm, {json.dumps(NAMES[canon])}) != 0) {{ printf("FAIL {canon}: name %s\\n", nm ? nm : "(null)"); fails++; }} }}',
         "    for (int i = 0; i < N_INPUT; i++) {",
         "        double o = IN[i][0], hi = IN[i][1], lo = IN[i][2], c = IN[i][3], v = IN[i][4];",
         "        (void)o; (void)hi; (void)lo; (void)c; (void)v;",
         "        double got[128]; int gn = 0;"]
    if a in ("scalar_f64", "multi_f64"):
        call_args = "h, c"
    elif a in ("pairwise", "multi_pairwise"):
        call_args = "h, c, o"
    elif a in ("scalar_candle", "multi_candle") or a.startswith("profile"):
        call_args = "h, o, hi, lo, c, v, (int64_t)i"
    elif a == "trade":
        call_args = "h, c, v, c >= o, (int64_t)i"
    elif a == "trademid":
        call_args = "h, c, v, c >= o, (int64_t)i, (hi + lo) / 2.0"
    elif a == "ob":
        L.append("        double bp[5], bs[5], ap[5], asz[5];")
        L.append("        for (int k = 0; k < 5; k++) { double kf = k + 1; bp[k] = c - 0.1*kf; bs[k] = v/kf; ap[k] = c + 0.1*kf; asz[k] = v*0.9/kf; }")
        call_args = "h, bp, bs, 5, ap, asz, 5"
    elif a == "cross":
        L.append("        double chg[5], vol[5]; bool nh[5], nl[5], am[5], ob[5];")
        L.append("        for (int j = 0; j < 5; j++) { chg[j] = (c-o)+j; vol[j] = v + j*10.0; nh[j] = (j%2==0); nl[j] = (j%3==0); am[j] = (j%2==0); ob[j] = (j%3==0); }")
        call_args = "h, chg, vol, nh, nl, am, ob, 5, (int64_t)i"
    elif a in ("deriv", "deriv_multi"):
        L.append("        double fr=(c-o)/c*0.01, mp=c, ip=c-0.5, fp=c+1.0, oi=v*10.0, ls=v*0.6, ss=v*0.4, tbv=v*0.55, tsv=v*0.45, ll=hi-c, sl=c-lo;")
        call_args = "h, fr, mp, ip, fp, oi, ls, ss, tbv, tsv, ll, sl, (int64_t)i"
    elif a == "bars_close":
        call_args = "h, c, c, c, c, 1.0, 0"
    elif a == "bars_candle4":
        call_args = "h, o, hi, lo, c, 1.0, 0"
    elif a == "bars_candle5":
        call_args = "h, o, hi, lo, c, v, 0"
    elif a == "footprint":
        call_args = "h, c, v, c >= o, (int64_t)i"
    else:
        raise SystemExit("arch " + a)

    # output handling
    if a in ("scalar_f64", "scalar_candle", "pairwise", "trade", "trademid", "ob", "cross", "deriv"):
        L.append(f"        got[gn++] = {upd}({call_args});")
    elif a in ("multi_f64", "multi_candle", "multi_pairwise", "deriv_multi"):
        st = out_struct(p)
        fields = csv_header(canon)
        L.append(f"        {st} out;")
        L.append(f"        if ({upd}({call_args}, &out)) {{")
        for f in fields:
            L.append(f"            got[gn++] = out.{c_field(f)};")
        L.append("        } else {")
        L.append(f"            for (int z = 0; z < {len(fields)}; z++) got[gn++] = NANV;")
        L.append("        }")
    elif a == "profile_bins":
        w = s["width"]
        L.append("        double vbuf[256];")
        L.append(f"        intptr_t k = {upd}({call_args}, vbuf, 256);")
        L.append(f"        if (k < 0) {{ for (int z = 0; z < {w}; z++) got[gn++] = NANV; }}")
        L.append(f"        else {{ for (int z = 0; z < {w}; z++) got[gn++] = vbuf[z]; }}")
    elif a == "profile_pricebins":
        w = s["width"]
        st = out_struct(p)
        L.append("        double vbuf[256];")
        L.append(f"        {st} sc;")
        L.append(f"        intptr_t k = {upd}({call_args}, &sc, vbuf, 256);")
        L.append(f"        if (k < 0) {{ for (int z = 0; z < {w}; z++) got[gn++] = NANV; }}")
        L.append("        else { got[gn++] = sc.price_low; got[gn++] = sc.price_high;")
        L.append(f"            for (int z = 0; z < {w - 2}; z++) got[gn++] = vbuf[z]; }}")
    else:  # bars_* / footprint
        elem = out_struct(p)
        fields = BAR_FIELDS[canon]
        cap = 256
        L.append(f"        {elem} bbuf[{cap}];")
        if a == "footprint":
            L.append(f"        intptr_t k = {upd}({call_args}, bbuf, {cap});")
            L.append("        if (k < 0) k = 0;")
        else:
            L.append(f"        uintptr_t k = {upd}({call_args}, bbuf, {cap});")
        L.append("        for (uintptr_t b = 0; b < (uintptr_t)k; b++) {")
        for f in fields:
            L.append(f"            got[gn++] = (double)bbuf[b].{f};")
        L.append("        }")

    L.append("        fails += cmp_row(\"" + canon + "\", i, exp[i], EXPLEN[i], got, gn);")
    L.append("    }")
    L.append("    free_fixture(exp, rows);")
    L.append(f"    wickra_{p}_free(h);")
    L.append("    return fails;")
    L.append("}")
    return "\n".join(L)


def struct_name(prefix):
    ret, _ = SIG["wickra_" + prefix + "_new"]
    m = re.search(r"struct (\w+)", ret)
    return m.group(1)


HEADER = r'''/* Generated by gen_golden_test.py. DO NOT EDIT.
 *
 * Value-parity for the whole 515-indicator catalogue through the Wickra C ABI.
 * The same source compiles as C (gcc) and C++ (g++) since wickra.h is extern "C".
 * Each indicator replays the shared golden input and is checked bit-for-bit
 * against the Rust reference fixtures testdata/golden/g_<Canonical>.csv. */
#include <math.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include "wickra.h"

#define NANV (nan(""))
#define MAXROWS 512
#define MAXCOLS 256

static double IN[MAXROWS][8];
static int N_INPUT = 0;
static int EXPLEN[MAXROWS];

static const char *GDIR = NULL;

static double parse_cell(const char *s) {
    if (strcmp(s, "nan") == 0) return NANV;
    if (strcmp(s, "inf") == 0) return INFINITY;
    if (strcmp(s, "-inf") == 0) return -INFINITY;
    return atof(s);
}

static int split_line(char *line, double *out, int max) {
    int n = 0;
    char *p = line;
    while (*p && n < max) {
        char *comma = strchr(p, ',');
        if (comma) *comma = '\0';
        out[n++] = parse_cell(p);
        if (!comma) break;
        p = comma + 1;
    }
    return n;
}

static FILE *open_fixture(const char *name) {
    char path[1024];
    snprintf(path, sizeof(path), "%s/%s.csv", GDIR, name);
    return fopen(path, "r");
}

static void load_input(void) {
    FILE *f = open_fixture("input");
    if (!f) { fprintf(stderr, "cannot open input.csv in %s\n", GDIR); exit(2); }
    char line[8192];
    int first = 1;
    while (fgets(line, sizeof(line), f)) {
        line[strcspn(line, "\r\n")] = '\0';
        if (first) { first = 0; continue; }
        if (line[0] == '\0') continue;
        N_INPUT += (split_line(line, IN[N_INPUT], 8) > 0);
    }
    fclose(f);
}

/* Read a fixture, keeping blank rows (a candle on which no bar closed). */
static int read_fixture(const char *name, double ***outp) {
    FILE *f = open_fixture(name);
    if (!f) { fprintf(stderr, "cannot open %s.csv\n", name); exit(2); }
    double **rows = (double **)malloc(sizeof(double *) * MAXROWS);
    int n = 0, first = 1;
    char line[8192];
    while (fgets(line, sizeof(line), f) && n < MAXROWS) {
        line[strcspn(line, "\r\n")] = '\0';
        if (first) { first = 0; continue; }
        double *vals = (double *)malloc(sizeof(double) * MAXCOLS);
        int c = (line[0] == '\0') ? 0 : split_line(line, vals, MAXCOLS);
        EXPLEN[n] = c;
        rows[n++] = vals;
    }
    fclose(f);
    *outp = rows;
    return n;
}

static void free_fixture(double **rows, int n) {
    for (int i = 0; i < n; i++) free(rows[i]);
    free(rows);
}

static int close_to(double g, double w) {
    if (isnan(w)) return isnan(g);
    if (isinf(w)) return isinf(g) && ((g > 0) == (w > 0));
    double tol = 1e-6 * (fabs(w) > 1.0 ? fabs(w) : 1.0);
    return fabs(g - w) <= tol;
}

static int cmp_row(const char *name, int i, const double *want, int wn, const double *got, int gn) {
    if (wn != gn) {
        printf("FAIL %s row %d: arity %d vs %d\n", name, i, gn, wn);
        return 1;
    }
    for (int k = 0; k < wn; k++) {
        if (!close_to(got[k], want[k])) {
            printf("FAIL %s row %d col %d: got %g want %g\n", name, i, k, got[k], want[k]);
            return 1;
        }
    }
    return 0;
}

'''

MAIN_HEAD = r'''
int main(int argc, char **argv) {
    GDIR = (argc > 1) ? argv[1] : "testdata/golden";
    load_input();
    int total = 0, failed = 0;
'''

out = [HEADER]
for canon in canons:
    out.append(gen_check(canon))
out.append(MAIN_HEAD)
for canon in canons:
    out.append(f"    total++; if (check_{canon}()) failed++;")
out.append(r'''    printf("\nC/C++ golden: %d passed, %d failed (of %d)\n", total - failed, failed, total);
    return failed ? 1 : 0;
}''')

dest = os.path.join(ROOT, "examples", "c", "golden_test.c")
open(dest, "w", encoding="utf-8").write("\n".join(out) + "\n")
print("generated golden_test.c with", len(canons), "indicators")
