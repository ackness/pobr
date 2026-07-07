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
  AttributionResponse,
  BuildJson,
  CalculateBuildRequest,
  CalculateBuildResponse,
  ConfigInputValue,
  EnemyTier,
  PassiveTreeMeta,
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
  params: CalcParams;
}

export interface BuildSession {
  bootMessage: string | null;
  bootError: string | null;
  /** 解码出的原始 build（Items/Skills 展示；白手 build 为 null）。 */
  build: BuildJson | null;
  treeMeta: PassiveTreeMeta | null;
  character: CharacterState | null;
  allocatedNodes: number[];
  calc: CalculateBuildResponse | null;
  calcParams: CalcParams;
  busy: boolean;
  error: string | null;
  importCode: (code: string) => Promise<void>;
  newBuild: (className: string, ascendancyName: string) => void;
  setCharacter: (patch: Partial<CharacterState>) => void;
  toggleNode: (skill: number) => void;
  updateParams: (patch: Partial<CalcParams>) => void;
  setConfigInput: (key: string, value: ConfigInputValue | null) => void;
  runAttribution: (fields: string[]) => Promise<AttributionResponse>;
}

function toRequest(state: BuildState): CalculateBuildRequest {
  return {
    pob_code: state.pobCode ?? undefined,
    character: state.character,
    allocated_nodes: state.allocatedNodes,
    main_socket_group: state.params.main_socket_group,
    enemy_tier: state.params.enemy_tier,
    config_inputs: state.params.config_inputs,
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

  /** 应用新状态并触发重算。 */
  const apply = useCallback(
    (next: BuildState) => {
      setState(next);
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
        const firstClass = meta.classes[0]?.name ?? 'Warrior';
        apply({
          pobCode: null,
          character: { level: 1, class_name: firstClass, ascendancy_name: '' },
          allocatedNodes: [],
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
        apply({
          pobCode: code,
          character: {
            level: decoded.character.level,
            class_name: decoded.character.class_name,
            ascendancy_name: decoded.character.ascendancy_name,
          },
          allocatedNodes: decoded.tree.allocated_nodes,
          params: { config_inputs: {} },
        });
      } catch (err) {
        setError(String(err));
        setBusy(false);
      }
    },
    [apply],
  );

  const newBuild = useCallback(
    (className: string, ascendancyName: string) => {
      setBuild(null);
      apply({
        pobCode: null,
        character: { level: 1, class_name: className, ascendancy_name: ascendancyName },
        allocatedNodes: [],
        params: { config_inputs: {} },
      });
    },
    [apply],
  );

  const setCharacter = useCallback(
    (patch: Partial<CharacterState>) => {
      if (!state) return;
      apply({ ...state, character: { ...state.character, ...patch } });
    },
    [apply, state],
  );

  const toggleNode = useCallback(
    (skill: number) => {
      if (!state) return;
      const has = state.allocatedNodes.includes(skill);
      const allocatedNodes = has
        ? state.allocatedNodes.filter((n) => n !== skill)
        : [...state.allocatedNodes, skill];
      apply({ ...state, allocatedNodes });
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

  const runAttribution = useCallback(
    async (fields: string[]) => {
      if (!state) throw new Error('build not ready');
      const backend = await getBackend();
      return backend.attribution({
        pob_code: state.pobCode ?? undefined,
        character: state.character,
        allocated_nodes: state.allocatedNodes,
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
    calc,
    calcParams: state?.params ?? { config_inputs: {} },
    busy,
    error,
    importCode,
    newBuild,
    setCharacter,
    toggleNode,
    updateParams,
    setConfigInput,
    runAttribution,
  };
}
