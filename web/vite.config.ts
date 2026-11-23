/// <reference types="vitest/config" />
import { defineConfig, type Plugin, type PreviewServer, type ViteDevServer } from 'vite';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import react from '@vitejs/plugin-react';
import tailwindcss from '@tailwindcss/vite';

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
        const { default: worker } = await import('./public/_worker.js');
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

// The feature panels bring their own stylesheets. They sit in a layer below Tailwind's utilities and
// the app shell's rules (src/app, which declares its own layers), so the shell always wins over them.
function panelStyles(): Plugin {
  return { name: 'panel-style-layer', enforce: 'pre', transform(source, id) {
    if (!id.endsWith('.css') || !id.includes('/src/') || id.includes('/src/app/')) return;
    const order = '@layer theme, base, panels, components, utilities, app;';
    if (id.endsWith('/shared/controls.css')) return { code: `${order}\n@layer app {\n${source}\n}`, map: null };
    const imports = source.match(/@import[^;]+;/g) ?? [];
    return { code: `${order}\n${imports.join('\n')}\n@layer panels {\n${source.replace(/@import[^;]+;/g, '')}\n}`, map: null };
  } };
}

export default defineConfig({
  define: { __POBR_APP_VERSION__: JSON.stringify(appVersion) },
  plugins: [panelStyles(), react(), tailwindcss(), importService()],
  resolve: { tsconfigPaths: true },
  test: {
    // e2e/ belongs to Playwright; Vitest runs only unit tests under src/.
    include: ['src/**/*.test.{ts,tsx}'],
  },
  // The wasm-pack output (src/wasm/pkg) is imported as ES modules + .wasm assets.
  assetsInclude: ['**/*.wasm'],
  // The engine runs in a module worker that loads the WASM package by dynamic import.
  worker: { format: 'es' },
  server: {
    fs: {
      // Allow referencing one level above web/ (WASM pkg stays inside web/; kept conservatively).
      allow: ['..'],
    },
  },
});
