import { spawn } from 'node:child_process';
import { realpath, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { basename, dirname, join } from 'node:path';
import { createInterface } from 'node:readline';
import { Readable } from 'node:stream';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { parseArgs } from 'node:util';
import { build, preview } from 'vite';
import { syncData } from './sync-data.mjs';

const webRoot = fileURLToPath(new URL('..', import.meta.url));
const wegamePath = '/api/import/wegame';

function apiError(response, status, error) {
  response.writeHead(status, { 'content-type': 'application/json', 'cache-control': 'no-store' });
  response.end(JSON.stringify({ error }));
}

function tunnelApi(wegame, cleanup) {
  return {
    name: 'tunnel-preview-api',
    closePreviewServer: cleanup,
    configurePreviewServer(server) {
      server.middlewares.use('/api', async (request, response) => {
        if (!wegame || request.url.split('?')[0] !== '/import/wegame') {
          return apiError(response, 404, 'This API is disabled in the public preview.');
        }
        if (request.method !== 'POST') {
          response.setHeader('allow', 'POST');
          return apiError(response, 405, 'Method not allowed.');
        }
        const origin = wegame.origin();
        if (!origin) return apiError(response, 503, 'The public preview is not ready.');
        // cloudflared rewrites Host to loopback. Check against the assigned HTTPS
        // origin, not a client-provided Host or Forwarded header.
        if (request.headers.origin !== origin) return apiError(response, 403, 'Forbidden.');
        if (Number(request.headers['content-length']) > 2048) {
          return apiError(response, 413, 'Request body is too large.');
        }
        try {
          const result = await wegame.worker.fetch(new Request(`${origin}${wegamePath}`, {
            method: 'POST',
            headers: { origin },
            body: Readable.toWeb(request),
            duplex: 'half',
          }));
          // Quick Tunnel can replace a 502 body with its own error page. Keep
          // the Worker's error code so the browser can show why the import failed.
          response.writeHead(result.status === 502 ? 424 : result.status, Object.fromEntries(result.headers));
          response.end(await result.text());
        } catch {
          if (!response.headersSent && !response.destroyed) apiError(response, 502, 'WeGame import failed.');
        }
      });
    },
  };
}

export function startPreview(root, port, { outDir, wegame, cleanup } = {}) {
  return preview({
    root,
    // Public output comes from a dedicated build and uses only selected APIs.
    ...(outDir ? { configFile: false, build: { outDir }, plugins: [tunnelApi(wegame, cleanup)] } : {}),
    preview: { host: '127.0.0.1', port, strictPort: true, ...(outDir ? { cors: false } : {}) },
  });
}

async function main(port, stage, allowWegame) {
  if (stage) {
    const temporaryRoot = await realpath(tmpdir());
    const resolved = await realpath(stage);
    if (dirname(resolved) !== temporaryRoot || !basename(resolved).startsWith('pobr-web-tunnel-')) {
      throw new Error('Use mise run web:preview --tunnel to prepare a temporary build.');
    }
    stage = resolved;
  }
  // The local Vite config resolves application paths relative to web/.
  process.chdir(webRoot);
  let child;
  let stopped = false;
  let stop;
  const stopping = new Promise(resolve => { stop = resolve; });
  const shutdown = () => {
    stopped = true;
    child?.kill('SIGTERM');
    stop();
  };
  process.on('SIGINT', shutdown);
  process.on('SIGTERM', shutdown);
  let server;
  let publicOrigin;
  let cleanupPromise;
  // Vite waits for its close hooks before exiting on a signal.
  const cleanup = () => stage && (cleanupPromise ??= rm(stage, { recursive: true, force: true }));
  try {
    let outDir;
    let wegame;
    if (stage) {
      const publicDir = join(stage, 'web/public');
      outDir = join(stage, 'dist');
      syncData({ dataRoot: join(stage, 'data'), destRoot: join(publicDir, 'data') });
      await build({
        root: webRoot,
        publicDir,
        build: { outDir },
        resolve: { alias: { '../../public/build-import.mjs': join(publicDir, 'build-import.mjs') } },
      });
      if (allowWegame) {
        const { default: worker } = await import(pathToFileURL(join(publicDir, '_worker.js')).href);
        wegame = { origin: () => publicOrigin, worker };
      }
    }
    if (stopped) return;
    server = await startPreview(webRoot, port, { outDir, wegame, cleanup });
    if (stopped) return;
    const host = `127.0.0.1:${server.httpServer.address().port}`;
    const origin = `http://${host}`;
    console.log(`PoBR preview: ${origin}`);
    if (!stage) {
      await stopping;
      return;
    }
    console.log('Starting a public Cloudflare Quick Tunnel. Anyone with the URL can open this preview.');
    child = spawn('cloudflared', [
      'tunnel', '--no-autoupdate', '--metrics', '127.0.0.1:0',
      '--url', origin, '--http-host-header', host,
    ], { stdio: ['ignore', 'inherit', 'pipe'] });
    createInterface({ input: child.stderr }).on('line', line => {
      console.error(line);
      const assigned = line.match(/\|\s+(https:\/\/[a-z0-9-]+\.trycloudflare\.com)\s+\|/);
      if (assigned && !publicOrigin) {
        publicOrigin = assigned[1];
        if (allowWegame) console.log(`WeGame import enabled for ${publicOrigin}`);
      }
    });
    const exited = new Promise((resolve, reject) => {
      child.once('error', reject);
      child.once('exit', (code, signal) => resolve({ code, signal }));
    });
    const result = await Promise.race([stopping, exited]);
    if (result) throw new Error(`cloudflared exited (${result.signal ?? result.code})`);
  } finally {
    child?.kill('SIGTERM');
    try {
      await server?.close();
    } finally {
      process.off('SIGINT', shutdown);
      process.off('SIGTERM', shutdown);
      await cleanup();
    }
  }
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const { values, positionals } = parseArgs({
    allowPositionals: true,
    options: { tunnel: { type: 'string' }, 'allow-wegame': { type: 'boolean' } },
  });
  main(Number(positionals[0]), values.tunnel, values['allow-wegame']).catch(error => {
    console.error(error.message);
    process.exitCode = 1;
  });
}
