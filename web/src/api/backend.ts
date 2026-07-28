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
  FullDpsResponse,
  GemCatalogEntry,
  ItemLineJson,
  NodePowerResponse,
  OptimizeVariantsRequest,
  OptimizeVariantsResponse,
  PassiveNode,
  PassiveTreeMeta,
  TreeArt,
  RuneCatalogEntry,
} from './types';

export interface PobrBackend {
  /** 初始化（wasm 模块加载 + 游戏数据注入）；幂等。 */
  init(onProgress?: (message: string) => void): Promise<void>;
  decodeBuild(pobCode: string): Promise<BuildJson>;
  /**
   * 切到指定 loadout 后重新解码（成组切换天赋/装备/技能）。
   * 三个序号取自 `BuildJson.loadouts[]`；null 表示该类保持原样。
   */
  switchLoadout(
    pobCode: string,
    sel: { tree: number; item: number | null; skill: number | null },
  ): Promise<BuildJson>;
  /** 解析国服导出的 `.build` 文件（JSON 文本）。 */
  decodeBuildFile(content: string): Promise<BuildJson>;
  calculateBuild(request: CalculateBuildRequest): Promise<CalculateBuildResponse>;
  /** 编辑态 → PoB2 分享 code（请求同 calculateBuild + 可选 notes）。 */
  encodeBuild(request: CalculateBuildRequest & { notes?: string }): Promise<string>;
  /** 逐技能组 DPS + FullDPS 汇总（点击触发；计算量 = 1 + 启用伤害组数）。 */
  fullDps(request: CalculateBuildRequest): Promise<FullDpsResponse>;
  attribution(request: AttributionRequest): Promise<AttributionResponse>;
  /** 天赋树静态数据（静态资产，不经 wasm）。 */
  loadPassiveTree(): Promise<PassiveNode[]>;
  /** 天赋树节点美术边车（未生成时返回 null，界面回退纯 SVG 圆点）。 */
  loadTreeArt(): Promise<TreeArt | null>;
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
  /** 物品文本 → 逐行类别（Items 面板上色；空/异常返回 []，调用方回落无区分渲染）。 */
  classifyItemLines(text: string): Promise<ItemLineJson[]>;
  /** 树节点威力热力图（PoB2 PowerBuilder 语义：深度内单点试加求目标属性增量）。 */
  nodePower(
    request: CalculateBuildRequest,
    powerStat: string,
    maxDepth: number,
  ): Promise<NodePowerResponse>;
  /** 通用变体评估（寻优框架计算面：变体 → 全量重算 → 属性值；打分在前端）。 */
  optimizeVariants(request: OptimizeVariantsRequest): Promise<OptimizeVariantsResponse>;
  /** 符文/魂核目录（符文槽选择器）；`itemText` 给定时逐符文附对该物品适用的效果行。 */
  runeCatalog(itemText?: string): Promise<RuneCatalogEntry[]>;
  /** 重插符文：重写物品文本的 Rune 命名行 + {rune} 词条行；`sockets` 给定时
   *  同步加减孔（重写/新增/移除 `Sockets:` 行）。非法输入抛错。 */
  reforgeRunes(text: string, runes: string[], sockets?: number): Promise<string>;
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
