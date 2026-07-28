/**
 * wasm 后端：wasm-pack 产物加载 + 游戏数据 fetch/注入 + JSON 契约调用。
 *
 * 产物路径 `../wasm/pkg`（`npm run build-wasm` 生成，gitignored）；
 * 数据路径 `/data/`（`npm run sync-data` 从仓库 data/ 同步，gitignored）。
 */

import { EXPECTED_SCHEMA_VERSION } from './types';
import type {
  AttributionRequest,
  ClassNames,
  AttributionResponse,
  BuildJson,
  CalculateBuildRequest,
  CalculateBuildResponse,
  ConfigCatalogFile,
  FullDpsResponse,
  GemCatalogEntry,
  ItemLineJson,
  NodePowerResponse,
  OptimizeVariantsResponse,
  PassiveNode,
  TreeArt,
  PassiveTreeMeta,
  RuneCatalogEntry,
} from './types';
import type { PobrBackend } from './backend';

interface WasmModule {
  default: () => Promise<unknown>;
  /** 可选：旧构建产物没有该导出，按版本 0 处理（同样判为错配）。 */
  schemaVersion?(): number;
  stageDataFile(path: string, content: string): void;
  initStagedData(): void;
  isDataReady(): boolean;
  decodeBuildJson(code: string): string;
  decodeBuildLoadoutJson(requestJson: string): string;
  decodeBuildFileJson(content: string): string;
  calculateBuildJson(requestJson: string): string;
  encodeBuildJson(requestJson: string): string;
  fullDpsJson(requestJson: string): string;
  attributionJson(requestJson: string): string;
  gemCatalogJson(): string;
  translateLinesToZhCn(linesJson: string): string;
  classifyItemLinesJson(text: string): string;
  runeCatalogJson(itemText: string): string;
  reforgeRunesJson(requestJson: string): string;
  nodePowerJson(requestJson: string): string;
  optimizeVariantsJson(requestJson: string): string;
  translate(lang: string, key: string): string;
}

interface DataManifest {
  version: string;
  files: string[];
}

async function fetchText(url: string): Promise<string> {
  const res = await fetch(url);
  if (!res.ok) {
    throw new Error(`fetch ${url}: ${res.status} ${res.statusText}`);
  }
  return res.text();
}

/** 并发受限地拉取数据文件（数据 ~30MB、上百文件；8 路并发足够）。 */
async function fetchAll(
  urls: string[],
  fetchOne: (url: string) => Promise<void>,
  concurrency = 8,
): Promise<void> {
  const queue = [...urls];
  const workers = Array.from({ length: Math.min(concurrency, queue.length) }, async () => {
    for (let next = queue.shift(); next !== undefined; next = queue.shift()) {
      await fetchOne(next);
    }
  });
  await Promise.all(workers);
}

export async function createWasmBackend(): Promise<PobrBackend> {
  // 动态 import：pkg 未构建时给出可操作的错误提示，而非白屏。
  let wasm: WasmModule;
  try {
    // @ts-ignore -- pkg 由 `npm run build-wasm` 生成（gitignored）；未构建时走 catch 提示。
    wasm = (await import('../wasm/pkg/pobr_wasm.js')) as unknown as WasmModule;
  } catch (err) {
    throw new Error(
      `wasm 模块未构建：先运行 \`npm run build-wasm\`（详见 web/README.md）。原始错误：${err}`,
    );
  }
  await wasm.default();

  // 契约版本握手：wasm 与前端同仓同发，错配只出现在「旧前端缓存 + 新 wasm 资产」。
  // boot 时校验一次即可（会话中这对组合不会变），不往每个响应里塞版本字段。
  const actualSchema = wasm.schemaVersion?.() ?? 0;
  if (actualSchema !== EXPECTED_SCHEMA_VERSION) {
    throw new Error(
      `前端与 wasm 契约版本不匹配（前端 ${EXPECTED_SCHEMA_VERSION}，wasm ${actualSchema}）：` +
        '请强制刷新页面（Cmd/Ctrl+Shift+R）清除旧缓存。',
    );
  }

  let manifest: DataManifest | null = null;
  // 单飞：init 可能被并发调用（React dev StrictMode 会把挂载 effect 跑两遍）。
  // 若两次 init 并行 stage 文件，先完成的 initStagedData() 会清空 wasm 侧暂存区，
  // 后一次只带残缺文件表构建 → “file not in memory data”。共享同一个 Promise 根治。
  let initPromise: Promise<void> | null = null;

  const doInit = async (onProgress?: (message: string) => void): Promise<void> => {
    if (wasm.isDataReady()) return;
    onProgress?.('加载数据清单…');
    manifest = JSON.parse(await fetchText('/data/manifest.json')) as DataManifest;
    const { version, files } = manifest;
    let done = 0;
    await fetchAll(files, async (rel) => {
      const content = await fetchText(`/data/${version}/${rel}`);
      wasm.stageDataFile(rel, content);
      done += 1;
      if (done % 20 === 0 || done === files.length) {
        onProgress?.(`加载游戏数据 ${done}/${files.length}…`);
      }
    });
    onProgress?.('构建数据索引…');
    wasm.initStagedData();
    onProgress?.('就绪');
  };

  const backend: PobrBackend = {
    init(onProgress) {
      // 失败后清掉缓存的 Promise，允许刷新/重试重新初始化。
      initPromise ??= doInit(onProgress).catch((err: unknown) => {
        initPromise = null;
        throw err;
      });
      return initPromise;
    },
    async decodeBuild(pobCode) {
      return JSON.parse(wasm.decodeBuildJson(pobCode)) as BuildJson;
    },
    async switchLoadout(pobCode, sel) {
      const req = JSON.stringify({ code: pobCode, ...sel });
      return JSON.parse(wasm.decodeBuildLoadoutJson(req)) as BuildJson;
    },
    async decodeBuildFile(content) {
      return JSON.parse(wasm.decodeBuildFileJson(content)) as BuildJson;
    },
    async calculateBuild(request: CalculateBuildRequest) {
      return JSON.parse(wasm.calculateBuildJson(JSON.stringify(request))) as CalculateBuildResponse;
    },
    async encodeBuild(request) {
      return wasm.encodeBuildJson(JSON.stringify(request));
    },
    async fullDps(request) {
      return JSON.parse(wasm.fullDpsJson(JSON.stringify(request))) as FullDpsResponse;
    },
    async attribution(request: AttributionRequest) {
      return JSON.parse(wasm.attributionJson(JSON.stringify(request))) as AttributionResponse;
    },
    async loadPassiveTree() {
      if (!manifest) {
        manifest = JSON.parse(await fetchText('/data/manifest.json')) as DataManifest;
      }
      const text = await fetchText(`/data/${manifest.version}/base/passive_tree.json`);
      return JSON.parse(text) as PassiveNode[];
    },
    async loadTreeArt(): Promise<TreeArt | null> {
      if (!manifest) {
        manifest = JSON.parse(await fetchText('/data/manifest.json')) as DataManifest;
      }
      const base = `/tree-art/${manifest.version}/`;
      try {
        // manifest 里是相对路径，统一前缀成绝对 URL，渲染层直接用。
        const raw = JSON.parse(await fetchText(`${base}manifest.json`)) as TreeArt;
        const nodeIcons: Record<string, string> = {};
        for (const [skill, rel] of Object.entries(raw.nodeIcons)) {
          nodeIcons[skill] = base + rel;
        }
        const frames: TreeArt['frames'] = {};
        for (const [kind, states] of Object.entries(raw.frames)) {
          frames[kind] = {
            unalloc: states.unalloc ? base + states.unalloc : undefined,
            alloc: states.alloc ? base + states.alloc : undefined,
          };
        }
        return {
          nodeIcons,
          frames,
          masteryIcon: raw.masteryIcon ? base + raw.masteryIcon : undefined,
        };
      } catch {
        return null; // 美术未生成（未跑 build-tree-art）→ 回退纯圆点。
      }
    },
    async gemCatalog() {
      return JSON.parse(wasm.gemCatalogJson()) as GemCatalogEntry[];
    },
    async translateLines(lines) {
      return JSON.parse(wasm.translateLinesToZhCn(JSON.stringify(lines))) as string[];
    },
    async classifyItemLines(text) {
      return JSON.parse(wasm.classifyItemLinesJson(text)) as ItemLineJson[];
    },
    async nodePower(request, powerStat, maxDepth) {
      return JSON.parse(
        wasm.nodePowerJson(
          JSON.stringify({ request, power_stat: powerStat, max_depth: maxDepth }),
        ),
      ) as NodePowerResponse;
    },
    async optimizeVariants(request) {
      return JSON.parse(
        wasm.optimizeVariantsJson(JSON.stringify(request)),
      ) as OptimizeVariantsResponse;
    },
    async runeCatalog(itemText) {
      return JSON.parse(wasm.runeCatalogJson(itemText ?? '')) as RuneCatalogEntry[];
    },
    async reforgeRunes(text, runes, sockets) {
      const out = JSON.parse(
        wasm.reforgeRunesJson(JSON.stringify({ text, runes, sockets })),
      ) as { text: string };
      return out.text;
    },
    async loadConfigOptions() {
      if (!manifest) {
        manifest = JSON.parse(await fetchText('/data/manifest.json')) as DataManifest;
      }
      const text = await fetchText(`/data/${manifest.version}/overlay/config_options.json`);
      return (JSON.parse(text) as ConfigCatalogFile).options;
    },
    async loadClassNames() {
      if (!manifest) {
        manifest = JSON.parse(await fetchText('/data/manifest.json')) as DataManifest;
      }
      try {
        const text = await fetchText(`/data/${manifest.version}/i18n/zh-CN/classes.json`);
        return JSON.parse(text) as ClassNames;
      } catch {
        return { classes: {}, ascendancies: {} };
      }
    },
    async loadTreeMeta() {
      if (!manifest) {
        manifest = JSON.parse(await fetchText('/data/manifest.json')) as DataManifest;
      }
      const text = await fetchText(`/data/${manifest.version}/base/passive_tree_meta.json`);
      return JSON.parse(text) as PassiveTreeMeta;
    },
    translate(lang, key) {
      return wasm.translate(lang, key);
    },
  };
  return backend;
}
