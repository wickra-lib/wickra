// Generic golden-fixture parity for the WASM (wasm-bindgen) binding, run under
// Node's test runner against the nodejs-target build in ../pkg.
//
// Every one of the 515 indicators is reconstructed from wasm_manifest.json (JS
// class, constructor params, ordered update args parsed from the generated
// .d.ts), fed the synthetic stream derived from the shared golden input — the
// same construction gen_golden uses — and checked against the Rust
// reference fixtures g_<Canonical>.csv.
//
//   wasm-pack build --target nodejs --out-dir pkg
//   node --test tests/

const test = require('node:test');
const assert = require('node:assert/strict');
const W = require('../pkg/wickra_wasm.js');
const {
  MANIFEST,
  NAMES,
  ROWS,
  closeEq,
  flat,
  isBars,
  nanRow,
  readCsv,
  readRows,
  resolveArg,
  tolFor,
  widthOf,
} = require('./harness.js');

for (const spec of MANIFEST) {
  test(`wasm golden: ${spec.canonical}`, () => {
    const Cls = W[spec.js];
    assert.ok(Cls, `missing WASM class ${spec.js}`);
    const ind = new Cls(...spec.ctor);
    const tol = tolFor(spec.canonical);
    assert.equal(ind.name(), NAMES[spec.canonical], `${spec.canonical}: name()`);
    const bars = isBars(spec);
    const expected = bars ? readRows('g_' + spec.canonical) : readCsv('g_' + spec.canonical);

    for (let i = 0; i < ROWS.length; i++) {
      const [o, h, l, c, v] = ROWS[i];
      const args = spec.args.map((a) => resolveArg(a, o, h, l, c, v, i));
      const raw = ind.update(...args);
      const want = expected[i];
      const label = `${spec.canonical} row ${i}`;

      let got = flat(raw);
      if (bars) {
        if (got === null) got = [];
      } else if (got === null) {
        got = nanRow(widthOf(spec));
      }
      assert.equal(got.length, want.length, `${label}: arity ${got.length} vs ${want.length}`);
      for (let k = 0; k < want.length; k++) closeEq(got[k], want[k], `${label} col ${k}`, tol);
    }
  });
}
