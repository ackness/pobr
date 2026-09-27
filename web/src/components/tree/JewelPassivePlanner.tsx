import { useEffect, useMemo, useRef, useState } from 'react';
import type { AttributeChoice, CalculateBuildRequest, PassiveNode } from '../../api/types';
import type { BuildSession } from '../../hooks/useBuildSession';
import { bindT, type Lang } from '../../lib/i18n';
import { normalizeCopiedItem } from '../../lib/itemReplacement';
import {
  JEWEL_PASSIVE_CANDIDATE_LIMIT, planJewelPassiveUpgrades,
  type JewelPassiveCandidate, type JewelPassivePlan, type JewelPassivePlannerResult,
} from '../../lib/jewelPassivePlanner';
import { allocationRoutes } from '../../lib/passiveGraph';
import { passivePlanningContext, withTravelAttributes, type PassivePlan } from '../../lib/passivePlanner';
import { feasibleOf, type Objective } from '../../lib/optimize';
import { AppSelect } from '../shared/AppSelect';
import { OptimizerProgress } from '../shared/OptimizerControls';

const COPY = {
  'en-US': {
    title: 'Jewels and passive routes',
    hint: 'Paste a jewel and add the sockets to compare. Each plan combines one candidate placement with passive changes, using the goal, point budget and travel attributes above. Paths to empty sockets cost points.',
    text: 'Candidate jewel text', socket: 'Candidate socket', add: 'Add jewel placement', remove: 'Remove placement', run: 'Find jewel and passive plans',
    empty: 'Add a jewel placement to compare it with keeping your current jewels.',
    limit: 'Up to 8 placements and 512 route evaluations per placement, plus current jewels. Search is bounded; the global optimum is not guaranteed.',
    exclusive: 'Joint jewel planning requires a shared tree without weapon-set-only passives.',
    current: 'Keep current jewels', preview: 'Show joint route on tree', apply: 'Apply jewel and passive plan', clear: 'Clear joint preview',
    evaluated: 'Plans evaluated', bounded: 'Search was limited; only evaluated routes are covered.', none: 'No improving legal plan was found.',
    addNodes: 'Allocate', removeNodes: 'Refund', route: 'Complete route and jewel', diagnostics: 'Excluded or unmodeled candidates',
    invalid: 'Paste one complete jewel item.', stale: 'The build changed. Run the search again.', skipped: 'Plans with new unmodeled effects skipped',
    socketCost: 'new points to reach socket', full: 'The placement list is full.', noSockets: 'No socket within the point budget',
  },
  'zh-CN': {
    title: '珠宝与天赋联合规划',
    hint: '粘贴珠宝，添加要比较的珠宝孔。每个方案联合评估一个珠宝位置和多个天赋点，沿用上方目标、点数预算及过路属性。通往空珠宝孔的路径也计入点数。',
    text: '候选珠宝文本', socket: '候选珠宝孔', add: '添加珠宝位置', remove: '移除位置', run: '寻找珠宝与天赋方案',
    empty: '添加珠宝位置后，与保留当前珠宝的天赋方案一起比较。',
    limit: '最多比较 8 个珠宝位置，每个位置及当前珠宝分别评估最多 512 条路线。搜索范围有限，不保证全局最优。',
    exclusive: '联合珠宝规划仅支持没有武器组专属天赋的共用主树。',
    current: '保留当前珠宝', preview: '在树上查看联合路线', apply: '应用珠宝与天赋方案', clear: '清除联合预览',
    evaluated: '已重算方案', bounded: '已达搜索上限，结果仅涵盖已评估路线。', none: '未找到合法且优于当前配置的方案。',
    addNodes: '新增', removeNodes: '退还', route: '完整路线与珠宝', diagnostics: '排除原因与未建模效果',
    invalid: '请粘贴一件完整珠宝的物品文本。', stale: '角色配置已变化，请重新搜索。', skipped: '已排除含新增未建模效果的方案',
    socketCost: '到达珠宝孔所需新点数', full: '候选位置已满。', noSockets: '点数预算内没有可达珠宝孔',
  },
  'zh-TW': {
    title: '珠寶與天賦聯合規劃',
    hint: '貼上珠寶，加入要比較的珠寶孔。每個方案聯合評估一個珠寶位置與多個天賦點，沿用上方目標、點數預算及過路屬性。通往空珠寶孔的路徑也計入點數。',
    text: '候選珠寶文字', socket: '候選珠寶孔', add: '加入珠寶位置', remove: '移除位置', run: '尋找珠寶與天賦方案',
    empty: '加入珠寶位置後，與保留目前珠寶的天賦方案一起比較。',
    limit: '最多比較 8 個珠寶位置，每個位置及目前珠寶分別評估最多 512 條路線。搜尋範圍有限，不保證全域最優。',
    exclusive: '聯合珠寶規劃僅支援沒有武器組專屬天賦的共用主樹。',
    current: '保留目前珠寶', preview: '在樹上查看聯合路線', apply: '套用珠寶與天賦方案', clear: '清除聯合預覽',
    evaluated: '已重算方案', bounded: '已達搜尋上限，結果僅涵蓋已評估路線。', none: '未找到合法且優於目前配置的方案。',
    addNodes: '新增', removeNodes: '退還', route: '完整路線與珠寶', diagnostics: '排除原因與未建模效果',
    invalid: '請貼上一件完整珠寶的物品文字。', stale: '角色配置已變更，請重新搜尋。', skipped: '已排除含新增未建模效果的方案',
    socketCost: '到達珠寶孔所需新點數', full: '候選位置已滿。', noSockets: '點數預算內沒有可達珠寶孔',
  },
};

export function JewelPassivePlanner({ session, lang, nodes, nodeLabel, mode, points, attribute, objective, onPreview, onStart }: {
  session: BuildSession; lang: Lang; nodes: PassiveNode[]; nodeLabel: (id: number) => string;
  mode: 'allocate' | 'reallocate'; points: number; attribute: AttributeChoice; objective: Objective;
  onPreview: (plan: PassivePlan | null) => void; onStart: () => void;
}) {
  const text = COPY[lang];
  const tt = bindT(lang);
  const [raw, setRaw] = useState('');
  const [socket, setSocket] = useState('');
  const [candidates, setCandidates] = useState<JewelPassiveCandidate[]>([]);
  const [completed, setCompleted] = useState<{ key: string; request: CalculateBuildRequest; result: JewelPassivePlannerResult } | null>(null);
  const [progress, setProgress] = useState<{ done: number; total: number } | null>(null);
  const [error, setError] = useState('');
  const [preview, setPreview] = useState<number | null>(null);
  const controller = useRef<AbortController | null>(null);
  const sequence = useRef(0);
  const previewRef = useRef(onPreview);
  previewRef.current = onPreview;
  const request = session.currentRequest();
  const exclusive = session.weaponSwap?.exclusive_nodes.flat() ?? [];
  const ascendancy = session.treeMeta?.classes.flatMap(entry => entry.ascendancies ?? [])
    .find(entry => entry.name === session.character?.ascendancy_name)?.id ?? session.character?.ascendancy_name;
  const context = useMemo(() => passivePlanningContext(nodes, session.allocatedNodes, session.character?.class_name,
    session.weaponSwap?.exclusive_nodes.flat() ?? [], session.jewels.map(jewel => jewel.socket_node), ascendancy, session.calc?.tree_effects),
  [nodes, session.allocatedNodes, session.character?.class_name, session.weaponSwap, session.jewels, ascendancy, session.calc]);
  const sockets = useMemo(() => {
    const routes = allocationRoutes(context.graph, context.allocated, context.root, context.grants, points);
    return nodes.filter(node => node.kind === 'jewel_socket' && context.byId.has(node.skill) && routes.has(node.skill))
      .map(node => ({ node, cost: routes.get(node.skill)!.length }))
      .sort((a, b) => a.cost - b.cost || a.node.skill - b.node.skill);
  }, [context, nodes, points]);
  const selectedSocket = sockets.some(entry => String(entry.node.skill) === socket) ? socket : String(sockets[0]?.node.skill ?? '');
  const key = JSON.stringify([session.stateVersion, session.activeWeaponSet, request, candidates, mode, points, attribute, objective]);
  const latestKey = useRef(key);
  latestKey.current = key;
  const result = completed?.key === key ? completed.result : null;
  useEffect(() => {
    controller.current?.abort(); controller.current = null;
    setCompleted(null); setProgress(null); setError(''); setPreview(null);
    previewRef.current(null);
  }, [key]);
  useEffect(() => () => { controller.current?.abort(); previewRef.current(null); }, []);

  const add = () => {
    if (!selectedSocket || candidates.length >= JEWEL_PASSIVE_CANDIDATE_LIMIT) return;
    try {
      const normalized = normalizeCopiedItem(raw);
      if (candidates.some(candidate => candidate.jewel.socket_node === Number(selectedSocket) && candidate.jewel.text === normalized)) return;
      const lines = normalized.split('\n');
      const rarity = lines.findIndex(line => /^Rarity:/i.test(line));
      const label = lines[rarity + 1] || text.text;
      setCandidates(previous => [...previous, { id: String(++sequence.current), label,
        jewel: { socket_node: Number(selectedSocket), text: normalized } }]);
      setError('');
    } catch { setError(text.invalid); }
  };
  const run = async () => {
    if (!request || session.busy || exclusive.length || !candidates.length) return;
    onStart();
    controller.current?.abort();
    const abort = new AbortController(); controller.current = abort;
    setError(''); setCompleted(null); setPreview(null); onPreview(null); setProgress({ done: 0, total: 0 });
    try {
      const result = await planJewelPassiveUpgrades({ request: withTravelAttributes(request, nodes, attribute), nodes,
        className: session.character?.class_name, ascendancy, exclusiveNodes: exclusive, candidates, points, mode, objective,
        signal: abort.signal, onProgress: (done, total) => {
          if (!abort.signal.aborted && latestKey.current === key) setProgress({ done, total });
        } });
      if (!abort.signal.aborted && !result.aborted && latestKey.current === key) setCompleted({ key, request, result });
    } catch (error) {
      if (!abort.signal.aborted && latestKey.current === key) setError(error instanceof Error ? error.message : String(error));
    } finally {
      if (controller.current === abort) { controller.current = null; setProgress(null); }
    }
  };
  const apply = (plan: JewelPassivePlan) => {
    if (!completed || completed.key !== latestKey.current || session.busy || !feasibleOf(plan.stats, objective)) return;
    if (!session.applyTreePlan(completed.request, plan)) { setError(text.stale); return; }
    setCompleted(null); setPreview(null); onPreview(null);
  };
  const format = (value: number) => value.toLocaleString(lang, { maximumFractionDigits: 2 });
  const delta = (value: number, baseline: number) => baseline === 0 ? `${value >= 0 ? '+' : ''}${format(value)}`
    : `${value >= baseline ? '+' : ''}${format((value / baseline - 1) * 100)}%`;

  return <section className="jewel-passive-planner" aria-label={text.title}>
    <h4>{text.title}</h4><p className="tree-planner-hint">{text.hint}</p>
    <label className="jewel-plan-input">{text.text}<textarea aria-label={text.text} value={raw} onChange={event => setRaw(event.target.value)} rows={5} /></label>
    <div className="tree-planner-controls"><label>{text.socket}<AppSelect value={selectedSocket} ariaLabel={text.socket}
      onChange={setSocket} disabled={!sockets.length} placeholder={text.noSockets} options={sockets.map(({ node, cost }) => ({ value: String(node.skill),
        label: `${nodeLabel(node.skill)} #${node.skill}`, hint: `${text.socketCost}: ${cost}` }))} /></label>
      <button disabled={!raw.trim() || !selectedSocket || Boolean(progress) || candidates.length >= JEWEL_PASSIVE_CANDIDATE_LIMIT} onClick={add}>{text.add}</button>
      <span>{candidates.length} / {JEWEL_PASSIVE_CANDIDATE_LIMIT}</span></div>
    {candidates.length >= JEWEL_PASSIVE_CANDIDATE_LIMIT && <p className="tree-planner-hint">{text.full}</p>}
    {!candidates.length && <p className="tree-planner-hint">{text.empty}</p>}
    <ul className="jewel-plan-candidates">{candidates.map(candidate => <li key={candidate.id}>
      <details><summary>{candidate.label} · {nodeLabel(candidate.jewel.socket_node)} #{candidate.jewel.socket_node}</summary><pre>{candidate.jewel.text}</pre></details>
      <button disabled={Boolean(progress)} onClick={() => setCandidates(values => values.filter(value => value.id !== candidate.id))}>{text.remove}</button>
    </li>)}</ul>
    <p className="tree-planner-hint">{text.limit}</p>
    {exclusive.length > 0 && <p className="tree-planner-hint">{text.exclusive}</p>}
    <button className="tree-planner-run" disabled={!candidates.length || session.busy || Boolean(session.error) || Boolean(progress) || exclusive.length > 0}
      onClick={() => void run()}>{text.run}</button>
    {progress && <OptimizerProgress {...progress} lang={lang} onCancel={() => {
      controller.current?.abort(); controller.current = null; setProgress(null);
    }} />}
    {error && <p className="opt-error" role="alert">{error}</p>}
    {result && <div className="jewel-plan-results" aria-live="polite">
      <p className="tree-planner-hint">{text.evaluated}: {result.evaluated}</p>
      {result.limited && <p className="tree-planner-hint">{text.bounded}</p>}
      {result.unmodeled > 0 && <p className="tree-planner-hint">{text.skipped}: {result.unmodeled}</p>}
      {result.diagnostics.length > 0 && <details><summary>{text.diagnostics}</summary><ul>{result.diagnostics.map((reason, index) => <li key={index}>{reason}</li>)}</ul></details>}
      {!result.plans.length && <p>{text.none}</p>}
      {preview !== null && <button onClick={() => { setPreview(null); onPreview(null); }}>{text.clear}</button>}
      <ol className="tree-planner-results">{result.plans.map((plan, index) => <li key={index} className={preview === index ? 'is-previewed' : ''}>
        <div className="tree-plan-heading"><strong>{plan.candidateId === null ? text.current : plan.candidateLabel}</strong>
          <span>{text.addNodes} {plan.allocate.length} · {text.removeNodes} {plan.deallocate.length}</span></div>
        <div className="tree-plan-metrics">{(['TotalDPS', 'TotalEHP', 'Life'] as const).map(stat => <span key={stat}>
          {stat === 'TotalDPS' ? 'DPS' : stat === 'TotalEHP' ? 'EHP' : tt('opt.objLife')} <b>{delta(plan.stats[stat] ?? 0, result.baseline[stat] ?? 0)}</b>
          <small>{format(result.baseline[stat] ?? 0)} → <output data-stat={stat}>{format(plan.stats[stat] ?? 0)}</output></small>
        </span>)}</div>
        <details><summary>{text.route}</summary><p>{text.addNodes}: {plan.allocate.map(nodeLabel).join(' · ') || '—'}</p>
          <p>{text.removeNodes}: {plan.deallocate.map(nodeLabel).join(' · ') || '—'}</p>
          {plan.candidateId !== null && <pre>{candidates.find(candidate => candidate.id === plan.candidateId)?.jewel.text}</pre>}</details>
        <div className="tree-plan-actions"><button onClick={() => { setPreview(index); onPreview(plan); }}>{text.preview}</button>
          <button disabled={session.busy || !feasibleOf(plan.stats, objective)} onClick={() => apply(plan)}>{text.apply}</button></div>
      </li>)}</ol>
    </div>}
  </section>;
}
