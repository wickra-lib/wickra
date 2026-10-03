// wasm-pack lists the generated files in package.json's `files`, but not the
// `snippets/` directory holding the `#[wasm_bindgen(inline_js)]` modules, so
// `npm pack` leaves it out and the packed module fails to load. Add it.
//
//   node bindings/wasm/scripts/package-files.mjs bindings/wasm/pkg
import { existsSync, readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';

const dir = process.argv[2] ?? '.';
const file = join(dir, 'package.json');
const pkg = JSON.parse(readFileSync(file, 'utf8'));
if (existsSync(join(dir, 'snippets'))) {
  pkg.files = [...new Set([...(pkg.files ?? []), 'snippets/'])];
  writeFileSync(file, `${JSON.stringify(pkg, null, 2)}\n`);
}
