// Bind generated bindings to the source snapshot that actually compiled them.
import { createHash } from 'node:crypto';
import { execFileSync } from 'node:child_process';
import { existsSync, lstatSync, readFileSync, readdirSync } from 'node:fs';
import { join } from 'node:path';

export const bindingFiles = ['package.json', 'pobr_wasm.js', 'pobr_wasm_bg.wasm', 'pobr_wasm.d.ts', 'pobr_wasm_bg.wasm.d.ts'];
export const sha256 = bytes => createHash('sha256').update(bytes).digest('hex');

export function sourceFingerprint(root) {
  const files = [];
  function collect(relative) {
    const path = join(root, relative);
    if (!existsSync(path)) return;
    const stat = lstatSync(path);
    if (stat.isSymbolicLink()) throw new Error(`Symlink in WASM build input: ${relative}`);
    if (stat.isDirectory()) {
      for (const name of readdirSync(path).sort()) collect(`${relative}/${name}`);
    } else if (stat.isFile()) files.push(relative);
  }
  for (const path of ['Cargo.toml', 'Cargo.lock', 'rust-toolchain', 'rust-toolchain.toml', '.cargo',
    'apps/pobr-wasm/Cargo.toml', 'apps/pobr-wasm/build.rs', 'apps/pobr-wasm/src', 'web/src/api/types.ts',
    'web/scripts/build-wasm.mjs', 'web/scripts/wasm-build-inputs.mjs']) collect(path);
  // All seven libraries are dependencies of the WASM engine; tests and runtime
  // game data are deliberately excluded from the compile-input fingerprint.
  if (existsSync(join(root, 'crates'))) {
    for (const crate of readdirSync(join(root, 'crates')).sort()) {
      for (const path of ['Cargo.toml', 'build.rs', 'src', 'locales']) collect(`crates/${crate}/${path}`);
    }
  }
  return sha256(JSON.stringify(files.sort().map(path => [path, sha256(readFileSync(join(root, path)))])));
}

export function bindingHashes(pkgRoot) {
  return Object.fromEntries(bindingFiles.map(file => {
    const path = join(pkgRoot, file);
    if (lstatSync(path).isSymbolicLink()) throw new Error(`Symlink in WASM binding: ${file}`);
    return [file, sha256(readFileSync(path))];
  }));
}

export function compiledSchema(pkgRoot) {
  // A fresh process avoids caching previously imported bindings after a rebuild.
  return Number(execFileSync(process.execPath, ['--input-type=module', '-e', `
    import { readFileSync } from 'node:fs';
    import { pathToFileURL } from 'node:url';
    import { join } from 'node:path';
    const root = process.argv[1];
    const engine = await import(pathToFileURL(join(root, 'pobr_wasm.js')));
    engine.initSync({ module: readFileSync(join(root, 'pobr_wasm_bg.wasm')) });
    console.log(engine.schemaVersion());
  `, pkgRoot], { encoding: 'utf8', timeout: 30_000 }).trim());
}

export function verifyBuildReceipt(root, pkgRoot) {
  const path = join(pkgRoot, 'build-receipt.json');
  if (!existsSync(path)) throw new Error('Missing WASM build receipt: run pnpm --dir web build-wasm');
  const receipt = JSON.parse(readFileSync(path, 'utf8'));
  if (receipt.version !== 1 || receipt.sourceFingerprint !== sourceFingerprint(root)) {
    throw new Error('Stale WASM source snapshot: run pnpm --dir web build-wasm');
  }
  const hashes = bindingHashes(pkgRoot);
  if (bindingFiles.some(file => receipt.files?.[file] !== hashes[file])) {
    throw new Error('WASM bindings differ from the build receipt: rebuild WASM');
  }
  const schema = compiledSchema(pkgRoot);
  if (!Number.isInteger(schema) || schema < 1 || schema !== receipt.schemaVersion) {
    throw new Error('WASM compiled schema differs from the build receipt');
  }
  return receipt;
}
