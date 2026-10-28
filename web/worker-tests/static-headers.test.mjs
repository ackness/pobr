import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
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
