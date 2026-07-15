/**
 * Build 会话状态（PoB2 语义：启动即有一个可编辑的空 build）。
 *
 * 可编辑面：角色身份（职业/升华/等级）、已加点集合（树交互加点）、主技能组、
 * config 覆盖；导入 build code 则整体替换基线。每次编辑触发重算，带请求序号
 * 防止乱序返回覆盖新状态。所有后端交互经 `api/backend`。
 */

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
  EnemyTier,
  FullDpsResponse,
  JewelInput,
  PassiveTreeMeta,
  SlotItemInput,
  SocketGroupInput,
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
}

/** 会话完整可编辑状态（重算请求由此派生）。 */
interface BuildState {
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

export interface BuildSession {
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
  importSession: (json: string) => void;
  importCode: (code: string) => Promise<void>;
  newBuild: (className: string, ascendancyName: string) => void;
  setCharacter: (patch: Partial<CharacterState>) => void;
  /** 点选加点/取消；属性小点加点时带三选一。 */
  toggleNode: (skill: number, choice?: AttributeChoice) => void;
  /** 整表替换属性三选一（批量调配 / 快捷键改单点）。 */
  setAttributeChoices: (choices: Record<string, AttributeChoice>) => void;
  /** 当前完整计算请求（对比预览用：克隆后改一处再算一次）。 */
  currentRequest: () => CalculateBuildRequest | null;
  /** 状态版本号（每次编辑 +1；hover 收益等缓存的失效键）。 */
  stateVersion: number;
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
    character: state.character,
    allocated_nodes: state.allocatedNodes,
    attribute_choices: state.attributeChoices,
    socket_groups: state.socketGroups,
    items: state.items,
    flasks: state.flasks,
    jewels: state.jewels,
    main_socket_group: state.params.main_socket_group,
    enemy_tier: state.params.enemy_tier,
    extra_modifiers: state.params.extra_modifiers,
    config_inputs: state.params.config_inputs,
  };
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

function saveToStorage(state: BuildState, notes: string) {
  try {
    const saved: SavedSession = { version: 1, state, notes };
    localStorage.setItem(STORAGE_KEY, JSON.stringify(saved));
  } catch {
    // 配额/隐私模式失败时静默——持久化是增强，不阻断编辑。
  }
}

/** 解析并校验存档信封（导入文件 / localStorage 共用）；非法返回 null。 */
function parseSaved(json: string): SavedSession | null {
  try {
    const parsed = JSON.parse(json) as SavedSession;
    if (parsed.version !== 1 || typeof parsed.state !== 'object' || parsed.state === null) {
      return null;
    }
    const state = parsed.state;
    if (
      typeof state.character?.class_name !== 'string' ||
      !Array.isArray(state.allocatedNodes) ||
      !Array.isArray(state.socketGroups) ||
      !Array.isArray(state.items)
    ) {
      return null;
    }
    return {
      version: 1,
      state: {
        pobCode: typeof state.pobCode === 'string' ? state.pobCode : null,
        character: state.character,
        allocatedNodes: state.allocatedNodes,
        attributeChoices: state.attributeChoices ?? {},
        socketGroups: state.socketGroups,
        items: state.items,
        flasks: state.flasks ?? [],
        jewels: state.jewels ?? [],
        annotations: state.annotations ?? {},
        params: state.params ?? { config_inputs: {} },
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
function backfillFromDecoded(state: BuildState, decoded: BuildJson): BuildState {
  return {
    ...state,
    params: {
      ...state.params,
      main_socket_group: state.params.main_socket_group ?? decoded.main_socket_group ?? undefined,
      config_inputs: { ...decoded.config_inputs, ...state.params.config_inputs },
    },
  };
}

/** 解码结果 → 可编辑技能组/装备状态（物化，之后全走覆盖）。 */
function materialize(
  decoded: BuildJson,
): Pick<BuildState, 'socketGroups' | 'items' | 'flasks' | 'jewels'> {
  return {
    socketGroups: decoded.socket_groups.map((g) => ({
      slot: g.slot,
      enabled: g.enabled,
      source: g.source,
      gems: g.gems.map((gem) => ({
        skill_id: gem.skill_id,
        level: gem.level,
        quality: gem.quality,
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

export function useBuildSession(): BuildSession {
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

  const notesRef = useRef('');
  const stateRef = useRef<BuildState | null>(null);

  const setNotes = useCallback((text: string) => {
    setNotesState(text);
    notesRef.current = text;
    localStorage.setItem('pobr-notes', text);
    if (stateRef.current) saveToStorage(stateRef.current, text);
  }, []);
  const [library, setLibrary] = useState<Library>(loadLibrary);
  const [stateVersion, setStateVersion] = useState(0);

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
        if (seqRef.current === seq) setError(String(err));
      })
      .finally(() => {
        if (seqRef.current === seq) setBusy(false);
      });
  }, []);

  /** 应用新状态并触发重算 + 自动保存到浏览器。 */
  const apply = useCallback(
    (next: BuildState) => {
      setState(next);
      stateRef.current = next;
      setStateVersion((v) => v + 1);
      saveToStorage(next, notesRef.current);
      recalc(next);
    },
    [recalc],
  );

  // 启动：初始化后端 → 加载职业元数据 → 以首个职业开一个空 build（PoB2 新建语义）。
  useEffect(() => {
    let cancelled = false;
    getBackend()
      .then(async (backend) => {
        await backend.init((msg) => !cancelled && setBootMessage(msg));
        const meta = await backend.loadTreeMeta();
        backend
          .loadClassNames()
          .then((names) => !cancelled && setClassNames(names))
          .catch(() => {});
        if (cancelled) return;
        setTreeMeta(meta);
        setBootMessage(null);
        const saved = parseSaved(localStorage.getItem(STORAGE_KEY) ?? '');
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
      ...decoded.items.equipped.map((it) => ({
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
      ? { id: crypto.randomUUID(), name: setName, groups: materialize(decoded).socketGroups }
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
    async (code: string) => {
      setBusy(true);
      setError(null);
      try {
        const backend = await getBackend();
        // 以 `{` 开头视为国服 .build 文件（JSON），否则按 PoB2 code 解码。
        const isBuildFile = code.trimStart().startsWith('{');
        const decoded = isBuildFile
          ? await backend.decodeBuildFile(code)
          : await backend.decodeBuild(code);
        setBuild(decoded);
        mergeImportedIntoLibrary(decoded);
        // <Notes> 里可能带 PoBR 注释标记段：拆成总览笔记 + 局部注释。
        const { overview, annotations } = splitNotes(decoded.notes ?? '');
        if (decoded.notes) {
          setNotes(overview);
        }
        apply({
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
          params: {
            config_inputs: decoded.config_inputs ?? {},
            main_socket_group: decoded.main_socket_group ?? undefined,
          },
        });
      } catch (err) {
        setError(String(err));
        setBusy(false);
      }
    },
    [apply, setNotes, mergeImportedIntoLibrary],
  );

  const newBuild = useCallback(
    (className: string, ascendancyName: string) => {
      setBuild(null);
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
      });
    },
    [apply],
  );

  const setSocketGroups = useCallback(
    (socketGroups: SocketGroupInput[]) => {
      if (!state) return;
      apply({ ...state, socketGroups });
    },
    [apply, state],
  );

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
    (jewels: JewelInput[]) => {
      if (!state) return;
      apply({ ...state, jewels });
    },
    [apply, state],
  );

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
      apply({ ...state, params: { ...state.params, ...patch } });
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
    [state],
  );

  const removeSocketGroup = useCallback(
    (index: number) => {
      if (!state) return;
      const socketGroups = state.socketGroups.filter((_, i) => i !== index);
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
      apply({ ...state, socketGroups, annotations });
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
    const request = toRequest(state);
    return backend.encodeBuild({ ...request, notes: composeNotes(notes, state.annotations) });
  }, [state, notes]);

  const importSession = useCallback(
    (json: string) => {
      const saved = parseSaved(json);
      if (!saved) {
        throw new Error('invalid session file');
      }
      setBuild(null);
      notesRef.current = saved.notes;
      setNotesState(saved.notes);
      localStorage.setItem('pobr-notes', saved.notes);
      if (saved.state.pobCode) {
        getBackend()
          .then((b) => b.decodeBuild(saved.state.pobCode!))
          .then((decoded) => {
            setBuild(decoded);
            apply(backfillFromDecoded(saved.state, decoded));
          })
          .catch(() => apply(saved.state));
      } else {
        apply(saved.state);
      }
    },
    [apply],
  );

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
    const backend = await getBackend();
    return backend.fullDps(toRequest(state));
  }, [state]);

  return {
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
    annotations: state?.annotations ?? {},
    setAnnotation,
    removeSocketGroup,
    exportSession,
    exportCode,
    importSession,
    importCode,
    newBuild,
    setCharacter,
    toggleNode,
    setAttributeChoices,
    setSocketGroups,
    setItems,
    setFlasks,
    setJewels,
    currentRequest,
    stateVersion,
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
