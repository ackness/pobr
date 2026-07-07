/**
 * Build 会话状态（PoB2 语义：启动即有一个可编辑的空 build）。
 *
 * 可编辑面：角色身份（职业/升华/等级）、已加点集合（树交互加点）、主技能组、
 * config 覆盖；导入 build code 则整体替换基线。每次编辑触发重算，带请求序号
 * 防止乱序返回覆盖新状态。所有后端交互经 `api/backend`。
 */

import { useCallback, useEffect, useRef, useState } from 'react';
import { getBackend } from '../api/backend';
import type {
  AttributeChoice,
  AttributionResponse,
  BuildJson,
  CalculateBuildRequest,
  CalculateBuildResponse,
  ConfigInputValue,
  EnemyTier,
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
  /** 装备槽原始文本（同上；珠宝/药剂 v1 只读，仍由 pob_code 基线携带）。 */
  items: SlotItemInput[];
  params: CalcParams;
}

export interface BuildSession {
  bootMessage: string | null;
  bootError: string | null;
  /** 解码出的原始 build（珠宝/药剂等只读展示；白手 build 为 null）。 */
  build: BuildJson | null;
  treeMeta: PassiveTreeMeta | null;
  character: CharacterState | null;
  allocatedNodes: number[];
  attributeChoices: Record<string, AttributeChoice>;
  socketGroups: SocketGroupInput[];
  items: SlotItemInput[];
  calc: CalculateBuildResponse | null;
  calcParams: CalcParams;
  busy: boolean;
  error: string | null;
  /** 笔记（本地持久化；导入 build 时被其 <Notes> 覆盖）。 */
  notes: string;
  setNotes: (text: string) => void;
  /** 导出完整会话（build 状态 + 笔记）为 JSON 文本。 */
  exportSession: () => string;
  /** 从导出的 JSON 恢复会话；非法输入抛错。 */
  importSession: (json: string) => void;
  importCode: (code: string) => Promise<void>;
  newBuild: (className: string, ascendancyName: string) => void;
  setCharacter: (patch: Partial<CharacterState>) => void;
  /** 点选加点/取消；属性小点加点时带三选一。 */
  toggleNode: (skill: number, choice?: AttributeChoice) => void;
  /** 整表替换属性三选一（批量调配 / 快捷键改单点）。 */
  setAttributeChoices: (choices: Record<string, AttributeChoice>) => void;
  /** 整份替换技能组（Skills 编辑器）。 */
  setSocketGroups: (groups: SocketGroupInput[]) => void;
  /** 整份替换装备（Items 编辑器）。 */
  setItems: (items: SlotItemInput[]) => void;
  updateParams: (patch: Partial<CalcParams>) => void;
  setConfigInput: (key: string, value: ConfigInputValue | null) => void;
  runAttribution: (fields: string[]) => Promise<AttributionResponse>;
}

function toRequest(state: BuildState): CalculateBuildRequest {
  return {
    pob_code: state.pobCode ?? undefined,
    character: state.character,
    allocated_nodes: state.allocatedNodes,
    attribute_choices: state.attributeChoices,
    socket_groups: state.socketGroups,
    items: state.items,
    main_socket_group: state.params.main_socket_group,
    enemy_tier: state.params.enemy_tier,
    config_inputs: state.params.config_inputs,
  };
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
        params: state.params ?? { config_inputs: {} },
      },
      notes: typeof parsed.notes === 'string' ? parsed.notes : '',
    };
  } catch {
    return null;
  }
}

/** 解码结果 → 可编辑技能组/装备状态（物化，之后全走覆盖）。 */
function materialize(decoded: BuildJson): Pick<BuildState, 'socketGroups' | 'items'> {
  return {
    socketGroups: decoded.socket_groups.map((g) => ({
      slot: g.slot,
      enabled: g.enabled,
      gems: g.gems.map((gem) => ({
        skill_id: gem.skill_id,
        level: gem.level,
        quality: gem.quality,
      })),
    })),
    items: decoded.items.equipped.map((item) => ({ slot: item.slot, text: item.text })),
  };
}

export function useBuildSession(): BuildSession {
  const [bootMessage, setBootMessage] = useState<string | null>('初始化…');
  const [bootError, setBootError] = useState<string | null>(null);
  const [treeMeta, setTreeMeta] = useState<PassiveTreeMeta | null>(null);
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
        if (cancelled) return;
        setTreeMeta(meta);
        setBootMessage(null);
        const saved = parseSaved(localStorage.getItem(STORAGE_KEY) ?? '');
        if (saved) {
          notesRef.current = saved.notes;
          setNotesState(saved.notes);
          if (saved.state.pobCode) {
            backend
              .decodeBuild(saved.state.pobCode)
              .then((decoded) => !cancelled && setBuild(decoded))
              .catch(() => {});
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

  const importCode = useCallback(
    async (code: string) => {
      setBusy(true);
      setError(null);
      try {
        const backend = await getBackend();
        const decoded = await backend.decodeBuild(code);
        setBuild(decoded);
        if (decoded.notes) {
          setNotes(decoded.notes);
        }
        apply({
          pobCode: code,
          character: {
            level: decoded.character.level,
            class_name: decoded.character.class_name,
            ascendancy_name: decoded.character.ascendancy_name,
          },
          allocatedNodes: decoded.tree.allocated_nodes,
          attributeChoices: decoded.tree.attribute_choices ?? {},
          ...materialize(decoded),
          params: { config_inputs: {} },
        });
      } catch (err) {
        setError(String(err));
        setBusy(false);
      }
    },
    [apply, setNotes],
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

  const exportSession = useCallback((): string => {
    if (!state) throw new Error('build not ready');
    const saved: SavedSession = { version: 1, state, notes };
    return JSON.stringify(saved, null, 2);
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
          .then(setBuild)
          .catch(() => {});
      }
      apply(saved.state);
    },
    [apply],
  );

  const runAttribution = useCallback(
    async (fields: string[]) => {
      if (!state) throw new Error('build not ready');
      const backend = await getBackend();
      return backend.attribution({
        pob_code: state.pobCode ?? undefined,
        character: state.character,
        allocated_nodes: state.allocatedNodes,
        attribute_choices: state.attributeChoices,
        socket_groups: state.socketGroups,
        items: state.items,
        fields,
        main_socket_group: state.params.main_socket_group,
        enemy_tier: state.params.enemy_tier,
      });
    },
    [state],
  );

  return {
    bootMessage,
    bootError,
    build,
    treeMeta,
    character: state?.character ?? null,
    allocatedNodes: state?.allocatedNodes ?? [],
    attributeChoices: state?.attributeChoices ?? {},
    socketGroups: state?.socketGroups ?? [],
    items: state?.items ?? [],
    calc,
    calcParams: state?.params ?? { config_inputs: {} },
    busy,
    error,
    notes,
    setNotes,
    exportSession,
    importSession,
    importCode,
    newBuild,
    setCharacter,
    toggleNode,
    setAttributeChoices,
    setSocketGroups,
    setItems,
    updateParams,
    setConfigInput,
    runAttribution,
  };
}
