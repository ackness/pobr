/**
 * Mock 后端：与 wasm 后端同签名，返回 fixture 数据。
 * 供 UI 独立开发（无需构建 wasm / 同步数据）与 vitest 组件测试。
 * 启用：`VITE_POBR_BACKEND=mock npm run dev`。
 */

import type { PobrBackend } from './backend';
import type {
  BuildJson,
  CalculateBuildResponse,
  AttributionResponse,
  ConfigOption,
  GemCatalogEntry,
  PassiveNode,
  PassiveTreeMeta,
} from './types';
import decodeFixture from '../fixtures/decode.json';
import calculateFixture from '../fixtures/calculate.json';
import attributionFixture from '../fixtures/attribution.json';
import treeFixture from '../fixtures/tree.json';
import treeMetaFixture from '../fixtures/tree_meta.json';
import gemCatalogFixture from '../fixtures/gem_catalog.json';
import configOptionsFixture from '../fixtures/config_options.json';

export function createMockBackend(): PobrBackend {
  return {
    async init(onProgress) {
      onProgress?.('mock 后端就绪');
    },
    async decodeBuild(pobCode) {
      if (!pobCode.trim()) {
        throw new Error('empty build code');
      }
      return decodeFixture as unknown as BuildJson;
    },
    async switchLoadout() {
      return decodeFixture as unknown as BuildJson;
    },
    async decodeBuildFile() {
      return decodeFixture as unknown as BuildJson;
    },
    async calculateBuild() {
      return calculateFixture as unknown as CalculateBuildResponse;
    },
    async encodeBuild() {
      return 'MOCK_POB_CODE';
    },
    async fullDps() {
      return { full_dps: 0, per_skill: [] };
    },
    async attribution() {
      return attributionFixture as unknown as AttributionResponse;
    },
    async loadPassiveTree() {
      return treeFixture as unknown as PassiveNode[];
    },
    async loadTreeArt() {
      return null;
    },
    async loadTreeMeta() {
      return treeMetaFixture as unknown as PassiveTreeMeta;
    },
    async loadClassNames() {
      return { classes: {}, ascendancies: {} };
    },
    async gemCatalog() {
      return gemCatalogFixture as unknown as GemCatalogEntry[];
    },
    async loadConfigOptions() {
      return configOptionsFixture as unknown as ConfigOption[];
    },
    async translateLines(lines) {
      return lines;
    },
    async classifyItemLines() {
      // mock 不分类；返回 [] 让面板回落到无区分渲染。
      return [];
    },
    async nodePower() {
      return { base: 0, entries: [] };
    },
    async optimizeVariants(request) {
      return {
        baseline: {},
        variants: request.variants.map((v, index) => ({
          index,
          label: v.label ?? null,
          stats: {},
          error: null,
        })),
      };
    },
    async runeCatalog() {
      // mock 无符文目录；面板隐藏符文编辑器。
      return [];
    },
    async reforgeRunes(text) {
      return text;
    },
    translate(_lang, key) {
      return key;
    },
  };
}
