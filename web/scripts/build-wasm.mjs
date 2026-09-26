import { execFileSync } from 'node:child_process';
import { rmSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { bindingHashes, compiledSchema, sourceFingerprint } from './wasm-build-inputs.mjs';

const root = fileURLToPath(new URL('../../', import.meta.url));
const pkgRoot = join(root, 'web/src/wasm/pkg');
const receiptPath = join(pkgRoot, 'build-receipt.json');
rmSync(receiptPath, { force: true });
const fingerprint = sourceFingerprint(root);
const commit = execFileSync('git', ['rev-parse', 'HEAD'], { cwd: root, encoding: 'utf8' }).trim();
const dirty = execFileSync('git', ['status', '--porcelain', '--untracked-files=normal'], { cwd: root, encoding: 'utf8' }).trim() !== '';
const args = ['build', 'apps/pobr-wasm', '--target', 'web', '--out-dir', '../../web/src/wasm/pkg', '--out-name', 'pobr_wasm', '--', '--features', 'wasm'];
execFileSync('wasm-pack', args, { cwd: root, stdio: 'inherit' });
if (sourceFingerprint(root) !== fingerprint) throw new Error('WASM sources changed during compilation; rebuild before packaging');
const receipt = {
  version: 1, sourceFingerprint: fingerprint, commit, dirty,
  schemaVersion: compiledSchema(pkgRoot), target: 'web', features: ['wasm'],
  rustc: execFileSync('rustc', ['--version'], { cwd: root, encoding: 'utf8' }).trim(),
  wasmPack: execFileSync('wasm-pack', ['--version'], { cwd: root, encoding: 'utf8' }).trim(),
  files: bindingHashes(pkgRoot),
};
writeFileSync(receiptPath, JSON.stringify(receipt, null, 2) + '\n');
