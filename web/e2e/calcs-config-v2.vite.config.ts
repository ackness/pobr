import { defineConfig, type Plugin } from 'vite';
import base from '../vite.config';

// The component branch can run before the shell's legacy-layer plugin lands.
// Once integrated, reuse that plugin instead of applying a second layer.
const legacy: Plugin = { name: 'legacy-style-layer', enforce: 'pre', transform(source, id) {
  if (!id.endsWith('.css') || !id.includes('/src/') || id.includes('/src/v2/')) return;
  const imports = source.match(/@import[^;]+;/g) ?? [];
  return { code: `${imports.join('\n')}\n@layer legacy {\n${source.replace(/@import[^;]+;/g, '')}\n}`, map: null };
} };
const plugins = base.plugins ?? [];
export default defineConfig({ ...base,
  plugins: plugins.some(plugin => plugin && 'name' in plugin && plugin.name === legacy.name) ? plugins : [legacy, ...plugins],
});
