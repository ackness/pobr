/// <reference types="vitest/config" />
import { defineConfig, type Plugin, type PreviewServer, type ViteDevServer } from 'vite';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import react from '@vitejs/plugin-react';
import tailwindcss from '@tailwindcss/vite';
import worker from './public/_worker.js';

const cargoManifest = readFileSync(resolve(process.cwd(), '../Cargo.toml'), 'utf8');
const appVersion = cargoManifest.match(/\[workspace\.package\]\s*\nversion\s*=\s*"([^"]+)"/)?.[1];
if (!appVersion) throw new Error('Missing [workspace.package].version in Cargo.toml');

// Use the production Pages handler in dev and preview, including upstream limits.
function importService(): Plugin {
  const middleware = (server: ViteDevServer | PreviewServer) => {
    server.middlewares.use('/api', async (req, res) => {
      try {
        const chunks: Buffer[] = [];
        let size = 0;
        for await (const chunk of req) {
          size += chunk.length;
          if (size > 32768) {
            res.writeHead(413);
            res.end();
            return;
          }
          chunks.push(chunk);
        }
        const headers = new Headers();
        for (const [name, value] of Object.entries(req.headers)) {
          if (value !== undefined) headers.set(name, Array.isArray(value) ? value.join(', ') : value);
        }
        const request = new Request(`http://${req.headers.host}/api${req.url}`, {
          method: req.method, headers,
          ...(req.method === 'POST' ? { body: Buffer.concat(chunks) } : {}),
        });
        const response = await worker.fetch(request, { ASSETS: { fetch: () => new Response('Not found', { status: 404 }) } });
        res.writeHead(response.status, Object.fromEntries(response.headers));
        res.end(await response.text());
      } catch {
        res.writeHead(502, { 'content-type': 'application/json' });
        res.end(JSON.stringify({ error: 'Trade/import service failed.' }));
      }
    });
  };
  return { name: 'wegame-import', configureServer: middleware, configurePreviewServer: middleware };
}

// Preserve the legacy cascade while allowing the V2 shadcn layers to own controls.
function legacyStyles(): Plugin {
  return { name: 'legacy-style-layer', enforce: 'pre', transform(source, id) {
    if (!id.endsWith('.css') || !id.includes('/src/') || id.includes('/src/v2/')) return;
    const order = '@layer legacy, theme, base, components, utilities, v2;';
    if (id.endsWith('/shared/controls.css')) return { code: `${order}\n@layer v2 {\n${source}\n}`, map: null };
    const imports = source.match(/@import[^;]+;/g) ?? [];
    return { code: `${order}\n${imports.join('\n')}\n@layer legacy {\n${source.replace(/@import[^;]+;/g, '')}\n}`, map: null };
  } };
}

export default defineConfig({
  define: { __POBR_APP_VERSION__: JSON.stringify(appVersion) },
  plugins: [legacyStyles(), react(), tailwindcss(), importService()],
  resolve: { alias: { '@': resolve(process.cwd(), 'src') } },
  test: {
    // e2e/ 归 Playwright；vitest 只跑 src 内单测。
    include: ['src/**/*.test.{ts,tsx}'],
  },
  // wasm-pack 产物（src/wasm/pkg）以 ES module + wasm 文件形式被引用。
  assetsInclude: ['**/*.wasm'],
  server: {
    fs: {
      // 允许引用仓库根（wasm pkg 在 web/ 内，无需越界；保守放开上一级备用）。
      allow: ['..'],
    },
  },
});
