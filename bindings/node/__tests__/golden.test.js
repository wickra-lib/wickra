// Generic golden-fixture parity for the Node binding.
//
// Every one of the 515 indicators is reconstructed from `node_manifest.json`
// (native class, constructor params, ordered update args), fed the synthetic
// stream derived from the shared `testdata/golden/input.csv` — the exact same
// construction the Rust `gen_golden` binary uses — and checked
// against the Rust-generated `g_<Canonical>.csv`. This pins the Node FFI to the
// Rust reference for the whole indicator catalogue, not just a few archetypes.
//
//   cd bindings/node && npm run build && npm test

const test = require('node:test');
const assert = require('node:assert/strict');
const wickra = require('..');
const {
  MANIFEST,
  NAMES,
  ROWS,
  closeEq,
  readBarRows,
  readCsv,
  resolveArg,
  tolFor,
} = require('./harness.js');

for (const spec of MANIFEST) {
  test(`golden: ${spec.canonical}`, () => {
    const Cls = wickra[spec.native];
    assert.ok(Cls, `missing Node class ${spec.native}`);
    const ind = new Cls(...spec.ctor);
    const tol = tolFor(spec.canonical);
    // name() must report the canonical core Indicator::name(), the same string
    // every other binding returns for this indicator.
    assert.equal(ind.name(), NAMES[spec.canonical], `${spec.canonical}: name()`);
    const isBars = spec.out === 'bars' || spec.out === 'footprint';
    const expected = isBars ? readBarRows('g_' + spec.canonical) : readCsv('g_' + spec.canonical);

    for (let i = 0; i < ROWS.length; i++) {
      const [o, h, l, c, v] = ROWS[i];
      const args = spec.args.map((a) => resolveArg(a, o, h, l, c, v, i));
      const got = ind.update(...args);
      const want = expected[i];
      const label = `${spec.canonical} row ${i}`;

      if (spec.out === 'scalar') {
        closeEq(got === null || got === undefined ? NaN : got, want[0], label, tol);
      } else if (spec.out === 'multi') {
        if (got === null || got === undefined) {
          assert.ok(want.every(Number.isNaN), `${label}: want ${want} got null`);
          continue;
        }
        // napi serialises the output struct's fields in declaration order, which
        // matches the CSV column order — compare positionally to avoid relying on
        // the exact camelCase of each field name.
        const vals = Object.values(got);
        assert.equal(vals.length, want.length, `${label}: arity ${vals.length} vs ${want.length}`);
        vals.forEach((gv, k) => {
          closeEq(gv === null || gv === undefined ? NaN : gv, want[k], `${label} col ${k}`, tol);
        });
      } else if (spec.out === 'profile_bins') {
        if (got === null || got === undefined) {
          assert.ok(want.every(Number.isNaN), `${label}: want all-NaN got null`);
          continue;
        }
        assert.equal(got.length, want.length, `${label}: width ${got.length} vs ${want.length}`);
        got.forEach((gv, k) => closeEq(gv, want[k], `${label} bin ${k}`, tol));
      } else if (spec.out === 'profile_pricebins') {
        if (got === null || got === undefined) {
          assert.ok(want.every(Number.isNaN), `${label}: want all-NaN got null`);
          continue;
        }
        const flat = [got.priceLow, got.priceHigh, ...got[spec.arrayField]];
        assert.equal(flat.length, want.length, `${label}: width ${flat.length} vs ${want.length}`);
        flat.forEach((gv, k) => closeEq(gv, want[k], `${label} col ${k}`, tol));
      } else {
        // bars / footprint: flatten array-of-objects in declared field order.
        const flat = [];
        for (const bar of got) {
          for (const f of spec.fields) flat.push(Number(bar[f]));
        }
        assert.equal(flat.length, want.length, `${label}: arity ${flat.length} vs ${want.length}`);
        flat.forEach((gv, k) => closeEq(gv, want[k], `${label} col ${k}`, tol));
      }
    }
  });
}
