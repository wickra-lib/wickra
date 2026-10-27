// Completeness contract for the Wickra Node bindings: every exported indicator
// class must expose the full streaming + batch + lifecycle interface. This
// catches a new indicator being wired into the binding without the standard
// methods (or an export silently disappearing) without needing a hand-written
// test per indicator.

const test = require('node:test');
const assert = require('node:assert/strict');
const { readFileSync } = require('node:fs');
const { join } = require('node:path');
const wickra = require('..');

// Bar builders (Renko / Kagi / Point & Figure) implement the `BarBuilder`
// contract, not `Indicator`: they emit a variable number of completed bars per
// candle and have no fixed warmup or ready state. They expose update/batch/reset
// but intentionally not isReady/warmupPeriod, so they are excluded from the
// Indicator completeness contract below (their interface is covered by the
// dedicated bar-builder tests).
const BAR_BUILDERS = new Set([
  'RenkoBars',
  'KagiBars',
  'PointAndFigureBars',
  'RangeBars',
  'TickBars',
  'VolumeBars',
  'DollarBars',
  'ImbalanceBars',
  'RunBars',
  'ThreeLineBreakBars',
]);

// Data-layer types (tick aggregator, resampler, CSV candle reader) are not
// `Indicator`s: they transform raw market data into candles and have their own
// update/flush/read shape, so they are excluded from the streaming-indicator
// completeness contract.
const DATA_LAYER = new Set(['TickAggregator', 'Resampler', 'CandleReader']);

// An "indicator class" is an exported constructor whose prototype carries the
// streaming `update` method. This excludes `version` (a plain function), the bar
// builders, the data-layer types, and any non-indicator export.
function indicatorClasses() {
  return Object.keys(wickra).filter((name) => {
    const value = wickra[name];
    return (
      typeof value === 'function' &&
      value.prototype &&
      typeof value.prototype.update === 'function' &&
      !BAR_BUILDERS.has(name) &&
      !DATA_LAYER.has(name)
    );
  });
}

// The catalogue is a deliberate number, so assert it exactly: a `>=` bound
// passes just as happily when a partial native build drops a hundred classes.
// Adding an indicator means bumping this, which is the point.
const INDICATOR_CLASS_COUNT = 505;

test('the binding exports the full indicator catalogue', () => {
  const names = indicatorClasses();
  assert.equal(
    names.length,
    INDICATOR_CLASS_COUNT,
    `expected ${INDICATOR_CLASS_COUNT} indicator classes, got ${names.length}` +
      ' — if an indicator was added or removed, update INDICATOR_CLASS_COUNT',
  );
});

test('no export resolves to undefined', () => {
  // NAPI-RS re-exports its TypeScript-only aliases as runtime values. The
  // native module never registers those names, so `module.exports.SmaNode`
  // was `undefined` — 518 of 1038 exports. `npm run build` prunes them
  // (scripts/prune-type-only-exports.mjs); this is what notices if a build
  // skipped that step, or if the generator grows a new way to export nothing.
  const dead = Object.keys(wickra).filter((name) => wickra[name] === undefined);
  assert.deepEqual(dead, [], `exports resolving to undefined: ${dead.join(', ')}`);
});

test('index.d.ts and the native module declare the same classes', () => {
  const dts = readFileSync(join(__dirname, '..', 'index.d.ts'), 'utf8');
  const declared = [...dts.matchAll(/^export declare class ([A-Za-z0-9_$]+)/gm)]
    .map((match) => match[1])
    .sort();
  // A runtime class is an exported constructor carrying prototype methods;
  // `version` and `fetchBinanceKlines` are plain functions and are declared as
  // such.
  const runtime = Object.keys(wickra)
    .filter((name) => {
      const value = wickra[name];
      return (
        typeof value === 'function' &&
        value.prototype &&
        Object.getOwnPropertyNames(value.prototype).length > 1
      );
    })
    .sort();
  assert.deepEqual(runtime, declared);
});

test('the committed loader was generated at the current package version', () => {
  // `index.js` hard-codes the version each per-platform binding must report.
  // It is a generated file that is committed and published as-is, so it goes
  // stale silently: it sat at 0.9.7 through two releases, which would have
  // rejected every correctly matched platform package under
  // NAPI_RS_ENFORCE_VERSION_CHECK.
  const { version } = require('../package.json');
  const js = readFileSync(join(__dirname, '..', 'index.js'), 'utf8');
  const pinned = [...js.matchAll(/bindingPackageVersion !== '([^']+)'/g)].map((m) => m[1]);
  assert.ok(pinned.length > 0, 'expected index.js to pin a binding version');
  assert.deepEqual(
    [...new Set(pinned)],
    [version],
    `index.js pins ${[...new Set(pinned)].join(', ')} but package.json is ${version}` +
      ' — regenerate with `npm run build` and commit index.js',
  );
});

test('every exported indicator exposes update / batch / reset / isReady / warmupPeriod', () => {
  const required = ['update', 'batch', 'reset', 'isReady', 'warmupPeriod'];
  const missing = [];
  for (const name of indicatorClasses()) {
    const proto = wickra[name].prototype;
    for (const method of required) {
      if (typeof proto[method] !== 'function') {
        missing.push(`${name}.${method}`);
      }
    }
  }
  assert.deepEqual(
    missing,
    [],
    `indicator classes missing required methods: ${missing.join(', ')}`,
  );
});

test('a freshly constructed indicator reports not-ready with a positive warmup', () => {
  // Every indicator that takes no constructor arguments must still satisfy the
  // pre-warmup contract. (Indicators with required parameters are exercised by
  // the dedicated suites; here we cover the zero-arg ones generically.)
  let checked = 0;
  for (const name of indicatorClasses()) {
    let instance;
    try {
      instance = new wickra[name]();
    } catch {
      continue; // needs constructor arguments — covered elsewhere
    }
    assert.equal(instance.isReady(), false, `${name} should start un-ready`);
    assert.ok(instance.warmupPeriod() >= 1, `${name} warmup must be >= 1`);
    checked += 1;
  }
  assert.ok(checked > 0, 'expected at least one zero-arg indicator to check');
});
