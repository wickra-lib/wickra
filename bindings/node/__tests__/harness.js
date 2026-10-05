// Shared driving code for the Node golden suites.
//
// `golden.test.js` owned the manifest loading, the per-bar argument
// construction and the output flattening. The lifecycle pass needs the same
// stream, and a second copy of it is a second chance for the two to drift, so
// the shared half lives here and both suites drive from it.
//
//   cd bindings/node && npm run build && npm test

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

function readCsv(name) {
  // Split on \r?\n so a CRLF checkout (Windows core.autocrlf) parses identically
  // to LF — otherwise `cell('inf\r')` falls through to Number() and becomes NaN.
  const lines = fs.readFileSync(path.join(GOLDEN, name + '.csv'), 'utf8').split(/\r?\n/);
  lines.shift(); // header
  return lines.filter((l) => l.length > 0).map((l) => l.split(',').map(cell));
}

// Bars keep blank lines (one row per candle, blank == no bar closed).
function readBarRows(name) {
  const lines = fs.readFileSync(path.join(GOLDEN, name + '.csv'), 'utf8').split(/\r?\n/);
  lines.shift();
  // Drop only the single trailing-newline artifact, keeping legitimate blank
  // rows (a candle on which no bar closed) so rows stay aligned to the input.
  if (lines.length && lines[lines.length - 1] === '') lines.pop();
  return lines.map((l) => (l.length === 0 ? [] : l.split(',').map(cell)));
}

const MANIFEST = JSON.parse(fs.readFileSync(path.join(GOLDEN, 'node_manifest.json'), 'utf8'));
// Canonical Indicator::name() per indicator (the same string every binding must
// return). Keyed by the Rust canonical; values are the core names, which may
// differ from the registered class name (e.g. ChaikinMoneyFlow -> "CMF").
const NAMES = JSON.parse(fs.readFileSync(path.join(GOLDEN, 'names.json'), 'utf8'));
const ROWS = readCsv('input');

function derivFields(o, h, l, c, v) {
  return {
    fundingRate: ((c - o) / c) * 0.01,
    markPrice: c,
    indexPrice: c - 0.5,
    futuresPrice: c + 1.0,
    openInterest: v * 10.0,
    longSize: v * 0.6,
    shortSize: v * 0.4,
    takerBuyVolume: v * 0.55,
    takerSellVolume: v * 0.45,
    longLiquidation: h - c,
    shortLiquidation: c - l,
  };
}

function resolveArg(arg, o, h, l, c, v, i) {
  const name = arg.name;
  // A pair with domain-named arguments: the golden feeds (close, open).
  if (arg.role === 'x') return c;
  if (arg.role === 'y') return o;
  if (arg.array) {
    switch (name) {
      case 'change':
        return [0, 1, 2, 3, 4].map((j) => c - o + j);
      case 'volume':
        return [0, 1, 2, 3, 4].map((j) => v + j * 10.0);
      case 'newHigh':
        return [0, 1, 2, 3, 4].map((j) => j % 2 === 0);
      case 'newLow':
        return [0, 1, 2, 3, 4].map((j) => j % 3 === 0);
      case 'aboveMa':
        return [0, 1, 2, 3, 4].map((j) => j % 2 === 0);
      case 'onBuySignal':
        return [0, 1, 2, 3, 4].map((j) => j % 3 === 0);
      case 'bidPx':
        return [0, 1, 2, 3, 4].map((k) => c - 0.1 * (k + 1));
      case 'bidSz':
        return [0, 1, 2, 3, 4].map((k) => v / (k + 1));
      case 'askPx':
        return [0, 1, 2, 3, 4].map((k) => c + 0.1 * (k + 1));
      case 'askSz':
        return [0, 1, 2, 3, 4].map((k) => (v * 0.9) / (k + 1));
      default:
        throw new Error('unknown array arg ' + name);
    }
  }
  switch (name) {
    case 'value':
    case 'close':
    case 'price':
    case 'x':
    case 'a':
    case 'asset':
      return c;
    case 'y':
    case 'b':
    case 'benchmark':
    case 'open':
      return o;
    case 'high':
      return h;
    case 'low':
      return l;
    case 'volume':
    case 'size':
      return v;
    case 'timestamp':
      return i;
    case 'isBuy':
      return c >= o;
    case 'mid':
      return (h + l) / 2.0;
    default: {
      const d = derivFields(o, h, l, c, v);
      if (name in d) return d[name];
      throw new Error('unknown scalar arg ' + name);
    }
  }
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
  if (Number.isNaN(want)) {
    assert.ok(Number.isNaN(got), `${label}: want NaN got ${got}`);
    return;
  }
  if (!Number.isFinite(want)) {
    assert.ok(got === want, `${label}: want ${want} got ${got}`);
    return;
  }
  const bound = tol * Math.max(1.0, Math.abs(want));
  assert.ok(Math.abs(got - want) <= bound, `${label}: got ${got} want ${want}`);
}


// Flatten one `update` return into the flat number row the fixture stores, a
// warmup row spelled as NaN so the shape stays rectangular.
function flatRow(spec, got, width) {
  if (spec.out === 'bars' || spec.out === 'footprint') {
    const flat = [];
    for (const bar of got) {
      for (const f of spec.fields) flat.push(Number(bar[f]));
    }
    return flat;
  }
  if (got === null || got === undefined) return Array.from({ length: width }, () => NaN);
  if (spec.out === 'scalar') return [Number(got)];
  if (spec.out === 'profile_bins') return Array.from(got, Number);
  if (spec.out === 'profile_pricebins') {
    return [got.priceLow, got.priceHigh, ...got[spec.arrayField]].map(Number);
  }
  // napi serialises the output struct's fields in declaration order, which
  // matches the CSV column order.
  return Object.values(got).map((v) => (v === null || v === undefined ? NaN : Number(v)));
}

// Drive one indicator over the whole golden input and return its rows.
function driveRows(ind, spec, width) {
  const rows = [];
  for (let i = 0; i < ROWS.length; i++) {
    const [o, h, l, c, v] = ROWS[i];
    const args = spec.args.map((a) => resolveArg(a, o, h, l, c, v, i));
    rows.push(flatRow(spec, ind.update(...args), width));
  }
  return rows;
}

module.exports = {
  GOLDEN,
  MANIFEST,
  NAMES,
  ROWS,
  cell,
  closeEq,
  derivFields,
  driveRows,
  flatRow,
  readBarRows,
  readCsv,
  resolveArg,
  tolFor,
};
