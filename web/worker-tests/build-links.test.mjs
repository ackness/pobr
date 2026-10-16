import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { test } from 'node:test';
import { Miniflare } from 'miniflare';

const code = readFileSync(new URL('./fixtures/build-code.txt', import.meta.url), 'utf8').trim();
const fixtures = JSON.parse(readFileSync(new URL('./fixtures/build-links.json', import.meta.url), 'utf8'));
const endpoint = 'https://pobr.test/api/import/build-code';
const options = { modules: true, cf: false, compatibilityDate: '2026-07-30',
  scriptPath: fileURLToPath(new URL('../public/_worker.js', import.meta.url)) };
const request = url => ({ method: 'POST', headers: { origin: 'https://pobr.test',
  'content-type': 'application/json', cookie: 'private=secret', authorization: 'Bearer private' },
  body: JSON.stringify({ url }) });

test('all supported providers return the recorded code from fixed raw endpoints', async t => {
  const seen = [];
  const mf = new Miniflare({ ...options, outboundService: async request => {
    seen.push(request.url);
    assert.equal(request.method, 'GET');
    assert.equal(request.headers.get('cookie'), null);
    assert.equal(request.headers.get('authorization'), null);
    return new Response(`\n${code}\n`);
  } });
  t.after(() => mf.dispose());
  for (const fixture of fixtures) {
    const response = await mf.dispatchFetch(endpoint, request(`${fixture.url}?tracking=ignored#fragment`));
    assert.equal(response.status, 200);
    assert.equal(response.headers.get('cache-control'), 'no-store');
    assert.deepEqual(await response.json(), { code });
  }
  assert.deepEqual(seen, fixtures.map(fixture => fixture.target));
});

test('invalid hosts, paths, origins, methods and oversized requests never reach upstream', async t => {
  let calls = 0;
  const mf = new Miniflare({ ...options, outboundService: async () => { calls++; return new Response(code); } });
  t.after(() => mf.dispose());
  for (const [url, error] of [
    ['https://pobb.in.evil/id', 'unsupported_host'], ['https://127.0.0.1/id', 'unsupported_host'],
    ['https://pobb.in/a/b', 'invalid_url'], ['https://pobb.in/a%2Fb', 'invalid_url'],
    ['https://user:pass@pobb.in/id', 'invalid_url'], ['http://pobb.in/id', 'invalid_url'],
    ['https://poe.ninja/poe2/builds/league/character/account/name', 'ninja_profile'],
  ]) {
    const response = await mf.dispatchFetch(endpoint, request(url));
    assert.equal(response.status, 400);
    assert.deepEqual(await response.json(), { error_code: error });
  }
  assert.equal((await mf.dispatchFetch(endpoint)).status, 405);
  assert.equal((await mf.dispatchFetch(endpoint, { ...request(fixtures[0].url), headers: { origin: 'https://evil.test' } })).status, 403);
  assert.equal((await mf.dispatchFetch(endpoint, request('x'.repeat(5000)))).status, 400);
  assert.equal(calls, 0);
});

test('upstream failures, redirects, invalid content and size limits are bounded errors', async t => {
  let responseFactory, calls = 0;
  const mf = new Miniflare({ ...options, outboundService: async () => { calls++; return responseFactory(); } });
  t.after(() => mf.dispose());
  for (const [factory, status, error] of [
    [() => new Response('missing', { status: 404 }), 404, 'not_found'],
    [() => new Response('private', { status: 403 }), 502, 'unavailable'],
    [() => new Response('limited', { status: 429 }), 502, 'unavailable'],
    [() => new Response(null, { status: 302, headers: { location: 'https://evil.test' } }), 502, 'upstream'],
    [() => new Response('<html>error</html>'), 502, 'invalid_response'],
    [() => new Response(''), 502, 'invalid_response'],
    [() => new Response('a'.repeat(1024 * 1024 + 1)), 502, 'too_large'],
    [() => new Response(new ReadableStream({ start(controller) {
      controller.enqueue(new TextEncoder().encode('a'.repeat(1024 * 1024)));
      controller.enqueue(new Uint8Array([97])); controller.close();
    } })), 502, 'too_large'],
  ]) {
    responseFactory = factory;
    const before = calls;
    const response = await mf.dispatchFetch(endpoint, request(fixtures[0].url));
    assert.equal(response.status, status);
    assert.deepEqual(await response.json(), { error_code: error });
    assert.equal(calls, before + 1, 'no redirects or retries');
  }
});
