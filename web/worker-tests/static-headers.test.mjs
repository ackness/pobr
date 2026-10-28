import assert from 'node:assert/strict';
import { existsSync, readFileSync } from 'node:fs';
import { test } from 'node:test';

// Pages serves the static files itself (the worker only gets the routes in _routes.json), so what a
// visitor is told to keep forever is decided by public/_headers. A rule matches a path, so it may
// only name paths that can never hold other content. These tests hold the file to that.
const read = (name) => readFileSync(new URL(`../public/${name}`, import.meta.url), 'utf8');

function rules() {
  const out = [];
  for (const line of read('_headers').split('\n')) {
    if (!line.trim() || line.trim().startsWith('#')) continue;
    if (/^\S/.test(line)) out.push({ pattern: line.trim(), headers: {} });
    else {
      const [name, ...value] = line.trim().split(':');
      out.at(-1).headers[name.trim().toLowerCase()] = value.join(':').trim();
    }
  }
  return out;
}
const matches = (pattern, path) => pattern.endsWith('*') ? path.startsWith(pattern.slice(0, -1)) : path === pattern;
const immutable = (path) => rules().some((rule) => matches(rule.pattern, path) && /\bimmutable\b/.test(rule.headers['cache-control'] ?? ''));

test('only content-addressed paths are kept for good', () => {
  const kept = rules().filter((rule) => /\bimmutable\b/.test(rule.headers['cache-control'] ?? '')).map((rule) => rule.pattern);
  assert.deepEqual(kept.sort(), ['/assets/*', '/data/h/*']);
  for (const rule of rules()) {
    assert.match(rule.headers['cache-control'], /max-age=31536000/, rule.pattern);
  }
});

test('the files that must be revalidated every visit are not matched', () => {
  const digest = 'a'.repeat(16);
  assert.equal(immutable(`/data/h/${digest}/base/mods.json`), true);
  assert.equal(immutable('/assets/entry-DlLWRb60.js'), true);
  for (const path of ['/', '/index.html', '/v2/build', '/data/manifest.json', '/data/0.5.5.4.2/base/mods.json',
    '/data/0.5.5.4.2/manifest.json', '/tree-art/0.5/manifest.json', '/_worker.js', '/_routes.json']) {
    assert.equal(immutable(path), false, path);
  }
});

test('static files never reach the worker, which is why their headers live in _headers', () => {
  const { include, exclude } = JSON.parse(read('_routes.json'));
  const routed = (path) => include.some((pattern) => matches(pattern, path)) && !exclude.some((pattern) => matches(pattern, path));
  for (const path of ['/assets/entry-DlLWRb60.js', `/data/h/${'a'.repeat(16)}/base/mods.json`, '/data/manifest.json']) {
    assert.equal(routed(path), false, `${path} would be served by the worker, not by the host's headers`);
  }
  assert.equal(routed('/api/trade/search'), true);
});

// A path that names no file is answered with the nearest `404.html` up the path, with a 404 status that
// the host never lets be kept; only when there is none up to the root is it answered with the app shell
// and a 200, which is what client routes like /v2/build rely on. Without a page of its own, a missing
// file under a path `_headers` keeps for good would be answered with the shell and kept for good.
const notFoundPage = (path, pages) => {
  for (let dir = path; dir;) {
    dir = dir.slice(0, dir.lastIndexOf('/'));
    if (pages.includes(`${dir}/404.html`)) return `${dir}/404.html`;
  }
  return null;
};
// The pages the build ships: this one is a file of the repo, the one under /data/h/ is written by scripts/sync-data.mjs.
const pages = ['/assets/404.html', '/data/h/404.html'];

test('a missing file under a path that is kept for good gets a 404 page, not the app shell', () => {
  assert.equal(existsSync(new URL('../public/assets/404.html', import.meta.url)), true);
  assert.doesNotMatch(read('assets/404.html'), /<script|<link|src=/i);
  assert.equal(notFoundPage('/assets/entry-Missing1.js', pages), '/assets/404.html');
  assert.equal(notFoundPage('/assets/nested/chunk-Missing1.js', pages), '/assets/404.html');
  assert.equal(notFoundPage(`/data/h/${'b'.repeat(16)}/base/mods.json`, pages), '/data/h/404.html');
  // Every kept-for-good rule has one, so that no path it matches can be answered with the shell.
  for (const rule of rules().filter((entry) => /\bimmutable\b/.test(entry.headers['cache-control'] ?? ''))) {
    assert.notEqual(notFoundPage(`${rule.pattern.slice(0, -1)}missing-file.js`, pages), null, rule.pattern);
  }
});

test('client routes still fall back to the app shell, so no 404 page may sit above them', () => {
  for (const path of ['/', '/v2/build', '/v2/tree', '/v2/calcs', '/some/unknown/route', '/data/manifest.json']) {
    assert.equal(notFoundPage(path, pages), null, path);
  }
  // A 404.html at the top, or one in a directory a route lives in, would turn every such route into a 404.
  assert.equal(existsSync(new URL('../public/404.html', import.meta.url)), false);
  assert.equal(existsSync(new URL('../public/v2/404.html', import.meta.url)), false);
});
