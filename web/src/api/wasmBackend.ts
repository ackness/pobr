/**
 * wasm 后端：wasm-pack 产物加载 + 游戏数据 fetch/注入 + JSON 契约调用。
 *
 * 产物路径 `../wasm/pkg`（`npm run build-wasm` 生成，gitignored）；
 * 数据路径 `/data/`（`npm run sync-data` 从仓库 data/ 同步，gitignored）。
 */

import type {
  AttributionRequest,
  AttributionResponse,
  BuildJson,
  CalculateBuildRequest,
  CalculateBuildResponse,
  PassiveNode,
} from './types';
import type { PobrBackend } from './backend';

interface WasmModule {
  default: () => Promise<unknown>;
  stageDataFile(path: string, content: string): void;
  initStagedData(): void;
  isDataReady(): boolean;
  decodeBuildJson(code: string): string;
  calculateBuildJson(requestJson: string): string;
  attributionJson(requestJson: string): string;
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

  let manifest: DataManifest | null = null;
  let initialized = false;

  const backend: PobrBackend = {
    async init(onProgress) {
      if (initialized) return;
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
      initialized = true;
      onProgress?.('就绪');
    },
    async decodeBuild(pobCode) {
      return JSON.parse(wasm.decodeBuildJson(pobCode)) as BuildJson;
    },
    async calculateBuild(request: CalculateBuildRequest) {
      return JSON.parse(wasm.calculateBuildJson(JSON.stringify(request))) as CalculateBuildResponse;
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
    translate(lang, key) {
      return wasm.translate(lang, key);
    },
  };
  return backend;
}
