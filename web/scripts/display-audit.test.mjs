import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { test } from 'node:test';

test('display audit loads its TypeScript display modules through native Node before opening the package', () => {
  // This directory has no WASM package. Reaching its missing data file proves
  // the real CLI's display imports passed Node's strip-only loader.
  const packageDir = fileURLToPath(new URL('./', import.meta.url));
  const missingData = fileURLToPath(new URL('./pobr-data.json.gz', import.meta.url));
  const result = spawnSync(process.execPath, [
    fileURLToPath(new URL('./display-audit.mjs', import.meta.url)),
    '--strict', '--package', packageDir, 'unused-build-code.txt',
  ], { encoding: 'utf8', timeout: 10_000 });

  assert.ifError(result.error);
  assert.equal(result.status, 1, result.stderr);
  assert.match(result.stderr, /ENOENT/);
  assert.ok(result.stderr.includes(missingData), result.stderr);
});
