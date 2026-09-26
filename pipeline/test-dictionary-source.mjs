import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';
import { dictionarySource } from './dictionary-source.mjs';

test('dictionary refresh pins every file to one commit and reuses complete snapshots offline', async () => {
  const cacheRoot = await fs.mkdtemp(path.join(os.tmpdir(), 'pobr-dictionary-'));
  try {
    const ref = 'a'.repeat(40);
    const urls = [];
    const fetcher = async url => {
      urls.push(url);
      return { ok: true, json: async () => ({ sha: ref }), text: async () => '{"fixture":true}' };
    };
    const result = await dictionarySource({ cacheRoot, files: ['lookup/lines.json', 'meta.json'], refresh: true, fetcher });
    assert.equal(result.ref, ref);
    assert.equal(urls.length, 3);
    assert.equal(urls[0], 'https://api.github.com/repos/addohm/poe2-en-cn-dict/commits/HEAD');
    assert.ok(urls.slice(1).every(url => url.includes(`/${ref}/dictionary/`)));
    assert.deepEqual(await dictionarySource({ cacheRoot, files: ['lookup/lines.json', 'meta.json'], ref,
      fetcher: () => { throw new Error('Unexpected network request'); } }), result);
  } finally { await fs.rm(cacheRoot, { recursive: true, force: true }); }
});

test('failed or invalid default HEAD lookup does not start a dictionary download', async () => {
  const cacheRoot = await fs.mkdtemp(path.join(os.tmpdir(), 'pobr-dictionary-'));
  try {
    for (const response of [{ ok: false, status: 422 }, { ok: true, json: async () => ({ sha: 'main' }) }]) {
      let calls = 0;
      await assert.rejects(dictionarySource({ cacheRoot, files: ['meta.json'], refresh: true,
        fetcher: async url => {
          assert.equal(url, 'https://api.github.com/repos/addohm/poe2-en-cn-dict/commits/HEAD');
          calls++;
          return response;
        } }), /HTTP 422|Invalid dictionary commit response/);
      assert.equal(calls, 1);
      assert.deepEqual(await fs.readdir(cacheRoot), []);
    }
  } finally { await fs.rm(cacheRoot, { recursive: true, force: true }); }
});

test('failed downloads leave no mixed or incomplete snapshot', async () => {
  const cacheRoot = await fs.mkdtemp(path.join(os.tmpdir(), 'pobr-dictionary-'));
  try {
    let calls = 0;
    await assert.rejects(dictionarySource({ cacheRoot, files: ['first.json', 'second.json'], ref: 'b'.repeat(40),
      fetcher: async () => ++calls === 1
        ? { ok: true, text: async () => '{}' }
        : { ok: false, status: 503 } }), /503/);
    assert.deepEqual(await fs.readdir(cacheRoot), []);
    await assert.rejects(dictionarySource({ cacheRoot, files: [], ref: 'master' }), /commit SHA/);
  } finally { await fs.rm(cacheRoot, { recursive: true, force: true }); }
});
