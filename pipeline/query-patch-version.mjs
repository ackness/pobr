#!/usr/bin/env node
// 向 GGG patch 协议服务器查询当前 PoE2 补丁版本号。
//
// 握手：TCP 连接后发送 [0x01,0x07]，服务器回包内含 UTF-16LE 的 CDN 根 URL，
// 形如 `https://patch-poe2.poecdn.com/4.5.0.3.4/`，末段即版本号。
// PoE2 patch 协议服务器：patch.pathofexile2.com:13060（PoE1 为 patch.pathofexile.com:12995）。

import net from 'node:net';

const HOST = process.argv[2] || 'patch.pathofexile2.com';
const PORT = Number(process.argv[3] || 13060);

const url = await new Promise((resolve) => {
  const sock = net.connect({ host: HOST, port: PORT }, () => sock.write(Buffer.from([1, 7])));
  let buf = Buffer.alloc(0);
  const timer = setTimeout(() => { sock.destroy(); resolve(null); }, 8000);
  sock.on('data', (d) => {
    buf = Buffer.concat([buf, d]);
    if (buf.length <= 8) return;
    clearTimeout(timer);
    sock.destroy();
    let txt = '';
    for (let i = 0; i < buf.length - 1; i++) {
      const c = buf.readUInt16LE(i);
      if (c >= 32 && c < 127) { txt += String.fromCharCode(c); i++; }
      else if (txt.length > 4) break;
      else txt = '';
    }
    resolve(txt);
  });
  sock.on('error', () => { clearTimeout(timer); resolve(null); });
});

if (!url) { console.error(`查询失败：${HOST}:${PORT}`); process.exit(1); }
const version = url.replace(/\/+$/, '').split('/').pop();
console.log(JSON.stringify({ cdn: url, version }, null, 2));
