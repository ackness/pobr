import { execFileSync } from 'node:child_process';
import { rmSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { bindingHashes, buildInputs, compiledSchema, sourceFingerprint, verifyBuildReceipt } from './wasm-build-inputs.mjs';

const root = fileURLToPath(new URL('../../', import.meta.url));
const pkgRoot = join(root, 'web/src/wasm/pkg');
const receiptPath = join(pkgRoot, 'build-receipt.json');
const options = process.argv.slice(2);
if (options.some(option => option !== '--force')) throw new Error('Usage: build-wasm.mjs [--force]');
const fingerprint = sourceFingerprint(root);
const inputs = buildInputs(root);
let reusable = false;
if (!options.includes('--force')) {
  try {
    const receipt = verifyBuildReceipt(root, pkgRoot, fingerprint);
    reusable = JSON.stringify(receipt.buildInputs) === JSON.stringify(inputs);
  } catch {
    // Missing, stale or damaged generated output must be rebuilt.
  }
}
if (reusable && sourceFingerprint(root) === fingerprint) {
  console.log('Reusing verified WASM bindings');
  process.exit(0);
}
rmSync(receiptPath, { force: true });
const commit = execFileSync('git', ['rev-parse', 'HEAD'], { cwd: root, encoding: 'utf8' }).trim();
const dirty = execFileSync('git', ['status', '--porcelain', '--untracked-files=normal'], { cwd: root, encoding: 'utf8' }).trim() !== '';
const args = ['build', 'apps/pobr-wasm', '--target', 'web', '--out-dir', '../../web/src/wasm/pkg', '--out-name', 'pobr_wasm', '--', '--features', 'wasm'];
execFileSync('wasm-pack', args, { cwd: root, stdio: 'inherit' });
if (sourceFingerprint(root) !== fingerprint) throw new Error('WASM sources changed during compilation; rebuild before packaging');
if (JSON.stringify(buildInputs(root)) !== JSON.stringify(inputs)) throw new Error('WASM build inputs changed during compilation; rebuild before packaging');
const receipt = {
  version: 1, sourceFingerprint: fingerprint, commit, dirty,
  schemaVersion: compiledSchema(pkgRoot), target: 'web', features: ['wasm'],
  rustc: inputs.rustc, wasmPack: inputs.wasmPack, buildInputs: inputs,
  files: bindingHashes(pkgRoot),
};
writeFileSync(receiptPath, JSON.stringify(receipt, null, 2) + '\n');
