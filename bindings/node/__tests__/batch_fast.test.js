// The typed-array batch surface: `Series` inputs (Float64Array or number[]),
// the opt-in `batchFast`, and the caller-buffer `batchInto` / `batchFastInto`.
//
// `batch` and `batchInto` are bit for bit the streaming result. `batchFast`
// runs a SIMD kernel where the indicator has one; the kernel reassociates the
// arithmetic, so each value agrees with `batch` to within a few units in the
// last place rather than bit for bit, with NaN in the same places. Without a
// kernel it is `batch` exactly.
//
//   cd bindings/node && npm run build && npm test

const test = require('node:test');
const assert = require('node:assert/strict');
const wickra = require('..');

const N = 1003;
const CLOSE = Array.from({ length: N }, (_, i) => 100 + Math.sin(i * 0.0137) * 5 + Math.cos(i * 0.37));
const OTHER = CLOSE.slice().reverse().map((c, i) => c * 0.5 + 3 + Math.sin(i * 0.11));
const HIGH = CLOSE.map((c) => c + 1);
const LOW = CLOSE.map((c) => c - 1);
const VOLUME = CLOSE.map((_, i) => 1000 + (i % 7));

// Constructor arguments; everything else takes a single period of 14.
const CONSTRUCT = {
  LaguerreRSI: [0.5],
  HoltWinters: [0.3, 0.1],
  ALMA: [14, 0.85, 6],
  JMA: [14, 0, 2],
  T3: [5, 0.7],
  ValueAtRisk: [20, 0.95],
  Garch11: [0.000001, 0.1, 0.85],
  RollingQuantile: [20, 0.5],
  GD: [5, 0.7],
  ElderImpulse: [13, 12, 26, 9],
  EwmaVolatility: [0.94],
  KAMA: [10, 2, 30],
  ConditionalValueAtRisk: [20, 0.95],
  STC: [23, 50, 10, 0.5],
  DerivativeOscillator: [14, 5, 3, 9],
  BANDPASS: [20, 0.3],
  EmpiricalModeDecomposition: [20, 0.5],
  SAMPLEENT: [20, 2, 0.2],
  FAMA: [0.5, 0.05],
  MACD: [12, 26, 9],
  BollingerBands: [20, 2],
  ChaikinOscillator: [3, 10],
  PearsonCorrelation: [20],
  DecyclerOscillator: [10, 20],
};
// Batch columns for the classes whose batch is not a single price series.
const COLUMNS = {
  ATR: [HIGH, LOW, CLOSE],
  ChaikinOscillator: [HIGH, LOW, CLOSE, VOLUME],
  PearsonCorrelation: [CLOSE, OTHER],
};
// Rows per input for the flat multi-output batches.
const WIDTH = { MACD: 3, BollingerBands: 4 };

const FAST = Object.keys(wickra)
  .filter((n) => wickra[n] && wickra[n].prototype && typeof wickra[n].prototype.batchFast === 'function')
  .sort();

// Argument shapes tried in order for a class without an explicit entry above.
const TRIES = [[14], [10, 20], [20, 2], [20, 0.5], [12, 26, 9], [14, 3, 3]];

function make(name) {
  if (CONSTRUCT[name]) return new wickra[name](...CONSTRUCT[name]);
  for (const args of TRIES) {
    try {
      return new wickra[name](...args);
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
    if (!Number.isFinite(x)) {
      assert.ok(Object.is(x, y) || (Number.isNaN(x) && Number.isNaN(y)), `${label}: ${x} vs ${y} at ${i}`);
    } else {
      assert.ok(Math.abs(x - y) <= tol * Math.max(1, Math.abs(x)), `${label}: ${x} vs ${y} at ${i}`);
    }
  }
}

function assertBits(a, b, label) {
  assert.equal(a.length, b.length, `${label}: length`);
  for (let i = 0; i < a.length; i++) {
    assert.ok(Object.is(a[i], b[i]), `${label}: ${a[i]} vs ${b[i]} at ${i}`);
  }
}

test('batchFast mirrors the 155 C ABI fast entry points', () => {
  assert.equal(FAST.length, 155);
});

test('batchFast agrees with batch for every indicator that has it', () => {
  for (const name of FAST) {
    const cols = columnsOf(name);
    const exact = make(name).batch(...cols);
    const fast = make(name).batchFast(...cols);
    assert.ok(fast instanceof Float64Array, `${name}: batchFast returns a Float64Array`);
    assertWithin(exact, fast, 1e-11, name);
  }
});

test('batchInto is batch bit for bit, and batchFastInto is batchFast', () => {
  for (const name of FAST) {
    const cols = columnsOf(name);
    const rows = CLOSE.length * (WIDTH[name] || 1);
    const into = new Float64Array(rows);
    make(name).batchInto(...cols, into);
    assertBits(make(name).batch(...cols), into, `${name} batchInto`);
    const fastInto = new Float64Array(rows);
    make(name).batchFastInto(...cols, fastInto);
    assertBits(make(name).batchFast(...cols), fastInto, `${name} batchFastInto`);
  }
});

test('a Float64Array input gives the same bits as a number[] input', () => {
  const typed = Float64Array.from(CLOSE);
  assertBits(new wickra.EMA(20).batch(CLOSE), new wickra.EMA(20).batch(typed), 'batch');
  assertBits(new wickra.EMA(20).batchFast(CLOSE), new wickra.EMA(20).batchFast(typed), 'batchFast');
  const atr = [HIGH, LOW, CLOSE].map((c) => Float64Array.from(c));
  assertBits(new wickra.ATR(14).batch(HIGH, LOW, CLOSE), new wickra.ATR(14).batch(...atr), 'ATR');
});

test('a typed array of another element type is refused', () => {
  assert.throws(() => new wickra.SMA(3).batch(Float32Array.from(CLOSE)), /Float64Array/);
});

test('without a kernel batchFast is the exact batch', () => {
  assertBits(new wickra.ROC(10).batch(CLOSE), new wickra.ROC(10).batchFast(CLOSE), 'ROC');
});

test('the state after batchFast keeps streaming', () => {
  for (const name of ['SMA', 'EMA', 'WMA', 'HMA', 'SMMA', 'DEMA', 'TEMA', 'RSI']) {
    const exact = make(name);
    const fast = make(name);
    exact.batch(CLOSE);
    fast.batchFast(CLOSE);
    for (const price of [101, 99.5, 100.25]) {
      const a = exact.update(price);
      const b = fast.update(price);
      assert.ok(Math.abs(a - b) <= 1e-11 * Math.max(1, Math.abs(a)), `${name}: ${a} vs ${b}`);
    }
  }
});

test('the output buffer is checked before anything is written', () => {
  const sma = new wickra.SMA(3);
  const input = Float64Array.from(CLOSE.slice(0, 16));
  assert.throws(() => sma.batchInto(input, new Float64Array(15)), /must hold 16 values/);
  assert.throws(() => sma.batchInto(input, [0, 0]), /Float64Array/);
  assert.throws(() => sma.batchInto(input, input), /share memory/);
  const overlapping = new Float64Array(input.buffer, 8, 15);
  assert.throws(() => sma.batchFastInto(overlapping, new Float64Array(input.buffer, 0, 15)), /share memory/);
  assert.throws(() => sma.batchInto(input, new Float64Array(new SharedArrayBuffer(16 * 8))), /SharedArrayBuffer/);
  assert.throws(() => new wickra.MACD(12, 26, 9).batchInto(input, new Float64Array(16)), /must hold 48 values/);
  // Nothing was consumed by the refused calls.
  assertBits(sma.batch(input), new wickra.SMA(3).batch(input), 'state untouched');
});

test('column batches refuse mismatched lengths and invalid bars without consuming them', () => {
  assert.throws(() => new wickra.PearsonCorrelation(5).batchFast(CLOSE, OTHER.slice(1)), /equal length/);
  assert.throws(() => new wickra.ATR(14).batchFast(HIGH, LOW, CLOSE.slice(1)), /equal length/);
  const badHigh = HIGH.slice();
  badHigh[500] = LOW[500] - 1;
  for (const method of ['batch', 'batchFast']) {
    const atr = new wickra.ATR(14);
    assert.throws(() => atr[method](badHigh, LOW, CLOSE));
    assertBits(atr.batch(HIGH, LOW, CLOSE), new wickra.ATR(14).batch(HIGH, LOW, CLOSE), `ATR ${method}`);
    const adosc = new wickra.ChaikinOscillator(3, 10);
    assert.throws(() => adosc[method](badHigh, LOW, CLOSE, VOLUME));
    assertBits(
      adosc.batch(HIGH, LOW, CLOSE, VOLUME),
      new wickra.ChaikinOscillator(3, 10).batch(HIGH, LOW, CLOSE, VOLUME),
      `ADOSC ${method}`,
    );
  }
});

test('ATR, Chaikin, MACD and Bollinger exact batches match streaming bit for bit', () => {
  const atr = new wickra.ATR(14);
  const atrBatch = new wickra.ATR(14).batch(HIGH, LOW, CLOSE);
  const adosc = new wickra.ChaikinOscillator(3, 10);
  const adoscBatch = new wickra.ChaikinOscillator(3, 10).batch(HIGH, LOW, CLOSE, VOLUME);
  const macd = new wickra.MACD(12, 26, 9);
  const macdBatch = new wickra.MACD(12, 26, 9).batch(CLOSE);
  const bb = new wickra.BollingerBands(20, 2);
  const bbBatch = new wickra.BollingerBands(20, 2).batch(CLOSE);
  const nanOr = (v) => (v === null ? NaN : v);
  for (let i = 0; i < N; i++) {
    assert.ok(Object.is(nanOr(atr.update(HIGH[i], LOW[i], CLOSE[i])), atrBatch[i]), `ATR ${i}`);
    assert.ok(Object.is(nanOr(adosc.update(HIGH[i], LOW[i], CLOSE[i], VOLUME[i])), adoscBatch[i]), `ADOSC ${i}`);
    const m = macd.update(CLOSE[i]);
    const mRow = m === null ? [NaN, NaN, NaN] : [m.macd, m.signal, m.histogram];
    assertBits(mRow, macdBatch.slice(i * 3, i * 3 + 3), `MACD ${i}`);
    const b = bb.update(CLOSE[i]);
    const bRow = b === null ? [NaN, NaN, NaN, NaN] : [b.upper, b.middle, b.lower, b.stddev];
    assertBits(bRow, bbBatch.slice(i * 4, i * 4 + 4), `BB ${i}`);
  }
});

test('empty inputs give empty outputs', () => {
  assert.equal(new wickra.EMA(5).batchFast(new Float64Array(0)).length, 0);
  new wickra.EMA(5).batchInto([], new Float64Array(0));
  assert.equal(new wickra.MACD(12, 26, 9).batchFast([]).length, 0);
});
