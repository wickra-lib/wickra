// The contract around the values, for every one of the 515 Node indicators.
//
// `golden.test.js` checks what an indicator computes and stops there, so a
// `reset` that forgot a field, or an `isReady` keyed off a value that happens to
// move at the right moment, replays perfectly clean through it. This drives the
// same stream twice with a `reset` in between and asserts the second pass
// reproduces the first.
//
//   cd bindings/node && npm run build && npm test

const test = require('node:test');
const assert = require('node:assert/strict');
const wickra = require('..');
const { MANIFEST, driveRows, readBarRows } = require('./harness.js');

// The ten bar builders implement BarBuilder, not Indicator: one candle can
// complete any number of bars, so they carry no warmup or ready state. They do
// have `reset`. Footprint is an Indicator despite its bar-shaped output.
const isBarBuilder = (spec) => spec.out === 'bars';

// A row's width, so a warmup return can stand in as the right number of NaN.
function widthOf(spec) {
  if (spec.out === 'multi') return spec.fields.length;
  if (spec.out === 'profile_bins') return spec.width;
  if (spec.out === 'profile_pricebins') return spec.width;
  return 1;
}

// Whether the reference fixture holds a single finite value. A few indicators
// never emit over this input, and for those "ready once the series is done"
// would be the wrong assertion. Reading it off the fixture beats inferring it at
// runtime from a row that might legitimately be NaN.
function fixtureEmits(canonical) {
  for (const row of readBarRows('g_' + canonical)) {
    for (const value of row) {
      if (Number.isFinite(value)) return true;
    }
  }
  return false;
}

// Equality, not tolerance: the same code over the same input in the same
// process has no reason to differ in a single bit, and a tolerance here would
// hide exactly the leftover state this is looking for.
function assertSameRuns(canonical, first, second) {
  assert.equal(second.length, first.length, `${canonical}: row count changed after reset`);
  for (let i = 0; i < first.length; i++) {
    assert.equal(
      second[i].length,
      first[i].length,
      `${canonical} row ${i}: ${first[i].length} values before reset, ${second[i].length} after`,
    );
    for (let k = 0; k < first[i].length; k++) {
      const before = first[i][k];
      const after = second[i][k];
      if (Number.isNaN(before) && Number.isNaN(after)) continue;
      assert.equal(
        after,
        before,
        `${canonical} row ${i} col ${k}: ${before} before reset, ${after} after`,
      );
    }
  }
}

for (const spec of MANIFEST) {
  test(`lifecycle: ${spec.canonical}`, () => {
    const Cls = wickra[spec.native];
    assert.ok(Cls, `missing Node class ${spec.native}`);
    const ind = new Cls(...spec.ctor);
    const width = widthOf(spec);
    const stateful = !isBarBuilder(spec);

    if (stateful) {
      assert.equal(ind.isReady(), false, `${spec.canonical}: ready before any input`);
      const warmup = ind.warmupPeriod();
      assert.ok(
        Number.isInteger(warmup) && warmup >= 1,
        `${spec.canonical}: warmup period ${warmup}, want an integer >= 1`,
      );
    }

    const first = driveRows(ind, spec, width);

    if (stateful && fixtureEmits(spec.canonical)) {
      assert.equal(
        ind.isReady(),
        true,
        `${spec.canonical}: not ready after the whole series, but the fixture has values`,
      );
    }

    ind.reset();
    if (stateful) {
      assert.equal(ind.isReady(), false, `${spec.canonical}: still ready after reset`);
    }

    assertSameRuns(spec.canonical, first, driveRows(ind, spec, width));
  });
}
