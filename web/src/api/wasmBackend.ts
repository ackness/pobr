import { loadGemDescriptionIndex, type GemDescriptionIndex } from '../lib/gemDescriptions';
import { GameTextLocalizer, type GameDisplayName } from '../lib/localizedGameText';
import { nativeDisplayTexts, type DisplayText } from '../lib/nativeDisplayTexts';
import { NativeNameIndex, itemClassNames, nativeClassNames, nativeCraftBaseNames, nativeGemNames, type GemNameLink, type GemNameOverride } from '../lib/nativeDisplayNames';
import { lineTemplateLocale, setGameLocales } from '../lib/gameLocales';
import type { Lang } from '../lib/i18n';
import type { UniqueEntry, UniqueDraft } from './uniques';
import type { CraftingMetadata } from './itemCrafting';
/**
 * wasm 后端：wasm-pack 产物加载 + 游戏数据 fetch/注入 + JSON 契约调用。
 *
 * 产物路径 `../wasm/pkg`（`npm run build-wasm` 生成，gitignored）；
 * 数据路径 `/data/`（`npm run sync-data` 从仓库 data/ 同步，gitignored）。
 */

import { dataUrl, loadDataPack, type DataManifest } from './dataPack';
import { fetchReleaseFile, validateRelease, type RuntimeRelease } from './runtimeRelease';
import { nativeDigest } from './dataIntegrity';
import { EXPECTED_SCHEMA_VERSION } from './types';
import type {
  AttributionRequest,
  ClassNames,
  AttributionResponse,
  LineUsageResponse,
  BuildJson,
  SelectedLoadoutJson,
  CalculateBuildRequest,
  CalculateBuildResponse,
  DisplayCatalogEntry,
  ConfigCatalogFile,
  SkillMetricsResponse,
  GemCatalogEntry,
  MinionCatalog,
  ItemLineJson,
  NodePowerResponse,
  OptimizeVariantsResponse,
  PassiveNode,
  PassiveTreeMeta,
  RuneCatalogEntry,
  ItemAugmentInfo,
} from './types';
import type { JewelBase, PobrBackend } from './backend';
import type { EnemyPresets } from './enemyPresets';

interface WasmModule {
  dataInfo(): string;
  default: () => Promise<unknown>;
  /** 可选：旧构建产物没有该导出，按版本 0 处理（同样判为错配）。 */
  schemaVersion?(): number;
  stageDataFile(path: string, content: string): void;
  stageDataBytes(path: string, bytes: Uint8Array): void;
  initStagedData(): void;
  /** `initStagedData` for files already compared with the manifest's digests. */
  initStagedDataVerified(): void;
  isDataReady(): boolean;
  decodeBuildJson(code: string): string;
  decodeBuildLoadoutJson(requestJson: string): string;
  manageLoadoutJson(requestJson: string): string;
  encodeBuildFileJson(request: string): string;
  decodeBuildFileJson(content: string): string;
  importTradeItemsJson(input: string): string;
  calculateBuildJson(requestJson: string): string;
  /** Static: the detail kind of every display stat. */
  breakdownDescriptorsJson(): string;
  displayCatalogJson(): string;
  encodeBuildJson(requestJson: string): string;
  skillMetricsJson(requestJson: string): string;
  importPartyMemberJson(code: string): string;
  attributionJson(requestJson: string): string;
  lineUsageJson(requestJson: string): string;
  gemCatalogJson(): string;
  minionCatalogJson(): string;
  /** English mod lines -> `lang` display lines (JSON arrays); lines nothing recognizes come back as given. */
  translateLines(lang: string, linesJson: string): string;
  /** `translateLines` as JSON `[{text, origin}]` (origin of the translation library's supplement text, or null).
   * Optional: a WASM build older than the export lacks it, and its texts are then official. */
  translateLinesDetailed?(lang: string, linesJson: string): string;
  classifyItemLinesJson(text: string): string;
  runeCatalogJson(itemText: string): string;
  itemAugmentInfoJson(itemText: string): string;
  reforgeRunesJson(requestJson: string): string;
  uniqueCatalogJson(): string;
  uniqueItemDraftJson(request: string): string;
  itemCraftingCatalogJson(): string;
  normalizeItemDraftJson(text: string): string;
  nodePowerJson(requestJson: string): string;
  previewTreeConversionJson(requestJson: string): string;
  previewTreeEditJson(requestJson: string): string;
  optimizeVariantsJson(requestJson: string): string;
  supportGroupsCompatibleJson(requestJson: string): string;
}

async function fetchText(url: string): Promise<string> {
  const res = await fetch(url);
  if (!res.ok) {
    throw new Error(`fetch ${url}: ${res.status} ${res.statusText}`);
  }
  return res.text();
}

async function fetchBytes(url: string): Promise<Uint8Array> {
  const res = await fetch(url);
  if (!res.ok) {
    throw new Error(`fetch ${url}: ${res.status} ${res.statusText}`);
  }
  return new Uint8Array(await res.arrayBuffer());
}

/**
 * Data files the page reads again after start-up (the planners, catalogs and name
 * tables), kept as the bytes that were staged. Every other file is released once the
 * engine has built its data: it holds its own copy, and a file not kept here is simply
 * fetched again if something asks for it.
 */
function readAgainAfterBoot(rel: string): boolean {
  return rel.startsWith('base/passive_trees/')
    || /^i18n\/[^/]+\/(?:classes|base_items|words|display_texts)\.json$/.test(rel)
    || [
      'base/base_items.json',
      'base/skill_gems.json',
      'base/enemy_presets.json',
      'base/passive_tree.json',
      'base/passive_tree_meta.json',
      'overlay/trade_catalog.json',
      'overlay/config_options.json',
      'overlay/gem_effects.json',
      'overlay/item_class_names.json',
    ].includes(rel);
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
  let dataInfo: import('./types').DataInfo | undefined;
  const fetchedFiles = new Map<string, Promise<string>>();
  /** Staged bytes of the files in `readAgainAfterBoot`, by url, until they are first read as text. */
  const keptBytes = new Map<string, Uint8Array>();
  const fetchVersionFile = (version: string, rel: string): Promise<string> => {
    const url = `/data/${version}/${rel}`;
    const existing = fetchedFiles.get(url);
    if (existing) return existing;
    const kept = keptBytes.get(url);
    // Asked for by its digest where the manifest has one, so the browser keeps it.
    const pending = kept
      ? Promise.resolve(new TextDecoder().decode(kept))
      : manifest?.schema !== undefined
        ? (async () => {
          const digest = nativeDigest();
          if (!digest) throw new Error('Runtime blocks require SHA-256 support');
          const release = await validateRelease(manifest, digest);
          return new TextDecoder().decode(await fetchReleaseFile(release, rel, fetchBytes, digest));
        })()
        : fetchText(dataUrl({ version, digests: manifest?.digests }, rel));
    pending.catch(() => fetchedFiles.delete(url));
    fetchedFiles.set(url, pending);
    return pending;
  };
  // 单飞：init 可能被并发调用（React dev StrictMode 会把挂载 effect 跑两遍）。
  // 若两次 init 并行 stage 文件，先完成的 initStagedData() 会清空 wasm 侧暂存区，
  // 后一次只带残缺文件表构建 → “file not in memory data”。共享同一个 Promise 根治。
  let initPromise: Promise<void> | null = null;

  /** Hands the loaded pack's locales and fallbacks to the game-text chain, once per load. */
  let localesSynced = false;
  const syncGameLocales = () => {
    if (localesSynced || !wasm.isDataReady()) return;
    dataInfo = JSON.parse(wasm.dataInfo()) as import('./types').DataInfo;
    setGameLocales(dataInfo.locales);
    localesSynced = true;
  };

  const doInit = async (onProgress?: (message: string) => void): Promise<void> => {
    if (wasm.isDataReady()) { syncGameLocales(); return; }
    onProgress?.('加载数据清单…');
    manifest = JSON.parse(await fetchText('/data/manifest.json')) as DataManifest;
    const { version } = manifest;
    // Each file is compared with the snapshot manifest's digest as it arrives and handed to the
    // engine, then let go; the browser's SHA-256 does the hashing in a fraction of the time WASM
    // takes. Where it cannot (no secure context, an older manifest) the engine compares itself.
    const pack = await loadDataPack({
      manifest,
      fetchBytes,
      stage: (rel, bytes) => wasm.stageDataBytes(rel, bytes),
      keep: readAgainAfterBoot,
      onProgress: (done, total) => {
        if (done % 20 === 0 || done === total) onProgress?.(`加载游戏数据 ${done}/${total}…`);
      },
    });
    onProgress?.('构建数据索引…');
    if (pack.verified) wasm.initStagedDataVerified();
    else wasm.initStagedData();
    for (const [rel, bytes] of pack.kept) keptBytes.set(`/data/${version}/${rel}`, bytes);
    localesSynced = false;
    syncGameLocales();
    onProgress?.('就绪');
  };

  let nativeCatalogs: ReturnType<typeof loadNativeCatalogs> | undefined;
  const loadNativeCatalogs = async () => {
    if (!manifest) manifest = JSON.parse(await fetchText('/data/manifest.json')) as DataManifest;
    const { version, files } = manifest;
    const catalogs = await Promise.all((['zh-CN', 'zh-TW'] as const).map(async locale => {
      const rel = `i18n/${locale}/display_texts.json`;
      return files.includes(rel)
        ? nativeDisplayTexts(JSON.parse(await fetchVersionFile(version, rel)), version, locale) : null;
    }));
    return catalogs.filter(value => value !== null);
  };
  const displayCatalogs = () => nativeCatalogs ??= loadNativeCatalogs().catch((error: unknown) => {
    nativeCatalogs = undefined;
    throw error;
  });

  let descriptionIndex: Promise<GemDescriptionIndex | null> | undefined;
  const descriptions = () => descriptionIndex ??= (async () => {
    if (!manifest) manifest = JSON.parse(await fetchText('/data/manifest.json')) as DataManifest;
    return loadGemDescriptionIndex(manifest.files, async path => {
      const url = `/data/${manifest!.version}/${path}`;
      try { return JSON.parse(await fetchVersionFile(manifest!.version, path)) as unknown; }
      finally {
        // Keep only id -> active_skill in the index, not this large raw array/string.
        if (path === 'base/granted_effects.json') fetchedFiles.delete(url);
      }
    }, displayCatalogs);
  })().catch((error: unknown) => {
    descriptionIndex = undefined;
    throw error;
  });

  let displayLocalizer: Promise<{ localizer: GameTextLocalizer; names: NativeNameIndex }> | undefined;
  const loadDisplayLocalizer = async () => {
    if (!manifest) manifest = JSON.parse(await fetchText('/data/manifest.json')) as DataManifest;
    const { version, files } = manifest;
    const optionalJson = async <T,>(path: string, fallback: T): Promise<T> =>
      files.includes(path) ? JSON.parse(await fetchVersionFile(version, path)) as T : fallback;
    const [bases, cnBases, twBases, cnWords, catalogs, classes] = await Promise.all([
      optionalJson<Array<{ id: string; name: string }>>('base/base_items.json', []),
      optionalJson<Record<string, string>>('i18n/zh-CN/base_items.json', {}),
      optionalJson<Record<string, string>>('i18n/zh-TW/base_items.json', {}),
      optionalJson<Record<string, string>>('i18n/zh-CN/words.json', {}),
      displayCatalogs(),
      optionalJson<unknown>('overlay/item_class_names.json', undefined).then(value => value === undefined ? undefined
        : itemClassNames(value, version, manifest!.schema === undefined ? undefined : (manifest as RuntimeRelease).target)),
    ]);
    const names: GameDisplayName[] = [
      ...Object.entries(cnWords).map(([name, name_zh_cn]) => ({ name, name_zh_cn })),
      ...bases.map(base => ({ name: base.name, name_zh_cn: cnBases[base.id], name_zh_tw: twBases[base.id] })),
      ...JSON.parse(wasm.gemCatalogJson()) as GemCatalogEntry[],
    ];
    return { localizer: new GameTextLocalizer(names, catalogs), names: new NativeNameIndex(catalogs, bases, classes) };
  };
  const display = () => displayLocalizer ??= loadDisplayLocalizer().catch((error: unknown) => {
    displayLocalizer = undefined;
    throw error;
  });

  const backend: PobrBackend = {
    get dataVersion() {
      return dataInfo?.data_version;
    },
    get dataInfo() { return dataInfo; },
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
      return JSON.parse(wasm.decodeBuildLoadoutJson(req)) as SelectedLoadoutJson;
    },
    async manageLoadout(pobCode, op, name, target) {
      const req = JSON.stringify({ code: pobCode, op, name, ...target });
      return JSON.parse(wasm.manageLoadoutJson(req)) as string;
    },
    async decodeBuildFile(content) {
      return JSON.parse(wasm.decodeBuildFileJson(content)) as BuildJson;
    },
    async importTradeItems(items) {
      return JSON.parse(wasm.importTradeItemsJson(JSON.stringify(items)));
    },
    async calculateBuild(request: CalculateBuildRequest) {
      return JSON.parse(wasm.calculateBuildJson(JSON.stringify(request))) as CalculateBuildResponse;
    },
    async breakdownDescriptors() {
      return JSON.parse(wasm.breakdownDescriptorsJson()) as Awaited<ReturnType<PobrBackend['breakdownDescriptors']>>;
    },
    async displayCatalog() {
      return JSON.parse(wasm.displayCatalogJson()) as DisplayCatalogEntry[];
    },
    async encodeBuildFile(request) {
      return JSON.parse(wasm.encodeBuildFileJson(JSON.stringify(request)));
    },
    async encodeBuild(request) {
      return wasm.encodeBuildJson(JSON.stringify(request));
    },
    async importPartyMember(code) { return JSON.parse(wasm.importPartyMemberJson(code)); },
    async skillMetrics(request) {
      return JSON.parse(wasm.skillMetricsJson(JSON.stringify(request))) as SkillMetricsResponse;
    },
    async attribution(request: AttributionRequest) {
      return JSON.parse(wasm.attributionJson(JSON.stringify(request))) as AttributionResponse;
    },
    async lineUsage(request: CalculateBuildRequest) {
      return JSON.parse(wasm.lineUsageJson(JSON.stringify(request))) as LineUsageResponse;
    },
    async loadTradeCatalog() {
      await backend.init();
      // Reuse the exact bytes staged into WASM, including after a new deployment.
      return JSON.parse(await fetchVersionFile(manifest!.version, 'overlay/trade_catalog.json'));
    },
    async loadJewelBases() {
      await backend.init();
      const bases = JSON.parse(await fetchVersionFile(manifest!.version, 'base/base_items.json')) as (JewelBase & { item_class: string })[];
      return bases.filter(base => base.item_class === 'Jewel').map(({ name, drop_level, tags, implicits }) => ({ name, drop_level, tags, implicits }));
    },
    async loadPassiveTree(treeVersion) {
      await backend.init();
      if (!manifest) manifest = JSON.parse(await fetchText('/data/manifest.json')) as DataManifest;
      const currentTree = dataInfo!.tree_version;
      const rel = treeVersion && treeVersion !== currentTree && treeVersion !== 'Default'
        ? `base/passive_trees/${treeVersion}.json` : 'base/passive_tree.json';
      if (!manifest!.files.includes(rel)) throw new Error(`Passive tree ${treeVersion} is unavailable in the loaded data.`);
      return JSON.parse(await fetchVersionFile(manifest!.version, rel)) as PassiveNode[];
    },
    async minionCatalog() {
      return JSON.parse(wasm.minionCatalogJson()) as MinionCatalog;
    },
    gemDescriptions: descriptions,
    async gemCatalog() {
      const entries = JSON.parse(wasm.gemCatalogJson()) as GemCatalogEntry[];
      const catalogs = await displayCatalogs();
      if (!catalogs.length || !manifest!.files.includes('base/skill_gems.json')) return entries;
      const gems = JSON.parse(await fetchVersionFile(manifest!.version, 'base/skill_gems.json')) as GemNameLink[];
      const overrides = manifest!.files.includes('overlay/gem_effects.json')
        ? (JSON.parse(await fetchVersionFile(manifest!.version, 'overlay/gem_effects.json')) as { gems: GemNameOverride[] }).gems : [];
      return nativeGemNames(entries, gems, overrides, catalogs);
    },
    async translateLines(lines, lang: Lang = 'zh-CN') {
      if (lang === 'en-US' || lines.length === 0) return [...lines];
      return (await backend.translateLinesDetailed(lines, lang)).map(line => line.text);
    },
    async translateLinesDetailed(lines, lang: Lang = 'zh-CN') {
      if (lang === 'en-US' || lines.length === 0) return lines.map(text => ({ text, origin: null }));
      // Mod lines render in the first locale of `lang`'s chain whose pack has templates;
      // the localizer then writes them in `lang`'s script and names.
      syncGameLocales();
      const templates = lineTemplateLocale(lang);
      const rendered: DisplayText[] = !templates ? lines.map(text => ({ text, origin: null }))
        : wasm.translateLinesDetailed ? JSON.parse(wasm.translateLinesDetailed(templates, JSON.stringify(lines))) as DisplayText[]
          : (JSON.parse(wasm.translateLines(templates, JSON.stringify(lines))) as string[]).map(text => ({ text, origin: null }));
      const { localizer } = await display();
      return localizer.localizeLinesDetailed(lines, rendered, lang);
    },
    async translateNames(requests, lang: Lang = 'zh-CN') {
      return (await backend.translateNamesDetailed(requests, lang)).map(name => name.text);
    },
    async translateNamesDetailed(requests, lang: Lang = 'zh-CN') {
      if (lang === 'en-US' || requests.length === 0) return requests.map(request => ({ text: request.name, origin: null }));
      const { names } = await display();
      // Only names the locale's id-scoped catalog does not own take the text path.
      const { pending, mergeDetailed } = names.split(requests, lang);
      return mergeDetailed(pending.length ? await backend.translateLinesDetailed(pending, lang) : []);
    },
    async gameLocales() {
      syncGameLocales();
      return dataInfo?.locales ?? [];
    },
    async classifyItemLines(text) {
      return JSON.parse(wasm.classifyItemLinesJson(text)) as ItemLineJson[];
    },
    async previewTreeConversion(code, allSpecs) {
      return JSON.parse(wasm.previewTreeConversionJson(JSON.stringify({ code, all_specs: allSpecs })));
    },
    async previewTreeEdit(request, node, mode, action) {
      return JSON.parse(wasm.previewTreeEditJson(JSON.stringify({ request, node, mode, action })));
    },
    async nodePower(request, powerStat, maxDepth, nodes, window) {
      return JSON.parse(
        wasm.nodePowerJson(
          JSON.stringify({ request, power_stat: powerStat, max_depth: maxDepth, nodes, window }),
        ),
      ) as NodePowerResponse;
    },
    async optimizeVariants(request) {
      return JSON.parse(
        wasm.optimizeVariantsJson(JSON.stringify(request)),
      ) as OptimizeVariantsResponse;
    },
    async supportGroupsCompatible(groups) {
      return JSON.parse(wasm.supportGroupsCompatibleJson(JSON.stringify({ groups }))) as boolean[];
    },
    async runeCatalog(itemText) {
      return JSON.parse(wasm.runeCatalogJson(itemText ?? '')) as RuneCatalogEntry[];
    },
    async itemAugmentInfo(text) {
      return JSON.parse(wasm.itemAugmentInfoJson(text)) as ItemAugmentInfo;
    },
    async uniqueCatalog() { return JSON.parse(wasm.uniqueCatalogJson()) as UniqueEntry[]; },
    async uniqueItemDraft(selection) { return JSON.parse(wasm.uniqueItemDraftJson(JSON.stringify(selection))) as UniqueDraft; },
    async itemCraftingCatalog() {
      const data = JSON.parse(wasm.itemCraftingCatalogJson()) as CraftingMetadata;
      return nativeCraftBaseNames(data, await displayCatalogs());
    },
    async normalizeItemDraft(text) {
      return (JSON.parse(wasm.normalizeItemDraftJson(text)) as { text: string }).text;
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
      const text = await fetchVersionFile(manifest.version, 'overlay/config_options.json');
      return (JSON.parse(text) as ConfigCatalogFile).options;
    },
    async loadEnemyPresets() {
      if (!manifest) manifest = JSON.parse(await fetchText('/data/manifest.json')) as DataManifest;
      const path = 'base/enemy_presets.json';
      return manifest.files.includes(path)
        ? JSON.parse(await fetchVersionFile(manifest.version, path)) as EnemyPresets : null;
    },
    async loadClassNames() {
      if (!manifest) {
        manifest = JSON.parse(await fetchText('/data/manifest.json')) as DataManifest;
      }
      const legacy = manifest.files.includes('i18n/zh-CN/classes.json')
        ? JSON.parse(await fetchVersionFile(manifest.version, 'i18n/zh-CN/classes.json')) as ClassNames
        : { classes: {}, ascendancies: {} };
      const catalogs = await displayCatalogs();
      if (!catalogs.length || !manifest.files.includes('base/passive_tree_meta.json')) return legacy;
      const meta = JSON.parse(await fetchVersionFile(manifest.version, 'base/passive_tree_meta.json')) as PassiveTreeMeta;
      return nativeClassNames(legacy, meta, catalogs);
    },
    async loadTreeMeta() {
      if (!manifest) {
        manifest = JSON.parse(await fetchText('/data/manifest.json')) as DataManifest;
      }
      const text = await fetchVersionFile(manifest.version, 'base/passive_tree_meta.json');
      return JSON.parse(text) as PassiveTreeMeta;
    },
  };
  return backend;
}
