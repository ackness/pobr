import { useEffect, useMemo, useRef, useState } from 'react';
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
} from '../../lib/passivePlanner';
import { AppSelect } from '../shared/AppSelect';
import { ObjectiveEditor, OptimizerProgress, objectiveOf } from '../shared/OptimizerControls';
import { WeaponSetControl } from '../shared/WeaponSetControl';

interface Props {
  session: BuildSession;
  lang: Lang;
  nodes: PassiveNode[];
  nodeLabel: (skill: number) => string;
  onPreview: (plan: PassivePlan | null) => void;
  focusPlanner?: { nonce: number };
}

const COPY = {
  'en-US': {
    title: 'Plan passive upgrades', hint: 'Find connected routes from your current tree. Travel points are included, and each complete plan is recalculated with your current equipment and selected skill.',
    mode: 'Plan', allocate: 'Spend new points', reallocate: 'Reallocate existing points', points: 'Point budget', refundPoints: 'Maximum refunds',
    attribute: 'New travel attributes', str: 'Strength', dex: 'Dexterity', int: 'Intelligence',
    run: 'Find upgrade routes', preview: 'Show on tree', apply: 'Apply plan', clear: 'Clear preview',
    scope: 'Plans change the shared main tree. Ascendancy, mastery choices, weapon-set-only nodes and the value of an empty jewel socket are excluded. Attribute travel points use the choice above.',
    refundUnavailable: 'Automatic refunds are unavailable with weapon-set passives, mastery or unknown nodes, or when the imported tree cannot be fully connected to the class start.',
    noResults: 'No improving legal plan was found within this budget and search range.',
    bounded: 'Bounded search, not a global optimum. It compares connected routes and combinations; review the route before spending or refunding points.',
    skipped: 'Skipped plans with new unmodeled effects', evaluated: 'Plans evaluated', add: 'Allocate', remove: 'Refund', cost: 'Net points', score: 'Goal gain',
    needsBuild: 'Import or create a build to plan upgrades.', route: 'Complete route', baseline: 'Current build',
    unavailable: 'The class start or passive graph is unavailable.',
    refresh: 'After the first search, changes refresh this open planner automatically. Cancel or collapse to pause.',
    waiting: 'Waiting for the current build calculation, then searching the updated tree…',
    limits: 'At most 8 new or refunded points and 512 full-plan evaluations per search. Refund mode moves terminal branches within the current total point count.',
    limited: 'The search limit was reached; results cover only the evaluated routes.',
  },
  'zh-TW': {
    title: '天賦提升規劃', hint: '自動尋找目前天賦樹可連接的路線，過路點計入預算。每個完整方案都按目前裝備和主技能重新計算。',
    mode: '規劃方式', allocate: '投入新天賦點', reallocate: '原點數內洗點', points: '可投入點數', refundPoints: '最多退還點數',
    attribute: '新增過路屬性點', str: '力量', dex: '敏捷', int: '智慧',
    run: '尋找提升路線', preview: '在樹上查看', apply: '套用方案', clear: '清除預覽',
    scope: '規劃共用主樹，排除昇華、精通選項、武器組專屬點及空珠寶孔的未知收益。新增過路屬性點使用上方選擇。',
    refundUnavailable: '存在武器組專屬點、精通或未知節點，或匯入樹無法完整連接到職業起點時，暫不自動洗點。',
    noResults: '在本次點數預算和搜尋範圍內，未找到合法且優於目前配置的方案。',
    bounded: '這是有限範圍的路線與組合搜尋，不保證全域最優。請先查看路線，再投入或退還天賦點。',
    skipped: '已排除含新增未建模效果的方案', evaluated: '已重算方案', add: '新增', remove: '退還', cost: '淨投入', score: '目標提升',
    needsBuild: '匯入或新建角色後可規劃天賦提升。', route: '完整路線', baseline: '目前配置',
    unavailable: '缺少職業起點或天賦連接資料。',
    refresh: '首次搜尋後，修改配置會自動更新已展開的規劃。取消或收起可暫停。',
    waiting: '等待目前角色重算完成，再搜尋修改後的天賦樹…',
    limits: '每次最多投入或退還 8 點、重算 512 個完整方案。洗點模式在目前總點數內搬移末端分支。',
    limited: '已達搜尋上限；結果僅涵蓋本次已評估的路線。',
  },
  'zh-CN': {
    title: '天赋提升规划', hint: '自动寻找当前天赋树可连接的路线，过路点计入预算。每个完整方案都按当前装备和主技能重新计算。',
    mode: '规划方式', allocate: '投入新天赋点', reallocate: '原点数内洗点', points: '可投入点数', refundPoints: '最多退还点数',
    attribute: '新增过路属性点', str: '力量', dex: '敏捷', int: '智慧',
    run: '寻找提升路线', preview: '在树上查看', apply: '应用方案', clear: '清除预览',
    scope: '规划共用主树，排除升华、精通选项、武器组专属点及空珠宝孔的未知收益。新增过路属性点使用上方选择。',
    refundUnavailable: '存在武器组专属点、精通或未知节点，或导入树无法完整连接到职业起点时，暂不自动洗点。',
    noResults: '在本次点数预算和搜索范围内，未找到合法且优于当前配置的方案。',
    bounded: '这是有限范围的路线与组合搜索，不保证全局最优。请先查看路线，再投入或退还天赋点。',
    skipped: '已排除含新增未建模效果的方案', evaluated: '已重算方案', add: '新增', remove: '退还', cost: '净投入', score: '目标提升',
    needsBuild: '导入或新建角色后可规划天赋提升。', route: '完整路线', baseline: '当前配置',
    unavailable: '缺少职业起点或天赋连接数据。',
    refresh: '首次搜索后，修改配置会自动更新已展开的规划。取消或收起可暂停。',
    waiting: '等待当前角色重算完成，再搜索修改后的天赋树…',
    limits: '每次最多投入或退还 8 点、重算 512 个完整方案。洗点模式在当前总点数内搬移末端分支。',
    limited: '已达搜索上限；结果仅涵盖本次已评估的路线。',
  },
};

export function TreeOptimizer({ session, lang, nodes, nodeLabel, onPreview, focusPlanner }: Props) {
  const text = COPY[lang];
  const tt = bindT(lang);
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
  const [error, setError] = useState<string | null>(null);
  const [completed, setCompleted] = useState<{ key: string; request: CalculateBuildRequest;
    result: PassivePlannerResult; objective: Objective } | null>(null);
  const [tracking, setTracking] = useState(false);
  const [runId, setRunId] = useState(0);
  const [preview, setPreview] = useState<number | null>(null);
  const abortRef = useRef<AbortController | null>(null);
  const containerRef = useRef<HTMLDivElement | null>(null);
  const previewRef = useRef(onPreview);
  previewRef.current = onPreview;
  const context = useMemo(() => passivePlanningContext(nodes, session.allocatedNodes,
    session.character?.class_name, session.weaponSwap?.exclusive_nodes.flat() ?? [],
    session.jewels.map(jewel => jewel.socket_node), session.treeMeta?.classes.flatMap(entry => entry.ascendancies ?? [])
      .find(entry => entry.name === session.character?.ascendancy_name)?.id ?? session.character?.ascendancy_name),
  [nodes, session.allocatedNodes, session.character?.class_name, session.weaponSwap, session.jewels, session.treeMeta, session.character?.ascendancy_name]);
  const baselineStats = useMemo(() => Object.fromEntries((session.calc?.stats ?? []).map(stat => [stat.id, stat.value ?? 0])), [session.calc]);
  const objective = useMemo(() => objectiveOf(goal, baselineStats), [goal, baselineStats]);
  const request = useMemo(() => session.currentRequest(), [session.currentRequest, session.stateVersion]);
  // Include the materialized request as well as the revision: results never migrate to another tree.
  const snapshotKey = useMemo(() => JSON.stringify([session.stateVersion, request, mode, points, goal, attribute]),
    [session.stateVersion, request, mode, points, goal, attribute]);
  const latestKeyRef = useRef(snapshotKey);
  latestKeyRef.current = snapshotKey;
  const evaluated = completed?.key === snapshotKey ? completed.result : null;

  useEffect(() => {
    if (!focusPlanner) return;
    setExpanded(true);
    containerRef.current?.scrollIntoView({ block: 'start' });
  }, [focusPlanner?.nonce]);

  useEffect(() => {
    abortRef.current?.abort();
    setCompleted(null);
    setError(null);
    setPreview(null);
    previewRef.current(null);
  }, [snapshotKey]);

  useEffect(() => {
    abortRef.current?.abort();
    abortRef.current = null;
    setProgress(null);
    if (!tracking || !expanded || session.busy || session.error || !session.character
      || !request || context.root === null || (mode === 'reallocate' && !context.canRefund)) return;
    const controller = new AbortController();
    abortRef.current = controller;
    // Coalesce rapid edits and wait for the session's new baseline before spending the search budget.
    const timer = setTimeout(async () => {
      const prepared = withTravelAttributes(request, nodes, attribute);
      setProgress({ done: 0, total: PASSIVE_PLAN_LIMIT });
      setCompleted(null);
      setError(null);
      try {
        const result = await planPassiveUpgrades({ request: prepared, context, points, mode, objective,
          signal: controller.signal, onProgress: (done, total) => {
            if (!controller.signal.aborted && latestKeyRef.current === snapshotKey) setProgress({ done, total });
          } });
        if (!controller.signal.aborted && abortRef.current === controller && latestKeyRef.current === snapshotKey) {
          setCompleted({ key: snapshotKey, request: prepared, result, objective });
        }
      } catch (err) {
        if (!controller.signal.aborted && latestKeyRef.current === snapshotKey) setError(formatApiError(err));
      } finally {
        if (abortRef.current === controller) { setProgress(null); abortRef.current = null; }
      }
    }, 350);
    return () => {
      clearTimeout(timer);
      controller.abort();
      if (abortRef.current === controller) abortRef.current = null;
    };
  }, [snapshotKey, tracking, expanded, session.busy, session.error, session.character, request,
    context, nodes, attribute, points, mode, objective, runId]);

  const run = () => {
    setCompleted(null);
    setError(null);
    setPreview(null);
    previewRef.current(null);
    setTracking(true);
    setRunId(value => value + 1);
  };
  const cancel = () => {
    abortRef.current?.abort();
    abortRef.current = null;
    setTracking(false);
    setProgress(null);
  };
  const show = (index: number | null) => {
    if (latestKeyRef.current !== snapshotKey || (index !== null && !evaluated)) return;
    setPreview(index);
    previewRef.current(index === null ? null : evaluated?.plans[index] ?? null);
  };
  const apply = (plan: PassivePlan) => {
    if (!completed || completed.key !== latestKeyRef.current || session.busy) return;
    const next = passivePlanAllocation(completed.request, context, plan, points, mode);
    if (!next) return;
    // Apply the same full allocation and attribute choices used by the successful evaluation.
    session.setAllocatedNodes(next.allocatedNodes, next.attributeChoices);
  };
  const number = (value: number) => value.toLocaleString('en-US', { maximumFractionDigits: 1 });
  const delta = (value: number, baseline: number) => baseline > 0
    ? `${value >= baseline ? '+' : ''}${((value / baseline - 1) * 100).toFixed(1)}%`
    : `${value >= baseline ? '+' : ''}${number(value - baseline)}`;
  const currentStats = evaluated?.baseline ?? {};

  return (
    <div className="tree-planner" ref={containerRef} id="passive-upgrades">
      <button className="tree-planner-toggle" aria-expanded={expanded} onClick={() => { if (expanded) cancel(); setExpanded(!expanded); }}>
        <span aria-hidden>{expanded ? '▾' : '▸'}</span>{text.title}
      </button>
      {expanded && <div className="tree-planner-body">
        <p className="tree-planner-hint">{text.hint}</p>
        <WeaponSetControl session={session} lang={lang} />
        <div className="tree-planner-controls">
          <label>{text.mode}<AppSelect value={mode} ariaLabel={text.mode} onChange={value => setMode(value as typeof mode)} options={[
            { value: 'allocate', label: text.allocate }, { value: 'reallocate', label: text.reallocate },
          ]} /></label>
          <label>{mode === 'allocate' ? text.points : text.refundPoints}<input type="number" min={1} max={PASSIVE_POINT_LIMIT} value={points}
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
        <button className="tree-planner-run" onClick={run} disabled={session.busy || Boolean(session.error) || progress !== null || !session.character
          || context.root === null || (mode === 'reallocate' && !context.canRefund)}>{text.run}</button>
        {tracking && !evaluated && !progress && !error && !session.error && context.root !== null
          && (mode === 'allocate' || context.canRefund) && <p className="tree-planner-hint" role="status">{text.waiting}</p>}
        {progress && <OptimizerProgress {...progress} lang={lang} onCancel={cancel} />}
        {error && <p className="opt-error">{error}</p>}
        {evaluated && <>
          <div className="tree-planner-summary"><span>{text.evaluated}: {evaluated.evaluated}</span>
            {evaluated.evaluated > 0 && <span>{text.baseline}: DPS {number(currentStats.TotalDPS ?? 0)} · EHP {number(currentStats.TotalEHP ?? 0)}</span>}
            {preview !== null && <button onClick={() => show(null)}>{text.clear}</button>}
          </div>
          {evaluated.limited && <p className="tree-planner-hint">{text.limited}</p>}
          {evaluated.unmodeled > 0 && <p className="tree-planner-hint">{text.skipped}: {evaluated.unmodeled}</p>}
          {evaluated.plans.length === 0 && <p className="tree-planner-hint">{text.noResults}</p>}
          <ol className="tree-planner-results">
            {evaluated.plans.map((plan, index) => <li key={index} className={preview === index ? 'is-previewed' : ''}>
              <div className="tree-plan-heading"><strong>{index + 1}. {nodeLabel(plan.target)}</strong>
                <span>{text.cost} {plan.allocate.length - plan.deallocate.length} · {text.add} {plan.allocate.length}{plan.deallocate.length > 0 ? ` · ${text.remove} ${plan.deallocate.length}` : ''}</span></div>
              <div className="tree-plan-metrics">
                <span>{text.score} <b>{delta(scoreOf(plan.stats, completed!.objective), scoreOf(currentStats, completed!.objective))}</b></span>
                {(['TotalDPS', 'TotalEHP', 'Life'] as const).map(stat => <span key={stat}>{stat === 'TotalDPS' ? 'DPS' : stat === 'TotalEHP' ? 'EHP' : tt('opt.objLife')} <b className={(plan.stats[stat] ?? 0) >= (currentStats[stat] ?? 0) ? 'is-positive' : 'is-negative'}>{delta(plan.stats[stat] ?? 0, currentStats[stat] ?? 0)}</b></span>)}
              </div>
              <details><summary>{text.route}</summary><p>{text.add}: {plan.allocate.map(nodeLabel).join(' · ')}</p>
                {plan.deallocate.length > 0 && <p>{text.remove}: {plan.deallocate.map(nodeLabel).join(' · ')}</p>}</details>
              <div className="tree-plan-actions"><button onClick={() => show(index)}>{text.preview}</button><button disabled={session.busy} onClick={() => apply(plan)}>{text.apply}</button></div>
            </li>)}
          </ol>
          <p className="tree-planner-hint">{text.bounded}</p>
        </>}
      </div>}
    </div>
  );
}
