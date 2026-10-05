// The opt-in `batchFast` and the caller-buffer `batchInto` / `batchFastInto`.
//
// `batch` and `batchInto` are bit for bit the streaming result. `batchFast`
// runs a SIMD kernel where the indicator has one; the kernel reassociates the
// arithmetic, so each value agrees with `batch` to within a few units in the
// last place rather than bit for bit, with NaN in the same places. Without a
// kernel it is `batch` exactly.
//
//   wasm-pack build --target nodejs --out-dir pkg
//   node --test tests/

const test = require('node:test');
const assert = require('node:assert/strict');
const W = require('../pkg/wickra_wasm.js');

const N = 1003;
const CLOSE = Float64Array.from({ length: N }, (_, i) => 100 + Math.sin(i * 0.0137) * 5 + Math.cos(i * 0.37));
const OTHER = Float64Array.from(CLOSE.slice().reverse(), (c, i) => c * 0.5 + 3 + Math.sin(i * 0.11));
const HIGH = CLOSE.map((c) => c + 1);
const LOW = CLOSE.map((c) => c - 1);
const VOLUME = CLOSE.map((_, i) => 1000 + (i % 7));

const CONSTRUCT = {
  DerivativeOscillator: [14, 5, 3, 9],
  ElderImpulse: [13, 12, 26, 9],
  EwmaVolatility: [0.94],
  FAMA: [0.5, 0.05],
  Garch11: [0.000001, 0.1, 0.85],
  HoltWinters: [0.3, 0.1],
  JMA: [14, 0, 2],
  KAMA: [10, 2, 30],
  LaguerreRSI: [0.5],
  SAMPLEENT: [20, 2, 0.2],
  STC: [23, 50, 10, 0.5],
  MACD: [12, 26, 9],
  MACDFIX: [9],
  BollingerBands: [20, 2],
  ATR: [14],
  ChaikinOscillator: [3, 10],
  PearsonCorrelation: [20],
};
const COLUMNS = {
  ATR: [HIGH, LOW, CLOSE],
  ChaikinOscillator: [HIGH, LOW, CLOSE, VOLUME],
  PearsonCorrelation: [CLOSE, OTHER],
};
const WIDTH = { MACD: 3, MACDFIX: 3, BollingerBands: 4 };
const TRIES = [[14], [10, 20], [20, 2], [20, 0.5], [12, 26, 9], [5, 0.7], [14, 0.85, 6]];

const FAST = Object.keys(W)
  .filter((n) => W[n] && W[n].prototype && typeof W[n].prototype.batchFast === 'function')
  .sort();

function make(name) {
  if (CONSTRUCT[name]) return new W[name](...CONSTRUCT[name]);
  for (const args of TRIES) {
    try {
      return new W[name](...args);
    } catch {
      // Try the next shape.
    }
  }
  throw new Error(`no constructor arguments found for ${name}`);
}
const columnsOf = (name) => COLUMNS[name] || [CLOSE];

function assertWithin(exact, fast, tol, label) {
  assert.equal(exact.length, fast.length, `${label}: length`);
  for (let i = 0; i < exact.length; i++) {
    const x = exact[i];
    const y = fast[i];
    assert.equal(Number.isNaN(x), Number.isNaN(y), `${label}: NaN mismatch at ${i}`);
    if (Number.isFinite(x)) {
      assert.ok(Math.abs(x - y) <= tol * Math.max(1, Math.abs(x)), `${label}: ${x} vs ${y} at ${i}`);
    } else if (!Number.isNaN(x)) {
      assert.equal(x, y, `${label}: ${x} vs ${y} at ${i}`);
    }
  }
}

function assertBits(a, b, label) {
  assert.equal(a.length, b.length, `${label}: length`);
  for (let i = 0; i < a.length; i++) {
    assert.ok(Object.is(a[i], b[i]), `${label}: ${a[i]} vs ${b[i]} at ${i}`);
  }
}

test('the scalar macro and the column indicators expose batchFast', () => {
  // The same 156 as the C ABI's fast entry points.
  assert.equal(FAST.length, 156);
  for (const name of ['SMA', 'EMA', 'RSI', 'MACD', 'BollingerBands', 'ATR', 'ChaikinOscillator', 'PearsonCorrelation']) {
    assert.ok(FAST.includes(name), `${name} has batchFast`);
  }
});

test('batchFast agrees with batch, and the Into forms match their allocating twins', () => {
  for (const name of FAST) {
    const cols = columnsOf(name);
    const exact = make(name).batch(...cols);
    const fast = make(name).batchFast(...cols);
    assertWithin(exact, fast, 1e-11, name);
    const rows = N * (WIDTH[name] || 1);
    const into = new Float64Array(rows);
    make(name).batchInto(...cols, into);
    assertBits(exact, into, `${name} batchInto`);
    const fastInto = new Float64Array(rows);
    make(name).batchFastInto(...cols, fastInto);
    assertBits(fast, fastInto, `${name} batchFastInto`);
  }
});

test('without a kernel batchFast is the exact batch', () => {
  assertBits(new W.ROC(10).batch(CLOSE), new W.ROC(10).batchFast(CLOSE), 'ROC');
});

test('a wrong-length output is refused before anything is consumed', () => {
  const sma = new W.SMA(3);
  const input = CLOSE.subarray(0, 16);
  assert.throws(() => sma.batchInto(input, new Float64Array(15)), /must hold 16 values/);
  assert.throws(() => new W.MACD(12, 26, 9).batchFastInto(input, new Float64Array(16)), /must hold 48 values/);
  assertBits(sma.batch(input), new W.SMA(3).batch(input), 'state untouched');
});

test('an invalid bar is refused before the column batches consume anything', () => {
  const badHigh = HIGH.slice();
  badHigh[500] = LOW[500] - 1;
  for (const method of ['batch', 'batchFast']) {
    const atr = new W.ATR(14);
    assert.throws(() => atr[method](badHigh, LOW, CLOSE));
    assertBits(atr.batch(HIGH, LOW, CLOSE), new W.ATR(14).batch(HIGH, LOW, CLOSE), `ATR ${method}`);
    const adosc = new W.ChaikinOscillator(3, 10);
    assert.throws(() => adosc[method](badHigh, LOW, CLOSE, VOLUME));
    assertBits(
      adosc.batch(HIGH, LOW, CLOSE, VOLUME),
      new W.ChaikinOscillator(3, 10).batch(HIGH, LOW, CLOSE, VOLUME),
      `ADOSC ${method}`,
    );
  }
  assert.throws(() => new W.PearsonCorrelation(5).batchFast(CLOSE, OTHER.subarray(1)), /equal length/);
});
