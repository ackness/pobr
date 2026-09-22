// Validate the shipped archive in isolation, without source-tree data or JS imports.
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { execFileSync } from 'node:child_process';
import { mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = fileURLToPath(new URL('../../', import.meta.url));
const output = join(root, '.cache/wasm-release');
const report = JSON.parse(readFileSync(join(output, 'wasm-size.json'), 'utf8'));
const temp = mkdtempSync(join(tmpdir(), 'pobr-wasm-smoke-'));
try {
  for (const line of readFileSync(join(output, 'SHA256SUMS'), 'utf8').trim().split('\n')) {
    const [expected, name] = line.split('  ');
    assert.equal(createHash('sha256').update(readFileSync(join(output, name))).digest('hex'), expected, name);
  }
  execFileSync('tar', ['-xzf', join(output, report.archive.name), '-C', temp]);
  const cwd = join(temp, report.archive.name.replace(/\.tar\.gz$/, ''));
  for (const file of report.files) {
    const bytes = readFileSync(join(cwd, file.name));
    assert.equal(bytes.length, file.bytes, file.name);
    assert.equal(createHash('sha256').update(bytes).digest('hex'), file.sha256, file.name);
  }
  const fixture = join(root, 'examples/demo-bd-test/builds/monk-invoker-frost-bomb/code.txt');
  // Run both the convenient root example and the portable skill's own script.
  execFileSync(process.execPath, ['example.mjs'], { cwd, stdio: 'inherit', timeout: 60_000 });
  execFileSync(process.execPath, ['skills/use-pobr-wasm/scripts/demo.mjs', '--package', cwd, '--build-code', fixture],
    { cwd, stdio: 'inherit', timeout: 60_000 });
} finally {
  rmSync(temp, { recursive: true, force: true });
}
