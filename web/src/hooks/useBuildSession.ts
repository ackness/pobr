/**
 * Build 会话状态（PoB2 语义：启动即有一个可编辑的空 build）。
 *
 * 可编辑面：角色身份（职业/升华/等级）、已加点集合（树交互加点）、主技能组、
 * config 覆盖；导入 build code 则整体替换基线。每次编辑触发重算，带请求序号
 * 防止乱序返回覆盖新状态。所有后端交互经 `api/backend`。
 */

import { activeWorkspaceStage, addWorkspaceBuild, createWorkspace, duplicateWorkspaceStage, parseWorkspace, renameWorkspaceEntry, selectWorkspaceStage, updateWorkspaceStage, workspaceEnvelope, type BuildWorkspace } from '../lib/buildWorkspace';
import { formatApiError } from '../api/error';
import { resolveBuildInput } from '../api/import';
import { defaultMainSkill } from '../lib/mainSkill';
import { reconcileJewelAllocation } from '../lib/passiveGraph';
import { groupsForWeaponSet, skillWeaponSet, switchWeapons, validWeaponSwap } from '../lib/weaponSets';
import { useCallback, useEffect, useRef, useState } from 'react';
import { getBackend } from '../api/backend';
import { composeNotes, splitNotes, type Annotations } from '../lib/annotations';
import type {
  AttributeChoice,
  AttributionResponse,
  BuildJson,
  ClassNames,
  CalculateBuildRequest,
  CalculateBuildResponse,
  ConfigInputValue,
  CustomModifierBlock,
  EnemyTier,
  FullDpsResponse,
  JewelInput,
  LoadoutJson,
  PassiveTreeMeta,
  SlotItemInput,
  SocketGroupInput,
  WeaponSwap,
} from '../api/types';

export interface CharacterState {
  level: number;
  class_name: string;
  ascendancy_name: string;
}

export interface CalcParams {
  main_socket_group?: number;
  enemy_tier?: EnemyTier;
  config_inputs: Record<string, ConfigInputValue>;
  /** 额外全局 modifier 文本（Config 页自定义词缀，一行一条）。 */
  extra_modifiers?: string[];
  custom_modifier_blocks?: CustomModifierBlock[];
}

/** 会话完整可编辑状态（重算请求由此派生）。 */
interface BuildState {
  treeVersion?: string | null;
  weaponSwap?: WeaponSwap | null;
  pobCode: string | null;
  character: CharacterState;
  allocatedNodes: number[];
  /** 属性小点三选一（skill id → str/dex/int）。 */
  attributeChoices: Record<string, AttributeChoice>;
  /** 技能组（导入时物化，手动可增删改；始终以整份覆盖上行）。 */
  socketGroups: SocketGroupInput[];
  /** 装备槽原始文本（同上）。 */
  items: SlotItemInput[];
  /** 激活态药剂/护符（槽名 `Flask 1/2`、`Charm 1..3` + PoB 文本；整份覆盖上行）。 */
  flasks: SlotItemInput[];
  /** 树插槽珠宝（插槽号 + PoB 文本；插槽加点才生效）。 */
  jewels: JewelInput[];
  /** 局部注释（key 约定见 lib/annotations）；不参与计算，分享时嵌入 <Notes>。 */
  annotations: Annotations;
  params: CalcParams;
}

export type ImportDestination = 'newBuild' | 'currentStage';
export interface ImportOptions { destination: ImportDestination; name?: string }

export interface BuildSession {
  treeVersion?: string | null;
  dataVersion: string | null;
  workspace: BuildWorkspace | null;
  storageFailed: boolean;
  selectStage: (buildId: string, stageId?: string) => void;
  createLocalBuild: (name: string) => void;
  duplicateStage: (name: string) => void;
  renameWorkspace: (kind: 'build' | 'stage', name: string) => void;
  exportWorkspace: () => string;
  exportLocalBuild: () => string;
  activeWeaponSet: 1 | 2;
  weaponSwap?: WeaponSwap | null;
  setWeaponSet: (set: 1 | 2) => void;
  bootMessage: string | null;
  bootError: string | null;
  /** 解码出的原始 build（珠宝/药剂等只读展示；白手 build 为 null）。 */
  build: BuildJson | null;
  treeMeta: PassiveTreeMeta | null;
  /** 职业/升华名的简中对照表（英文名 → 简中名；空表时界面显示英文原名）。 */
  classNames: ClassNames;
  character: CharacterState | null;
  allocatedNodes: number[];
  attributeChoices: Record<string, AttributeChoice>;
  socketGroups: SocketGroupInput[];
  items: SlotItemInput[];
  flasks: SlotItemInput[];
  jewels: JewelInput[];
  calc: CalculateBuildResponse | null;
  calcParams: CalcParams;
  busy: boolean;
  error: string | null;
  /** 笔记（本地持久化；导入 build 时被其 <Notes> 覆盖）。 */
  notes: string;
  setNotes: (text: string) => void;
  /** Unapplied item text, retained across tabs until saved, cancelled or the build is replaced. */
  editorDrafts: Record<string, string>;
  setEditorDraft: (key: string, text: string | null) => void;
  /** 局部注释（装备/技能组/珠宝旁的说明；随分享 code 与存档往返）。 */
  annotations: Annotations;
  /** 写/清一条局部注释（空文本 = 删除；不触发重算）。 */
  setAnnotation: (key: string, text: string) => void;
  /** 删技能组并顺移 `skill:<index>` 注释键（Skills 页删除入口）。 */
  removeSocketGroup: (index: number) => void;
  /** 导出完整会话（build 状态 + 笔记）为 JSON 文本。 */
  exportSession: () => string;
  /** 编辑态 → PoB2 分享 code（可粘回 PoB2 / 二次导入）。 */
  exportCode: () => Promise<string>;
  /** 从导出的 JSON 恢复会话；非法输入抛错。 */
  importSession: (json: string, destination?: ImportDestination) => void;
  importCode: (code: string, options?: ImportOptions) => Promise<boolean>;
  /** 切到指定 loadout（成组换天赋/装备/技能）；会覆盖本地编辑。 */
  switchLoadout: (sel: { tree: number; item: number | null; skill: number | null }) => Promise<void>;
  /** 当前 build 的 loadout 清单（导入后可用；手搓 build 为空）。 */
  loadouts: LoadoutJson[];
  /** 当前选中的 loadout 下标。 */
  activeLoadout: number | null;
  /** 复制 / 重命名 / 删除当前 loadout（同名写进三类 set；随后整份重载）。 */
  manageLoadout: (op: 'duplicate' | 'rename' | 'remove', name?: string) => Promise<void>;
  newBuild: (className: string, ascendancyName: string) => void;
  setCharacter: (patch: Partial<CharacterState>) => void;
  /** 点选加点/取消；属性小点加点时带三选一。 */
  toggleNode: (skill: number, choice?: AttributeChoice) => void;
  /** 整体替换加点集合（树寻路一次点亮/熄灭整条路径）。 */
  setAllocatedNodes: (skills: number[], choices?: Record<string, AttributeChoice>) => void;
  /** 整表替换属性三选一（批量调配 / 快捷键改单点）。 */
  setAttributeChoices: (choices: Record<string, AttributeChoice>) => void;
  /** 当前完整计算请求（对比预览用：克隆后改一处再算一次）。 */
  currentRequest: () => CalculateBuildRequest | null;
  /** 状态版本号（每次编辑 +1；hover 收益等缓存的失效键）。 */
  stateVersion: number;
  /** 自上次导入 / 切换 loadout 以来是否有编辑（切换会整份覆盖，据此提醒）。 */
  isDirty: boolean;
  /** Whether starting over would discard character progress, including restored saves. */
  hasBuildContent: boolean;
  /** 物品/珠宝/技能组套装库（独立持久化，跨 build 复用）。 */
  library: Library;
  saveLibraryItem: (kind: 'item' | 'jewel', text: string, slot?: string) => void;
  removeLibraryItem: (id: string) => void;
  saveSkillSet: (name: string) => void;
  applySkillSet: (id: string) => void;
  removeSkillSet: (id: string) => void;
  /** 整份替换技能组（Skills 编辑器）。 */
  setSocketGroups: (groups: SocketGroupInput[]) => void;
  /** 整份替换装备（Items 编辑器）。 */
  setItems: (items: SlotItemInput[]) => void;
  /** 整份替换激活态药剂/护符（Items 编辑器）。 */
  setFlasks: (flasks: SlotItemInput[]) => void;
  /** 整份替换树插槽珠宝（Tree 页珠宝编辑器）。 */
  setJewels: (jewels: JewelInput[]) => void;
  removeJewelSocket: (socket: number, allocatedNodes: number[]) => void;
  updateParams: (patch: Partial<CalcParams>) => void;
  setConfigInput: (key: string, value: ConfigInputValue | null) => void;
  runAttribution: (fields: string[]) => Promise<AttributionResponse>;
  /** 逐技能组 DPS（Calcs 页技能列表消费；每组一次 scoped 完整计算）。 */
  runFullDps: () => Promise<FullDpsResponse>;
}

// 计算请求不带 pob_code：导入时 XML 内容已全量物化进 state（materialize +
// config_inputs/main_socket_group），再带 code 只是让 Rust 每次重算都白解码
// 一遍然后立刻被覆盖项冲掉。state.pobCode 仅存档用（恢复会话时重建 build 视图）。
function toRequest(state: BuildState): CalculateBuildRequest {
  return {
    tree_version: state.treeVersion,
    character: state.character,
    allocated_nodes: state.allocatedNodes,
    attribute_choices: state.attributeChoices,
    socket_groups: groupsForWeaponSet(state.socketGroups, state.weaponSwap?.active ?? 1),
    items: state.items,
    flasks: state.flasks,
    jewels: state.jewels,
    main_socket_group: state.params.main_socket_group,
    enemy_tier: state.params.enemy_tier,
    extra_modifiers: state.params.extra_modifiers,
    custom_modifier_blocks: state.params.custom_modifier_blocks,
    config_inputs: state.params.config_inputs,
  };
}

/** PoB config keys with dedicated editable controls use one request lane. */
function paramsFromConfigInputs(rawInputs: Record<string, ConfigInputValue>, overrides: Partial<CalcParams> = {}): CalcParams {
  const config_inputs = { ...rawInputs };
  const rawTier = config_inputs.enemyIsBoss;
  const tier = typeof rawTier === 'string' ? rawTier.toLowerCase() : '';
  const enemy_tier = ['none', 'boss', 'pinnacle', 'uber'].includes(tier)
    ? tier as EnemyTier : undefined;
  if (enemy_tier) delete config_inputs.enemyIsBoss;
  const rawMods = config_inputs.customMods;
  const legacyMods = typeof rawMods === 'string'
    ? rawMods.split(/\r?\n/).filter(line => line.trim().length > 0) : undefined;
  if (legacyMods) delete config_inputs.customMods;
  const params: CalcParams = { ...overrides, config_inputs };
  // The old request sent raw Inputs and dedicated overrides together. Raw
  // enemyIsBoss took precedence, while both custom-modifier lanes stacked.
  const selectedTier = enemy_tier ?? overrides.enemy_tier;
  const selectedMods = legacyMods
    ? [...legacyMods, ...(overrides.extra_modifiers ?? [])]
    : overrides.extra_modifiers;
  if (selectedTier !== undefined) params.enemy_tier = selectedTier;
  if (overrides.custom_modifier_blocks !== undefined) {
    delete params.extra_modifiers;
    delete config_inputs.customMods;
  } else if (selectedMods !== undefined) params.extra_modifiers = selectedMods;
  return params;
}

export function paramsFromDecoded(decoded: BuildJson): CalcParams {
  return paramsFromConfigInputs(decoded.config_inputs, decoded.custom_modifier_blocks === undefined
    ? {} : { custom_modifier_blocks: decoded.custom_modifier_blocks });
}

/** 库条目：可复用的装备/珠宝（PoB 文本）。 */
export interface LibraryItem {
  id: string;
  kind: 'item' | 'jewel';
  /** 展示名（取文本第二行，即物品名）。 */
  name: string;
  text: string;
  /** 来源装备槽（导入/保存时记录；旧条目缺失 → 不参与槽位过滤）。 */
  slot?: string;
}

/** 技能组套装：整套 socket_groups 快照，可随时切换。 */
export interface SkillSet {
  id: string;
  name: string;
  groups: SocketGroupInput[];
  main_socket_group?: number;
}

/** 库（独立于单个 build 持久化——换 build 仍可复用）。 */
export interface Library {
  items: LibraryItem[];
  skillSets: SkillSet[];
}

const LIBRARY_KEY = 'pobr-library';

function loadLibrary(): Library {
  try {
    const parsed = JSON.parse(localStorage.getItem(LIBRARY_KEY) ?? '') as Library;
    return {
      items: Array.isArray(parsed.items) ? parsed.items : [],
      skillSets: Array.isArray(parsed.skillSets) ? parsed.skillSets : [],
    };
  } catch {
    return { items: [], skillSets: [] };
  }
}

/** 本地存档信封（localStorage / 导出文件共用同一形状）。 */
export interface SavedSession {
  /** 存档格式版本（前向兼容闸门）。 */
  version: 1;
  state: BuildState;
  notes: string;
}

const STORAGE_KEY = 'pobr-build-state';

/** 解析并校验存档信封（导入文件 / localStorage 共用）；非法返回 null。 */
export function parseSaved(json: string): SavedSession | null {
  try {
    const record = (value: unknown): value is Record<string, unknown> =>
      value !== null && typeof value === 'object' && !Array.isArray(value);
    const uint = (value: unknown): value is number => Number.isInteger(value) && Number(value) >= 0 && Number(value) <= 0xffffffff;
    const optionalText = (value: unknown) => value == null || typeof value === 'string';
    const optionalIndex = (value: unknown, min = 0) => value == null || uint(value) && value >= min;
    const strings = (value: unknown): value is string[] => Array.isArray(value) && value.every(entry => typeof entry === 'string');
    const nodes = (value: unknown): value is number[] => Array.isArray(value) && value.every(uint);
    const textMap = (value: unknown) => record(value) && Object.values(value).every(entry => typeof entry === 'string');
    const itemList = (value: unknown, slots: RegExp): value is SlotItemInput[] => Array.isArray(value)
      && value.every(item => record(item) && typeof item.slot === 'string' && slots.test(item.slot) && typeof item.text === 'string');
    const parsed: unknown = JSON.parse(json);
    if (!record(parsed) || parsed.version !== 1 || !record(parsed.state)) return null;
    const state = parsed.state;
    const character = state.character;
    if (!record(character) || typeof character.class_name !== 'string'
      || !optionalText(character.ascendancy_name) || !optionalIndex(character.level, 1)
      || Number(character.level ?? 1) > 100 || !optionalText(state.pobCode) || !optionalText(state.treeVersion)
      || !nodes(state.allocatedNodes) || !Array.isArray(state.socketGroups)
      || !itemList(state.items, /^(weapon[12]|helmet|bodyarmour|gloves|boots|amulet|ring[123]|belt)$/)) return null;
    const socketGroups: SocketGroupInput[] = [];
    for (const group of state.socketGroups) {
      if (!record(group) || !Array.isArray(group.gems) || !optionalText(group.slot) || !optionalText(group.source)
        || group.enabled != null && typeof group.enabled !== 'boolean'
        || group.weapon_set != null && ![1, 2].includes(Number(group.weapon_set))
        || group.weapon_set != null && typeof group.weapon_set !== 'number'
        || !optionalIndex(group.main_active_skill, 1)) return null;
      const gems: SocketGroupInput['gems'] = [];
      for (const gem of group.gems) {
        if (!record(gem) || typeof gem.skill_id !== 'string' || !uint(gem.level) || gem.level < 1
          || !uint(gem.quality) || !optionalIndex(gem.stat_set_index, 1)) return null;
        gems.push({ skill_id: gem.skill_id, level: gem.level, quality: gem.quality,
          ...(gem.stat_set_index != null ? { stat_set_index: Number(gem.stat_set_index) } : {}) });
      }
      socketGroups.push({ enabled: group.enabled == null ? true : group.enabled as boolean,
        slot: group.slot as string | null | undefined, source: group.source as string | null | undefined,
        weapon_set: group.weapon_set as 1 | 2 | null | undefined,
        main_active_skill: group.main_active_skill as number | null | undefined, gems });
    }
    const flasks = state.flasks ?? [];
    const jewels = state.jewels ?? [];
    const annotations = state.annotations ?? {};
    const attributeChoices = state.attributeChoices ?? {};
    const params = state.params ?? {};
    if (!itemList(flasks, /^(Flask [12]|Charm [123])$/)
      || !Array.isArray(jewels) || !jewels.every(jewel => record(jewel) && uint(jewel.socket_node) && typeof jewel.text === 'string')
      || !textMap(annotations) || !record(attributeChoices)
      || !Object.entries(attributeChoices).every(([node, choice]) => /^\d+$/.test(node) && uint(Number(node)) && typeof choice === 'string' && ['str', 'dex', 'int'].includes(choice))
      || !record(params) || !optionalIndex(params.main_socket_group)
      || params.enemy_tier != null && (typeof params.enemy_tier !== 'string' || !['none', 'boss', 'pinnacle', 'uber'].includes(params.enemy_tier))
      || params.extra_modifiers != null && !strings(params.extra_modifiers)
      || params.custom_modifier_blocks !== undefined && (!Array.isArray(params.custom_modifier_blocks)
        || !params.custom_modifier_blocks.every(block => record(block) && typeof block.title === 'string'
          && typeof block.enabled === 'boolean' && typeof block.text === 'string'))) return null;
    const configInputs = params.config_inputs ?? {};
    if (!record(configInputs) || !Object.values(configInputs).every(value =>
      typeof value === 'boolean' || typeof value === 'string' || typeof value === 'number' && Number.isFinite(value))) return null;
    const weaponSwap = validWeaponSwap(state.weaponSwap);
    if (state.weaponSwap != null && !weaponSwap) return null;
    return {
      version: 1,
      state: {
        treeVersion: state.treeVersion as string | null | undefined,
        pobCode: typeof state.pobCode === 'string' ? state.pobCode : null,
        character: { class_name: character.class_name, level: Number(character.level ?? 1),
          ascendancy_name: typeof character.ascendancy_name === 'string' ? character.ascendancy_name : '' },
        allocatedNodes: state.allocatedNodes,
        attributeChoices: attributeChoices as Record<string, AttributeChoice>,
        socketGroups,
        items: state.items,
        weaponSwap,
        flasks,
        jewels: jewels as JewelInput[],
        annotations: annotations as Annotations,
        params: paramsFromConfigInputs(configInputs as Record<string, ConfigInputValue>, {
          ...(params.main_socket_group != null ? { main_socket_group: Number(params.main_socket_group) } : {}),
          ...(params.enemy_tier != null ? { enemy_tier: params.enemy_tier as EnemyTier } : {}),
          ...(params.extra_modifiers != null ? { extra_modifiers: params.extra_modifiers as string[] } : {}),
          ...(params.custom_modifier_blocks !== undefined ? { custom_modifier_blocks: params.custom_modifier_blocks as CustomModifierBlock[] } : {}),
        }),
      },
      notes: typeof parsed.notes === 'string' ? parsed.notes : '',
    };
  } catch {
    return null;
  }
}

/** 库展示名：取第一条非 Rarity 行（即物品名）。 */
function itemName(text: string): string {
  return (
    text
      .split('\n')
      .map((l) => l.trim())
      .filter((l) => l && !/^Rarity:/i.test(l))[0] ?? 'Item'
  );
}

/**
 * 旧会话迁移：calc 请求已不带 pob_code（见 toRequest），此前靠 code 在 Rust 侧
 * 兜底的 XML config / 主技能组，恢复会话时从解码结果回填（已保存的显式值优先）。
 */
export function backfillFromDecoded(state: BuildState, decoded: BuildJson): BuildState {
  const decodedParams = paramsFromDecoded(decoded);
  const savedParams = paramsFromConfigInputs(state.params.config_inputs, state.params);
  const sourceBlocks = decodedParams.custom_modifier_blocks;
  const sourceLines = sourceBlocks?.filter(block => block.enabled)
    .flatMap(block => block.text.split(/\r?\n/).filter(line => line.trim().length > 0));
  // Recover source metadata only when historical flattened edits still match.
  // Changed legacy lines remain authoritative; their old grouping is unknown.
  const restoredBlocks = savedParams.custom_modifier_blocks ?? (savedParams.extra_modifiers === undefined
    || JSON.stringify(savedParams.extra_modifiers) === JSON.stringify(sourceLines) ? sourceBlocks : undefined);
  return {
    ...state,
    treeVersion: state.treeVersion ?? decoded.tree.tree_version,
    socketGroups: state.socketGroups.map((group, index) => {
      const original = decoded.socket_groups[index];
      // Only restore missing metadata when the saved gem order still matches
      // the source. Edited groups cannot safely inherit an old ordinal.
      if (!original || group.gems.length !== original.gems.length
        || !group.gems.every((gem, i) => gem.skill_id === original.gems[i].skill_id)) return group;
      return { ...group,
        main_active_skill: group.main_active_skill ?? original.main_active_skill,
        gems: group.gems.map((gem, i) => ({ ...gem,
          stat_set_index: gem.stat_set_index ?? original.gems[i].stat_set_index,
        })),
      };
    }),
    params: {
      ...savedParams,
      main_socket_group: state.params.main_socket_group ?? decoded.main_socket_group ?? undefined,
      enemy_tier: savedParams.enemy_tier ?? decodedParams.enemy_tier,
      // An explicit empty block list must not resurrect source modifiers.
      ...(restoredBlocks !== undefined
        ? { custom_modifier_blocks: restoredBlocks, extra_modifiers: undefined }
        : savedParams.extra_modifiers !== undefined
          ? { extra_modifiers: savedParams.extra_modifiers }
          : { custom_modifier_blocks: decodedParams.custom_modifier_blocks, extra_modifiers: decodedParams.extra_modifiers }),
      config_inputs: { ...decodedParams.config_inputs, ...savedParams.config_inputs },
    },
  };
}

/** 解码结果 → 可编辑技能组/装备状态（物化，之后全走覆盖）。 */
function materialize(
  decoded: BuildJson,
): Pick<BuildState, 'treeVersion' | 'socketGroups' | 'items' | 'flasks' | 'jewels' | 'weaponSwap'> {
  return {
    treeVersion: decoded.tree.tree_version,
    weaponSwap: decoded.weapon_swap,
    socketGroups: decoded.socket_groups.map((g) => ({
      weapon_set: g.weapon_set,
      slot: g.slot,
      enabled: g.enabled,
      main_active_skill: g.main_active_skill,
      source: g.source,
      gems: g.gems.map((gem) => ({
        skill_id: gem.skill_id,
        level: gem.level,
        quality: gem.quality,
        stat_set_index: gem.stat_set_index,
      })),
    })),
    items: decoded.items.equipped.map((item) => ({ slot: item.slot, text: item.text })),
    flasks: (decoded.items.flasks ?? []).map((f) => ({ slot: f.slot, text: f.text })),
    jewels: (decoded.items.socket_jewels ?? []).map((j) => ({
      socket_node: j.socket_node,
      text: j.text,
    })),
  };
}

async function fullDpsForState(state: BuildState): Promise<FullDpsResponse> {
  const backend = await getBackend();
  const active = state.weaponSwap?.active ?? 1;
  const sets = new Set<1 | 2>([active]);
  for (const group of state.socketGroups) if (group.enabled && group.weapon_set) sets.add(group.weapon_set);
  const per_skill: FullDpsResponse['per_skill'] = [];
  for (const set of sets) {
    const report = await backend.fullDps(toRequest(switchWeapons(state, set)));
    per_skill.push(...report.per_skill.filter(entry => {
      const group = state.socketGroups[entry.group_index];
      return group ? (group.weapon_set ?? active) === set : set === active;
    }));
  }
  per_skill.sort((a, b) => a.group_index - b.group_index);
  return { full_dps: per_skill.reduce((sum, entry) => sum + entry.dps, 0), per_skill };
}

export function useBuildSession(): BuildSession {
  const [dataVersion, setDataVersion] = useState<string | null>(null);
  const [workspace, setWorkspace] = useState<BuildWorkspace | null>(null);
  const workspaceRef = useRef<BuildWorkspace | null>(null);
  const [storageFailed, setStorageFailed] = useState(false);
  const [bootMessage, setBootMessage] = useState<string | null>('初始化…');
  const [bootError, setBootError] = useState<string | null>(null);
  const [treeMeta, setTreeMeta] = useState<PassiveTreeMeta | null>(null);
  const [classNames, setClassNames] = useState<ClassNames>({ classes: {}, ascendancies: {} });
  const [build, setBuild] = useState<BuildJson | null>(null);
  const [state, setState] = useState<BuildState | null>(null);
  const [calc, setCalc] = useState<CalculateBuildResponse | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [notes, setNotesState] = useState<string>(
    () => localStorage.getItem('pobr-notes') ?? '',
  );

  const notesRef = useRef(notes);
  const stateRef = useRef<BuildState | null>(null);
  const [editorDrafts, setEditorDrafts] = useState<Record<string, string>>({});
  const draftsRef = useRef<Record<string, string>>({});
  const persistWorkspace = useCallback((next: BuildWorkspace) => {
    workspaceRef.current = next;
    setWorkspace(next);
    try {
      localStorage.setItem(STORAGE_KEY, workspaceEnvelope(next));
      setStorageFailed(false);
    } catch { setStorageFailed(true); }
  }, []);
  const saveToStorage = useCallback((next: BuildState, text: string) => {
    const saved: SavedSession = { version: 1, state: next, notes: text };
    persistWorkspace(updateWorkspaceStage(workspaceRef.current ?? createWorkspace(saved), saved, draftsRef.current));
  }, [persistWorkspace]);
  const setEditorDraft = useCallback((key: string, text: string | null) => {
    const next = { ...draftsRef.current };
    if (text === null) delete next[key];
    else next[key] = text;
    draftsRef.current = next;
    setEditorDrafts(next);
    if (stateRef.current) saveToStorage(stateRef.current, notesRef.current);
  }, [saveToStorage]);
  const hasDrafts = Object.keys(editorDrafts).length > 0;
  useEffect(() => {
    if (!hasDrafts) return;
    const warn = (event: BeforeUnloadEvent) => { event.preventDefault(); event.returnValue = ''; };
    window.addEventListener('beforeunload', warn);
    return () => window.removeEventListener('beforeunload', warn);
  }, [hasDrafts]);

  const setNotes = useCallback((text: string) => {
    setNotesState(text);
    notesRef.current = text;
    if (stateRef.current) saveToStorage(stateRef.current, text);
  }, [saveToStorage]);
  const [library, setLibrary] = useState<Library>(loadLibrary);
  const [stateVersion, setStateVersion] = useState(0);
  /**
   * 「干净」基线：导入 / 切换 loadout 落地时记下当时的版本号。之后每次编辑
   * `stateVersion` 递增，超过基线即视为有未保存改动——切换 loadout 会整份重解码
   * 覆盖状态，切之前要据此提醒。
   */
  const cleanVersionRef = useRef(0);
  const cleanNotesRef = useRef(notes);
  /** `stateVersion` 的同步副本（apply 内自增，避免在 setState updater 里做副作用）。 */
  const versionRef = useRef(0);

  const persistLibrary = useCallback((next: Library) => {
    setLibrary(next);
    try {
      localStorage.setItem(LIBRARY_KEY, JSON.stringify(next));
    } catch {
      // 配额失败静默。
    }
  }, []);

  // 重算请求序号：只应用最新一次的结果（快速连点加点时防乱序覆盖）。
  const seqRef = useRef(0);

  const recalc = useCallback((next: BuildState) => {
    const seq = ++seqRef.current;
    setBusy(true);
    setError(null);
    getBackend()
      .then((backend) => backend.calculateBuild(toRequest(next)))
      .then((result) => {
        if (seqRef.current === seq) setCalc(result);
      })
      .catch((err) => {
        if (seqRef.current === seq) setError(formatApiError(err));
      })
      .finally(() => {
        if (seqRef.current === seq) setBusy(false);
      });
  }, []);

  /** 应用新状态并触发重算 + 自动保存到浏览器。 */
  const apply = useCallback(
    (next: BuildState, opts?: { clean?: boolean }) => {
      const previous = stateRef.current;
      if (!opts?.clean && previous?.weaponSwap && next.weaponSwap === previous.weaponSwap && next.allocatedNodes !== previous.allocatedNodes) {
        const index = previous.weaponSwap.active - 1;
        next = { ...next, weaponSwap: { ...previous.weaponSwap, exclusive_nodes: previous.weaponSwap.exclusive_nodes.map((nodes, i) => i === index ? nodes.filter(node => next.allocatedNodes.includes(node)) : nodes) as [number[], number[]] } };
      }
      next = switchWeapons(next, skillWeaponSet(next, next.params.main_socket_group));
      setState(next);
      stateRef.current = next;
      versionRef.current += 1;
      setStateVersion(versionRef.current);
      // 整份替换（导入 / 切 loadout）落地即为新基线，不算「未保存改动」。
      if (opts?.clean) {
        cleanVersionRef.current = versionRef.current;
        cleanNotesRef.current = notesRef.current;
        draftsRef.current = {};
        setEditorDrafts({});
      }
      saveToStorage(next, notesRef.current);
      recalc(next);
    },
    [recalc, saveToStorage],
  );


  // 启动：初始化后端 → 加载职业元数据 → 以首个职业开一个空 build（PoB2 新建语义）。
  useEffect(() => {
    let cancelled = false;
    getBackend()
      .then(async (backend) => {
        await backend.init((msg) => !cancelled && setBootMessage(msg));
        const meta = await backend.loadTreeMeta();
        if (!cancelled) setDataVersion(backend.dataVersion ?? null);
        backend
          .loadClassNames()
          .then((names) => !cancelled && setClassNames(names))
          .catch(() => {});
        if (cancelled) return;
        setTreeMeta(meta);
        setBootMessage(null);
        const raw = localStorage.getItem(STORAGE_KEY) ?? '';
        const saved = parseSaved(raw);
        if (raw && !saved) throw new Error('浏览器存档格式无效，原始数据已保留。请下载原始存档备份以便修复。');
        if (saved) {
          const envelope = JSON.parse(raw);
          const restored = envelope.workspace === undefined ? createWorkspace(saved) : parseWorkspace(envelope.workspace, parseSaved);
          if (!restored) throw new Error('浏览器工作区格式无效，原始数据已保留。请下载原始存档备份以便修复。');
          workspaceRef.current = restored;
          setWorkspace(restored);
          draftsRef.current = activeWorkspaceStage(restored).drafts;
          setEditorDrafts(draftsRef.current);
        }
        if (saved) {
          notesRef.current = saved.notes;
          setNotesState(saved.notes);
          if (saved.state.pobCode) {
            // 等解码完成再 apply：旧会话的 XML config 需要回填后才能进首次计算。
            backend
              .decodeBuild(saved.state.pobCode)
              .then((decoded) => {
                if (cancelled) return;
                setBuild(decoded);
                apply(backfillFromDecoded(saved.state, decoded));
              })
              .catch(() => !cancelled && apply(saved.state));
            return;
          }
          apply(saved.state);
          return;
        }
        const firstClass = meta.classes[0]?.name ?? 'Warrior';
        apply({
          pobCode: null,
          character: { level: 1, class_name: firstClass, ascendancy_name: '' },
          allocatedNodes: [],
          attributeChoices: {},
          socketGroups: [],
          items: [],
          flasks: [],
          jewels: [],
          annotations: {},
          params: { config_inputs: {} },
        });
      })
      .catch((err) => !cancelled && setBootError(String(err)));
    return () => {
      cancelled = true;
    };
    // apply 稳定（useCallback 无依赖变化）；仅挂载时启动一次。
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  /** 导入时把装备/珠宝/技能组自动收进库（按文本/套装名去重，避免重复导入堆叠）。 */
  const mergeImportedIntoLibrary = useCallback((decoded: BuildJson) => {
    const imported: LibraryItem[] = [
      ...[...decoded.items.equipped, ...(decoded.weapon_swap?.alternate_items ?? [])].map((it) => ({
        kind: 'item' as const,
        text: it.text,
        slot: it.slot as string | undefined,
      })),
      ...(decoded.items.socket_jewels ?? []).map((j) => ({
        kind: 'jewel' as const,
        text: j.text,
        slot: undefined,
      })),
    ].map((e) => ({
      id: crypto.randomUUID(),
      kind: e.kind,
      name: itemName(e.text),
      text: e.text,
      slot: e.slot,
    }));

    const setName = decoded.character.ascendancy_name || decoded.character.class_name;
    const skillSet: SkillSet | null = decoded.socket_groups.length
      ? { id: crypto.randomUUID(), name: setName, groups: materialize(decoded).socketGroups, main_socket_group: decoded.main_socket_group ?? undefined }
      : null;

    setLibrary((prev) => {
      // 同文本的旧条目若缺 slot（记 slot 功能之前存的），借本次导入回填——
      // 否则永远进不了槽位切换候选。
      const slotByText = new Map(imported.filter((i) => i.slot).map((i) => [i.text, i.slot]));
      let backfilled = false;
      const existing = prev.items.map((i) => {
        const slot = !i.slot ? slotByText.get(i.text) : undefined;
        if (slot) {
          backfilled = true;
          return { ...i, slot };
        }
        return i;
      });
      const seen = new Set(existing.map((i) => i.text));
      const newItems = imported.filter((i) => !seen.has(i.text));
      const skillSets =
        skillSet && !prev.skillSets.some((s) => s.name === skillSet.name)
          ? [...prev.skillSets, skillSet]
          : prev.skillSets;
      if (newItems.length === 0 && !backfilled && skillSets === prev.skillSets) return prev;
      const next: Library = { items: [...existing, ...newItems], skillSets };
      try {
        localStorage.setItem(LIBRARY_KEY, JSON.stringify(next));
      } catch {
        // 配额失败静默。
      }
      return next;
    });
  }, []);

  const importCode = useCallback(
    async (code: string, options: ImportOptions = { destination: 'newBuild' }) => {
      setBusy(true);
      setError(null);
      try {
        const backend = await getBackend();
        code = await resolveBuildInput(code);
        // JSON includes China-server .build files and WeGame share bundles.
        const isBuildFile = code.trimStart().startsWith('{');
        let decoded = isBuildFile
          ? await backend.decodeBuildFile(code)
          : await backend.decodeBuild(code);
        if (decoded.main_socket_group == null && decoded.socket_groups.some(group => group.enabled && group.gems.length)) {
          const materialized = materialize(decoded);
          const report = await fullDpsForState({
            ...materialized, pobCode: null, character: decoded.character,
            allocatedNodes: decoded.tree.allocated_nodes,
            attributeChoices: decoded.tree.attribute_choices ?? {}, annotations: {},
            params: paramsFromDecoded(decoded),
          });
          decoded = { ...decoded, main_socket_group: defaultMainSkill(materialized.socketGroups, report) ?? null };
        }
        setBuild(decoded);
        mergeImportedIntoLibrary(decoded);
        // <Notes> 里可能带 PoBR 注释标记段：拆成总览笔记 + 局部注释。
        const { overview, annotations } = splitNotes(decoded.notes ?? '');
        const importedState: BuildState = {
          pobCode: isBuildFile ? null : code,
          character: {
            level: decoded.character.level,
            class_name: decoded.character.class_name,
            ascendancy_name: decoded.character.ascendancy_name,
          },
          allocatedNodes: decoded.tree.allocated_nodes,
          attributeChoices: decoded.tree.attribute_choices ?? {},
          ...materialize(decoded),
          annotations,
          // XML 的 <Config> 与主技能组一并物化——calc 请求不再回传 pob_code，
          // 这里就是它们唯一的入口（Config 页也因此能直接显示导入值）。
          params: { ...paramsFromDecoded(decoded), main_socket_group: decoded.main_socket_group ?? undefined },
        };
        if (options.destination === 'newBuild' && workspaceRef.current) {
          const name = options.name?.trim() || decoded.character.ascendancy_name || decoded.character.class_name;
          workspaceRef.current = addWorkspaceBuild(workspaceRef.current, name, { version: 1, state: importedState, notes: overview });
        }
        // Do not persist imported notes into the stage being left behind.
        notesRef.current = overview;
        setNotesState(overview);
        apply(importedState, { clean: true });
        return true;
      } catch (err) {
        setError(formatApiError(err));
        setBusy(false);
        return false;
      }
    },
    [apply, mergeImportedIntoLibrary],
  );

  /**
   * 切到另一个 loadout（成组换天赋/装备/技能）。
   *
   * 实现上等价于「用同一份 code 换个视角重新导入」——切换在 wasm 的 XML 层完成
   * （改写三个 active 属性后重解析），所以结果与在 PoB2 里手动切三个下拉一致。
   *
   * 因此**会丢弃导入后的本地编辑**：整份状态由重解码结果覆盖，与 importCode 同
   * 语义。调用方负责在有未保存改动时先提示。无 `pobCode`（手搓/国服 .build 导入）
   * 时无从重解析，直接忽略。
   */
  const switchLoadout = useCallback(
    async (sel: { tree: number; item: number | null; skill: number | null }) => {
      const code = state?.pobCode;
      if (!code) return;
      setBusy(true);
      setError(null);
      try {
        const decoded = await (await getBackend()).switchLoadout(code, sel);
        setBuild(decoded);
        const { overview, annotations } = splitNotes(decoded.notes ?? '');
        setNotes(overview);
        apply({
          pobCode: decoded.code,
          character: {
            level: decoded.character.level,
            class_name: decoded.character.class_name,
            ascendancy_name: decoded.character.ascendancy_name,
          },
          allocatedNodes: decoded.tree.allocated_nodes,
          attributeChoices: decoded.tree.attribute_choices ?? {},
          ...materialize(decoded),
          annotations,
          params: { ...paramsFromDecoded(decoded), main_socket_group: decoded.main_socket_group ?? undefined },
        }, { clean: true });
      } catch (err) {
        setError(formatApiError(err));
        setBusy(false);
      }
    },
    [apply, setNotes, state?.pobCode],
  );

  /**
   * 组管理：复制 / 重命名 / 删除当前 loadout，然后按新 code 重新载入。
   *
   * `name` 会同时写进天赋树 / 装备 / 技能三类 set 的 title——**同名即成组**，所以
   * 用户只需要起个阶段名（"1-30级"、"mapping"），不必知道 `{tag}` 绑定语法。
   *
   * 与切换同样是整份重载，因此同样会丢弃未保存编辑；调用方负责先确认。
   * 无 `pobCode`（手搓 build）时无从操作，直接忽略。
   */
  const manageLoadout = useCallback(
    async (op: 'duplicate' | 'rename' | 'remove', name?: string) => {
      const code = stateRef.current?.pobCode;
      if (!code) return;
      setBusy(true);
      setError(null);
      try {
        const backend = await getBackend();
        // 目标 = 当前选中组（后端缺省即 active，这里显式传以免竞态）。
        const current = build?.loadouts?.[build?.active_loadout ?? 0];
        const nextCode = await backend.manageLoadout(
          code,
          op,
          name,
          current
            ? { tree: current.tree, item: current.item, skill: current.skill }
            : undefined,
        );
        await importCode(nextCode);
      } catch (err) {
        setError(formatApiError(err));
        setBusy(false);
      }
    },
    [importCode, build],
  );

  const newBuild = useCallback(
    (className: string, ascendancyName: string) => {
      setBuild(null);
      setNotes('');
      apply({
        pobCode: null,
        character: { level: 1, class_name: className, ascendancy_name: ascendancyName },
        allocatedNodes: [],
        attributeChoices: {},
        socketGroups: [],
        items: [],
        flasks: [],
        jewels: [],
        annotations: {},
        params: { config_inputs: {} },
      }, { clean: true });
    },
    [apply, setNotes],
  );

  const setSocketGroups = useCallback(
    (socketGroups: SocketGroupInput[]) => {
      if (!state) return;
      apply({ ...state, socketGroups });
    },
    [apply, state],
  );

  const setWeaponSet = useCallback((set: 1 | 2) => {
    if (!state) return;
    const main = state.params.main_socket_group ?? 0;
    const next = switchWeapons(state, set);
    // Explicitly switching the selected skill changes its exclusive binding too.
    apply({ ...next, socketGroups: next.socketGroups.map((group, i) =>
      i === main && group.weapon_set ? { ...group, weapon_set: set } : group) });
  }, [apply, state]);

  const setItems = useCallback(
    (items: SlotItemInput[]) => {
      if (!state) return;
      apply({ ...state, items });
    },
    [apply, state],
  );

  const setFlasks = useCallback(
    (flasks: SlotItemInput[]) => {
      if (!state) return;
      apply({ ...state, flasks });
    },
    [apply, state],
  );

  const setJewels = useCallback(
    (jewels: JewelInput[], allocation?: number[]) => {
      const current = stateRef.current;
      if (!current) return;
      const next = { ...current, jewels, allocatedNodes: allocation ?? current.allocatedNodes };
      const commit = (removed: Set<number>) => {
        const allocatedNodes = next.allocatedNodes.filter(id => !removed.has(id));
        const weaponSwap = next.weaponSwap ? { ...next.weaponSwap,
          exclusive_nodes: next.weaponSwap.exclusive_nodes.map(nodes => nodes.filter(id => !removed.has(id))) as [number[], number[]],
        } : next.weaponSwap;
        const kept = new Set([...allocatedNodes, ...(weaponSwap?.exclusive_nodes.flat() ?? [])]);
        const attributeChoices = Object.fromEntries(Object.entries(next.attributeChoices)
          .filter(([id]) => kept.has(Number(id))));
        apply({ ...next, allocatedNodes, weaponSwap, attributeChoices });
      };
      const inactive = current.weaponSwap?.exclusive_nodes.some(nodes => nodes.length > 0);
      if (!calc?.tree_effects?.allocation_grants.length && !busy && !inactive) {
        commit(new Set(current.allocatedNodes.filter(id => !next.allocatedNodes.includes(id))));
        return;
      }
      const seq = ++seqRef.current;
      setBusy(true);
      setError(null);
      getBackend().then(async backend => {
        const nodes = await backend.loadPassiveTree(current.treeVersion);
        const active = current.weaponSwap?.active ?? 1;
        const sets: (1 | 2)[] = inactive ? [active, active === 1 ? 2 : 1] : [active];
        const removed = new Set<number>();
        const views: { previous: BuildState; candidate: BuildState; before: CalculateBuildResponse; after: CalculateBuildResponse }[] = [];
        for (const set of sets) {
          const previous = switchWeapons(current, set);
          const candidate = switchWeapons(next, set);
          const before = set === active && !busy && calc ? calc : await backend.calculateBuild(toRequest(previous));
          const after = await backend.calculateBuild(toRequest(candidate));
          if (seqRef.current !== seq || stateRef.current !== current) return;
          views.push({ previous, candidate, before, after });
        }
        // Shared points must remain legal in both sets. Removing one can revoke
        // another provider, so converge on the retained allocations before saving.
        let changed = true;
        while (changed) {
          const count = removed.size;
          for (const { previous, candidate, before, after } of views) {
            const kept = new Set(reconcileJewelAllocation(nodes, previous.allocatedNodes,
              current.character.class_name, before.tree_effects, after.tree_effects,
              candidate.allocatedNodes.filter(id => !removed.has(id))));
            previous.allocatedNodes.forEach(id => { if (!kept.has(id)) removed.add(id); });
          }
          changed = removed.size !== count;
        }
        commit(removed);
      }).catch(err => {
        if (seqRef.current === seq) { setError(formatApiError(err)); setBusy(false); }
      });
    },
    [apply, busy, calc],
  );

  const removeJewelSocket = useCallback((socket: number, allocatedNodes: number[]) => {
    const current = stateRef.current;
    if (!current) return;
    setJewels(current.jewels.filter(jewel => jewel.socket_node !== socket), allocatedNodes);
  }, [setJewels]);

  const setCharacter = useCallback(
    (patch: Partial<CharacterState>) => {
      if (!state) return;
      apply({ ...state, character: { ...state.character, ...patch } });
    },
    [apply, state],
  );

  const toggleNode = useCallback(
    (skill: number, choice?: AttributeChoice) => {
      if (!state) return;
      const has = state.allocatedNodes.includes(skill);
      const allocatedNodes = has
        ? state.allocatedNodes.filter((n) => n !== skill)
        : [...state.allocatedNodes, skill];
      const attributeChoices = { ...state.attributeChoices };
      if (has) {
        delete attributeChoices[String(skill)];
      } else if (choice) {
        attributeChoices[String(skill)] = choice;
      }
      apply({ ...state, allocatedNodes, attributeChoices });
    },
    [apply, state],
  );

  /** Apply a complete passive plan and travel-attribute choices atomically.
   * Retained nodes keep their choices; refunded nodes lose theirs.
   */
  const setAllocatedNodes = useCallback(
    (skills: number[], choices?: Record<string, AttributeChoice>) => {
      if (!state) return;
      const allocatedNodes = [...new Set(skills)];
      const kept = new Set(allocatedNodes);
      const attributeChoices = Object.fromEntries(
        Object.entries({ ...state.attributeChoices, ...choices }).filter(([skill]) => kept.has(Number(skill))),
      );
      apply({ ...state, allocatedNodes, attributeChoices });
    },
    [apply, state],
  );

  const setAttributeChoices = useCallback(
    (attributeChoices: Record<string, AttributeChoice>) => {
      if (!state) return;
      apply({ ...state, attributeChoices });
    },
    [apply, state],
  );

  const updateParams = useCallback(
    (patch: Partial<CalcParams>) => {
      if (!state) return;
      apply({ ...state, params: paramsFromConfigInputs(patch.config_inputs ?? state.params.config_inputs, { ...state.params, ...patch }) });
    },
    [apply, state],
  );

  const setConfigInput = useCallback(
    (key: string, value: ConfigInputValue | null) => {
      if (!state) return;
      const config_inputs = { ...state.params.config_inputs };
      if (value === null) {
        delete config_inputs[key];
      } else {
        config_inputs[key] = value;
      }
      apply({ ...state, params: { ...state.params, config_inputs } });
    },
    [apply, state],
  );

  const setAnnotation = useCallback(
    (key: string, text: string) => {
      if (!state) return;
      const annotations = { ...state.annotations };
      if (text.trim()) {
        annotations[key] = text;
      } else {
        delete annotations[key];
      }
      // 注释不影响计算：只落状态与存档，不触发重算。
      const next = { ...state, annotations };
      setState(next);
      stateRef.current = next;
      saveToStorage(next, notesRef.current);
    },
    [state, saveToStorage],
  );

  const removeSocketGroup = useCallback(
    (index: number) => {
      if (!state) return;
      const socketGroups = state.socketGroups.filter((_, i) => i !== index);
      // Keep the same selected group after compaction; replace it only if removed.
      const main = state.params.main_socket_group ?? 0;
      const nextMain = main === index ? socketGroups.findIndex(group => group.enabled)
        : main > index ? main - 1 : main;
      // `skill:<index>` 注释键跟随组序号：删除组的注释一并删，后续组的键前移。
      const annotations: Annotations = {};
      for (const [key, text] of Object.entries(state.annotations)) {
        const m = key.match(/^skill:(\d+)$/);
        if (!m) {
          annotations[key] = text;
          continue;
        }
        const i = Number(m[1]);
        if (i === index) continue;
        annotations[i > index ? `skill:${i - 1}` : key] = text;
      }
      apply({ ...state, socketGroups, annotations, params: { ...state.params,
        main_socket_group: nextMain >= 0 && nextMain < socketGroups.length ? nextMain : undefined } });
    },
    [apply, state],
  );

  const exportSession = useCallback((): string => {
    if (!state) throw new Error('build not ready');
    const saved: SavedSession = { version: 1, state, notes };
    return JSON.stringify(saved, null, 2);
  }, [state, notes]);

  const exportCode = useCallback(async (): Promise<string> => {
    if (!state) throw new Error('build not ready');
    const backend = await getBackend();
    // encode 走全量覆盖（toRequest 本就不带 pob_code——分享内容 = 当前编辑态本身）；
    // 局部注释嵌入 <Notes> 标记段随 code 往返（PoB2 里显示为普通笔记）。
    // The base code includes the latest loadout selection; merge edits there while
    // preserving all other loadouts.
    const request = toRequest(state);
    return backend.encodeBuild({
      ...request,
      socket_groups: state.socketGroups,
      weapon_swap: state.weaponSwap,
      notes: composeNotes(notes, state.annotations),
      base_code: state.pobCode ?? undefined,
    });
  }, [state, notes]);

  const restoreSession = useCallback((saved: SavedSession, drafts: Record<string, string>) => {
    setBuild(null);
    setCalc(null);
    notesRef.current = saved.notes;
    setNotesState(saved.notes);
    draftsRef.current = drafts;
    setEditorDrafts(drafts);
    apply(saved.state);
    // Decode only display/export metadata. A delayed response must never replace
    // a newer stage or edits made while it was loading.
    const restoredState = stateRef.current;
    const restoredStageId = workspaceRef.current ? activeWorkspaceStage(workspaceRef.current).id : null;
    if (saved.state.pobCode) {
      void getBackend().then(backend => backend.decodeBuild(saved.state.pobCode!)).then(decoded => {
        if (!workspaceRef.current || activeWorkspaceStage(workspaceRef.current).id !== restoredStageId
          || stateRef.current?.pobCode !== saved.state.pobCode) return;
        setBuild(decoded);
        if (stateRef.current !== restoredState) return;
        const restored = backfillFromDecoded(saved.state, decoded);
        if (JSON.stringify(restored) !== JSON.stringify(saved.state)) apply(restored);
      }).catch(() => {});
    }
  }, [apply]);

  const importSession = useCallback((json: string, destination: ImportDestination = 'newBuild') => {
    const input = JSON.parse(json);
    if (input.format === 'pobr-build' && input.version === 1) {
      const imported = parseWorkspace({ version: 1, activeBuild: input.build?.id, builds: [input.build] }, parseSaved);
      if (!imported || !workspaceRef.current) throw new Error('invalid multi-stage build file');
      const source = imported.builds[0];
      const stages = source.stages.map(stage => ({ ...stage, id: crypto.randomUUID() }));
      const build = { ...source, id: crypto.randomUUID(), stages,
        activeStage: stages[source.stages.findIndex(stage => stage.id === source.activeStage)].id };
      workspaceRef.current = { ...workspaceRef.current, activeBuild: build.id, builds: [...workspaceRef.current.builds, build] };
      const stage = activeWorkspaceStage(workspaceRef.current);
      restoreSession(stage.saved, stage.drafts);
      return;
    }
    const saved = parseSaved(json);
    if (!saved) throw new Error('invalid session file');
    const envelope = JSON.parse(json);
    if (envelope.workspace !== undefined) {
      const restored = parseWorkspace(envelope.workspace, parseSaved);
      if (!restored) throw new Error('invalid workspace file');
      workspaceRef.current = restored;
      const stage = activeWorkspaceStage(restored);
      restoreSession(stage.saved, stage.drafts);
    } else {
      if (destination === 'newBuild' && workspaceRef.current) {
        workspaceRef.current = addWorkspaceBuild(workspaceRef.current,
          saved.state.character.ascendancy_name || saved.state.character.class_name, saved);
      }
      restoreSession(saved, {});
    }
  }, [restoreSession]);

  const selectStage = useCallback((buildId: string, stageId?: string) => {
    if (!workspaceRef.current || busy) return;
    const next = selectWorkspaceStage(workspaceRef.current, buildId, stageId);
    if (next === workspaceRef.current) return;
    const stage = activeWorkspaceStage(next);
    workspaceRef.current = next;
    restoreSession(stage.saved, stage.drafts);
  }, [busy, restoreSession]);

  const createLocalBuild = useCallback((name: string) => {
    if (!workspaceRef.current || !stateRef.current || busy || !name.trim()) return;
    const saved: SavedSession = { version: 1, notes: '', state: {
      pobCode: null, character: { ...stateRef.current.character, level: 1 },
      allocatedNodes: [], attributeChoices: {}, socketGroups: [], items: [], flasks: [], jewels: [], annotations: {}, params: { config_inputs: {} },
    } };
    workspaceRef.current = addWorkspaceBuild(workspaceRef.current, name, saved);
    restoreSession(saved, {});
  }, [busy, restoreSession]);

  const duplicateStage = useCallback((name: string) => {
    if (!workspaceRef.current || busy || !name.trim()) return;
    const next = duplicateWorkspaceStage(workspaceRef.current, name);
    workspaceRef.current = next;
    const stage = activeWorkspaceStage(next);
    restoreSession(stage.saved, stage.drafts);
  }, [busy, restoreSession]);

  const renameWorkspace = useCallback((kind: 'build' | 'stage', name: string) => {
    if (workspaceRef.current) persistWorkspace(renameWorkspaceEntry(workspaceRef.current, kind, name));
  }, [persistWorkspace]);

  const exportWorkspace = useCallback(() => {
    if (!workspaceRef.current) throw new Error('build not ready');
    return workspaceEnvelope(workspaceRef.current);
  }, []);

  const exportLocalBuild = useCallback(() => {
    const current = workspaceRef.current;
    if (!current) throw new Error('build not ready');
    const build = current.builds.find(entry => entry.id === current.activeBuild)!;
    // Unapplied editor drafts stay in backups, not in files shared with players.
    return JSON.stringify({ format: 'pobr-build', version: 1, build: {
      ...build, stages: build.stages.map(stage => ({ ...stage, drafts: {} })),
    } }, null, 2);
  }, []);

  const currentRequest = useCallback(
    (): CalculateBuildRequest | null => (state ? toRequest(state) : null),
    [state],
  );

  const saveLibraryItem = useCallback(
    (kind: 'item' | 'jewel', text: string, slot?: string) => {
      persistLibrary({
        ...library,
        items: [
          ...library.items,
          { id: crypto.randomUUID(), kind, name: itemName(text), text, slot },
        ],
      });
    },
    [library, persistLibrary],
  );

  const removeLibraryItem = useCallback(
    (id: string) => {
      persistLibrary({ ...library, items: library.items.filter((i) => i.id !== id) });
    },
    [library, persistLibrary],
  );

  const saveSkillSet = useCallback(
    (name: string) => {
      if (!state) return;
      persistLibrary({
        ...library,
        skillSets: [
          ...library.skillSets,
          {
            id: crypto.randomUUID(),
            name,
            groups: state.socketGroups,
            main_socket_group: state.params.main_socket_group,
          },
        ],
      });
    },
    [library, persistLibrary, state],
  );

  const applySkillSet = useCallback(
    (id: string) => {
      if (!state) return;
      const set = library.skillSets.find((s) => s.id === id);
      if (!set) return;
      // 整套换组后旧的 `skill:<index>` 注释指向已不存在的组——一并清掉。
      const annotations = Object.fromEntries(
        Object.entries(state.annotations).filter(([key]) => !key.startsWith('skill:')),
      );
      apply({
        ...state,
        socketGroups: set.groups,
        annotations,
        params: { ...state.params, main_socket_group: set.main_socket_group },
      });
    },
    [apply, library, state],
  );

  const removeSkillSet = useCallback(
    (id: string) => {
      persistLibrary({ ...library, skillSets: library.skillSets.filter((s) => s.id !== id) });
    },
    [library, persistLibrary],
  );

  const runAttribution = useCallback(
    async (fields: string[]) => {
      if (!state) throw new Error('build not ready');
      const backend = await getBackend();
      return backend.attribution({ request: toRequest(state), fields });
    },
    [state],
  );

  const runFullDps = useCallback(async (): Promise<FullDpsResponse> => {
    if (!state) throw new Error('build not ready');
    return fullDpsForState(state);
  }, [state]);

  return {
    treeVersion: state?.treeVersion,
    dataVersion, workspace, storageFailed, selectStage, createLocalBuild, duplicateStage, renameWorkspace, exportWorkspace, exportLocalBuild,
    activeWeaponSet: state?.weaponSwap?.active ?? 1,
    weaponSwap: state?.weaponSwap,
    setWeaponSet,
    bootMessage,
    bootError,
    build,
    treeMeta,
    classNames,
    character: state?.character ?? null,
    allocatedNodes: state?.allocatedNodes ?? [],
    attributeChoices: state?.attributeChoices ?? {},
    socketGroups: state?.socketGroups ?? [],
    items: state?.items ?? [],
    flasks: state?.flasks ?? [],
    jewels: state?.jewels ?? [],
    calc,
    calcParams: state?.params ?? { config_inputs: {} },
    busy,
    error,
    notes,
    setNotes,
    editorDrafts,
    setEditorDraft,
    annotations: state?.annotations ?? {},
    setAnnotation,
    removeSocketGroup,
    exportSession,
    exportCode,
    importSession,
    importCode,
    switchLoadout,
    manageLoadout,
    loadouts: build?.loadouts ?? [],
    activeLoadout: build?.active_loadout ?? null,
    newBuild,
    setCharacter,
    toggleNode,
    setAllocatedNodes,
    setAttributeChoices,
    setSocketGroups,
    setItems,
    setFlasks,
    setJewels,
    removeJewelSocket,
    currentRequest,
    stateVersion,
    isDirty: stateVersion > cleanVersionRef.current || notes !== cleanNotesRef.current || hasDrafts,
    hasBuildContent: !!build || hasDrafts || !!notes.trim() || !!state && Boolean(
      state.character.level > 1 || state.character.ascendancy_name
      || state.items.length || state.flasks.length || state.jewels.length
      || state.socketGroups.length || state.allocatedNodes.length
      || state.weaponSwap?.alternate_items.length
      || state.weaponSwap?.exclusive_nodes.some(nodes => nodes.length > 0)
      || Object.keys(state.annotations).length
      || Object.keys(state.params.config_inputs).length
      || Object.entries(state.params).some(([key, value]) => key !== 'config_inputs' && value !== undefined)
    ),
    library,
    saveLibraryItem,
    removeLibraryItem,
    saveSkillSet,
    applySkillSet,
    removeSkillSet,
    updateParams,
    setConfigInput,
    runAttribution,
    runFullDps,
  };
}
