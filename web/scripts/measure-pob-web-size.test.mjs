import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { test } from 'node:test';
import { gzipSync } from 'node:zlib';
import { measureReferenceArtifacts } from './measure-pob-web-size.mjs';

const sha256 = bytes => createHash('sha256').update(bytes).digest('hex');

test('runtime sizes distinguish raw files from compressed data and never double-compress transfer data', () => {
  const wasm = Buffer.from('wasm bytes '.repeat(300));
  const js = Buffer.from('JavaScript bytes '.repeat(100));
  const data = Buffer.from(JSON.stringify({ files: { 'data.lua': 'Lua data '.repeat(1000) } }));
  const [wasmSize, jsSize, dataSize] = measureReferenceArtifacts({ wasm, js, data });
  assert.equal(wasmSize.bytes, wasm.length);
  assert.equal(jsSize.bytes, js.length);
  assert.equal(wasmSize.gzipBytes, gzipSync(wasm, { level: 9 }).length);
  assert.equal(jsSize.gzipBytes, gzipSync(js, { level: 9 }).length);
  const compressedData = gzipSync(data, { level: 9 });
  assert.equal(dataSize.bytes, compressedData.length);
  assert.equal(dataSize.gzipBytes, compressedData.length);
  assert.equal(dataSize.uncompressedBytes, data.length);
  assert.equal(dataSize.sha256, sha256(compressedData));
  assert.equal(dataSize.uncompressedSha256, sha256(data));
});

test('missing or empty artifact bytes cannot produce a valid size baseline', () => {
  const valid = { wasm: Buffer.from('wasm'), js: Buffer.from('js'), data: Buffer.from('{"files":{}}') };
  for (const key of ['wasm', 'js', 'data']) {
    for (const invalid of [undefined, null, '', 'non-buffer', Buffer.alloc(0)]) {
      assert.throws(() => measureReferenceArtifacts({ ...valid, [key]: invalid }), /nonempty/);
    }
  }
});
