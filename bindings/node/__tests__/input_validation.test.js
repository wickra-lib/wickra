// Input-validation tests for the Wickra Node bindings: malformed constructor
// parameters and mismatched batch inputs must raise a JS Error (the napi
// wrapper turns the Rust `Err` into a thrown Error), not crash the process.
// Node counterpart of bindings/python/tests/test_input_validation.py.

const test = require('node:test');
const assert = require('node:assert/strict');
const wickra = require('..');

// --- Constructors reject invalid periods / parameters ---

test('ATR rejects a zero period at construction', () => {
  // ATR validates its period (it drives the Wilder-smoothing length). The
  // plain moving averages (SMA/EMA/RSI/StdDev) instead treat period 0 as a
  // warmup-1 pass-through rather than an error, so they are not asserted here.
  assert.throws(() => new wickra.ATR(0), /.*/);
});

test('MACD rejects zero and non-increasing fast/slow periods', () => {
  assert.throws(() => new wickra.MACD(0, 0, 0), /.*/);
  // fast must be strictly less than slow.
  assert.throws(() => new wickra.MACD(26, 12, 9), /.*/);
});

test('BollingerBands rejects a negative standard-deviation multiplier', () => {
  assert.throws(() => new wickra.BollingerBands(20, -1), /.*/);
});

test('PSAR rejects a step greater than its maximum', () => {
  assert.throws(() => new wickra.PSAR(0.3, 0.02, 0.2), /.*/);
});

test('ValueArea rejects zero periods and out-of-range value-area percentages', () => {
  assert.throws(() => new wickra.ValueArea(0, 50, 0.7), /.*/);
  assert.throws(() => new wickra.ValueArea(20, 0, 0.7), /.*/);
  assert.throws(() => new wickra.ValueArea(20, 50, 0.0), /.*/);
  assert.throws(() => new wickra.ValueArea(20, 50, 1.5), /.*/);
});

test('InitialBalance and OpeningRange reject a zero period', () => {
  assert.throws(() => new wickra.InitialBalance(0), /.*/);
  assert.throws(() => new wickra.OpeningRange(0), /.*/);
});

test('Ichimoku rejects zero and non-increasing periods', () => {
  assert.throws(() => new wickra.Ichimoku(0, 26, 52, 26), /.*/);
  assert.throws(() => new wickra.Ichimoku(9, 26, 52, 0), /.*/);
  // Periods must satisfy tenkan < kijun < senkouB.
  assert.throws(() => new wickra.Ichimoku(26, 9, 52, 26), /.*/);
  assert.throws(() => new wickra.Ichimoku(9, 52, 52, 26), /.*/);
});

test('Family 10 (Ehlers / cycle) indicators reject invalid parameters', () => {
  // InverseFisherTransform needs a non-zero scaling factor.
  assert.throws(() => new wickra.InverseFisherTransform(0.0), /.*/);
  // DecyclerOscillator / RoofingFilter need the short cutoff below the long one.
  assert.throws(() => new wickra.DecyclerOscillator(30, 10), /.*/);
  assert.throws(() => new wickra.RoofingFilter(48, 10), /.*/);
  // MAMA needs fast limit > slow limit.
  assert.throws(() => new wickra.MAMA(0.05, 0.5), /.*/);
  // EmpiricalModeDecomposition needs a positive fraction.
  assert.throws(() => new wickra.EmpiricalModeDecomposition(20, 0.0), /.*/);
  // The VECM behind the information share needs at least six observations.
  assert.throws(() => new wickra.HasbrouckInformationShare(5), /.*/);
  // NOTE: SuperSmoother(0) / FisherTransform(0) are NOT asserted: the Node
  // binding treats their period 0 as a warmup-1 pass-through (same as the
  // simple moving averages) rather than an error.
});

// --- Batch methods reject mismatched input lengths ---

test('candle batch methods reject unequal-length columns', () => {
  const high = [10, 11, 12];
  const low = [9, 10]; // one short
  const close = [9.5, 10.5, 11.5];
  assert.throws(() => new wickra.ATR(14).batch(high, low, close), /.*/);
  assert.throws(() => new wickra.WilliamsR(14).batch(high, low, close), /.*/);
  assert.throws(() => new wickra.Aroon(14).batch(high, low), /.*/);
});

test('ValueArea batch rejects unequal-length columns', () => {
  const high = [1, 2, 3];
  const low = [0.5, 1.5]; // short
  const volume = [10, 10, 10];
  assert.throws(() => new wickra.ValueArea(2, 10, 0.7).batch(high, low, volume), /.*/);
});

// --- A JS number reaching a period parameter is checked, not reinterpreted ---
//
// Every numeric parameter used to be declared `u32`, and napi converts a JS
// number to `u32` the way a bitwise operator would. `-1` arrived as 4294967295,
// `1.5` as `1`, and `1e10` as its low 32 bits — all plausible-looking periods,
// so the Rust constructor's validation had nothing to reject and the caller got
// an indicator quietly configured for something they never asked for.

test('a negative period is rejected rather than wrapping to 4294967295', () => {
  assert.throws(() => new wickra.SMA(-1), /non-negative/);
});

test('a fractional period is rejected rather than truncating', () => {
  // 1.5 used to become 1, which constructs successfully and behaves like a
  // pass-through: the wrong indicator, with no error anywhere.
  assert.throws(() => new wickra.SMA(1.5), /whole number/);
});

test('a period beyond the exact-integer range is rejected', () => {
  assert.throws(() => new wickra.SMA(1e300), /too large/);
});

test('NaN and Infinity are rejected', () => {
  assert.throws(() => new wickra.SMA(NaN), /whole number/);
  assert.throws(() => new wickra.SMA(Infinity), /whole number/);
});

test('the check applies to every numeric parameter, not just the first', () => {
  // MACD takes three; the second and third go through the same conversion.
  assert.throws(() => new wickra.MACD(12, -1, 9), /non-negative/);
  assert.throws(() => new wickra.MACD(12, 26, 9.5), /whole number/);
});

test('a valid period still constructs and computes', () => {
  const sma = new wickra.SMA(3);
  assert.equal(sma.update(1), null);
  assert.equal(sma.update(2), null);
  assert.equal(sma.update(3), 2);
});

test('a large but exact period is accepted by the conversion', () => {
  // The conversion only refuses numbers that are not exact integers; the
  // domain rules stay with the Rust constructor, which rejects this one for
  // exceeding the maximum window length.
  assert.throws(() => new wickra.SMA(2 ** 40), /period|window|length/i);
});
