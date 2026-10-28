import { useJobBeacon } from '../../v2/jobs/useJobBeacon';
import { AppButton, AppInput } from '../shared/controls';
import { useEffect, useLayoutEffect, useMemo, useRef, useState, type RefObject } from 'react';
import { formatApiError } from '../../api/error';
import type { AttributeChoice, CalculateBuildRequest, PassiveNode } from '../../api/types';
import type { BuildSession } from '../../hooks/useBuildSession';
import { bindT, type Lang } from '../../lib/i18n';
import { scoreOf, type Objective } from '../../lib/optimize';
import { useUpgradeGoal } from '../../hooks/useUpgradeGoal';
import {
  PASSIVE_PLAN_LIMIT,
  PASSIVE_POINT_LIMIT,
  passivePlanAllocation,
  passivePlanningContext,
  planPassiveUpgrades,
  withTravelAttributes,
  type PassivePlan,
  type PassivePlannerResult,
  type PassivePlanningContext,
} from '../../lib/passivePlanner';
import { AppSelect } from '../shared/AppSelect';
import { ObjectiveEditor, OptimizerProgress, objectiveOf, type ObjectiveState } from '../shared/OptimizerControls';
import { WeaponSetControl } from '../shared/WeaponSetControl';
import { JewelPassivePlanner, useJewelPassivePlanner, type JewelPassivePlannerState } from './JewelPassivePlanner';

interface Props {
  session: BuildSession;
  lang: Lang;
  active: boolean;
  nodes: PassiveNode[];
  nodeLabel: (skill: number) => string;
  onPreview: (plan: PassivePlan | null) => void;
  focusPlanner?: { nonce: number };
}

interface PassiveReceipt {
  key: string;
  request: CalculateBuildRequest;
  preparedRequest: CalculateBuildRequest;
  result: PassivePlannerResult;
  objective: Objective;
  dataVersion: BuildSession['dataVersion'];
  treeVersion: BuildSession['treeVersion'];
}

export interface TreeOptimizerState {
  session: BuildSession;
  lang: Lang;
  nodeLabel: (skill: number) => string;
  containerRef: RefObject<HTMLDivElement | null>;
  expanded: boolean;
  setExpanded: (value: boolean) => void;
  mode: 'allocate' | 'reallocate';
  setMode: (value: 'allocate' | 'reallocate') => void;
  points: number;
  setPoints: (value: number) => void;
  goal: ObjectiveState;
  setGoal: (value: ObjectiveState) => void;
  attribute: AttributeChoice;
  setAttribute: (value: AttributeChoice) => void;
  context: PassivePlanningContext;
  tracking: boolean;
  progress: { done: number; total: number } | null;
  error: string | null;
  completed: PassiveReceipt | null;
  evaluated: PassivePlannerResult | null;
  preview: number | null;
  applied: PassivePlan | null;
  run: () => void;
  cancel: () => void;
  show: (index: number | null) => void;
  apply: (plan: PassivePlan) => void;
  jewelPlanner: JewelPassivePlannerState;
}

const COPY = {
  'en-US': {
    title: 'Plan passive upgrades', hint: 'Find connected routes from your current tree. Travel points are included, and each complete plan is recalculated with your current equipment and selected skill.',
    mode: 'Plan', allocate: 'Spend new points', reallocate: 'Reallocate existing points', points: 'Point budget', refundPoints: 'Maximum refunds',
    attribute: 'New travel attributes', str: 'Strength', dex: 'Dexterity', int: 'Intelligence',
    run: 'Find upgrade routes', preview: 'Show on tree', apply: 'Apply plan', clear: 'Clear preview',
    applied: 'Passive plan applied',
    scope: 'Plans change the shared main tree. Ascendancy, mastery choices, weapon-set-only nodes and the value of an empty jewel socket are excluded. Attribute travel points use the choice above.',
    refundUnavailable: 'Automatic refunds are unavailable with weapon-set passives, mastery or unknown nodes, or when the imported tree cannot be fully connected to the class start.',
    noResults: 'No improving legal plan was found within this budget and search range.',
    bounded: 'Bounded search, not a global optimum. It compares connected routes and combinations; review the route before spending or refunding points.',
    skipped: 'Skipped plans with new unmodeled effects', evaluated: 'Plans evaluated', add: 'Allocate', remove: 'Refund', cost: 'Net points', score: 'Goal gain',
    needsBuild: 'Import or create a build to plan upgrades.', route: 'Complete route', baseline: 'Evaluated baseline',
    unavailable: 'The class start or passive graph is unavailable.',
    refresh: 'After the first search, changes refresh this open planner automatically. Cancel or collapse to pause.',
    waiting: 'Waiting for the current build calculation, then searching the updated tree…',
    limits: 'At most 8 new or refunded points and 512 full-plan evaluations per search. Refund mode moves terminal branches within the current total point count.',
    limited: 'The search limit was reached; results cover only the evaluated routes.',
    data: 'Data version', tree: 'Tree version', defaultTree: 'Default', unknownVersion: 'Unavailable',
    baselineLimits: 'Baseline calculation limits', baselineLimitsHint: 'These engine-reported effects are already present in the evaluated baseline. They are not new route effects; calculated gains do not resolve these limits.',
    excluded: 'Excluded complete routes', excludedHint: 'These evaluated edits are excluded from recommendations and cannot be applied.',
    newLimits: 'New unsupported effects', engineError: 'Engine error', invalidStats: 'The engine returned non-finite calculated values.',
    previewHint: 'The tree preview highlights a proposed route, not the current allocation. Gains compare the recalculated route with the evaluated baseline.',
  },
  'zh-TW': {
    title: '天賦提升規劃', hint: '自動尋找目前天賦樹可連接的路線，過路點計入預算。每個完整方案都按目前裝備和主技能重新計算。',
    mode: '規劃方式', allocate: '投入新天賦點', reallocate: '原點數內洗點', points: '可投入點數', refundPoints: '最多退還點數',
    attribute: '新增過路屬性點', str: '力量', dex: '敏捷', int: '智慧',
    run: '尋找提升路線', preview: '在樹上查看', apply: '套用方案', clear: '清除預覽',
    applied: '已套用天賦方案',
    scope: '規劃共用主樹，排除昇華、精通選項、武器組專屬點及空珠寶孔的未知收益。新增過路屬性點使用上方選擇。',
    refundUnavailable: '存在武器組專屬點、精通或未知節點，或匯入樹無法完整連接到職業起點時，暫不自動洗點。',
    noResults: '在本次點數預算和搜尋範圍內，未找到合法且優於目前配置的方案。',
    bounded: '這是有限範圍的路線與組合搜尋，不保證全域最優。請先查看路線，再投入或退還天賦點。',
    skipped: '已排除含新增未建模效果的方案', evaluated: '已重算方案', add: '新增', remove: '退還', cost: '淨投入', score: '目標提升',
    needsBuild: '匯入或新建角色後可規劃天賦提升。', route: '完整路線', baseline: '評估基準',
    unavailable: '缺少職業起點或天賦連接資料。',
    refresh: '首次搜尋後，修改配置會自動更新已展開的規劃。取消或收起可暫停。',
    waiting: '等待目前角色重算完成，再搜尋修改後的天賦樹…',
    limits: '每次最多投入或退還 8 點、重算 512 個完整方案。洗點模式在目前總點數內搬移末端分支。',
    limited: '已達搜尋上限；結果僅涵蓋本次已評估的路線。',
    data: '資料版本', tree: '天賦樹版本', defaultTree: '預設', unknownVersion: '無法取得',
    baselineLimits: '基準計算限制', baselineLimitsHint: '這些引擎回報的效果已存在於評估基準，並非路線新增效果；計算出的提升不代表已解決這些限制。',
    excluded: '已排除的完整路線', excludedHint: '這些已評估的變更不列入推薦，也無法套用。',
    newLimits: '新增未支援效果', engineError: '引擎錯誤', invalidStats: '引擎回傳了非有限數值的計算結果。',
    previewHint: '樹上預覽標示的是提案路線，而非目前配置。提升數值比較重算路線與評估基準。',
  },
  'zh-CN': {
    title: '天赋提升规划', hint: '自动寻找当前天赋树可连接的路线，过路点计入预算。每个完整方案都按当前装备和主技能重新计算。',
    mode: '规划方式', allocate: '投入新天赋点', reallocate: '原点数内洗点', points: '可投入点数', refundPoints: '最多退还点数',
    attribute: '新增过路属性点', str: '力量', dex: '敏捷', int: '智慧',
    run: '寻找提升路线', preview: '在树上查看', apply: '应用方案', clear: '清除预览',
    applied: '已应用天赋方案',
    scope: '规划共用主树，排除升华、精通选项、武器组专属点及空珠宝孔的未知收益。新增过路属性点使用上方选择。',
    refundUnavailable: '存在武器组专属点、精通或未知节点，或导入树无法完整连接到职业起点时，暂不自动洗点。',
    noResults: '在本次点数预算和搜索范围内，未找到合法且优于当前配置的方案。',
    bounded: '这是有限范围的路线与组合搜索，不保证全局最优。请先查看路线，再投入或退还天赋点。',
    skipped: '已排除含新增未建模效果的方案', evaluated: '已重算方案', add: '新增', remove: '退还', cost: '净投入', score: '目标提升',
    needsBuild: '导入或新建角色后可规划天赋提升。', route: '完整路线', baseline: '评估基准',
    unavailable: '缺少职业起点或天赋连接数据。',
    refresh: '首次搜索后，修改配置会自动更新已展开的规划。取消或收起可暂停。',
    waiting: '等待当前角色重算完成，再搜索修改后的天赋树…',
    limits: '每次最多投入或退还 8 点、重算 512 个完整方案。洗点模式在当前总点数内搬移末端分支。',
    limited: '已达搜索上限；结果仅涵盖本次已评估的路线。',
    data: '数据版本', tree: '天赋树版本', defaultTree: '默认', unknownVersion: '无法取得',
    baselineLimits: '基准计算限制', baselineLimitsHint: '这些引擎报告的效果已存在于评估基准，并非路线新增效果；计算出的提升不代表已解决这些限制。',
    excluded: '已排除的完整路线', excludedHint: '这些已评估的变更不列入推荐，也无法应用。',
    newLimits: '新增未支持效果', engineError: '引擎错误', invalidStats: '引擎返回了非有限数值的计算结果。',
    previewHint: '树上预览标示的是提案路线，而非当前配置。提升数值比较重算路线与评估基准。',
  },
};

export function useTreeOptimizer({ session, lang, active, nodes, nodeLabel, onPreview, focusPlanner }: Props): TreeOptimizerState {
  const [expanded, setExpanded] = useState(Boolean(focusPlanner));
  const [mode, setMode] = useState<'allocate' | 'reallocate'>('allocate');
  const [points, setPoints] = useState(3);
  const { goal, setGoal } = useUpgradeGoal();
  const [attribute, setAttribute] = useState<AttributeChoice>(() => {
    const counts = { str: 0, dex: 0, int: 0 };
    Object.values(session.attributeChoices).forEach(choice => { counts[choice] += 1; });
    if (Object.values(counts).some(Boolean)) return (Object.keys(counts) as AttributeChoice[]).sort((a, b) => counts[b] - counts[a])[0];
    return ['Sorceress', 'Witch', 'Templar', 'Druid'].includes(session.character?.class_name ?? '') ? 'int'
      : ['Warrior', 'Marauder'].includes(session.character?.class_name ?? '') ? 'str' : 'dex';
  });
  const [progress, setProgress] = useState<{ done: number; total: number } | null>(null);
  useJobBeacon(progress !== null);
  const [error, setError] = useState<string | null>(null);
  const [completed, setCompleted] = useState<PassiveReceipt | null>(null);
  const [applied, setApplied] = useState<{ version: number; plan: PassivePlan } | null>(null);
  const [tracking, setTracking] = useState(false);
  const [runId, setRunId] = useState(0);
  const [preview, setPreview] = useState<number | null>(null);
  const abortRef = useRef<AbortController | null>(null);
  const timerRef = useRef<number | null>(null);
  const containerRef = useRef<HTMLDivElement | null>(null);
  const consumedFocus = useRef<number | undefined>(undefined);
  const previewRef = useRef(onPreview);
  previewRef.current = onPreview;
  const context = useMemo(() => passivePlanningContext(nodes, session.allocatedNodes,
    session.character?.class_name, session.weaponSwap?.exclusive_nodes.flat() ?? [],
    session.jewels.map(jewel => jewel.socket_node), session.treeMeta?.classes.flatMap(entry => entry.ascendancies ?? [])
      .find(entry => entry.name === session.character?.ascendancy_name)?.id ?? session.character?.ascendancy_name,
    session.calc?.tree_effects),
  [nodes, session.allocatedNodes, session.character?.class_name, session.weaponSwap, session.jewels, session.treeMeta, session.character?.ascendancy_name, session.calc?.tree_effects]);
  const baselineStats = useMemo(() => Object.fromEntries((session.calc?.stats ?? []).map(stat => [stat.id, stat.value ?? 0])), [session.calc]);
  const objective = useMemo(() => objectiveOf(goal, baselineStats), [goal, baselineStats]);
  const request = useMemo(() => session.currentRequest(), [session.currentRequest, session.stateVersion]);
  // Pair the full evaluation with its effective goal and authoritative jewel topology.
  const snapshotKey = useMemo(() => JSON.stringify([session.stateVersion, session.dataVersion, session.treeVersion,
    session.activeWeaponSet, request, session.calc?.tree_effects, mode, points, goal, attribute, objective]),
  [session.stateVersion, session.dataVersion, session.treeVersion, session.activeWeaponSet, request,
    session.calc?.tree_effects, mode, points, goal, attribute, objective]);
  const latestKeyRef = useRef(snapshotKey);
  latestKeyRef.current = snapshotKey;
  const activeRef = useRef(active);
  activeRef.current = active;
  const evaluated = completed?.key === snapshotKey ? completed.result : null;

  useLayoutEffect(() => {
    if (!active || !focusPlanner || !containerRef.current || consumedFocus.current === focusPlanner.nonce) return;
    consumedFocus.current = focusPlanner.nonce;
    setExpanded(true);
    containerRef.current?.scrollIntoView({ block: 'start' });
  }, [active, focusPlanner?.nonce, nodes]);

  const stop = () => {
    abortRef.current?.abort();
    abortRef.current = null;
    if (timerRef.current !== null) window.clearTimeout(timerRef.current);
    timerRef.current = null;
    setProgress(null);
    setPreview(null);
    previewRef.current(null);
  };
  const cancel = () => {
    stop();
    setTracking(false);
  };
  useLayoutEffect(() => {
    stop();
    setCompleted(null);
    setError(null);
  }, [snapshotKey]);
  useLayoutEffect(() => {
    if (!active) { cancel(); setError(null); }
  }, [active]);
  useLayoutEffect(() => () => {
    abortRef.current?.abort();
    abortRef.current = null;
    if (timerRef.current !== null) window.clearTimeout(timerRef.current);
    timerRef.current = null;
  }, []);

  useEffect(() => {
    abortRef.current?.abort();
    abortRef.current = null;
    setProgress(null);
    if (!active || !tracking || !expanded || completed?.key === snapshotKey
      || session.busy || session.calcError || !session.calc || !session.character
      || !request || context.root === null || (mode === 'reallocate' && !context.canRefund)) return;
    const controller = new AbortController();
    abortRef.current = controller;
    // Coalesce rapid edits and wait for the session's new baseline before spending the search budget.
    const timer = window.setTimeout(async () => {
      if (timerRef.current === timer) timerRef.current = null;
      if (!activeRef.current || controller.signal.aborted || abortRef.current !== controller || latestKeyRef.current !== snapshotKey) return;
      const prepared = withTravelAttributes(request, nodes, attribute);
      setProgress({ done: 0, total: PASSIVE_PLAN_LIMIT });
      setCompleted(null);
      setError(null);
      try {
        const result = await planPassiveUpgrades({ request: prepared, context, points, mode, objective,
          signal: controller.signal, onProgress: (done, total) => {
            if (activeRef.current && !controller.signal.aborted && abortRef.current === controller && latestKeyRef.current === snapshotKey) setProgress({ done, total });
          } });
        if (activeRef.current && !controller.signal.aborted && abortRef.current === controller && latestKeyRef.current === snapshotKey) {
          setCompleted({ key: snapshotKey, request, preparedRequest: prepared, result, objective,
            dataVersion: session.dataVersion, treeVersion: session.treeVersion });
        }
      } catch (err) {
        if (activeRef.current && !controller.signal.aborted && abortRef.current === controller && latestKeyRef.current === snapshotKey) setError(formatApiError(err));
      } finally {
        if (abortRef.current === controller) { setProgress(null); abortRef.current = null; }
      }
    }, 350);
    timerRef.current = timer;
    return () => {
      window.clearTimeout(timer);
      if (timerRef.current === timer) timerRef.current = null;
      controller.abort();
      if (abortRef.current === controller) abortRef.current = null;
    };
  }, [active, snapshotKey, tracking, expanded, completed, session.busy, session.calcError, session.calc, session.character, request,
    context, nodes, attribute, points, mode, objective, runId]);

  const run = () => {
    if (!active || session.busy || session.calcError || !session.calc) return;
    stop();
    jewelPlanner.stop();
    setCompleted(null);
    setError(null);
    setTracking(true);
    setRunId(value => value + 1);
  };
  const show = (index: number | null) => {
    if (!active || latestKeyRef.current !== snapshotKey || (index !== null && !evaluated?.plans[index])) return;
    setPreview(index);
    previewRef.current(index === null ? null : evaluated?.plans[index] ?? null);
  };
  const apply = (plan: PassivePlan) => {
    if (!active || !completed || completed.key !== latestKeyRef.current || session.busy
      || JSON.stringify(session.currentRequest()) !== JSON.stringify(completed.request)
      || !completed.result.plans.some(candidate => candidate === plan)) return;
    const next = passivePlanAllocation(completed.preparedRequest, context, plan, points, mode);
    if (!next) return;
    // Apply the same full allocation and attribute choices used by the successful evaluation.
    stop();
    setCompleted(null);
    session.setAllocatedNodes(next.allocatedNodes, next.attributeChoices);
    // A successful atomic session edit increments stateVersion exactly once.
    // Later edits cannot revive this receipt, even if they restore the same allocation.
    setApplied({ version: session.stateVersion + 1, plan });
  };
  const jewelPlanner = useJewelPassivePlanner({ session, lang, nodes, nodeLabel, active: active && expanded,
    mode, points, attribute, objective, onPreview, onStart: cancel });
  return { session, lang, nodeLabel, containerRef, expanded, setExpanded, mode, setMode, points, setPoints,
    goal, setGoal, attribute, setAttribute, context, tracking, progress, error, completed, evaluated, preview,
    applied: applied?.version === session.stateVersion && !session.busy && !session.calcError ? applied.plan : null,
    run, cancel, show, apply, jewelPlanner };
}

export function TreeOptimizer({ planner }: { planner: TreeOptimizerState }) {
  const { session, lang, nodeLabel, containerRef, expanded, setExpanded, mode, setMode, points, setPoints,
    goal, setGoal, attribute, setAttribute, context, tracking, progress, error, completed, evaluated, preview, applied,
    run, cancel, show, apply, jewelPlanner } = planner;
  const text = COPY[lang];
  const tt = bindT(lang);
  const number = (value: number) => value.toLocaleString('en-US', { maximumFractionDigits: 2 });
  const delta = (value: number, baseline: number) => baseline > 0
    ? `${value >= baseline ? '+' : ''}${((value / baseline - 1) * 100).toFixed(1)}%`
    : `${value >= baseline ? '+' : ''}${number(value - baseline)}`;
  const currentStats = evaluated?.baseline ?? {};

  return (
    <div className="tree-planner" ref={containerRef} id="passive-upgrades">
      <AppButton className="tree-planner-toggle" aria-expanded={expanded} onClick={() => { if (expanded) cancel(); setExpanded(!expanded); }}>
        <span aria-hidden>{expanded ? '▾' : '▸'}</span>{text.title}
      </AppButton>
      {expanded && <div className="tree-planner-body">
        <p className="tree-planner-hint">{text.hint}</p>
        {applied && <p className="tree-planner-hint tree-planner-applied" role="status">
          <strong>{text.applied}</strong><br />
          {text.add} {applied.allocate.length}: {applied.allocate.map(id => `${nodeLabel(id)} #${id}`).join(' · ') || '—'}<br />
          {text.remove} {applied.deallocate.length}: {applied.deallocate.map(id => `${nodeLabel(id)} #${id}`).join(' · ') || '—'}
        </p>}
        <WeaponSetControl session={session} lang={lang} />
        <div className="tree-planner-controls">
          <label>{text.mode}<AppSelect value={mode} ariaLabel={text.mode} onChange={value => setMode(value as typeof mode)} options={[
            { value: 'allocate', label: text.allocate }, { value: 'reallocate', label: text.reallocate },
          ]} /></label>
          <label>{mode === 'allocate' ? text.points : text.refundPoints}<AppInput type="number" min={1} max={PASSIVE_POINT_LIMIT} value={points}
            onChange={event => { const value = Number(event.target.value); if (Number.isInteger(value) && value >= 1 && value <= PASSIVE_POINT_LIMIT) setPoints(value); }} /></label>
          <ObjectiveEditor value={goal} onChange={setGoal} lang={lang} />
          <label>{text.attribute}<AppSelect value={attribute} ariaLabel={text.attribute} onChange={value => setAttribute(value as AttributeChoice)} options={[
            { value: 'str', label: text.str }, { value: 'dex', label: text.dex }, { value: 'int', label: text.int },
          ]} /></label>
        </div>
        <p className="tree-planner-hint">{text.scope}</p>
        <p className="tree-planner-hint">{text.limits} {text.refresh}</p>
        {mode === 'reallocate' && !context.canRefund && <p className="tree-planner-hint">{text.refundUnavailable}</p>}
        {!session.character && <p className="tree-planner-hint">{text.needsBuild}</p>}
        {context.root === null && session.character && <p className="tree-planner-hint">{text.unavailable}</p>}
        <AppButton className="tree-planner-run" onClick={run} disabled={session.busy || Boolean(session.calcError) || !session.calc || progress !== null || !session.character
          || context.root === null || (mode === 'reallocate' && !context.canRefund)}>{text.run}</AppButton>
        {tracking && !evaluated && !progress && !error && !session.calcError && context.root !== null
          && (mode === 'allocate' || context.canRefund) && <p className="tree-planner-hint" role="status">{text.waiting}</p>}
        {progress && <OptimizerProgress {...progress} lang={lang} onCancel={cancel} />}
        {error && <p className="opt-error">{error}</p>}
        {evaluated && <>
          <div className="tree-planner-summary"><span>{text.evaluated}: {evaluated.evaluated}</span>
            {evaluated.evaluated > 0 && <span>{text.baseline}: DPS {number(currentStats.TotalDPS ?? 0)} · EHP {number(currentStats.TotalEHP ?? 0)}</span>}
            <span>{text.data}: {completed!.dataVersion ?? text.unknownVersion} · {text.tree}: {completed!.treeVersion ?? text.defaultTree}</span>
            {preview !== null && <AppButton onClick={() => show(null)}>{text.clear}</AppButton>}
          </div>
          {evaluated.limited && <p className="tree-planner-hint">{text.limited}</p>}
          {evaluated.unmodeled > 0 && <p className="tree-planner-hint">{text.skipped}: {evaluated.unmodeled}</p>}
          {preview !== null && <p className="tree-planner-hint" role="status">{text.previewHint}</p>}
          {evaluated.baselineUnsupported.length > 0 && <details className="tree-planner-diagnostics tree-planner-baseline-unsupported">
            <summary>{text.baselineLimits}: {evaluated.baselineUnsupported.length}</summary>
            <p className="tree-planner-hint">{text.baselineLimitsHint}</p>
            <ul>{evaluated.baselineUnsupported.map((reason, index) => <li key={index}>{reason}</li>)}</ul>
          </details>}
          {evaluated.excluded.length > 0 && <details className="tree-planner-diagnostics">
            <summary>{text.excluded}: {evaluated.excluded.length}</summary>
            <p className="tree-planner-hint">{text.excludedHint}</p>
            <ol className="tree-planner-excluded">{evaluated.excluded.map(({ plan, unsupported, error: engineError }, index) => <li key={index}>
              <details>
                <summary>{text.route}: {nodeLabel(plan.target)} · {text.add} {plan.allocate.length} · {text.remove} {plan.deallocate.length}</summary>
                <p>{text.add}: {plan.allocate.map(id => `${nodeLabel(id)} #${id}`).join(' · ') || '—'}</p>
                <p>{text.remove}: {plan.deallocate.map(id => `${nodeLabel(id)} #${id}`).join(' · ') || '—'}</p>
                {unsupported.length > 0 && <><p>{text.newLimits}</p>
                  <ul>{unsupported.map((reason, reasonIndex) => <li key={reasonIndex}>{reason}</li>)}</ul></>}
                {engineError && <p className="opt-error">{text.engineError}: {engineError}</p>}
                {unsupported.length === 0 && !engineError && <p className="opt-error">{text.invalidStats}</p>}
              </details>
            </li>)}</ol>
          </details>}
          {evaluated.plans.length === 0 && <p className="tree-planner-hint">{text.noResults}</p>}
          <ol className="tree-planner-results">
            {evaluated.plans.map((plan, index) => <li key={index} className={preview === index ? 'is-previewed' : ''}>
              <div className="tree-plan-heading"><strong>{index + 1}. {nodeLabel(plan.target)}</strong>
                <span>{text.cost} {plan.allocate.length - plan.deallocate.length} · {text.add} {plan.allocate.length}{plan.deallocate.length > 0 ? ` · ${text.remove} ${plan.deallocate.length}` : ''}</span></div>
              <div className="tree-plan-metrics">
                <span>{text.score} <b>{delta(scoreOf(plan.stats, completed!.objective), scoreOf(currentStats, completed!.objective))}</b></span>
                {(['TotalDPS', 'TotalEHP', 'Life'] as const).map(stat => <span key={stat}>
                  {stat === 'TotalDPS' ? 'DPS' : stat === 'TotalEHP' ? 'EHP' : tt('opt.objLife')} <b className={(plan.stats[stat] ?? 0) >= (currentStats[stat] ?? 0) ? 'is-positive' : 'is-negative'}>{delta(plan.stats[stat] ?? 0, currentStats[stat] ?? 0)}</b>
                  <small>{number(currentStats[stat] ?? 0)} → <output data-stat={stat}>{number(plan.stats[stat] ?? 0)}</output></small>
                </span>)}
              </div>
              <details><summary>{text.route}</summary><p>{text.add}: {plan.allocate.map(nodeLabel).join(' · ')}</p>
                {plan.deallocate.length > 0 && <p>{text.remove}: {plan.deallocate.map(nodeLabel).join(' · ')}</p>}</details>
              <div className="tree-plan-actions"><AppButton onClick={() => show(index)}>{text.preview}</AppButton><AppButton disabled={session.busy} onClick={() => apply(plan)}>{text.apply}</AppButton></div>
            </li>)}
          </ol>
          <p className="tree-planner-hint">{text.bounded}</p>
        </>}
        <JewelPassivePlanner planner={jewelPlanner} />
      </div>}
    </div>
  );
}
