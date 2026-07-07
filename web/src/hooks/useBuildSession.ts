/**
 * Build 会话状态：导入 → 解码 + 计算；参数变更（主技能组 / config / 敌人档位）
 * → 重算。所有后端交互经 `api/backend`，组件层只消费本 hook。
 */

import { useCallback, useEffect, useRef, useState } from 'react';
import { getBackend } from '../api/backend';
import type {
  AttributionResponse,
  BuildJson,
  CalculateBuildResponse,
  ConfigInputValue,
  EnemyTier,
} from '../api/types';

export interface CalcParams {
  main_socket_group?: number;
  enemy_tier?: EnemyTier;
  config_inputs: Record<string, ConfigInputValue>;
}

export interface BuildSession {
  /** 后端初始化状态消息（null = 就绪）。 */
  bootMessage: string | null;
  bootError: string | null;
  build: BuildJson | null;
  calc: CalculateBuildResponse | null;
  calcParams: CalcParams;
  /** 正在计算（导入或重算中）。 */
  busy: boolean;
  error: string | null;
  importCode: (code: string) => Promise<void>;
  updateParams: (patch: Partial<CalcParams>) => void;
  setConfigInput: (key: string, value: ConfigInputValue | null) => void;
  runAttribution: (fields: string[]) => Promise<AttributionResponse>;
}

export function useBuildSession(): BuildSession {
  const [bootMessage, setBootMessage] = useState<string | null>('初始化…');
  const [bootError, setBootError] = useState<string | null>(null);
  const [build, setBuild] = useState<BuildJson | null>(null);
  const [calc, setCalc] = useState<CalculateBuildResponse | null>(null);
  const [calcParams, setCalcParams] = useState<CalcParams>({ config_inputs: {} });
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const codeRef = useRef<string>('');

  useEffect(() => {
    let cancelled = false;
    getBackend()
      .then((backend) => backend.init((msg) => !cancelled && setBootMessage(msg)))
      .then(() => !cancelled && setBootMessage(null))
      .catch((err) => !cancelled && setBootError(String(err)));
    return () => {
      cancelled = true;
    };
  }, []);

  const recalc = useCallback(async (code: string, params: CalcParams) => {
    const backend = await getBackend();
    return backend.calculateBuild({
      pob_code: code,
      main_socket_group: params.main_socket_group,
      enemy_tier: params.enemy_tier,
      config_inputs: params.config_inputs,
    });
  }, []);

  const importCode = useCallback(
    async (code: string) => {
      setBusy(true);
      setError(null);
      try {
        const backend = await getBackend();
        const decoded = await backend.decodeBuild(code);
        codeRef.current = code;
        const params: CalcParams = { config_inputs: {} };
        setCalcParams(params);
        setBuild(decoded);
        setCalc(await recalc(code, params));
      } catch (err) {
        setError(String(err));
      } finally {
        setBusy(false);
      }
    },
    [recalc],
  );

  const applyParams = useCallback(
    (params: CalcParams) => {
      setCalcParams(params);
      if (!codeRef.current) return;
      setBusy(true);
      setError(null);
      recalc(codeRef.current, params)
        .then(setCalc)
        .catch((err) => setError(String(err)))
        .finally(() => setBusy(false));
    },
    [recalc],
  );

  const updateParams = useCallback(
    (patch: Partial<CalcParams>) => {
      applyParams({ ...calcParams, ...patch });
    },
    [applyParams, calcParams],
  );

  const setConfigInput = useCallback(
    (key: string, value: ConfigInputValue | null) => {
      const config_inputs = { ...calcParams.config_inputs };
      if (value === null) {
        delete config_inputs[key];
      } else {
        config_inputs[key] = value;
      }
      applyParams({ ...calcParams, config_inputs });
    },
    [applyParams, calcParams],
  );

  const runAttribution = useCallback(
    async (fields: string[]) => {
      const backend = await getBackend();
      return backend.attribution({
        pob_code: codeRef.current,
        fields,
        main_socket_group: calcParams.main_socket_group,
        enemy_tier: calcParams.enemy_tier,
      });
    },
    [calcParams],
  );

  return {
    bootMessage,
    bootError,
    build,
    calc,
    calcParams,
    busy,
    error,
    importCode,
    updateParams,
    setConfigInput,
    runAttribution,
  };
}
