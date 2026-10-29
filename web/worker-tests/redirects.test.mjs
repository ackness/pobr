import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';

// Pages applies public/_redirects before it serves a file: first match wins, `*` captures the rest of the
// path as `:splat`, and the visitor's query string follows to the destination.
const rules = readFileSync(new URL('../public/_redirects', import.meta.url), 'utf8')
  .split('\n').map((line) => line.trim()).filter((line) => line && !line.startsWith('#'))
  .map((line) => { const [from, to, status] = line.split(/\s+/); return { from, to, status: Number(status) }; });

function redirect(path) {
  for (const { from, to, status } of rules) {
    if (from.endsWith('*') ? path.startsWith(from.slice(0, -1)) : path === from) {
      return { status, to: to.replace(':splat', from.endsWith('*') ? path.slice(from.length - 1) : '') };
    }
  }
  return null;
}

test('links from the time the app lived under /v2 land on the same page at the site root', () => {
  assert.deepEqual(redirect('/v2'), { status: 301, to: '/' });
  assert.deepEqual(redirect('/v2/'), { status: 301, to: '/' });
  assert.deepEqual(redirect('/v2/tree'), { status: 301, to: '/tree' });
  assert.deepEqual(redirect('/v2/build'), { status: 301, to: '/build' });
});

test('nothing else is redirected', () => {
  for (const path of ['/', '/tree', '/calcs', '/v20', '/assets/entry-DlLWRb60.js', '/data/manifest.json', '/api/trade/search']) {
    assert.equal(redirect(path), null, path);
  }
});
