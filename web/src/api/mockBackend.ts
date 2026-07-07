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
    async calculateBuild() {
      return calculateFixture as unknown as CalculateBuildResponse;
    },
    async attribution() {
      return attributionFixture as unknown as AttributionResponse;
    },
    async loadPassiveTree() {
      return treeFixture as unknown as PassiveNode[];
    },
    async loadTreeMeta() {
      return treeMetaFixture as unknown as PassiveTreeMeta;
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
    translate(_lang, key) {
      return key;
    },
  };
}
