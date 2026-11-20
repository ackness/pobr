import { withBreakdownDescriptors } from './descriptors';
import type { EnemyPresets } from './enemyPresets';
import type { UniqueEntry, UniqueSelection, UniqueDraft } from './uniques';
import type { CraftingMetadata } from './itemCrafting';
/**
 * Sole entrypoint of the API layer: UI components only import here and never see wasm directly.
 *
 * - Default: wasm backend (real calculation);
 * - With `VITE_POBR_BACKEND=mock`: fixture mock (standalone UI dev / vitest).
 */

import type {
  SocketGroupInput, TreeConversionResponse, TreeAllocationMode, TreeEditAction, TreeEditResponse,
  AttributionRequest,
  ClassNames,
  AttributionResponse,
  LineUsageResponse,
  BuildJson,
  BuildPlannerExport,
  SetSelection,
  SelectedLoadoutJson,
  CalculateBuildRequest,
  CalculateBuildResponse,
  DisplayCatalogEntry,
  ConfigOption,
  SkillMetricsResponse,
  GemCatalogEntry,
  MinionCatalog,
  ItemLineJson,
  NodePowerResponse,
  NodePowerWindow,
  OptimizeVariantsRequest,
  OptimizeVariantsResponse,
  PassiveNode,
  PassiveTreeMeta,
  RuneCatalogEntry,
  ItemAugmentInfo,
} from './types';

/** Authoritative jewel bases, including unique-only bases absent from crafting catalogs. */
export interface JewelBase {
  name: string;
  drop_level: number;
  tags: string[];
  implicits: string[];
}

export interface PobrBackend {
  /** Initialize (wasm module load + game data staging); idempotent. */
  init(onProgress?: (message: string) => void): Promise<void>;
  /** Version of the game data successfully loaded into this backend. */
  readonly dataVersion?: string;
  readonly dataInfo?: import('./types').DataInfo;
  decodeBuild(pobCode: string): Promise<BuildJson>;
  /**
   * Switch to the given loadout and re-decode (bulk swap of passive tree / gear / skills).
   * Indices come from `BuildJson.loadouts[]`; `null` keeps that category unchanged.
   * The returned code contains the selected loadout and becomes the new export base.
   */
  switchLoadout(
    pobCode: string,
    sel: SetSelection,
  ): Promise<SelectedLoadoutJson>;
  /**
   * Loadout set management: duplicate / rename / remove a loadout and return the **new build code**.
   * `name` writes into all three set titles—matching names form a group without exposing `{tag}` syntax.
   */
  manageLoadout(
    pobCode: string,
    op: 'create' | 'duplicate' | 'rename' | 'remove',
    name?: string,
    target?: SetSelection & { kind?: 'config' | 'skill' },
  ): Promise<string>;
  /** Current game planner format, one file per selected source loadout. */
  encodeBuildFile(request: { code: string; name: string; all_loadouts: boolean; source?: string }): Promise<BuildPlannerExport>;
  /** Current planner and legacy China-server .build JSON. */
  decodeBuildFile(content: string): Promise<BuildJson>;
  importTradeItems(items: unknown[]): Promise<{ text?: string; warnings?: string[]; error?: string }[]>;
  calculateBuild(request: CalculateBuildRequest): Promise<CalculateBuildResponse>;
  /**
   * The detail kind of every display stat (the response's `breakdown_descriptors`). A
   * calculation response carries it only on request; `getBackend()` puts this table
   * into the responses it returns, so callers do not have to.
   */
  breakdownDescriptors(): Promise<NonNullable<CalculateBuildResponse['breakdown_descriptors']>>;
  /** Static display units, categories and comparison direction from the engine. */
  displayCatalog(): Promise<DisplayCatalogEntry[]>;
  /** Editable state -> PoB2 share code (same request as `calculateBuild` plus optional notes/base_code). */
  encodeBuild(request: CalculateBuildRequest & { notes?: string; base_code?: string }): Promise<string>;
  /** Independent metrics for every active effect, including utility and unavailable skills. */
  skillMetrics(request: CalculateBuildRequest): Promise<SkillMetricsResponse>;
  importPartyMember(code: string): Promise<import('./types').PartyMember>;
  attribution(request: AttributionRequest): Promise<AttributionResponse>;
  /** Trace how each modifier line entered calculation (slower than `calculateBuild`; fetch on demand). */
  lineUsage(request: CalculateBuildRequest): Promise<LineUsageResponse>;
  /** Static passive tree data (static asset, bypasses wasm). */
  loadPassiveTree(treeVersion?: string | null): Promise<PassiveNode[]>;
  previewTreeConversion(code: string, allSpecs: boolean): Promise<TreeConversionResponse>;
  previewTreeEdit(request: CalculateBuildRequest, node: number, mode: TreeAllocationMode, action: TreeEditAction): Promise<TreeEditResponse>;
  /** Catalog from the same immutable data snapshot used by calculation. */
  loadTradeCatalog(): Promise<unknown>;
  uniqueCatalog(): Promise<UniqueEntry[]>;
  uniqueItemDraft(selection: UniqueSelection): Promise<UniqueDraft>;
  itemCraftingCatalog(): Promise<CraftingMetadata>;
  normalizeItemDraft(text: string): Promise<string>;
  loadJewelBases(): Promise<JewelBase[]>;
  /** Passive node artwork sidecar (returns null when absent; UI falls back to plain SVG circles). */
  /** Class/ascendancy metadata (for new build selector). */
  loadTreeMeta(): Promise<PassiveTreeMeta>;
  /** Load Simplified Chinese class/ascendancy name table (returns empty map when missing; UI keeps English names). */
  loadClassNames(): Promise<ClassNames>;
  /** Gem catalog (for manual skill picker). */
  gemDescriptions(): Promise<import('../lib/gemDescriptions').GemDescriptionIndex | null>;
  gemCatalog(): Promise<GemCatalogEntry[]>;
  minionCatalog(): Promise<MinionCatalog>;
  /** Built-in config catalog (Config page section toggles). */
  loadConfigOptions(): Promise<ConfigOption[]>;
  /** Enemy tier facts from the same pinned snapshot as the calculation. */
  loadEnemyPresets(): Promise<EnemyPresets | null>;
  /** English mod lines -> `lang` display (template reverse lookup in language pack following fallback chain; unrecognized lines returned verbatim). */
  translateLines(lines: string[], lang?: import('../lib/i18n').Lang): Promise<string[]>;
  /** Names of known identity (passive node id, item base line) -> `lang` display: the locale's
   * id-scoped native catalog first (unresolved stays English), otherwise the same path as `translateLines`. */
  translateNames(names: readonly import('../lib/nativeDisplayNames').GameNameRequest[], lang?: import('../lib/i18n').Lang): Promise<string[]>;
  /** {@link translateLines} with who wrote each shown text: `origin` `agent` / `human` for the translation
   * library's own supplement, `null` for official text and English. The texts are `translateLines`' own. */
  translateLinesDetailed(lines: string[], lang?: import('../lib/i18n').Lang): Promise<import('../lib/nativeDisplayTexts').DisplayText[]>;
  /** {@link translateNames} with origins (as {@link translateLinesDetailed}). */
  translateNamesDetailed(names: readonly import('../lib/nativeDisplayNames').GameNameRequest[], lang?: import('../lib/i18n').Lang): Promise<import('../lib/nativeDisplayTexts').DisplayText[]>;
  /** Languages declared by the data pack and their fallback chains (empty before init); pages resolve game-text fallbacks from this. */
  gameLocales(): Promise<import('../lib/gameLocales').GameLocaleInfo[]>;
  /** Item text -> per-line categories (for Items panel coloring; returns [] on empty/error so caller falls back to unstyled rendering). */
  classifyItemLines(text: string): Promise<ItemLineJson[]>;
  /** Passive tree node power heatmap (PoB2 PowerBuilder semantics: single-node trial allocation within depth for target stat delta). */
  nodePower(
    request: CalculateBuildRequest,
    powerStat: string,
    maxDepth: number,
    nodes?: number[],
    /** Evaluate only one window of candidates (response includes `total`); omit to evaluate all. */
    window?: NodePowerWindow,
  ): Promise<NodePowerResponse>;
  /** Generic variant evaluation (optimizer computation side: variant -> full recalc -> stat values; scoring stays in frontend). */
  optimizeVariants(request: OptimizeVariantsRequest): Promise<OptimizeVariantsResponse>;
  /** One result per group; shares the calculation engine's support fixed point. */
  supportGroupsCompatible(groups: SocketGroupInput[]): Promise<boolean[]>;
  /** Rune / Soul Core catalog (for rune socket selector); when `itemText` is supplied, attaches applicable effects per rune. */
  runeCatalog(itemText?: string): Promise<RuneCatalogEntry[]>;
  itemAugmentInfo(text: string): Promise<ItemAugmentInfo>;
  /** Reforge runes: rewrites `Rune:` lines and `{rune}` mod lines in item text; when `sockets` is given,
   *  adds/removes sockets (rewriting/adding/removing the `Sockets:` line). Throws on invalid input. */
  reforgeRunes(text: string, runes: string[], sockets?: number): Promise<string>;
}

let backendPromise: Promise<PobrBackend> | null = null;

/** Get the active backend (lazy-loaded singleton). */
export function getBackend(): Promise<PobrBackend> {
  backendPromise ??= createBackend().then(withBreakdownDescriptors);
  return backendPromise;
}

/**
 * Whether the engine should run in a Web Worker. It does by default wherever workers
 * exist, so that a calculation never freezes the interface. `?engine=main` in the page
 * address, or `VITE_POBR_WORKER=0` at build time, keeps it on the page's own thread
 * (for debugging a worker-only problem).
 */
function wantsWorker(): boolean {
  if (typeof Worker === 'undefined' || import.meta.env.VITE_POBR_WORKER === '0') return false;
  try {
    return new URLSearchParams(globalThis.location?.search).get('engine') !== 'main';
  } catch {
    return true;
  }
}

async function createBackend(): Promise<PobrBackend> {
  if (import.meta.env.VITE_POBR_BACKEND === 'mock') {
    return (await import('./mockBackend')).createMockBackend();
  }
  if (wantsWorker()) {
    try {
      return await (await import('./workerBackend')).createWorkerBackend();
    } catch (error) {
      // A worker that cannot start (blocked, not built for this deployment) must not
      // leave the page without an engine: run it on this thread, as before.
      console.warn('Engine worker unavailable, running the engine on the page thread.', error);
    }
  }
  return (await import('./wasmBackend')).createWasmBackend();
}
