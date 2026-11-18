import assert from 'node:assert/strict';
import { mkdtemp, mkdir, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { test } from 'node:test';
import worker from '../public/_worker.js';
import { startPreview } from './preview.mjs';

test('public preview serves built assets and SPA routes without loading the dev config or APIs', async t => {
  const root = await mkdtemp(join(tmpdir(), 'pobr-preview-'));
  t.after(() => rm(root, { recursive: true, force: true }));
  const outDir = join(root, 'tunnel-output');
  await mkdir(outDir);
  await mkdir(join(root, 'dist'));
  const wasm = Buffer.from([0, 97, 115, 109, 1, 0, 0, 0]);
  const html = '<!doctype html><title>PoBR preview fixture</title>';
  await Promise.all([
    writeFile(join(outDir, 'index.html'), html),
    writeFile(join(outDir, 'engine.wasm'), wasm),
    writeFile(join(root, 'vite.config.mjs'), 'throw new Error("do not load the local Vite config");'),
  ]);
  const server = await startPreview(root, 0, { outDir });
  t.after(() => server.close());
  // A normal local rebuild may update dist while the tunnel is online.
  await writeFile(join(root, 'dist/index.html'), '<title>Local preview</title>');
  await writeFile(join(root, 'dist/private.txt'), 'local-only fixture');
  assert.equal(server.httpServer.address().address, '127.0.0.1');
  const base = `http://127.0.0.1:${server.httpServer.address().port}`;
  const route = await fetch(`${base}/items`, { headers: { accept: 'text/html' } });
  assert.equal(route.status, 200);
  assert.equal(await route.text(), html);
  const binary = await fetch(`${base}/engine.wasm`);
  assert.equal(binary.headers.get('content-type'), 'application/wasm');
  assert.deepEqual(Buffer.from(await binary.arrayBuffer()), wasm);
  const api = await fetch(`${base}/api/trade/search`, { method: 'POST' });
  assert.equal(api.status, 404);
  assert.deepEqual(await api.json(), { error: 'This API is disabled in the public preview.' });
  const wegame = await fetch(`${base}/api/import/wegame`, { method: 'POST' });
  assert.equal(wegame.status, 404);
  await wegame.text();
  const privateFile = await fetch(`${base}/private.txt`, { headers: { accept: 'text/plain' } });
  assert.equal(privateFile.status, 404);
  await privateFile.text();
});

test('opt-in forwards only same-origin WeGame imports to the existing Worker', async t => {
  const root = await mkdtemp(join(tmpdir(), 'pobr-preview-wegame-'));
  t.after(() => rm(root, { recursive: true, force: true }));
  const outDir = join(root, 'tunnel-output');
  await mkdir(outDir);
  let publicOrigin;
  const server = await startPreview(root, 0, { outDir, wegame: { origin: () => publicOrigin, worker } });
  t.after(() => server.close());
  const base = `http://127.0.0.1:${server.httpServer.address().port}`;
  const fetchLocal = fetch;
  const payloads = {
    GetRoleInfo: { role: { level: 59, class_name: 'Deadeye', openid: 'private' } },
    GetEquipments: { equipments: [{ baseType: 'Linen Belt', id: 'private' }] },
    GetTalentTree: { talent_tree: { hashes: [], quest_stats: [] } },
    GetJewels: { jewel_data: '[]' },
    GetSkills: { skills: [] },
  };
  const methods = [];
  let expired = false;
  t.mock.method(globalThis, 'fetch', async (url, init) => {
    const target = new URL(url);
    const method = target.pathname.split('/').at(-1);
    assert.equal(target.origin, 'https://www.wegame.com.cn');
    assert.ok(Object.hasOwn(payloads, method));
    assert.equal(init.credentials, 'omit');
    assert.equal(init.redirect, 'manual');
    assert.equal(new Headers(init.headers).has('cookie'), false);
    assert.equal(JSON.parse(init.body).share_code, 'SyntheticShareKey_123456');
    methods.push(method);
    return Response.json({ result: { error_code: expired ? 1 : 0 }, ...payloads[method] });
  });

  const origin = 'https://preview-example.trycloudflare.com';
  const post = (path, url, headers = {}) => fetchLocal(`${base}${path}`, {
    method: 'POST', headers: { 'content-type': 'application/json', origin, ...headers },
    body: JSON.stringify({ url }),
  });
  const path = '/api/import/wegame';
  const share = 'https://www.wegame.com.cn/helper/poe2/#/share/SyntheticShareKey_123456';
  assert.equal((await post(path, share)).status, 503);
  publicOrigin = origin;
  assert.equal((await fetchLocal(`${base}${path}`)).status, 405);
  assert.equal((await post('/api/import/build-code', share)).status, 404);
  assert.equal((await post('/api/trade/search', share)).status, 404);
  assert.equal((await post(path, share, { origin: 'https://unrelated.trycloudflare.com' })).status, 403);
  assert.equal((await post(path, 'x'.repeat(2049))).status, 413);
  const invalid = await post(path, 'https://example.com/');
  assert.equal(invalid.status, 400);
  assert.equal((await invalid.json()).error_code, 'invalid_url');
  assert.equal(methods.length, 0);

  const imported = await post(path, share, { cookie: 'not-forwarded=1' });
  assert.equal(imported.status, 200);
  assert.equal(imported.headers.get('cache-control'), 'no-store');
  const bundle = await imported.json();
  assert.equal(bundle.format, 'wegame');
  assert.deepEqual(bundle.role, { level: 59, class_name: 'Deadeye' });
  assert.deepEqual(bundle.equipments, [{ baseType: 'Linen Belt' }]);
  assert.deepEqual(methods.sort(), Object.keys(payloads).sort());
  expired = true;
  const unavailable = await post(path, share);
  assert.equal(unavailable.status, 424);
  assert.equal((await unavailable.json()).error_code, 'not_found');
});
