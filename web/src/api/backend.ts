/**
 * API 层唯一入口：组件层只 import 这里，看不到 wasm。
 *
 * - 默认 wasm 后端（真实计算）；
 * - `VITE_POBR_BACKEND=mock` 时用 fixture mock（UI 独立开发 / vitest）。
 */

import type {
  AttributionRequest,
  ClassNames,
  AttributionResponse,
  BuildJson,
  CalculateBuildRequest,
  CalculateBuildResponse,
  ConfigOption,
  GemCatalogEntry,
  PassiveNode,
  PassiveTreeMeta,
} from './types';

export interface PobrBackend {
  /** 初始化（wasm 模块加载 + 游戏数据注入）；幂等。 */
  init(onProgress?: (message: string) => void): Promise<void>;
  decodeBuild(pobCode: string): Promise<BuildJson>;
  calculateBuild(request: CalculateBuildRequest): Promise<CalculateBuildResponse>;
  attribution(request: AttributionRequest): Promise<AttributionResponse>;
  /** 天赋树静态数据（静态资产，不经 wasm）。 */
  loadPassiveTree(): Promise<PassiveNode[]>;
  /** 职业/升华元数据（新建 build 选择器）。 */
  loadTreeMeta(): Promise<PassiveTreeMeta>;
  /** 加载职业/升华名的简中对照表（数据包缺该文件时返回空表，界面显示英文原名）。 */
  loadClassNames(): Promise<ClassNames>;
  /** 宝石目录（手动技能编辑选择器）。 */
  gemCatalog(): Promise<GemCatalogEntry[]>;
  /** 内置配置目录（Config 页分区开关）。 */
  loadConfigOptions(): Promise<ConfigOption[]>;
  /** 英文词条行 → 简中显示（模板反查；不认识原样返回）。 */
  translateLines(lines: string[]): Promise<string[]>;
  translate(lang: string, key: string): string;
}

let backendPromise: Promise<PobrBackend> | null = null;

/** 取当前后端（懒加载单例）。 */
export function getBackend(): Promise<PobrBackend> {
  if (!backendPromise) {
    backendPromise =
      import.meta.env.VITE_POBR_BACKEND === 'mock'
        ? import('./mockBackend').then((m) => m.createMockBackend())
        : import('./wasmBackend').then((m) => m.createWasmBackend());
  }
  return backendPromise;
}
