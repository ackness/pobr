import assert from 'node:assert/strict';
import { fileURLToPath } from 'node:url';
import { test } from 'node:test';
import { Miniflare } from 'miniflare';

// The deployed module in workerd, with the static asset host replaced by one that answers like
// Pages does (revalidate every visit) for the files it has. What a visitor is told to keep, and
// what not, is what is under test.
const revalidate = 'public, max-age=0, must-revalidate';
const assets = new Map([
  ['/data/manifest.json', 'manifest'],
  ['/data/0.5.5/base/mods.json', '[]'],
  ['/assets/entry-DlLWRb60.js', 'js'],
  ['/assets/pobr_wasm_bg-Ab_-9xYz.wasm', 'wasm'],
  ['/assets/unhashed.js', 'js'],
  ['/index.html', 'html'],
]);

async function fetchFrom(t, path, init) {
  const seen = [];
  const mf = new Miniflare({
    modules: true,
    scriptPath: fileURLToPath(new URL('../public/_worker.js', import.meta.url)),
    compatibilityDate: '2026-07-30',
    cf: false,
    serviceBindings: {
      ASSETS: async (request) => {
        const { pathname } = new URL(request.url);
        seen.push(request.url);
        return assets.has(pathname)
          ? new Response(assets.get(pathname), { headers: { 'cache-control': revalidate, etag: '"abc"' } })
          : new Response('not found', { status: 404, headers: { 'cache-control': revalidate } });
      },
    },
  });
  t.after(() => mf.dispose());
  const response = await mf.dispatchFetch(`https://app.test${path}`, init);
  return { response, seen };
}

const digest = 'a'.repeat(16);

test('a data file asked for by its digest is kept for good, and is otherwise untouched', async (t) => {
  const { response, seen } = await fetchFrom(t, `/data/0.5.5/base/mods.json?h=${digest}`);
  assert.equal(response.status, 200);
  assert.equal(response.headers.get('cache-control'), 'public, max-age=31536000, immutable');
  assert.equal(response.headers.get('etag'), '"abc"');
  assert.equal(await response.text(), '[]');
  // The host is asked for the same URL, digest and all.
  assert.deepEqual(seen, [`https://app.test/data/0.5.5/base/mods.json?h=${digest}`]);
});

test('a data file asked for without a usable digest is revalidated as before', async (t) => {
  for (const query of ['', '?h=', '?h=xyz', `?h=${'A'.repeat(16)}`, `?h=${'a'.repeat(15)}`, `?h=${'a'.repeat(65)}`]) {
    const { response } = await fetchFrom(t, `/data/0.5.5/base/mods.json${query}`);
    assert.equal(response.headers.get('cache-control'), revalidate, query);
  }
});

test('the manifest, which names what to ask for, is never kept', async (t) => {
  const { response } = await fetchFrom(t, `/data/manifest.json?h=${digest}`);
  assert.equal(response.headers.get('cache-control'), revalidate);
});

test('the bundler\'s content-hashed files are kept for good, the others are not', async (t) => {
  for (const path of ['/assets/entry-DlLWRb60.js', '/assets/pobr_wasm_bg-Ab_-9xYz.wasm']) {
    const { response } = await fetchFrom(t, path);
    assert.equal(response.headers.get('cache-control'), 'public, max-age=31536000, immutable', path);
  }
  for (const path of ['/assets/unhashed.js', '/index.html']) {
    const { response } = await fetchFrom(t, path);
    assert.equal(response.headers.get('cache-control'), revalidate, path);
  }
});

test('only a found file is kept: a miss and a write are passed through', async (t) => {
  const miss = await fetchFrom(t, `/data/0.5.5/base/missing.json?h=${digest}`);
  assert.equal(miss.response.status, 404);
  assert.equal(miss.response.headers.get('cache-control'), revalidate);
  const write = await fetchFrom(t, `/data/0.5.5/base/mods.json?h=${digest}`, { method: 'POST' });
  assert.notEqual(write.response.headers.get('cache-control'), 'public, max-age=31536000, immutable');
});
