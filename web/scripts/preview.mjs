import { spawn } from 'node:child_process';
import { createInterface } from 'node:readline';
import { Readable } from 'node:stream';
import { fileURLToPath } from 'node:url';
import { preview } from 'vite';
import worker from '../public/_worker.js';

const webRoot = fileURLToPath(new URL('..', import.meta.url));
const wegamePath = '/api/import/wegame';

function apiError(response, status, error) {
  response.writeHead(status, { 'content-type': 'application/json', 'cache-control': 'no-store' });
  response.end(JSON.stringify({ error }));
}

function tunnelApi(wegameOrigin) {
  return {
    name: 'tunnel-preview-api',
    configurePreviewServer(server) {
      server.middlewares.use('/api', async (request, response) => {
        if (!wegameOrigin || request.url.split('?')[0] !== '/import/wegame') {
          return apiError(response, 404, 'This API is disabled in the public preview.');
        }
        if (request.method !== 'POST') {
          response.setHeader('allow', 'POST');
          return apiError(response, 405, 'Method not allowed.');
        }
        const origin = wegameOrigin();
        if (!origin) return apiError(response, 503, 'The public preview is not ready.');
        // cloudflared rewrites Host to loopback. Check against the assigned HTTPS
        // origin, not a client-provided Host or Forwarded header.
        if (request.headers.origin !== origin) return apiError(response, 403, 'Forbidden.');
        if (Number(request.headers['content-length']) > 2048) {
          return apiError(response, 413, 'Request body is too large.');
        }
        try {
          const result = await worker.fetch(new Request(`${origin}${wegamePath}`, {
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

export function startPreview(root, port, { tunnel = false, wegameOrigin } = {}) {
  return preview({
    root,
    // Serve dist without the local Vite config or any unselected API routes.
    ...(tunnel ? { configFile: false, plugins: [tunnelApi(wegameOrigin)] } : {}),
    preview: { host: '127.0.0.1', port, strictPort: true, ...(tunnel ? { cors: false } : {}) },
  });
}

async function main(port, tunnel, allowWegame) {
  // The local Vite config resolves application paths relative to web/.
  process.chdir(webRoot);
  let child;
  let stop;
  const stopping = new Promise(resolve => { stop = resolve; });
  const shutdown = () => {
    child?.kill('SIGTERM');
    stop();
  };
  process.on('SIGINT', shutdown);
  process.on('SIGTERM', shutdown);
  let server;
  let publicOrigin;
  try {
    server = await startPreview(webRoot, port, {
      tunnel, wegameOrigin: allowWegame ? () => publicOrigin : undefined,
    });
    const host = `127.0.0.1:${server.httpServer.address().port}`;
    const origin = `http://${host}`;
    console.log(`PoBR preview: ${origin}`);
    if (!tunnel) {
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
    await server?.close();
    process.off('SIGINT', shutdown);
    process.off('SIGTERM', shutdown);
  }
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  main(Number(process.argv[2]), process.argv.includes('--tunnel'), process.argv.includes('--allow-wegame')).catch(error => {
    console.error(error.message);
    process.exitCode = 1;
  });
}
