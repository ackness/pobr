#!/usr/bin/env node
// Discover the current official patch; TCP chunks are not complete messages.
import net from 'node:net';
import { pathToFileURL } from 'node:url';

export function queryPatchVersion({ host = 'patch.pathofexile2.com', port = 13060, timeoutMs = 8000 } = {}) {
  return new Promise((resolve, reject) => {
    const socket = net.connect({ host, port }, () => socket.write(Buffer.from([1, 7])));
    let buffer = Buffer.alloc(0);
    let finished = false;
    const finish = (error, result) => {
      if (finished) return;
      finished = true;
      clearTimeout(timer);
      socket.destroy();
      if (error) reject(error); else resolve(result);
    };
    const timer = setTimeout(() => finish(new Error('Patch discovery timed out')), timeoutMs);
    socket.on('data', chunk => {
      buffer = Buffer.concat([buffer, chunk]);
      if (buffer.length > 65536) return finish(new Error('Patch response exceeds 64 KiB'));
      // A binary header can place UTF-16LE on either byte alignment. Require
      // a complete official CDN URL before accepting the version number.
      for (const offset of [0, 1]) {
        const text = buffer.subarray(offset).toString('utf16le');
        const match = text.match(/https?:\/\/patch-poe2\.poecdn\.com\/(\d+(?:\.\d+)+)\//);
        if (match) return finish(null, { cdn: match[0], version: match[1] });
      }
    });
    socket.on('error', error => finish(error));
    socket.on('end', () => finish(new Error('Patch response ended without a complete official CDN URL')));
    socket.on('close', () => finish(new Error('Patch connection closed before discovery')));
  });
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  try {
    const result = await queryPatchVersion({ host: process.argv[2], port: Number(process.argv[3] ?? 13060) });
    console.log(JSON.stringify(result, null, 2));
  } catch (error) {
    console.error(`Patch discovery failed: ${error.message}`);
    process.exitCode = 1;
  }
}
