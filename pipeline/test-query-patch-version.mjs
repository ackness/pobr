import { strict as assert } from 'node:assert';
import net from 'node:net';
import { test } from 'node:test';
import { queryPatchVersion } from './query-patch-version.mjs';

async function withServer(t, respond) {
  const server = net.createServer(socket => socket.once('data', handshake => {
    assert.deepEqual(handshake, Buffer.from([1, 7]));
    respond(socket);
  }));
  await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
  t.after(() => new Promise(resolve => server.close(resolve)));
  return { host: '127.0.0.1', port: server.address().port, timeoutMs: 1000 };
}

test('discovers an unseen version across arbitrary TCP and UTF-16 boundaries', async t => {
  const cdn = 'https://patch-poe2.poecdn.com/9.12.345.6/';
  const address = await withServer(t, socket => {
    const bytes = Buffer.concat([Buffer.from([0, 2, 0]), Buffer.from(cdn, 'utf16le')]);
    socket.write(bytes.subarray(0, 13));
    setTimeout(() => socket.write(bytes.subarray(13, 44)), 5);
    setTimeout(() => socket.end(bytes.subarray(44)), 10);
  });
  assert.deepEqual(await queryPatchVersion(address), { cdn, version: '9.12.345.6' });
});

for (const text of ['https://patch-poe2.poecdn.com/9.1', 'https://other.example/9.1/', 'garbage']) {
  test(`rejects incomplete or invalid discovery: ${text}`, async t => {
    const address = await withServer(t, socket => socket.end(Buffer.from(text, 'utf16le')));
    await assert.rejects(queryPatchVersion(address), /without a complete/);
  });
}

test('timeouts close an unresponsive connection', async t => {
  const address = await withServer(t, () => {});
  await assert.rejects(queryPatchVersion({ ...address, timeoutMs: 20 }), /timed out/);
});
