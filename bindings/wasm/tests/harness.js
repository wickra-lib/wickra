// Shared driving code for the WASM suites.
//
// `golden.test.js` and `batch.test.js` each carried their own copy of the
// manifest loading, the per-bar argument construction and the output
// flattening, which is two chances for the streaming and batch passes to stop
// feeding the indicators the same stream without anyone noticing. Adding a
// third copy for the lifecycle pass would have made that worse, so the shared
// half lives here and every suite drives from it.
//
//   wasm-pack build --target nodejs --out-dir pkg
//   node --test tests/

const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');

const GOLDEN = path.resolve(__dirname, '..', '..', '..', 'testdata', 'golden');

function cell(s) {
  if (s === 'nan') return NaN;
  if (s === 'inf') return Infinity;
  if (s === '-inf') return -Infinity;
  return Number(s);
}

// Keeps blank lines, which is how a bar fixture records a candle that closed no
// bar; a non-bar fixture contains none.
function readRows(name) {
  const lines = fs.readFileSync(path.join(GOLDEN, name + '.csv'), 'utf8').split('\n');
  lines.shift();
  if (lines.length && lines[lines.length - 1] === '') lines.pop();
  return lines.map((l) => (l.length === 0 ? [] : l.split(',').map(cell)));
}

function readCsv(name) {
  return readRows(name).filter((row) => row.length > 0);
}

const MANIFEST = JSON.parse(fs.readFileSync(path.join(GOLDEN, 'wasm_manifest.json'), 'utf8'));
// Canonical Indicator::name() per indicator, the same string every binding returns.
const NAMES = JSON.parse(fs.readFileSync(path.join(GOLDEN, 'names.json'), 'utf8'));
const ROWS = readCsv('input');

function deriv(o, h, l, c, v) {
  return {
    funding_rate: ((c - o) / c) * 0.01,
    mark_price: c,
    index_price: c - 0.5,
    futures_price: c + 1.0,
    open_interest: v * 10.0,
    long_size: v * 0.6,
    short_size: v * 0.4,
    taker_buy_volume: v * 0.55,
    taker_sell_volume: v * 0.45,
    long_liquidation: h - c,
    short_liquidation: c - l,
  };
}

// The per-bar arguments for one indicator, in the order its `update` declares
// them. The same construction `gen_golden` uses on the Rust side.
function resolveArg(arg, o, h, l, c, v, i) {
  const n = arg.name;
  // A pair with domain-named arguments: the golden feeds (close, open).
  if (arg.role === 'x') return c;
  if (arg.role === 'y') return o;
  if (arg.array) {
    const j5 = [0, 1, 2, 3, 4];
    switch (n) {
      case 'change': return Float64Array.from(j5.map((j) => c - o + j));
      case 'volume': return Float64Array.from(j5.map((j) => v + j * 10.0));
      case 'new_high': return Float64Array.from(j5.map((j) => (j % 2 === 0 ? 1 : 0)));
      case 'new_low': return Float64Array.from(j5.map((j) => (j % 3 === 0 ? 1 : 0)));
      case 'above_ma': return Float64Array.from(j5.map((j) => (j % 2 === 0 ? 1 : 0)));
      case 'on_buy_signal': return Float64Array.from(j5.map((j) => (j % 3 === 0 ? 1 : 0)));
      case 'bid_px': return Float64Array.from(j5.map((k) => c - 0.1 * (k + 1)));
      case 'bid_sz': return Float64Array.from(j5.map((k) => v / (k + 1)));
      case 'ask_px': return Float64Array.from(j5.map((k) => c + 0.1 * (k + 1)));
      case 'ask_sz': return Float64Array.from(j5.map((k) => (v * 0.9) / (k + 1)));
      default: throw new Error('array arg ' + n);
    }
  }
  if (arg.bigint) return BigInt(i);
  switch (n) {
    case 'value': case 'close': case 'price': case 'x': case 'a': case 'asset': return c;
    case 'y': case 'b': case 'open': case 'benchmark': return o;
    case 'high': return h;
    case 'low': return l;
    case 'volume': case 'size': return v;
    case 'is_buy': return c >= o;
    case 'mid': return (h + l) / 2.0;
    case 'timestamp': return BigInt(i);
    default: {
      const d = deriv(o, h, l, c, v);
      if (n in d) return d[n];
      throw new Error('scalar arg ' + n);
    }
  }
}

// A batch parameter is the per-bar argument lifted to a series: a typed array
// for the numeric ones, a `BigInt64Array` for timestamps, and a plain array for
// booleans and for the per-bar `Float64Array` snapshots.
function column(arg) {
  const values = ROWS.map(([o, h, l, c, v], i) => resolveArg(arg, o, h, l, c, v, i));
  if (arg.array) return values;
  if (arg.bigint) return BigInt64Array.from(values);
  if (typeof values[0] === 'boolean') return values;
  return Float64Array.from(values);
}

// Recursively flatten a WASM output (number, object with field props, typed or
// plain array of either) into a flat number list. Null for warmup, which WASM
// spells `undefined`.
function flat(v) {
  if (v === null || v === undefined) return null;
  if (typeof v === 'number' || typeof v === 'bigint') return [Number(v)];
  if (Array.isArray(v) || ArrayBuffer.isView(v)) {
    const out = [];
    for (const e of v) { const f = flat(e); if (f) out.push(...f); }
    return out;
  }
  if (typeof v === 'object') {
    const out = [];
    for (const val of Object.values(v)) { const f = flat(val); if (f) out.push(...f); }
    return out;
  }
  return [Number(v)];
}

function nanRow(n) {
  return Array.from({ length: n }, () => NaN);
}

function widthOf(spec) {
  if (spec.out.startsWith('multi') || spec.out === 'deriv_multi') return spec.n;
  if (spec.out.startsWith('profile')) return spec.width;
  return 1; // scalar archetypes
}

function isBars(spec) {
  return spec.out === 'bars' || spec.out === 'footprint';
}

// Flatten a batch result to the same number sequence the streaming run
// produces: a flat typed array is already it, and a container yields one entry
// per bar, with a warmup entry standing in as a row of NaN.
function flatten(result, spec) {
  if (ArrayBuffer.isView(result)) return Array.from(result, Number);
  const out = [];
  for (const entry of result) {
    const values = flat(entry);
    out.push(...(values === null ? nanRow(widthOf(spec)) : values));
  }
  return out;
}

// Drive one indicator over the whole golden input and return the rows it
// produced, a warmup row spelled as NaN so the shape stays rectangular.
function driveRows(ind, spec) {
  const rows = [];
  for (let i = 0; i < ROWS.length; i++) {
    const [o, h, l, c, v] = ROWS[i];
    const args = spec.args.map((a) => resolveArg(a, o, h, l, c, v, i));
    let got = flat(ind.update(...args));
    if (got === null) got = isBars(spec) ? [] : nanRow(widthOf(spec));
    rows.push(got);
  }
  return rows;
}

// Two bounds, not one. The loose one exists for the indicators that reach a
// transcendental in the platform math library, whose last bit is not portable;
// everything else is IEEE-754 arithmetic and is bit-identical everywhere. A
// blanket 1e-6 is ten orders looser than the 1-ulp difference it exists for,
// loose enough to hide a real defect.
const LIBM_DEPENDENT = new Set(
  fs.readFileSync(path.join(GOLDEN, 'libm_dependent.txt'), 'utf8')
    .split('\n').map((s) => s.trim()).filter(Boolean),
);
const tolFor = (canonical) => (LIBM_DEPENDENT.has(canonical) ? 1e-6 : 1e-12);

function closeEq(got, want, label, tol) {
  if (Number.isNaN(want)) { assert.ok(Number.isNaN(got), `${label}: want NaN got ${got}`); return; }
  if (!Number.isFinite(want)) { assert.ok(got === want, `${label}: want ${want} got ${got}`); return; }
  const bound = tol * Math.max(1.0, Math.abs(want));
  assert.ok(Math.abs(got - want) <= bound, `${label}: got ${got} want ${want}`);
}

module.exports = {
  GOLDEN,
  MANIFEST,
  NAMES,
  ROWS,
  cell,
  closeEq,
  column,
  deriv,
  driveRows,
  flat,
  flatten,
  isBars,
  nanRow,
  readCsv,
  readRows,
  resolveArg,
  tolFor,
  widthOf,
};
