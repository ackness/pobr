/// <reference types="vitest/config" />
import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';

export default defineConfig({
  plugins: [react()],
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
