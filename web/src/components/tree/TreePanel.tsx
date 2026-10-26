import { localizedName } from '../../lib/chinese';
import { useAppDialogs } from '../shared/AppDialogs';
import { useSessionAction } from '../shared/useSessionAction';
import { AppTooltip, AppButton, AppInput, AppTextarea, AppSelectControl } from '../shared/controls';
import { TreeConversionControls } from './TreeConversionControls';
import { PageHeader } from '../shared/PageHeader';
import { formatApiError } from '../../api/error';
import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react';
import { getBackend } from '../../api/backend';
import type { AttributeChoice, PassiveNode, TreeArt, TreeAllocationMode } from '../../api/types';
import { useTreeEditPreview } from '../../hooks/useTreeEditPreview';
import { treeAllocationRequest } from '../../lib/weaponSets';
import type { BuildSession } from '../../hooks/useBuildSession';
import { useItemDisplayNames } from '../../hooks/useLocalizedLines';
import { bindT, statNameLabel, type Lang } from '../../lib/i18n';
import {
  allocationAccess,
  buildPassiveGraph,
  classStartSkill,
} from '../../lib/passiveGraph';
import { NODE_RADIUS, clampView, isAttributeNode as isAttrNode, passiveTreeEdges, treeExtent, type ViewBox } from '../../lib/treeGeometry';
import { previewDiff, type DiffEntry } from '../../lib/compare';
import { DiffList } from '../shared/DiffList';
import { AppSelect } from '../shared/AppSelect';
import { NoteEditor } from '../shared/NoteEditor';
import { TreeOptimizer, useTreeOptimizer, type TreeOptimizerState } from './TreeOptimizer';
import type { PassivePlan } from '../../lib/passivePlanner';
import './tree.css';
import { NodePowerTable } from './NodePowerTable';

interface Props {
  session: BuildSession;
  lang: Lang;
  active: boolean;
  focusPlanner?: { nonce: number };
  onJewelSearch?: (socket: number) => void;
}

/** 节点内技能图标半径 = 圆半径 × 此系数。 */
const ICON_SCALE = 1.05;
/** 精通徽记是自带外框的整盘，铺满节点。 */
const MASTERY_ICON_SCALE = 1.5;
/** 外框半径 = 圆半径 × 此系数（外框美术含透明留白，放大到可见环对齐圆点；逐类型微调）。 */
const FRAME_SCALE: Record<string, number> = {
  normal: 1.95,
  notable: 1.55,
  keystone: 1.45,
  mastery: 1.6,
  jewel_socket: 1.7,
  ascendancy_start: 1.6,
};

const JEWEL_TEMPLATE = 'Rarity: RARE\nMy Jewel\nEmerald\n+50 to maximum Life';

/** 天赋树查看器：SVG 渲染 + 已加点高亮 + 缩放平移 / hover 词条 + 点选加点重算。 */
export function TreePanel(props: Props) {
  const { session, lang, active } = props;
  const currentTree = session.dataVersion && /^\d+\.\d+/.test(session.dataVersion)
    ? `0_${session.dataVersion.split('.')[1]}` : null;
  if (session.treeVersion && session.treeVersion !== 'Default' && currentTree && session.treeVersion !== currentTree) {
    if (!active) return null;
    const text = lang === 'en-US'
      ? `This build uses passive tree ${session.treeVersion}. Calculation and export preserve that version. Interactive tree editing and route planning are unavailable for historical trees.`
      : lang === 'zh-TW'
        ? `此角色使用 ${session.treeVersion} 天賦樹。計算與匯出會保留該版本，歷史天賦樹暫不支援互動編輯和路線規劃。`
        : `此角色使用 ${session.treeVersion} 天赋树。计算与导出会保留该版本，历史天赋树暂不支持交互编辑和路线规划。`;
    return <><TreeConversionControls session={session} lang={lang} /><section className="ui-page"><p role="status">{text}</p></section></>;
  }
  return <>{active && <TreeConversionControls session={session} lang={lang} />}<EditableTreePanel key={JSON.stringify([session.dataVersion, session.treeVersion])} {...props} /></>;
}

/** Retain planner inputs and complete receipts, never the interactive canvas. */
function EditableTreePanel({ session, lang, active, focusPlanner, onJewelSearch }: Props) {
  const [allocationMode, setAllocationMode] = useState<TreeAllocationMode>(0);
  const mode = allocationMode === 0 ? 0 : session.activeWeaponSet;
  const changeMode = (next: TreeAllocationMode) => {
    setAllocationMode(next);
    if (next !== 0) session.setWeaponSet(next);
  };
  const [rawNodes, setNodes] = useState<PassiveNode[] | null>(null);
  const treeEffects = session.calc?.tree_effects;
  const nodes = useMemo(() => rawNodes?.map(node => {
    const change = treeEffects?.nodes[String(node.skill)];
    return change ? { ...node, name: change.name,
      stats: change.replace ? change.stats : [...(node.stats ?? []), ...change.stats] } : node;
  }) ?? [], [rawNodes, treeEffects]);
  const [art, setArt] = useState<TreeArt | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [plannedNodes, setPlannedNodes] = useState<PassivePlan | null>(null);
  const byId = useMemo(() => new Map(nodes.map(node => [node.skill, node])), [nodes]);
  const planner = useTreeOptimizer({ session, lang, active, nodes,
    nodeLabel: skill => byId.get(skill)?.name ?? `#${skill}`,
    onPreview: setPlannedNodes, focusPlanner });
  useEffect(() => {
    let cancelled = false;
    getBackend()
      .then(b => Promise.all([b.loadPassiveTree(), b.loadTreeArt()]))
      .then(([loadedNodes, loadedArt]) => {
        if (cancelled) return;
        setNodes(loadedNodes); setArt(loadedArt);
      })
      .catch(err => { if (!cancelled) setError(formatApiError(err)); });
    return () => { cancelled = true; };
  }, []);
  if (!active) return null;
  const activeBuild = session.workspace?.builds.find(build => build.id === session.workspace?.activeBuild);
  const canvasContext = JSON.stringify([activeBuild?.id, activeBuild?.activeStage,
    session.activeLoadout, session.activeWeaponSet, session.socketGroupRevision,
    session.character?.class_name, session.character?.ascendancy_name]);
  return <TreeCanvas key={canvasContext} session={session} lang={lang} onJewelSearch={onJewelSearch}
    allocationMode={mode} onAllocationMode={changeMode} nodes={rawNodes ? nodes : null} art={art} error={error} plannedNodes={plannedNodes} planner={planner} />;
}

interface CanvasProps {
  allocationMode: TreeAllocationMode;
  onAllocationMode: (mode: TreeAllocationMode) => void;
  session: BuildSession;
  lang: Lang;
  onJewelSearch?: (socket: number) => void;
  nodes: PassiveNode[] | null;
  art: TreeArt | null;
  error: string | null;
  plannedNodes: PassivePlan | null;
  planner: TreeOptimizerState;
}

function TreeCanvas({ allocationMode, onAllocationMode, session, lang, onJewelSearch, nodes, art, error, plannedNodes, planner }: CanvasProps) {
  const tt = bindT(lang);
  const dialogs = useAppDialogs(lang);
  const captureAction = useSessionAction(session);
  const treeEffects = session.calc?.tree_effects;
  const grants = useMemo(() => treeEffects?.allocation_grants ?? [], [treeEffects]);
  const canvasGeneration = useRef(0);
  useLayoutEffect(() => () => { canvasGeneration.current += 1; }, []);
  const [hover, setHover] = useState<PassiveNode | null>(null);
  const [hoverPos, setHoverPos] = useState<{ x: number; y: number }>({ x: 0, y: 0 });
  const [hoverStats, setHoverStats] = useState<string[] | null>(null);
  /** hover 节点名的中文翻译（词典命中才有；null = 显示英文原名）。 */
  const [hoverName, setHoverName] = useState<string | null>(null);
  /** 节点搜索串（名称/词条子串匹配，高亮命中节点）。 */
  const [search, setSearch] = useState('');
  /** 「下一个命中」轮转下标（搜索串变化时归零）。 */
  const [hitIndex, setHitIndex] = useState(0);
  const searchRef = useRef<HTMLInputElement>(null);
  const nextHitRef = useRef<HTMLButtonElement>(null);
  const detailsRef = useRef<HTMLDivElement>(null);
  const focusDetailsRef = useRef(false);
  const [selectedSkill, setSelectedSkill] = useState<number | null>(null);
  const [attributeDraft, setAttributeDraft] = useState<{
    skill: number; saved?: AttributeChoice; choice: AttributeChoice;
  } | null>(null);
  /** 属性小点三选一弹窗（node + 屏幕坐标）。 */
  const [attrPicker, setAttrPicker] = useState<{ node: PassiveNode; x: number; y: number } | null>(
    null,
  );
  /** 珠宝插槽编辑器（正在编辑的插槽节点 id + 草稿）。 */
  const toolsRef = useRef<HTMLDivElement>(null);
  const jewelRef = useRef<HTMLDivElement>(null);
  const [jewelSocket, setJewelSocket] = useState<number | null>(() => {
    const key = Object.keys(session.editorDrafts).find(key => key.startsWith('jewel:'));
    return key ? Number(key.slice(6)) : null;
  });
  const jewelDraftKey = `jewel:${jewelSocket}`;
  const jewelEdit = jewelSocket === null ? null : { socket: jewelSocket,
    draft: session.editorDrafts[jewelDraftKey]
      ?? session.jewels.find(jewel => jewel.socket_node === jewelSocket)?.text ?? JEWEL_TEMPLATE };
  useEffect(() => {
    if (jewelSocket === null) return;
    jewelRef.current?.focus({ preventScroll: true });
    jewelRef.current?.scrollIntoView({ block: 'nearest' });
  }, [jewelSocket]);
  const closeJewelEditor = () => {
    session.setEditorDraft(jewelDraftKey, null);
    setJewelSocket(null);
  };
  /** hover 节点的加点/取消收益（防抖重算；按 stateVersion 失效的缓存）。 */
  const [hoverDiff, setHoverDiff] = useState<DiffEntry[] | null>(null);
  const [viewBox, setViewBox] = useState<ViewBox | null>(null);
  const dragRef = useRef<{
    pointerId: number;
    startX: number;
    startY: number;
    origin: DOMPoint;
    inverse: DOMMatrix;
    view: ViewBox;
    next: ViewBox;
    moved: boolean;
  } | null>(null);
  const svgRef = useRef<SVGSVGElement | null>(null);
  const sceneRef = useRef<SVGGElement | null>(null);
  const dragRafRef = useRef<number | null>(null);
  const suppressClickRef = useRef(false);
  const clickResetRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  /** 滚轮缩放的 rAF 合帧暂存（一帧最多提交一次 viewBox）。 */
  const wheelPendingRef = useRef<ViewBox | null>(null);
  const wheelRafRef = useRef<number | null>(null);
  /** Clear the scene offset in the same commit as the final viewBox. */
  const transformResetRef = useRef(false);
  useLayoutEffect(() => {
    if (!transformResetRef.current) return;
    transformResetRef.current = false;
    const svg = svgRef.current;
    if (svg) {
      sceneRef.current?.removeAttribute('transform');
      svg.classList.remove('is-dragging');
    }
  }, [viewBox]);

  useLayoutEffect(() => () => {
    if (wheelRafRef.current !== null) cancelAnimationFrame(wheelRafRef.current);
    if (dragRafRef.current !== null) cancelAnimationFrame(dragRafRef.current);
    if (clickResetRef.current !== null) clearTimeout(clickResetRef.current);
    dragRef.current = null;
    wheelPendingRef.current = null;
  }, []);

  useLayoutEffect(() => {
    const svg = svgRef.current;
    if (!svg) return;
    // React's delegated wheel listener is passive. Consume scrolling only over
    // the tree, so zooming does not also move the surrounding page.
    const consume = (event: WheelEvent) => event.preventDefault();
    svg.addEventListener('wheel', consume, { passive: false });
    return () => svg.removeEventListener('wheel', consume);
  }, [nodes]);

  const allocated = useMemo(() => new Set(session.allocatedNodes), [session.allocatedNodes]);

  // 珠宝库（插槽编辑器的切换下拉）；显示名「名称·基底」批量送翻译。
  // ⚠️hook 必须在下方 early return 之前（React #310）。
  const jewelLib = session.library.items.filter((i) => i.kind === 'jewel');
  const jewelNames = useItemDisplayNames(
    jewelLib.map((e) => e.text),
    lang,
  );

  // 热力图（PoB2 节点威力）：选定属性 + 深度，**点「计算」按钮才跑**——
  // 计算量大（深度内逐词条组合完整重算），不随加点/编辑自动刷新。
  const [heatStat, setHeatStat] = useState('');
  const [heatDepth, setHeatDepth] = useState(5);
  const [heatData, setHeatData] = useState<Map<number, number> | null>(null);
  const [heatBusy, setHeatBusy] = useState(false);
  /** 上次计算时的 stateVersion（build 变动后提示「已过期」而非自动重算）。 */
  const [heatVersion, setHeatVersion] = useState(-1);
  const runHeat = () => {
    if (!heatStat || heatBusy) return;
    const request = session.currentRequest();
    if (!request) return;
    const version = session.stateVersion;
    const generation = canvasGeneration.current;
    setHeatBusy(true);
    getBackend()
      .then(b => generation === canvasGeneration.current ? b.nodePower(request, heatStat, heatDepth) : null)
      .then((res) => {
        if (!res || generation !== canvasGeneration.current) return;
        setHeatData(new Map(res.entries.map((e) => [e.skill, e.delta])));
        setHeatVersion(version);
      })
      .catch(() => { if (generation === canvasGeneration.current) setHeatData(null); })
      .finally(() => { if (generation === canvasGeneration.current) setHeatBusy(false); });
  };

  const heatMax = useMemo(() => {
    let max = 0;
    heatData?.forEach((d) => {
      if (d > max) max = d;
    });
    return max;
  }, [heatData]);

  /** 热力图可选属性（按当前计算结果里实际存在的展示量过滤）。 */
  const heatStatOptions = useMemo(() => {
    const CANDIDATES = [
      'TotalDPS', 'Life', 'EnergyShield', 'TotalEHP', 'Mana',
      'Armour', 'Evasion', 'Spirit',
    ];
    const present = new Set((session.calc?.stats ?? []).filter(s => Number.isFinite(s.value)).map((s) => s.id));
    return CANDIDATES.filter((id) => present.has(id));
  }, [session.calc]);

  // 当前升华的稳定 id（如 `Warrior3`）——PoB2 语义：只渲染所选升华的节点簇，
  // 其它升华整簇隐藏（它们与主树平面重叠，全显示会一团乱）。
  const currentAscId = useMemo(() => {
    const name = session.character?.ascendancy_name;
    if (!name) return null;
    for (const cls of session.treeMeta?.classes ?? []) {
      const hit = (cls.ascendancies ?? []).find((a) => a.name === name);
      if (hit) return hit.id;
    }
    return null;
  }, [session.character?.ascendancy_name, session.treeMeta]);

  const placed = useMemo(
    () =>
      (nodes ?? []).filter(
        (n) =>
          n.x !== undefined &&
          n.y !== undefined &&
          (!n.ascendancy_id || n.ascendancy_id === currentAscId),
      ),
    [nodes, currentAscId],
  );

  const byId = useMemo(() => new Map(placed.map((n) => [n.skill, n])), [placed]);

  /**
   * 寻路图与职业起点。用 `nodes` 全集而非 `placed`——后者按当前飞升过滤，拿它建图
   * 会让路径在飞升边界断裂；`buildPassiveGraph` 内部已跳过跨飞升的伪边。
   */
  const startSkill = useMemo(
    () => treeEffects?.class_starts[session.character?.class_name.toLowerCase() ?? ''] ?? classStartSkill(nodes ?? [], session.character?.class_name),
    [nodes, session.character?.class_name, treeEffects],
  );

  const graph = useMemo(() => {
    const starts = new Set(Object.values(treeEffects?.class_starts ?? {}));
    const allowed = new Set([startSkill, ...grants.flatMap(grant => grant.roots)]);
    return buildPassiveGraph((nodes ?? []).filter(node => !starts.has(node.skill) || allowed.has(node.skill)));
  }, [nodes, treeEffects, startSkill, grants]);
  const freeNodes = useMemo(() => allocationAccess(graph, allocated, startSkill, grants).free,
    [graph, allocated, startSkill, grants]);
  /** 搜索命中集（名称 + 词条文本，剥 `[a|b]` 标记后不分大小写子串匹配）。 */
  const searchHits = useMemo(() => {
    const q = search.trim().toLowerCase();
    if (q.length < 2) return null;
    const hits = new Set<number>();
    for (const node of placed) {
      const haystack = [node.name ?? '', ...(node.stats ?? [])]
        .join('\n')
        .replace(/\[([^\]|]*)\|([^\]]*)\]/g, '$2')
        .replace(/\[([^\]]*)\]/g, '$1')
        .toLowerCase();
      if (haystack.includes(q)) hits.add(node.skill);
    }
    return hits;
  }, [placed, search]);
  const selectedNode = selectedSkill === null ? null : byId.get(selectedSkill) ?? null;
  const selectedAllocated = selectedSkill !== null && allocated.has(selectedSkill);
  const editRequest = session.currentRequest();
  const selectedPreview = useTreeEditPreview(editRequest, selectedSkill, allocationMode,
    selectedAllocated ? 'refund' : 'allocate');
  const hoverPreview = useTreeEditPreview(editRequest, hover?.skill ?? null, allocationMode,
    hover && allocated.has(hover.skill) ? 'refund' : 'allocate');
  const selectedPath = selectedPreview?.result?.added ?? [];
  const preview = hover ? hoverPreview : selectedPreview;
  const previewResult = preview?.result;
  const previewNodes = useMemo(() => ({
    added: new Set(previewResult?.added), removed: new Set(previewResult?.removed),
    promoted: new Set(previewResult?.promoted), path: new Set(previewResult?.path),
  }), [previewResult]);
  const owner = useMemo(() => new Map((session.weaponSwap?.exclusive_nodes ?? [[], []])
    .flatMap((ids, index) => ids.map(id => [id, index + 1] as const))), [session.weaponSwap]);
  const [editError, setEditError] = useState<string | null>(null);
  const editPending = useRef(false);
  const modeRef = useRef(allocationMode);
  modeRef.current = allocationMode;
  const selectedNeedsAttribute = Boolean(selectedNode && !selectedAllocated
    && isAttrNode(selectedNode) && selectedPath.length === 1);
  const selectedChoice = selectedSkill === null ? undefined : session.attributeChoices[String(selectedSkill)];
  const pendingAttribute = attributeDraft && attributeDraft.skill === selectedSkill && attributeDraft.saved === selectedChoice
    ? attributeDraft.choice : undefined;
  const selectedRefundCount = selectedPreview?.result?.removed.length ?? 0;
  const detailsLines = useMemo(() => selectedNode ? [
    selectedNode.name ?? selectedNode.id,
    ...(selectedNode.stats ?? []).map(line =>
      line.replace(/\[([^\]|]*)\|([^\]]*)\]/g, '$2').replace(/\[([^\]]*)\]/g, '$1')),
  ] : [], [selectedNode]);
  const detailsTextKey = JSON.stringify([selectedSkill, lang, detailsLines]);
  const [translatedDetails, setTranslatedDetails] = useState<{ key: string; lines: string[] } | null>(null);
  const displayedDetails = lang !== 'en-US' && translatedDetails?.key === detailsTextKey
    ? translatedDetails.lines : detailsLines;

  useEffect(() => {
    if (!selectedNode || lang === 'en-US') return;
    let cancelled = false;
    const generation = canvasGeneration.current;
    getBackend()
      .then(backend => !cancelled && generation === canvasGeneration.current
        ? backend.translateLines(detailsLines, lang) : null)
      .then(translated => {
        if (!translated || cancelled || generation !== canvasGeneration.current) return;
        setTranslatedDetails({ key: detailsTextKey,
          lines: detailsLines.map((line, index) => translated[index] ?? line) });
      })
      .catch(() => {});
    return () => { cancelled = true; };
  }, [selectedNode, lang, detailsLines, detailsTextKey]);

  useLayoutEffect(() => {
    if (!focusDetailsRef.current || !selectedNode) return;
    focusDetailsRef.current = false;
    detailsRef.current?.focus({ preventScroll: true });
    detailsRef.current?.scrollIntoView({ block: 'nearest' });
  }, [selectedNode, hitIndex]);

  const closeDetails = () => {
    setSelectedSkill(null);
    setAttributeDraft(null);
    setHover(null);
    focusDetailsRef.current = false;
    (nextHitRef.current ?? searchRef.current)?.focus({ preventScroll: true });
  };

  /** 已加点的属性小点（升序，批量调配的确定性分配序）。 */
  const allocatedAttrNodes = useMemo(
    () =>
      session.allocatedNodes
        .filter((skill) => {
          const node = byId.get(skill);
          return node ? isAttrNode(node) : false;
        })
        .sort((a, b) => a - b),
    [session.allocatedNodes, byId],
  );

  const attrCounts = useMemo(() => {
    const counts = { str: 0, dex: 0, int: 0 };
    for (const skill of allocatedAttrNodes) {
      const choice = session.attributeChoices[String(skill)];
      if (choice) counts[choice] += 1;
    }
    return counts;
  }, [allocatedAttrNodes, session.attributeChoices]);

  /** 批量调配：前 str 个给力量、其次 dex、再 int，其余不分配（引擎语义=无贡献）。 */
  const distributeAttributes = (str: number, dex: number, int: number) => {
    const next: Record<string, AttributeChoice> = {};
    // 保留非属性小点位（理论上没有，防御性）+ 重建属性小点位。
    for (const [k, v] of Object.entries(session.attributeChoices)) {
      if (!allocatedAttrNodes.includes(Number(k))) next[k] = v;
    }
    allocatedAttrNodes.forEach((skill, i) => {
      if (i < str) next[String(skill)] = 'str';
      else if (i < str + dex) next[String(skill)] = 'dex';
      else if (i < str + dex + int) next[String(skill)] = 'int';
    });
    session.setAttributeChoices(next);
  };

  const edges = useMemo(() => passiveTreeEdges(placed).map(edge => ({ ...edge,
    active: allocated.has(edge.from) && allocated.has(edge.to),
  })), [placed, allocated]);

  // ── 渲染性能：节点/连线子树 memo 化 ─────────────────────────────────────
  // 拖拽/缩放/悬停每帧都 setState；若子树不 memo，React 每帧要 diff 近万个
  // SVG 元素（拖动卡顿的根因）。memo 后这些帧只更新 svg viewBox 与 tooltip。
  // 点击回调经 sessionRef 取最新会话，避免 memo 闭包捕获过期 state。
  const sessionRef = useRef(session);
  sessionRef.current = session;

  /** Every manual allocation uses the same authoritative path and budget check. */
  const actOnNode = useCallback(async (skill: number,
    action: 'allocate' | 'refund' | 'promote' | 'attribute' | 'jewel', choice?: AttributeChoice) => {
    const s = sessionRef.current;
    const node = byId.get(skill);
    if (s.busy || editPending.current || !node) return;
    const isAlloc = s.allocatedNodes.includes(skill);
    if (action === 'attribute' && (!isAttrNode(node) || !choice)) return;
    if (action === 'attribute' && isAlloc) {
      s.setAttributeChoices({ ...s.attributeChoices, [String(skill)]: choice! });
      setAttributeDraft(null);
      return;
    }
    if (action === 'jewel' && node.kind !== 'jewel_socket') return;
    if (action === 'jewel' && isAlloc) { setJewelSocket(skill); return; }
    const request = s.currentRequest();
    if (!request) return;
    const generation = canvasGeneration.current;
    editPending.current = true;
    setEditError(null);
    try {
      const backend = await getBackend();
      const response = await backend.previewTreeEdit(request, skill, allocationMode,
        action === 'refund' || action === 'promote' ? action : 'allocate');
      if (generation !== canvasGeneration.current || modeRef.current !== allocationMode) return;
      if (!response.valid || !response.result) { setEditError(response.reason); return; }
      if (action !== 'refund' && response.result.added.length === 1 && isAttrNode(node) && !choice) return 'chooseAttribute';
      if (!sessionRef.current.applyTreeEdit(response.result, request, choice ? { [String(skill)]: choice } : undefined)) {
        setEditError('stale_edit'); return;
      }
      if (jewelSocket !== null && response.result.removed.includes(jewelSocket)) {
        s.setEditorDraft(`jewel:${jewelSocket}`, null); setJewelSocket(null);
      }
      setAttrPicker(null);
      setAttributeDraft(null);
      if (action !== 'refund' && node.kind === 'jewel_socket') setJewelSocket(skill);
    } catch { setEditError('backend_unavailable'); }
    finally { editPending.current = false; }
  }, [byId, allocationMode, jewelSocket]);

  const handleNodeClick = useCallback((node: PassiveNode, e: React.MouseEvent) => {
    if (suppressClickRef.current || dragRef.current?.moved) return;
    const action = node.kind === 'jewel_socket' ? 'jewel'
      : sessionRef.current.allocatedNodes.includes(node.skill) ? 'refund' : 'allocate';
    const rect = svgRef.current?.getBoundingClientRect();
    const pos = rect ? { x: e.clientX - rect.left, y: e.clientY - rect.top } : null;
    void actOnNode(node.skill, action).then(result => {
      if (result === 'chooseAttribute' && pos) setAttrPicker({ node, ...pos });
    });
  }, [actOnNode]);

  const edgesEl = useMemo(
    () => (
      <g className="tree-edges">
        {edges.map((e, i) => (
          <line
            key={i}
            x1={e.x1}
            y1={e.y1}
            x2={e.x2}
            y2={e.y2}
            className={`${e.active ? 'edge-active' : 'edge'}${previewNodes.path.has(e.from) && previewNodes.path.has(e.to) ? ' edge-preview' : ''}${previewNodes.removed.has(e.from) && previewNodes.removed.has(e.to) ? ' edge-refund' : ''}`}
          />
        ))}
      </g>
    ),
    [edges, previewNodes],
  );

  const filledJewelSockets = useMemo(
    () => new Set(session.jewels.map((j) => j.socket_node)),
    [session.jewels],
  );

  const nodesEl = useMemo(() => {
    /** 热力上色：正增量 暗红→亮金（√ 拉开低值区分度）；负增量冷蓝；无数据不覆盖。 */
    const heatFill = (skill: number): string | undefined => {
      if (!heatStat || !heatData || allocated.has(skill)) return undefined;
      const delta = heatData.get(skill);
      if (delta === undefined || delta === 0 || heatMax <= 0) return undefined;
      if (delta < 0) return 'hsl(210 55% 32%)';
      const t = Math.sqrt(Math.min(1, delta / heatMax));
      return `hsl(${Math.round(20 + t * 25)} ${Math.round(70 + t * 30)}% ${Math.round(28 + t * 32)}%)`;
    };
    return (
      <g className={`tree-nodes${art ? ' has-art' : ''}`}>
        {placed.map((node) => {
          const r = NODE_RADIUS[node.kind] ?? 40;
          const isAlloc = allocated.has(node.skill);
          const isMastery = node.kind === 'mastery';
          // 精通节点用通用徽记（自带外框，不叠外框、不压暗）；其余用各自技能图标。
          const icon = isMastery
            ? art?.masteryIcon
            : art?.nodeIcons[String(node.skill)];
          const frameSet = isMastery ? undefined : art?.frames[node.kind];
          const frame = frameSet
            ? isAlloc
              ? frameSet.alloc ?? frameSet.unalloc
              : frameSet.unalloc ?? frameSet.alloc
            : undefined;
          const iconR = r * (isMastery ? MASTERY_ICON_SCALE : ICON_SCALE);
          const frameR = r * (FRAME_SCALE[node.kind] ?? 1.7);
          const heat = heatFill(node.skill);
          return (
            <g key={node.skill}>
              {icon && (
                <image
                  href={icon}
                  x={node.x! - iconR}
                  y={node.y! - iconR}
                  width={iconR * 2}
                  height={iconR * 2}
                  clipPath="url(#tree-icon-clip)"
                  className={`node-icon${!isMastery && !isAlloc ? ' node-icon-dim' : ''}`}
                />
              )}
              {frame && (
                <image
                  href={frame}
                  x={node.x! - frameR}
                  y={node.y! - frameR}
                  width={frameR * 2}
                  height={frameR * 2}
                  className="node-frame"
                />
              )}
              <circle
                data-skill-id={node.skill}
                data-weapon-set={owner.get(node.skill) ?? (isAlloc ? 0 : undefined)}
                cx={node.x}
                cy={node.y}
                r={r}
                style={heat ? { fill: heat } : undefined}
                className={`node node-${node.kind}${icon || frame ? ' node-art' : ''}${node.ascendancy_id ? ' node-asc' : ''}${isAlloc ? ' node-allocated' : ''}${node.kind === 'jewel_socket' && filledJewelSockets.has(node.skill) ? ' node-jewel-filled' : ''}${searchHits?.has(node.skill) ? ' node-search-hit' : ''}${selectedSkill === node.skill ? ' node-selected' : ''}${freeNodes.has(node.skill) ? ' node-jewel-allocatable' : ''}${treeEffects?.nodes[String(node.skill)] ? ' node-transformed' : ''}${plannedNodes?.allocate.includes(node.skill) ? ' node-planned-add' : ''}${plannedNodes?.deallocate.includes(node.skill) ? ' node-planned-remove' : ''}${owner.has(node.skill) ? ` node-weapon-${owner.get(node.skill)}` : ''}${previewNodes.added.has(node.skill) ? ' node-preview-add' : ''}${previewNodes.promoted.has(node.skill) ? ' node-preview-promote' : ''}${previewNodes.removed.has(node.skill) ? ' node-preview-refund' : ''}`}
                onPointerEnter={(e) => {
                  if (dragRef.current?.moved) return;
                  setHover(node);
                  setHoverPos({ x: e.clientX, y: e.clientY });
                }}
                onPointerMove={(e) => {
                  // Pointer capture keeps sending events to the original node,
                  // even while dragging disables hit testing for other nodes.
                  if (!dragRef.current) setHoverPos({ x: e.clientX, y: e.clientY });
                }}
                onPointerLeave={() => setHover((h) => (h?.skill === node.skill ? null : h))}
                onClick={(e) => handleNodeClick(node, e)}
              />
            </g>
          );
        })}
      </g>
    );
  }, [placed, allocated, filledJewelSockets, searchHits, selectedSkill, heatStat, heatData, heatMax, handleNodeClick, art, plannedNodes, freeNodes, treeEffects, owner, previewNodes]);

  const fullExtent = useMemo(() => treeExtent(placed), [placed]);

  useEffect(() => {
    if (!plannedNodes || !fullExtent) return;
    const route = [...plannedNodes.allocate, ...plannedNodes.deallocate].map(id => byId.get(id))
      .filter((node): node is PassiveNode => node?.x !== undefined && node.y !== undefined);
    if (route.length === 0) return;
    const xs = route.map(node => node.x!);
    const ys = route.map(node => node.y!);
    const width = Math.max(1600, Math.max(...xs) - Math.min(...xs) + 900);
    const height = Math.max(1600, Math.max(...ys) - Math.min(...ys) + 900);
    setViewBox(clampView({ x: (Math.max(...xs) + Math.min(...xs) - width) / 2,
      y: (Math.max(...ys) + Math.min(...ys) - height) / 2, w: width, h: height }, fullExtent));
    const frame = requestAnimationFrame(() => svgRef.current?.closest('.tree-canvas')?.scrollIntoView({ block: 'center', behavior: 'smooth' }));
    return () => cancelAnimationFrame(frame);
  }, [plannedNodes, byId, fullExtent]);

  // 中文界面下把 hover 节点名 + 词条经词典/模板反查翻成简中（异步，结果晚到时
  // 校验仍是当前节点）。
  useEffect(() => {
    if (!hover) {
      setHoverStats(null);
      setHoverName(null);
      return;
    }
    const chosen = session.attributeChoices[String(hover.skill)];
    const chosenLine = chosen ? [`→ ${tt(`tree.attr.${chosen}` as Parameters<typeof tt>[0])}`] : [];
    const raw = [...(hover.stats ?? []), ...chosenLine.map((l) => l)].map((line) =>
      line.replace(/\[([^\]|]*)\|([^\]]*)\]/g, '$2').replace(/\[([^\]]*)\]/g, '$1'),
    );
    if (lang === 'en-US') {
      setHoverStats(raw);
      setHoverName(null);
      return;
    }
    const name = hover.name ?? hover.id;
    let cancelled = false;
    const generation = canvasGeneration.current;
    getBackend()
      .then(b => !cancelled && generation === canvasGeneration.current ? b.translateLines([name, ...raw], lang) : null)
      .then((translated) => {
        if (!translated || cancelled || generation !== canvasGeneration.current) return;
        setHoverName(translated[0] !== name ? translated[0] : null);
        setHoverStats(translated.slice(1));
      })
      .catch(() => {
        if (cancelled || generation !== canvasGeneration.current) return;
        setHoverStats(raw);
        setHoverName(null);
      });
    return () => {
      cancelled = true;
    };
  }, [hover, lang]);

  // 当前升华簇的包围盒（背景圆盘 + 自动聚焦用）。
  const ascExtent = useMemo((): ViewBox | null => {
    const cluster = placed.filter((n) => n.ascendancy_id === currentAscId);
    if (!currentAscId || cluster.length === 0) return null;
    const xs = cluster.map((n) => n.x!);
    const ys = cluster.map((n) => n.y!);
    const pad = 700;
    return {
      x: Math.min(...xs) - pad,
      y: Math.min(...ys) - pad,
      w: Math.max(...xs) - Math.min(...xs) + pad * 2,
      h: Math.max(...ys) - Math.min(...ys) + pad * 2,
    };
  }, [placed, currentAscId]);

  // 切换升华 → 自动缩放定位到升华小盘（PoB2 语义：升华子树单独看）。
  useEffect(() => {
    if (ascExtent) setViewBox(ascExtent);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [currentAscId]);

  // Preview the full validated edit, including both weapon sets and dependent refunds.
  useEffect(() => {
    setHoverDiff(null);
    if (!hover || !hoverPreview?.result || session.busy || !session.calc || !editRequest) return;
    let cancelled = false;
    const timer = setTimeout(() => {
      previewDiff(treeAllocationRequest(editRequest, hoverPreview.result!.allocation), session.calc!)
        .then(diffs => { if (!cancelled) setHoverDiff(diffs); }).catch(() => {});
    }, 300);
    return () => { cancelled = true; clearTimeout(timer); };
  }, [hover, hoverPreview, session.stateVersion, session.busy]);

  // 快捷键 S/D/I（或 1/2/3）：弹窗打开时选择；hover 属性小点时直接加点或改选。
  useLayoutEffect(() => {
    const KEYMAP: Record<string, AttributeChoice> = {
      s: 'str', d: 'dex', i: 'int', '1': 'str', '2': 'dex', '3': 'int',
    };
    const onKey = (e: KeyboardEvent) => {
      const target = e.target as HTMLElement | null;
      if (target && ['INPUT', 'TEXTAREA', 'SELECT'].includes(target.tagName)) return;
      // Hover shortcuts belong to the canvas (or its legacy body focus), not tools.
      if (target && target !== document.body && !target.closest('.tree-canvas')) return;
      const choice = KEYMAP[e.key.toLowerCase()];
      if (!choice) return;
      if (attrPicker) {
        actOnNode(attrPicker.node.skill, 'allocate', choice);
        e.preventDefault();
        return;
      }
      if (hover && isAttrNode(hover)) {
        actOnNode(hover.skill, 'attribute', choice);
        e.preventDefault();
      }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  });


  const view = viewBox ?? fullExtent;

  if (error) return <div className="calc-error">{error}</div>;
  if (!nodes || !view) return <div className="empty-hint">{tt('tree.loading')}</div>;

  // Keep the SVG viewport fixed: translating the already clipped root layer
  // exposes blank edges. Pan its scene once per animation frame instead, without
  // reconciling the memoized node/edge subtrees. Commit the viewBox on release.

  const onWheel = (e: React.WheelEvent<SVGSVGElement>) => {
    if (dragRef.current) return;
    const matrix = svgRef.current?.getScreenCTM();
    if (!matrix) return;
    const factor = e.deltaY > 0 ? 1.15 : 1 / 1.15;
    const point = new DOMPoint(e.clientX, e.clientY).matrixTransform(matrix.inverse());
    const tx = (point.x - view.x) / view.w;
    const ty = (point.y - view.y) / view.h;
    const base = wheelPendingRef.current ?? view;
    const px = base.x + tx * base.w;
    const py = base.y + ty * base.h;
    const w = Math.min(Math.max(base.w * factor, 800), (fullExtent?.w ?? 1) * 2);
    const h = (w / base.w) * base.h;
    wheelPendingRef.current = {
      x: px - tx * w,
      y: py - ty * h,
      w,
      h,
    };
    if (wheelRafRef.current === null) {
      wheelRafRef.current = requestAnimationFrame(() => {
        wheelRafRef.current = null;
        if (wheelPendingRef.current) {
          setViewBox(fullExtent ? clampView(wheelPendingRef.current, fullExtent) : wheelPendingRef.current);
          wheelPendingRef.current = null;
        }
      });
    }
  };

  const onPointerDown = (e: React.PointerEvent<SVGSVGElement>) => {
    if (e.button !== 0 || !e.isPrimary || dragRef.current) return;
    const svg = svgRef.current!;
    const base = wheelPendingRef.current ?? view;
    if (wheelRafRef.current !== null) cancelAnimationFrame(wheelRafRef.current);
    wheelRafRef.current = null;
    if (wheelPendingRef.current) {
      const pending = fullExtent ? clampView(base, fullExtent) : base;
      svg.setAttribute('viewBox', `${pending.x} ${pending.y} ${pending.w} ${pending.h}`);
      setViewBox(pending);
      wheelPendingRef.current = null;
    }
    const matrix = svg.getScreenCTM();
    if (!matrix) return;
    const inverse = matrix.inverse();
    const committed = svg.viewBox.baseVal;
    const start = { x: committed.x, y: committed.y, w: committed.width, h: committed.height };
    dragRef.current = { pointerId: e.pointerId, startX: e.clientX, startY: e.clientY,
      inverse, origin: new DOMPoint(e.clientX, e.clientY).matrixTransform(inverse), view: start, next: start, moved: false };
    if (clickResetRef.current !== null) clearTimeout(clickResetRef.current);
    suppressClickRef.current = false;
    // Keep node clicks targeted at the original circle until a drag is confirmed.
    (e.target as Element).setPointerCapture?.(e.pointerId);
  };

  const panAt = (drag: NonNullable<typeof dragRef.current>, x: number, y: number) => {
    const point = new DOMPoint(x, y).matrixTransform(drag.inverse);
    const next = { ...drag.view, x: drag.view.x - point.x + drag.origin.x, y: drag.view.y - point.y + drag.origin.y };
    return fullExtent ? clampView(next, fullExtent) : next;
  };

  const onPointerMove = (e: React.PointerEvent<SVGSVGElement>) => {
    const drag = dragRef.current;
    const svg = svgRef.current;
    if (!drag || !svg || e.pointerId !== drag.pointerId) return;
    const dx = e.clientX - drag.startX;
    const dy = e.clientY - drag.startY;
    if (!drag.moved && Math.abs(dx) + Math.abs(dy) > 3) {
      drag.moved = true;
      // 拖动确立后关闭节点命中测试（省掉每帧对 ~4900 个圆的 hover 命中）；
      // pointer-events 关闭后 leave 不再触发，主动清掉悬停 tooltip。
      svg.classList.add('is-dragging');
      setHover(null);
    }
    if (drag.moved) {
      drag.next = panAt(drag, e.clientX, e.clientY);
      if (dragRafRef.current === null) dragRafRef.current = requestAnimationFrame(() => {
        dragRafRef.current = null;
        if (dragRef.current === drag) sceneRef.current?.setAttribute('transform',
          `translate(${drag.view.x - drag.next.x} ${drag.view.y - drag.next.y})`);
      });
    }
  };

  const finishDrag = (e: React.PointerEvent<SVGSVGElement>, cancelled = false) => {
    const drag = dragRef.current;
    const svg = svgRef.current;
    if (!drag || e.pointerId !== drag.pointerId) return;
    if (dragRafRef.current !== null) cancelAnimationFrame(dragRafRef.current);
    dragRafRef.current = null;
    dragRef.current = null;
    if (drag?.moved && svg) {
      if (cancelled) {
        sceneRef.current?.removeAttribute('transform');
        svg.classList.remove('is-dragging');
      } else {
        transformResetRef.current = true;
        setViewBox(panAt(drag, e.clientX, e.clientY));
      }
      suppressClickRef.current = true;
      clickResetRef.current = setTimeout(() => { suppressClickRef.current = false; clickResetRef.current = null; }, 0);
    }
  };

  return (
    <section className="ui-page tree-panel" aria-labelledby="tree-heading" onKeyDown={e => {
      if (e.key !== 'Escape' || (selectedSkill === null && !attrPicker)) return;
      e.preventDefault();
      setAttrPicker(null);
      closeDetails();
    }}>
      <PageHeader id="tree-heading" title={tt('tree.title')} description={tt('tree.hint')}>
        <AppButton onClick={() => {
          toolsRef.current?.focus({ preventScroll: true });
          toolsRef.current?.scrollIntoView({ block: 'start' });
        }}>{tt('tree.editOptions')}</AppButton>
        <span className="ui-badge tree-count">{session.allocatedNodes.length} {tt('tree.allocated')}</span>
      </PageHeader>
      <div className="tree-toolbar">
        <label className="tree-allocation-mode">
          {tt('tree.allocationMode')}
          <AppSelectControl aria-label={tt('tree.allocationMode')} value={allocationMode} disabled={session.busy}
            onChange={e => { setEditError(null); setAttrPicker(null); setAttributeDraft(null);
              onAllocationMode(Number(e.target.value) as TreeAllocationMode); }}>
            <option value={0}>{tt('tree.shared')}</option>
            <option value={1}>{tt('tree.set1')}</option>
            <option value={2}>{tt('tree.set2')}</option>
          </AppSelectControl>
        </label>
        <span className="tree-search">
          <AppInput
            ref={searchRef}
            type="search"
            placeholder={tt('tree.search')}
            value={search}
            onChange={(e) => {
              setSearch(e.target.value);
              setHitIndex(0);
              setSelectedSkill(null);
              setAttributeDraft(null);
            }}
            aria-label={tt('tree.search')}
          />
          {searchHits && (
            <span className="tree-search-count">
              {searchHits.size} {tt('tree.matches')}
            </span>
          )}
          {searchHits && searchHits.size > 0 && (
            <AppButton
              ref={nextHitRef}
              onClick={() => {
                const sorted = [...searchHits].sort((a, b) => a - b);
                const node = byId.get(sorted[hitIndex % sorted.length]);
                setHitIndex((i) => i + 1);
                if (!node) return;
                setSelectedSkill(node.skill);
                setAttributeDraft(null);
                setAttrPicker(null);
                focusDetailsRef.current = true;
                // 保持当前缩放（过远时收到能看清节点簇的档位）平移到命中节点。
                const w = Math.min(view.w, 14000);
                const h = (w / view.w) * view.h;
                setViewBox({ x: node.x! - w / 2, y: node.y! - h / 2, w, h });
              }}
            >
              {tt('tree.nextHit')}
            </AppButton>
          )}
        </span>
        <span className="tree-heat">
          <AppTooltip content={tt('tree.heatHint')}><span tabIndex={0}>{tt('tree.heat')}</span></AppTooltip>
          <AppSelectControl
            value={heatStat}
            aria-label={tt('tree.heat')}
            onChange={(e) => {
              setHeatStat(e.target.value);
              setHeatData(null);
              setHeatVersion(-1);
            }}
          >
            <option value="">{tt('tree.heatOff')}</option>
            {heatStatOptions.map((id) => (
              <option key={id} value={id}>
                {statNameLabel(lang, id)}
              </option>
            ))}
          </AppSelectControl>
          {heatStat && (
            <>
              <label className="tree-heat-depth">
                {tt('tree.heatDepth')}
                <AppInput
                  type="number"
                  min={1}
                  max={12}
                  value={heatDepth}
                  onChange={(e) => {
                    const v = Number(e.target.value);
                    if (Number.isInteger(v) && v >= 1 && v <= 12) setHeatDepth(v);
                  }}
                />
              </label>
              <AppButton disabled={heatBusy || session.busy} onClick={runHeat}>
                {heatBusy ? tt('tree.heatComputing') : tt('tree.heatRun')}
              </AppButton>
              {heatData && heatVersion !== session.stateVersion && (
                <span className="tree-hint">{tt('tree.heatStale')}</span>
              )}
            </>
          )}
        </span>
        <label className="tree-asc-picker">
          {tt('build.class')}
          <AppSelectControl
            value={session.character?.class_name ?? ''}
            disabled={session.busy}
            onChange={async (e) => {
              const nextClass = e.target.value;
              const valid = captureAction();
              if (session.hasBuildContent && !await dialogs.confirm(tt('build.confirmClassChange'))) return;
              if (valid()) session.newBuild(nextClass, '');
            }}
          >
            {(session.treeMeta?.classes ?? []).map((c) => (
              <option key={c.name} value={c.name}>
                {localizedName(lang, c.name, session.classNames.classes[c.name])}
              </option>
            ))}
          </AppSelectControl>
        </label>
        <label className="tree-asc-picker">
          {tt('tree.ascendancy')}
          <AppSelectControl
            value={session.character?.ascendancy_name ?? ''}
            disabled={session.busy}
            onChange={(e) => session.setCharacter({ ascendancy_name: e.target.value })}
          >
            <option value="">{tt('build.none')}</option>
            {(session.treeMeta?.classes ?? [])
              .find((c) => c.name === session.character?.class_name)
              ?.ascendancies?.map((a) => (
                <option key={a.id} value={a.name}>
                  {localizedName(lang, a.name, session.classNames.ascendancies[a.name])}
                </option>
              ))}
          </AppSelectControl>
        </label>
        {ascExtent && (
          <AppButton onClick={() => setViewBox(ascExtent)}>{tt('tree.focusAsc')}</AppButton>
        )}
        <AppButton onClick={() => setViewBox(null)}>{tt('tree.reset')}</AppButton>
      </div>
      <p className="tree-hint">{tt('tree.weaponRules')}</p>
      {editError && <p role="alert">{treeEditError(editError, lang)}</p>}
      <div className="tree-edit-preview" role="status">
        {preview && <>
        {previewResult ? <>
          <span>{tt('tree.points')}: {previewResult.points_used} / {previewResult.points_max}</span>
          <span>{tt('tree.set1')}: {previewResult.weapon_points_used[0]} / {previewResult.weapon_points_max}</span>
          <span>{tt('tree.set2')}: {previewResult.weapon_points_used[1]} / {previewResult.weapon_points_max}</span>
          <span>{tt('tree.previewChanges')}: +{previewResult.added.length} / −{previewResult.removed.length} · {tt('tree.promoted')}: {previewResult.promoted.length}</span>
        </> : treeEditError(preview.reason, lang)}
        </>}
      </div>
      <div className="tree-workspace">
      <div className="tree-canvas">
        <svg
          ref={svgRef}
          viewBox={`${view.x} ${view.y} ${view.w} ${view.h}`}
          onWheel={onWheel}
          onPointerDown={onPointerDown}
          onPointerMove={onPointerMove}
          onPointerUp={e => finishDrag(e)}
          onPointerCancel={e => finishDrag(e, true)}
          onLostPointerCapture={e => finishDrag(e, true)}
          role="img"
          aria-label={tt('tree.title')}
        >
          <defs>
            {/* 技能图标裁成圆形（美术是方图，避免方角戳出外框） */}
            <clipPath id="tree-icon-clip" clipPathUnits="objectBoundingBox">
              <circle cx="0.5" cy="0.5" r="0.5" />
            </clipPath>
          </defs>
          <g ref={sceneRef} className="tree-scene">
          {ascExtent && (
            <circle
              className="asc-backdrop"
              cx={ascExtent.x + ascExtent.w / 2}
              cy={ascExtent.y + ascExtent.h / 2}
              r={Math.max(ascExtent.w, ascExtent.h) / 2}
            />
          )}
          {(treeEffects?.rings ?? []).filter(ring => ring.source === jewelSocket).map(ring => {
            const center = byId.get(ring.center);
            if (center?.x === undefined || center.y === undefined) return null;
            const circle = (r: number) => `M ${center.x! + r} ${center.y} a ${r} ${r} 0 1 0 ${-2 * r} 0 a ${r} ${r} 0 1 0 ${2 * r} 0`;
            return <path key={`${ring.source}:${ring.center}`} className="jewel-radius" data-jewel-source={ring.source}
              fillRule="evenodd" d={`${circle(ring.outer)} ${ring.inner > 0 ? circle(ring.inner) : ''}`} />;
          })}
          {edgesEl}
          {nodesEl}
          </g>
        </svg>
        {hover && !attrPicker && (
          <TreeTooltip
            node={hover}
            name={hoverName}
            stats={hoverStats ?? []}
            pos={hoverPos}
            canvasRef={svgRef}
            benefit={
              hoverDiff && (
                <div className="tooltip-benefit">
                  <span className="tooltip-benefit-title">
                    {allocated.has(hover.skill) ? tt('diff.ifDealloc') : tt('diff.ifAlloc')}
                  </span>
                  <DiffList diffs={hoverDiff} lang={lang} limit={5} />
                </div>
              )
            }
          />
        )}
        {attrPicker && (
          <div
            className="attr-picker"
            role="menu"
            style={{ left: attrPicker.x + 10, top: attrPicker.y + 10 }}
          >
            <div className="attr-picker-title">{tt('tree.attrPick')}</div>
            {(['str', 'dex', 'int'] as AttributeChoice[]).map((choice) => (
              <AppButton
                key={choice}
                role="menuitem"
                className={`attr-choice attr-${choice}`}
                disabled={session.busy}
                onClick={() => actOnNode(attrPicker.node.skill, 'allocate', choice)}
              >
                {tt(`tree.attr.${choice}` as Parameters<typeof tt>[0])}
                <kbd>{choice === 'str' ? 'S' : choice === 'dex' ? 'D' : 'I'}</kbd>
              </AppButton>
            ))}
            <AppButton className="attr-cancel" aria-label={tt('tree.closeDetails')}
              onClick={() => setAttrPicker(null)}>
              ×
            </AppButton>
          </div>
        )}
        {!currentAscId &&
          ((session.treeMeta?.classes ?? []).find(
            (c) => c.name === session.character?.class_name,
          )?.ascendancies?.length ?? 0) > 0 && (
            <div className="asc-hint">{tt('tree.pickAscHint')}</div>
          )}
      </div>
      <div className="tree-edit-tools" ref={toolsRef} tabIndex={-1}>
      {selectedNode && (
        <div className="tree-node-details" ref={detailsRef} tabIndex={-1}
          role="group" aria-label={tt('tree.nodeDetails')} data-skill-id={selectedNode.skill}>
          <header className="tree-node-details-header">
            <h2>{displayedDetails[0]}</h2>
            <AppButton onClick={closeDetails}>{tt('tree.closeDetails')}</AppButton>
          </header>
          <p className="tree-node-details-state">
            #{selectedNode.skill} · {tt(selectedAllocated ? 'tree.allocated' : 'tree.notAllocated')}
            {(selectedAllocated || owner.has(selectedNode.skill)) && <> · {tt(owner.get(selectedNode.skill) === 1 ? 'tree.set1' : owner.get(selectedNode.skill) === 2 ? 'tree.set2' : 'tree.shared')}</>}
          </p>
          <ul className="tree-node-details-stats">
            {displayedDetails.slice(1).map((line, index) => <li key={index}>{line}</li>)}
          </ul>
          {isAttrNode(selectedNode) && (selectedAllocated || selectedNeedsAttribute) && (
            <label className="tree-node-attribute">
              {tt('tree.attributeChoice')}
              <AppSelectControl aria-label={tt('tree.attributeChoice')} disabled={session.busy}
                value={pendingAttribute ?? selectedChoice ?? ''}
                onChange={e => setAttributeDraft({ skill: selectedNode.skill,
                  saved: selectedChoice, choice: e.target.value as AttributeChoice })}>
                <option value="" disabled>{tt('tree.attrUnassigned')}</option>
                {(['str', 'dex', 'int'] as AttributeChoice[]).map(choice => (
                  <option key={choice} value={choice}>
                    {tt(`tree.attr.${choice}` as Parameters<typeof tt>[0])}
                  </option>
                ))}
              </AppSelectControl>
            </label>
          )}
          {!selectedAllocated && selectedPreview?.valid === false && (
            <p className="tree-hint">{tt(selectedNode.skill === startSkill ? 'tree.currentStart' : 'tree.unallocatable')}</p>
          )}
          {!selectedAllocated && selectedPath.length > 1 && (
            <p className="tree-hint">{tt('tree.allocatePath')} · {selectedPath.length}</p>
          )}
          {selectedAllocated && selectedNode.kind !== 'jewel_socket' && selectedRefundCount > 1 && (
            <p className="tree-hint">{tt('tree.refundPath')} · {selectedRefundCount}</p>
          )}
          {selectedAllocated && selectedNode.kind === 'jewel_socket' && (
            <p className="tree-hint">{tt('tree.refundSocket')}</p>
          )}
          <div className="tree-node-details-actions">
            {selectedAllocated && isAttrNode(selectedNode) && (
              <AppButton disabled={session.busy || !pendingAttribute || pendingAttribute === selectedChoice}
                onClick={() => {
                  actOnNode(selectedNode.skill, 'attribute', pendingAttribute);
                  setAttributeDraft(null);
                }}>
                {tt('tree.applyAttribute')}
              </AppButton>
            )}
            {selectedAllocated ? (
              <AppButton disabled={session.busy || !selectedPreview?.valid} onClick={() => actOnNode(selectedNode.skill, 'refund')}>
                {tt('tree.refund')}
              </AppButton>
            ) : (
              <AppButton disabled={session.busy || !selectedPreview?.valid
                || (selectedNeedsAttribute && !pendingAttribute)}
                onClick={() => actOnNode(selectedNode.skill, 'allocate', pendingAttribute || undefined)}>
                {tt('tree.allocate')}
              </AppButton>
            )}
            {owner.has(selectedNode.skill) && <AppButton disabled={session.busy}
              onClick={() => actOnNode(selectedNode.skill, 'promote')}>{tt('tree.promote')}</AppButton>}
            {selectedAllocated && selectedNode.kind === 'jewel_socket' && (
              <AppButton disabled={session.busy} onClick={() => actOnNode(selectedNode.skill, 'jewel')}>
                {tt('tree.editJewel')}
              </AppButton>
            )}
          </div>
        </div>
      )}
      <div className="attr-distribute" role="group" aria-label={tt('tree.attrDistribute')}>
        {allocatedAttrNodes.length > 0 && (
        <>
          <span className="attr-distribute-title">
            {tt('tree.attrDistribute')}（{allocatedAttrNodes.length}）
          </span>
          {(['str', 'dex', 'int'] as AttributeChoice[]).map((key) => (
            <label key={key} className={`attr-count attr-${key}`}>
              {tt(`tree.attr.${key}` as Parameters<typeof tt>[0])}
              <AppInput
                type="number"
                min={0}
                max={allocatedAttrNodes.length}
                value={attrCounts[key]}
                disabled={session.busy}
                onChange={(e) => {
                  const value = Math.max(0, Math.min(allocatedAttrNodes.length, Number(e.target.value) || 0));
                  const next = { ...attrCounts, [key]: value };
                  // 超额时压缩其它两项（保持总和 ≤ 已加点数）。
                  const order: AttributeChoice[] = ['str', 'dex', 'int'];
                  let overflow = next.str + next.dex + next.int - allocatedAttrNodes.length;
                  for (const other of order.filter((o) => o !== key)) {
                    if (overflow <= 0) break;
                    const cut = Math.min(next[other], overflow);
                    next[other] -= cut;
                    overflow -= cut;
                  }
                  distributeAttributes(next.str, next.dex, next.int);
                }}
              />
            </label>
          ))}
          <span className="attr-distribute-rest">
            {tt('tree.attrUnassigned')}: {allocatedAttrNodes.length - attrCounts.str - attrCounts.dex - attrCounts.int}
          </span>
          <span className="tree-hint">{tt('tree.attrHotkeys')}</span>
        </>
        )}
          <span className="attr-quest" role="group" aria-label={tt('tree.questAttr')}>
            {tt('tree.questAttr')}
            {(
              [
                ['str', "questAct 4Halls Of The DeadNgamahu's Test", '+5 to Strength'],
                ['int', "questAct 4Halls Of The DeadTasalio's Test", '+5 to Intelligence'],
                ['dex', "questAct 4Halls Of The DeadTawhoa's Test", '+5 to Dexterity'],
              ] as const
            ).map(([key, cfgVar, value]) => {
              const active = session.calcParams.config_inputs[cfgVar] === value;
              return (
                <label key={key} className={`attr-count attr-${key}`}>
                  <AppInput
                    type="checkbox"
                    checked={active}
                    disabled={session.busy}
                    onChange={(e) =>
                      session.setConfigInput(cfgVar, e.target.checked ? value : null)
                    }
                  />
                  {tt(`tree.attr.${key}` as Parameters<typeof tt>[0])}
                </label>
              );
            })}
            <label className="attr-count">
              <AppInput
                type="checkbox"
                checked={
                  session.calcParams.config_inputs['questInterlude 2QimahSeven Pillars'] ===
                  '+5 to all Attributes'
                }
                disabled={session.busy}
                onChange={(e) =>
                  session.setConfigInput(
                    'questInterlude 2QimahSeven Pillars',
                    e.target.checked ? '+5 to all Attributes' : null,
                  )
                }
              />
              {tt('tree.questAllAttr')}
            </label>
          </span>
      </div>
      {Boolean(treeEffects?.warnings.length) && <p className="tree-hint" role="status">{tt('tree.jewelSeedMissing')}</p>}
      <NodePowerTable session={session} nodes={nodes ?? []} lang={lang}
        stat={heatStat || 'Life'} stats={heatStatOptions}
        onStat={stat => { setHeatStat(stat); setHeatData(null); setHeatVersion(-1); }}
        onFocus={node => {
          setSelectedSkill(node.skill); setAttributeDraft(null); setAttrPicker(null);
          const w = Math.min(view.w, 6000), h = w / view.w * view.h;
          setViewBox({ x: node.x! - w / 2, y: node.y! - h / 2, w, h });
          svgRef.current?.scrollIntoView({ block: 'center' });
        }} />
      <TreeOptimizer planner={planner} />
      {Object.keys(session.editorDrafts).filter(key => key.startsWith('jewel:')).map(key => (
        <AppButton key={key} onClick={() => setJewelSocket(Number(key.slice(6)))}>
          {tt('editor.draft')} · {tt('tree.jewel')} #{key.slice(6)}
        </AppButton>
      ))}
      {jewelEdit && (
        <div className="jewel-editor" ref={jewelRef} tabIndex={-1} role="group" aria-label={tt('tree.jewel')}>
          <header className="item-detail-header">
            <span className="item-slot">
              {tt('tree.jewel')} · #{jewelEdit.socket}
            </span>
            <span className="item-actions">
              <AppButton
                disabled={session.busy || !jewelEdit.draft.trim()}
                onClick={() => {
                  const rest = session.jewels.filter((j) => j.socket_node !== jewelEdit.socket);
                  session.setJewels([...rest, { socket_node: jewelEdit.socket, text: jewelEdit.draft }]);
                  closeJewelEditor();
                }}
              >
                {tt('items.apply')}
              </AppButton>
              {session.jewels.some((j) => j.socket_node === jewelEdit.socket) && (
                <AppButton
                  className="skill-remove"
                  disabled={session.busy}
                  onClick={() => {
                    session.setJewels(session.jewels.filter((j) => j.socket_node !== jewelEdit.socket));
                    closeJewelEditor();
                  }}
                >
                  {tt('items.remove')}
                </AppButton>
              )}
              <AppButton
                disabled={session.busy}
                onClick={() => {
                  actOnNode(jewelEdit.socket, 'refund');
                  closeJewelEditor();
                }}
              >
                {tt('tree.unallocSocket')}
              </AppButton>
              <AppButton
                disabled={session.busy}
                onClick={() => session.saveLibraryItem('jewel', jewelEdit.draft)}
              >
                {tt('lib.save')}
              </AppButton>
              <AppButton onClick={() => closeJewelEditor()}>{tt('items.cancel')}</AppButton>
            </span>
          </header>
          <p className="tree-hint">{tt('tree.jewelHint')}</p>
          <NoteEditor
            value={session.annotations[`jewel:${jewelEdit.socket}`] ?? ''}
            onCommit={(text) => session.setAnnotation(`jewel:${jewelEdit.socket}`, text)}
            lang={lang}
          />
          {jewelLib.length > 0 && (
            <div className="jewel-library">
              {(() => {
                const current = session.jewels.find(
                  (j) => j.socket_node === jewelEdit.socket,
                )?.text;
                const value =
                  jewelLib.find((e) => e.text === current)?.id ?? (current ? '__current' : '');
                return (
                  <AppSelect
                    ariaLabel={tt('items.switcher')}
                    value={value}
                    disabled={session.busy}
                    options={[
                      ...(current && !jewelLib.some((e) => e.text === current)
                        ? [{ value: '__current', label: tt('items.currentItem') }]
                        : []),
                      { value: '', label: tt('items.unequip') },
                      ...jewelLib.map((entry, i) => ({
                        value: entry.id,
                        label: jewelNames[i] || entry.name,
                      })),
                    ]}
                    onChange={(v) => {
                      if (v === '__current') return;
                      const rest = session.jewels.filter(
                        (j) => j.socket_node !== jewelEdit.socket,
                      );
                      if (v === '') {
                        session.setJewels(rest);
                        session.setEditorDraft(jewelDraftKey, null);
                        return;
                      }
                      const entry = jewelLib.find((x) => x.id === v);
                      if (!entry) return;
                      session.setJewels([
                        ...rest,
                        { socket_node: jewelEdit.socket, text: entry.text },
                      ]);
                      session.setEditorDraft(jewelDraftKey, null);
                    }}
                  />
                );
              })()}
            </div>
          )}
          {onJewelSearch && session.allocatedNodes.includes(jewelEdit.socket) && <AppButton
            onClick={() => onJewelSearch(jewelEdit.socket)}>{tt('trade.radiusJewel')}</AppButton>}
          <AppTextarea
            rows={7}
            value={jewelEdit.draft}
            spellCheck={false}
            aria-label={tt('tree.jewel')}
            onChange={(e) => session.setEditorDraft(jewelDraftKey,
              e.target.value === (session.jewels.find(jewel => jewel.socket_node === jewelSocket)?.text ?? JEWEL_TEMPLATE)
                ? null : e.target.value)}
          />
        </div>
      )}
      </div>
      </div>
    </section>
  );
}

/** 跟随鼠标的节点 tooltip（贴右下角偏移，靠近画布右/下缘时翻转）。 */
function TreeTooltip({
  node,
  name,
  stats,
  pos,
  canvasRef,
  benefit,
}: {
  node: PassiveNode;
  /** 本地化节点名（词典命中才给；null = 显示英文原名）。 */
  name: string | null;
  stats: string[];
  pos: { x: number; y: number };
  canvasRef: React.RefObject<SVGSVGElement | null>;
  benefit?: React.ReactNode;
}) {
  const rect = canvasRef.current?.getBoundingClientRect();
  if (!rect) return null;
  const OFFSET = 14;
  const flipX = pos.x - rect.left > rect.width * 0.6;
  const flipY = pos.y - rect.top > rect.height * 0.65;
  const style: React.CSSProperties = {
    left: flipX ? undefined : pos.x - rect.left + OFFSET,
    right: flipX ? rect.width - (pos.x - rect.left) + OFFSET : undefined,
    top: flipY ? undefined : pos.y - rect.top + OFFSET,
    bottom: flipY ? rect.height - (pos.y - rect.top) + OFFSET : undefined,
  };
  return (
    <div className="tree-tooltip" role="tooltip" style={style}>
      <strong className={`tooltip-name kind-${node.kind}`}>{name ?? node.name ?? node.id}</strong>
      {stats.map((line, i) => (
        <div key={i} className="tooltip-stat">
          {line}
        </div>
      ))}
      {benefit}
    </div>
  );
}

function treeEditError(reason: string | null, lang: Lang): string {
  const tt = bindT(lang);
  if (reason === 'passive_point_budget') return tt('tree.pointLimit');
  if (reason === 'weapon_point_budget') return tt('tree.weaponLimit');
  if (reason === 'ascendancy_point_budget') return tt('tree.ascLimit');
  if (reason === 'unsupported_node') return tt('tree.unsupportedNode');
  if (reason === 'stale_edit') return tt('tree.staleEdit');
  if (reason === 'backend_unavailable') return tt('tree.editUnavailable');
  if (reason === 'protected_root') return tt('tree.currentStart');
  if (reason === 'wrong_weapon_set') return tt('tree.wrongSet');
  if (['disconnected_shared_path', 'disconnected_weapon_path', 'overlapping_allocations', 'shared_only_node', 'unknown_node'].includes(reason ?? '')) return tt('tree.invalidImport');
  return tt('tree.noLegalPath');
}
