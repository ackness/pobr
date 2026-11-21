import { TreeNodeChanges } from './TreeNodeChanges';
import { useTreeHover } from './useTreeHover';
import { hasDisconnectedTreeNodes, isDisconnectedTree, isImportProblem, needsTreeConversion } from './treeImportStatus';
import { TreeRefundNotice, type RefundReceipt } from './TreeRefundNotice';
import { evaluateHeatmap } from './heatmap';
import { fitToParentInPixels } from './pixelSize';
import { trackBarBottom } from './barClearance';
import { SidebarResizeHandle } from './SidebarResizeHandle';
import { readSideWidth, writeSideWidth } from './sidebarWidth';
import { TreePointSummary } from './TreePointSummary';
import { jewelSocketNames } from './jewelSocketNames';
import { JewelDraftSources } from './JewelDraftSources';
import { defaultReportStat, distributeAttributeChoices, passivePointSummary, treeHoverLines } from './treePresentation';
import { ValueText } from '../shared/ValueText';
import { classDisplayName } from '../../lib/nativeDisplayNames';
import { useAppDialogs } from '../shared/AppDialogs';
import { useSessionAction } from '../shared/useSessionAction';
import { AppTooltip, AppButton, AppInput, AppTextarea, AppSelectControl } from '../shared/controls';
import { TreeConversionControls, useTreeConversion } from './TreeConversionControls';
import { formatApiError } from '../../api/error';
import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState, type ReactNode, type RefObject } from 'react';
import { getBackend } from '../../api/backend';
import type { AttributeChoice, CalculateBuildResponse, PassiveAllocationGrant, PassiveNode, TreeAllocationMode } from '../../api/types';
import { useTreeEditPreview } from '../../hooks/useTreeEditPreview';
import { treeAllocationRequest } from '../../lib/weaponSets';
import type { BuildSession } from '../../hooks/useBuildSession';
import { cleanLine, useItemDisplayNames } from '../../hooks/useLocalizedLines';
import { bindT, messages, t, type Lang } from '../../lib/i18n';
import {
  allocationAccess,
  buildPassiveGraph,
  classStartSkill,
  implicitClassStarts,
} from '../../lib/passiveGraph';
import { NODE_RADIUS, clampView, isAttributeNode as isAttrNode, passiveTreeEdges, treeExtent, type ViewBox } from '../../lib/treeGeometry';
import { diffStats, type DiffEntry } from '../../lib/compare';
import { TreeHoverBenefits } from './TreeHoverBenefits';
import { treeStatDelta, treeStatLabel, powerGain } from './treePresentation';
import { AppSelect } from '../shared/AppSelect';
import { NoteEditor } from '../shared/NoteEditor';
import { TreeOptimizer, useTreeOptimizer, type TreeOptimizerState } from './TreeOptimizer';
import type { PassivePlan } from '../../lib/passivePlanner';
import './tree.css';
import { NodePowerTable } from './NodePowerTable';
import { useTreeLocation } from './useTreeLocation';
import { placedRouteNodes, plannedEdgeClass, routeViewBox, type RouteScope } from './plannedRoute';
import { colon } from '../../app/text';
import { PanelRightClose, PanelRightOpen } from 'lucide-react';
import { passiveNodeMatches, passiveNodeName, translatePassiveCard, usePassiveNodeNames, type PassiveNodeNames } from './passiveNodeNames';

interface Props {
  session: BuildSession;
  lang: Lang;
  active: boolean;
  /** Display only; diff baselines and edit calculations still use the current session. */
  settledCalc?: CalculateBuildResponse | null;
  focusPlanner?: { nonce: number; node?: number };
  onJewelSearch?: (socket: number) => void;
}

interface RoutePreview { plan: PassivePlan }

const JEWEL_TEMPLATE = 'Rarity: RARE\nMy Jewel\nEmerald\n+50 to maximum Life';
const ATTRIBUTE_KEYMAP: Record<string, AttributeChoice> = {
  s: 'str', d: 'dex', i: 'int', '1': 'str', '2': 'dex', '3': 'int',
};

/** Passive tree viewer: SVG rendering + allocated highlights + pan/zoom / hover mods + click-to-allocate recalc. */
export function TreePanel(props: Props) {
  const { session, lang, active } = props;
  const conversion = useTreeConversion(session, lang, active);
  const currentTree = session.currentTreeVersion;
  if (session.treeVersion && session.treeVersion !== 'Default' && currentTree && session.treeVersion !== currentTree) {
    if (!active) return null;
    const text = t(lang, 'tree.historicalVersion', { version: session.treeVersion });
    return <section className="ui-page tree-historical"><p role="status">{bindT(lang)('tree.invalidImport')}</p><TreeConversionControls session={session} conversion={conversion} /><p>{text}</p></section>;
  }
  // The conversion controls sit in the canvas toolbar, not above the page.
  return <EditableTreePanel key={JSON.stringify([session.dataVersion, session.treeVersion])} {...props}
    conversion={active ? <TreeConversionControls session={session} conversion={conversion} /> : undefined} />;
}

/** Retain planner inputs and complete receipts, never the interactive canvas. */
function EditableTreePanel({ session, lang, active, settledCalc, focusPlanner, onJewelSearch, conversion }: Props & { conversion?: ReactNode }) {
  const [allocationMode, setAllocationMode] = useState<TreeAllocationMode>(0);
  const mode = allocationMode === 0 ? 0 : session.activeWeaponSet;
  const changeMode = (next: TreeAllocationMode) => {
    setAllocationMode(next);
    if (next !== 0) session.setWeaponSet(next);
  };
  const [rawNodes, setNodes] = useState<PassiveNode[] | null>(null);
  const displayCalc = session.calcError ? null : session.calc ?? settledCalc ?? null;
  const treeEffects = displayCalc?.tree_effects;
  // Recalculation clears the result before replacing it, often with identical node changes.
  const displayChangesKey = useMemo(() => JSON.stringify(treeEffects?.nodes ?? {}), [treeEffects?.nodes]);
  const nodes = useMemo(() => rawNodes?.map(node => {
    const change = treeEffects?.nodes[String(node.skill)];
    return change ? { ...node, name: change.name,
      stats: change.replace ? change.stats : [...(node.stats ?? []), ...change.stats] } : node;
  }) ?? [], [rawNodes, displayChangesKey]);
  const [error, setError] = useState<string | null>(null);
  const [routePreview, setRoutePreview] = useState<RoutePreview | null>(null);
  const byId = useMemo(() => new Map(nodes.map(node => [node.skill, node])), [nodes]);
  const ascendancy = session.treeMeta?.classes.flatMap(entry => entry.ascendancies ?? [])
    .find(entry => entry.name === session.character?.ascendancy_name)?.id;
  const namedNodes = useMemo(() => nodes.filter(node => !node.ascendancy_id || node.ascendancy_id === ascendancy), [nodes, ascendancy]);
  const [nameSearch, setNameSearch] = useState('');
  const nodeNames = usePassiveNodeNames(namedNodes, lang, active, nameSearch);
  const socketLabels = useMemo(() => jewelSocketNames(nodes, nodeNames, lang), [nodes, nodeNames, lang]);
  const [located, setLocated] = useState<{ nonce: number; node: number }>();
  const consumedLocation = useRef<number | undefined>(undefined);
  const ascendancyFocus = useRef<string | null>(null);
  const [overlayReset, setOverlayReset] = useState(0);
  const grants = useMemo(() => treeEffects?.allocation_grants ?? [], [treeEffects]);
  const allocated = useMemo(() => new Set(session.allocatedNodes), [session.allocatedNodes]);
  const startSkill = classStartSkill(treeEffects, session.character?.class_name);
  const implicitStarts = useMemo(() => implicitClassStarts(treeEffects), [treeEffects]);
  // Use all nodes, not the canvas subset filtered by current ascendancy.
  const graph = useMemo(() => {
    const allowed = new Set([startSkill, ...grants.flatMap(grant => grant.roots)]);
    return buildPassiveGraph(nodes.filter(node => !implicitStarts.has(node.skill) || allowed.has(node.skill)));
  }, [nodes, implicitStarts, startSkill, grants]);
  const editRequest = useMemo(() => session.currentRequest(), [session.currentRequest]);
  // A protected-root preview reports the current budget and diagnoses incompatible imports.
  const importCheck = useTreeEditPreview(editRequest, startSkill, 0, 'refund');
  const pointSummary = useMemo(() => passivePointSummary(nodes, session.allocatedNodes, session.weaponSwap, treeEffects, importCheck?.current_points),
    [nodes, session.allocatedNodes, session.weaponSwap, treeEffects, importCheck]);
  const planner = useTreeOptimizer({ session, lang, active, nodes, pointSummary,
    onLocate: node => setLocated({ node, nonce: Date.now() }),
    onRun: () => setOverlayReset(value => value + 1),
    nodeLabel: skill => {
      const node = byId.get(skill);
      return socketLabels.get(skill) ?? (node ? passiveNodeName(node, nodeNames) : t(lang, 'tree.passive'));
    },
    onPreview: plan => setRoutePreview(plan ? { plan } : null), focusPlanner: focusPlanner?.node === undefined ? focusPlanner : undefined });
  useEffect(() => {
    let cancelled = false;
    getBackend()
      .then(b => b.loadPassiveTree())
      .then(loadedNodes => { if (!cancelled) setNodes(loadedNodes); })
      .catch(err => { if (!cancelled) setError(formatApiError(err)); });
    return () => { cancelled = true; };
  }, []);
  if (!active) return null;
  const activeBuild = session.workspace?.builds.find(build => build.id === session.workspace?.activeBuild);
  const canvasContext = JSON.stringify([activeBuild?.id, activeBuild?.activeStage,
    session.activeLoadout, session.activeWeaponSet, session.socketGroupRevision,
    session.character?.class_name, session.character?.ascendancy_name]);
  return <TreeCanvas key={canvasContext} session={session} lang={lang} settledCalc={settledCalc} onJewelSearch={onJewelSearch}
    consumedLocation={consumedLocation} ascendancyFocus={ascendancyFocus} socketLabels={socketLabels} onSearch={setNameSearch}
    graph={graph} startSkill={startSkill} implicitStarts={implicitStarts} grants={grants} allocated={allocated} pointSummary={pointSummary}
    importCheckReason={importCheck?.reason} focusNode={located && located.nonce > (focusPlanner?.nonce ?? -1) ? located : focusPlanner?.node !== undefined ? focusPlanner : undefined} allocationMode={mode} onAllocationMode={changeMode} nodes={rawNodes ? nodes : null} nodeNames={nodeNames} error={error} routePreview={routePreview} planner={planner} conversion={conversion} overlayReset={overlayReset} />;
}

interface CanvasProps {
  focusNode?: { nonce: number; node?: number };
  consumedLocation: RefObject<number | undefined>;
  ascendancyFocus: RefObject<string | null>;
  allocationMode: TreeAllocationMode;
  onAllocationMode: (mode: TreeAllocationMode) => void;
  session: BuildSession;
  lang: Lang;
  settledCalc?: CalculateBuildResponse | null;
  onJewelSearch?: (socket: number) => void;
  nodes: PassiveNode[] | null;
  nodeNames: PassiveNodeNames;
  socketLabels: ReadonlyMap<number, string>;
  onSearch: (query: string) => void;
  error: string | null;
  routePreview: RoutePreview | null;
  graph: ReturnType<typeof buildPassiveGraph>;
  startSkill: number | null;
  implicitStarts: ReadonlySet<number>;
  grants: readonly PassiveAllocationGrant[];
  allocated: Set<number>;
  pointSummary: ReturnType<typeof passivePointSummary>;
  importCheckReason: string | null | undefined;
  planner: TreeOptimizerState;
  /** The tree-conversion controls, shown in the canvas toolbar. */
  conversion?: ReactNode;
  /** Counts the planner searches started; a new one replaces the heatmap's colours. */
  overlayReset: number;
}

function TreeCanvas({ focusNode, consumedLocation, ascendancyFocus, allocationMode, onAllocationMode, session, lang, settledCalc, onJewelSearch, nodes, nodeNames, socketLabels, onSearch, error, routePreview, graph, startSkill, implicitStarts, grants, allocated, pointSummary, importCheckReason, planner, conversion, overlayReset }: CanvasProps) {
  const plannedNodes = routePreview?.plan ?? null;
  const routeText = messages(lang, 'treeOptimizer');
  const tt = bindT(lang);
  /** Whether the planner / details column beside the canvas is shown. */
  const [sideOpen, setSideOpen] = useState(true);
  const [sideWidth, setSideWidth] = useState(readSideWidth);
  const panelRef = useRef<HTMLElement>(null);
  const commitSideWidth = useCallback((rem: number) => { setSideWidth(rem); writeSideWidth(rem); }, []);
  const { hover, setHover, hoverNode, keepHover, leaveHover, onFocus: focusHover, onBlur: blurHover } = useTreeHover();
  const dialogs = useAppDialogs(lang);
  const captureAction = useSessionAction(session);
  const displayCalc = session.calcError ? null : session.calc ?? settledCalc ?? null;
  const treeEffects = displayCalc?.tree_effects;
  const canvasGeneration = useRef(0);
  useLayoutEffect(() => () => { canvasGeneration.current += 1; }, []);
  const [hoverPos, setHoverPos] = useState<{ x: number; y: number }>({ x: 0, y: 0 });
  const [hoverStats, setHoverStats] = useState<string[] | null>(null);
  /** Localized hovered node name (when matched in dictionary; null = show English name). */
  const [hoverName, setHoverName] = useState<string | null>(null);
  /** Node search query (substring match against name/mods; highlights matching nodes). */
  const [search, setSearch] = useState('');
  const [reportSearch, setReportSearch] = useState('');
  useEffect(() => { onSearch(search.trim() || reportSearch); }, [onSearch, search, reportSearch]);
  useEffect(() => () => onSearch(''), [onSearch]);
  /** Cyclic index for "Next hit" (resets when search query changes). */
  const [hitIndex, setHitIndex] = useState(0);
  const searchRef = useRef<HTMLInputElement>(null);
  const nextHitRef = useRef<HTMLButtonElement>(null);
  const detailsRef = useRef<HTMLDivElement>(null);
  const focusDetailsRef = useRef(false);
  const [selectedSkill, setSelectedSkill] = useState<number | null>(null);
  // A selected node's details, and a planner opened from elsewhere, live in the side column.
  useEffect(() => { if (selectedSkill !== null) setSideOpen(true); }, [selectedSkill]);
  useEffect(() => { if (planner.expanded) setSideOpen(true); }, [planner.expanded]);
  const [attributeDraft, setAttributeDraft] = useState<{
    skill: number; saved?: AttributeChoice; choice: AttributeChoice;
  } | null>(null);
  /** 3-way attribute choice popover (`node` + screen coordinates). */
  const [attrPicker, setAttrPicker] = useState<{ node: PassiveNode; x: number; y: number } | null>(
    null,
  );
  /** Jewel socket editor (active socket node ID + draft text). */
  const toolsRef = useRef<HTMLDivElement>(null);
  const jewelRef = useRef<HTMLDivElement>(null);
  const applyJewelRef = useRef<HTMLButtonElement>(null);
  const focusStagedJewel = useCallback(() => {
    (applyJewelRef.current?.disabled ? jewelRef.current : applyJewelRef.current)?.focus();
  }, []);
  const [jewelSocket, setJewelSocket] = useState<number | null>(() => {
    const key = Object.keys(session.editorDrafts).find(key => key.startsWith('jewel:'));
    return key ? Number(key.slice(6)) : null;
  });
  const jewelDraftKey = `jewel:${jewelSocket}`;
  const socketText = session.jewels.find(jewel => jewel.socket_node === jewelSocket)?.text ?? '';
  const draftText = session.editorDrafts[jewelDraftKey] ?? socketText;
  const jewelEdit = useMemo(() => jewelSocket === null ? null : { socket: jewelSocket, draft: draftText },
    [jewelSocket, draftText, session.stateVersion]);
  const activeJewelEdit = useRef(jewelEdit);
  useLayoutEffect(() => {
    activeJewelEdit.current = jewelEdit;
    return () => { activeJewelEdit.current = null; };
  }, [jewelEdit]);
  const openJewel = useCallback((socket: number) => {
    setSideOpen(true);
    setJewelSocket(socket);
  }, []);
  useEffect(() => {
    if (jewelSocket === null) return;
    jewelRef.current?.focus({ preventScroll: true });
    jewelRef.current?.scrollIntoView({ block: 'nearest' });
  }, [jewelSocket]);
  const closeJewelEditor = (focusSkill = selectedSkill ?? jewelSocket) => {
    focusDetailsRef.current = true;
    setSelectedSkill(focusSkill);
    session.setEditorDraft(jewelDraftKey, null);
    setJewelSocket(null);
  };
  const applySocketJewel = (text: string, focusSkill: number | null) => {
    if (!jewelEdit) return;
    const jewels = session.jewels.filter(jewel => jewel.socket_node !== jewelEdit.socket);
    if (text) jewels.push({ socket_node: jewelEdit.socket, text });
    session.setJewels(jewels, () => {
      if (activeJewelEdit.current === jewelEdit) closeJewelEditor(focusSkill);
    });
  };
  /** Allocate/refund delta for hovered node (debounced recalc, invalidated per `stateVersion`). */
  const [refundReceipt, setRefundReceipt] = useState<RefundReceipt | null>(null);
  const [hoverGain, setHoverGain] = useState<number | null>(null);
  const [hoverCalculating, setHoverCalculating] = useState(false);
  const [hoverDiff, setHoverDiff] = useState<DiffEntry[] | null>(null);
  const [viewBox, setCameraView] = useState<ViewBox | null>(null);
  const [canvasSize, setCanvasSize] = useState({ width: 0, height: 0 });
  const [routeFocus, setRouteFocus] = useState<{ preview: RoutePreview; scope: RouteScope | null }>();
  const routeScope = routePreview && routeFocus?.preview === routePreview ? routeFocus.scope : routePreview && 'all';
  const routeControlsRef = useRef<HTMLDivElement>(null);
  const setViewBox = (view: ViewBox | null) => {
    if (routePreview && routeScope) setRouteFocus({ preview: routePreview, scope: null });
    setCameraView(view);
  };
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
  // Size the SVG in pixels: a percentage size made every layout beside the canvas lay out the whole tree again.
  const attachSvg = useCallback((svg: SVGSVGElement | null) => {
    svgRef.current = svg;
    if (!svg) return;
    const stop = fitToParentInPixels(svg, size => {
      setCanvasSize(prev => prev.width === size.width && prev.height === size.height ? prev : size);
    });
    return () => { stop(); svgRef.current = null; };
  }, []);
  const sceneRef = useRef<SVGGElement | null>(null);
  const dragRafRef = useRef<number | null>(null);
  const suppressClickRef = useRef(false);
  const clickResetRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  /** Coalesced rAF target viewBox for wheel zoom (commits at most once per frame). */
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
    // The listener sits on the canvas, so the drag shield that covers the SVG during a pan consumes it too.
    const target = svg.closest('.tree-canvas') ?? svg;
    const consume = (event: Event) => event.preventDefault();
    target.addEventListener('wheel', consume, { passive: false });
    return () => target.removeEventListener('wheel', consume);
  }, [nodes]);

  // Jewel library (swap dropdown inside socket editor); display names `"Name·Base"` translate in batch.
  // Note: hooks must execute before the early return below (React #310).
  const jewelLib = session.library.items.filter((i) => i.kind === 'jewel');
  const jewelNames = useItemDisplayNames(
    jewelLib.map((e) => e.text),
    lang,
  );
  const allocatedSockets = useMemo(() => [...socketLabels.keys()].filter(id => allocated.has(id)), [socketLabels, allocated]);
  const socketContents = useItemDisplayNames(allocatedSockets.map(id => session.jewels.find(jewel => jewel.socket_node === id)?.text), lang);

  // Heatmap probes run only on an explicit request, in cancellable windows.
  const [heatStat, setHeatStat] = useState('');
  const [heatDepth, setHeatDepth] = useState(5);
  const [heatData, setHeatData] = useState<Map<number, number> | null>(null);
  const [heatBusy, setHeatBusy] = useState(false);
  const [heatComplete, setHeatComplete] = useState(false);
  const [heatError, setHeatError] = useState(false);
  const [reportStat, setReportStat] = useState('');
  /** `stateVersion` when heatmap last ran (build changes show "stale" instead of auto-recomputing). */
  const [heatVersion, setHeatVersion] = useState(-1);
  /** Batch size per worker yield: keeps progress visible without blocking edits behind the full heatmap. */
  const [heatProgress, setHeatProgress] = useState<{ done: number; total: number } | null>(null);
  /** Generation token for the active heatmap run; changing stats/editing/re-running bumps it so old work stops before the next step. */
  const heatRun = useRef(0);
  const heatAbort = useRef<AbortController | null>(null);
  useEffect(() => () => heatAbort.current?.abort(), []);
  const cancelHeat = () => {
    heatRun.current += 1;
    heatAbort.current?.abort(); heatAbort.current = null;
    setHeatBusy(false);
    setHeatProgress(null);
  };
  // Any build edit renders the in-flight heatmap stale: abort it, keeping computed values marked outdated.
  useEffect(() => { cancelHeat(); }, [session.stateVersion]);
  const runHeat = () => {
    if (!heatStat || heatBusy) return;
    const request = session.currentRequest();
    if (!request) return;
    const version = session.stateVersion;
    const generation = canvasGeneration.current;
    const run = ++heatRun.current;
    const controller = new AbortController();
    heatAbort.current = controller;
    const current = () => generation === canvasGeneration.current && run === heatRun.current;
    // The colours and routes the previous calculations painted describe results that are
    // about to be replaced; they must not stay lit while the new one runs.
    setHeatData(null);
    setHeatComplete(false); setHeatError(false);
    setHeatVersion(-1);
    planner.clearPreview();
    setHeatBusy(true);
    setHeatProgress(null);
    void (async () => {
      try {
        const backend = await getBackend();
        const complete = await evaluateHeatmap({ request, stat: heatStat, depth: heatDepth,
          signal: controller.signal, probe: backend.nodePower, onBatch: (powers, done, total) => {
            if (!current()) return;
            setHeatData(powers); setHeatVersion(version); setHeatProgress({ done, total });
          } });
        if (current()) setHeatComplete(complete);
      } catch {
        if (current()) { setHeatData(null); setHeatError(true); }
      } finally {
        if (current()) {
          setHeatBusy(false);
          setHeatProgress(null);
        }
      }
    })();
  };

  // A planner search that starts replaces the heatmap's colours the same way.
  useEffect(() => {
    if (overlayReset === 0) return;
    cancelHeat();
    setHeatData(null);
    setHeatVersion(-1);
  }, [overlayReset]);

  const heatMax = useMemo(() => {
    let max = 0;
    heatData?.forEach((d) => {
      if (d > max) max = d;
    });
    return max;
  }, [heatData]);

  /** Available heatmap stats (filtered to display stats present in the active calc output). */
  const heatStatOptions = useMemo(() => {
    const CANDIDATES = [
      'MainMinionDPS', 'TotalDPS', 'Life', 'EnergyShield', 'TotalEHP', 'Mana',
      'Armour', 'Evasion', 'Spirit',
    ];
    const present = new Set((displayCalc?.stats ?? []).filter(s => Number.isFinite(s.value)).map((s) => s.id));
    return CANDIDATES.filter((id) => present.has(id));
  }, [displayCalc]);

  // Stable ID of active ascendancy (e.g. `Warrior3`). PoB2 semantics: render only the chosen
  // ascendancy's cluster; hide all others (they overlap on the same main-tree plane).
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

  const freeNodes = useMemo(() => allocationAccess(graph, allocated, startSkill, grants).free,
    [graph, allocated, startSkill, grants]);
  /** Search match set (case-insensitive substring over node names + mods after stripping `[a|b]` tags). */
  const searchHits = useMemo(() => {
    const q = search.trim().toLowerCase();
    if (q.length < 2) return null;
    const hits = new Set<number>();
    for (const node of placed) {
      if (passiveNodeMatches(node, q, nodeNames)) hits.add(node.skill);
    }
    return hits;
  }, [placed, search, nodeNames]);
  const [editError, setEditError] = useState<string | null>(null);
  const selectedNode = selectedSkill === null ? null : byId.get(selectedSkill) ?? null;
  const selectedAllocated = selectedSkill !== null && allocated.has(selectedSkill);
  const editRequest = useMemo(() => session.currentRequest(), [session.currentRequest]);
  const selectedPreview = useTreeEditPreview(editRequest, selectedSkill, allocationMode,
    selectedAllocated ? 'refund' : 'allocate');
  const hoverPreview = useTreeEditPreview(editRequest, hover?.skill ?? null, allocationMode,
    hover && allocated.has(hover.skill) ? 'refund' : 'allocate');
  const importProblem = needsTreeConversion(importCheckReason) || needsTreeConversion(editError);
  const disconnectedImport = useMemo(() => hasDisconnectedTreeNodes(nodes ?? [], graph, startSkill, grants,
    session.allocatedNodes, session.weaponSwap), [nodes, graph, startSkill, grants, session.allocatedNodes, session.weaponSwap])
    || isDisconnectedTree(importCheckReason) || isDisconnectedTree(editError);
  useLayoutEffect(() => { setEditError(null); }, [session.stateVersion]);
  const selectedPath = selectedPreview?.result?.added ?? [];
  const previewNode = hover ?? (jewelSocket === null ? selectedNode : null);
  const opensJewelEditor = previewNode?.kind === 'jewel_socket' && allocated.has(previewNode.skill);
  const preview = previewNode && !opensJewelEditor ? (hover ? hoverPreview : selectedPreview) : null;
  const previewResult = preview?.result;
  const previewNodes = useMemo(() => ({
    added: new Set(previewResult?.added), removed: new Set(previewResult?.removed),
    promoted: new Set(previewResult?.promoted), path: new Set(previewResult?.path),
  }), [previewResult]);
  const owner = useMemo(() => new Map((session.weaponSwap?.exclusive_nodes ?? [[], []])
    .flatMap((ids, index) => ids.map(id => [id, index + 1] as const))), [session.weaponSwap]);
  const editPending = useRef(false);
  const modeRef = useRef(allocationMode);
  modeRef.current = allocationMode;
  const selectedNeedsAttribute = Boolean(selectedNode && !selectedAllocated
    && isAttrNode(selectedNode) && selectedPath.length === 1);
  const selectedChoice = selectedSkill === null ? undefined : session.attributeChoices[String(selectedSkill)];
  const pendingAttribute = attributeDraft && attributeDraft.skill === selectedSkill && attributeDraft.saved === selectedChoice
    ? attributeDraft.choice : undefined;
  const selectedAttributeChoices = useMemo(() => !selectedAllocated && pendingAttribute && selectedSkill !== null
    ? { [String(selectedSkill)]: pendingAttribute } : undefined, [selectedAllocated, pendingAttribute, selectedSkill]);
  const selectedRefundCount = selectedPreview?.result?.removed.length ?? 0;
  const detailsLines = useMemo(() => selectedNode ? [
    selectedNode.name ?? selectedNode.id,
    ...(selectedNode.stats ?? []).map(cleanLine),
  ] : [], [selectedNode]);
  const detailsTextKey = useMemo(() => JSON.stringify([selectedSkill, lang, detailsLines]), [selectedSkill, lang, detailsLines]);
  const [translatedDetails, setTranslatedDetails] = useState<{ key: string; lines: string[] } | null>(null);
  const displayedDetails = lang !== 'en-US' && translatedDetails?.key === detailsTextKey
    ? translatedDetails.lines : detailsLines;

  useEffect(() => {
    if (!selectedNode || lang === 'en-US') return;
    let cancelled = false;
    const generation = canvasGeneration.current;
    getBackend()
      .then(backend => !cancelled && generation === canvasGeneration.current
        ? translatePassiveCard(backend, selectedNode, detailsLines.slice(1), lang) : null)
      .then(translated => {
        if (!translated || cancelled || generation !== canvasGeneration.current) return;
        setTranslatedDetails({ key: detailsTextKey, lines: translated });
      })
      .catch(() => {});
    return () => { cancelled = true; };
  }, [selectedNode, lang, detailsLines, detailsTextKey]);

  useLayoutEffect(() => {
    if (!focusDetailsRef.current || !selectedNode) return;
    focusDetailsRef.current = false;
    detailsRef.current?.focus({ preventScroll: true });
    detailsRef.current?.scrollIntoView({ block: 'nearest' });
  }, [selectedNode, hitIndex, jewelSocket]);

  const closeDetails = () => {
    setSelectedSkill(null);
    setAttributeDraft(null);
    setHover(null);
    focusDetailsRef.current = false;
    (nextHitRef.current ?? searchRef.current)?.focus({ preventScroll: true });
  };

  /** Allocated travel attribute nodes (ascending ID order for deterministic bulk redistribution). */
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

  const distributeAttributes = (str: number, dex: number, int: number) => {
    session.setAttributeChoices(distributeAttributeChoices(allocatedAttrNodes, session.attributeChoices, { str, dex, int }));
  };

  const edges = useMemo(() => passiveTreeEdges(placed).map(edge => ({ ...edge,
    active: allocated.has(edge.from) && allocated.has(edge.to),
  })), [placed, allocated]);

  // ── Rendering perf: memoize node/edge subtrees ───────────────────────────
  // Pan, zoom, and hover call setState every frame; without memoizing these subtrees React diffs
  // nearly ten thousand SVG elements per frame (root cause of drag stutter). With memo, frames update
  // only the SVG viewBox and tooltip. Click handlers read the latest session via `sessionRef` to avoid stale closures.
  const sessionRef = useRef(session);
  sessionRef.current = session;

  /** Every manual allocation uses the same authoritative path and budget check. */
  const actOnNode = useCallback(async (skill: number,
    action: 'allocate' | 'refund' | 'promote' | 'attribute' | 'jewel', choice?: AttributeChoice) => {
    const s = sessionRef.current;
    const node = byId.get(skill);
    if (s.busy || editPending.current || !node || importProblem) return;
    const isAlloc = s.allocatedNodes.includes(skill);
    if (action === 'attribute' && (!isAttrNode(node) || !choice)) return;
    if (action === 'attribute' && isAlloc) {
      s.setAttributeChoices({ ...s.attributeChoices, [String(skill)]: choice! });
      setAttributeDraft(null);
      return;
    }
    if (action === 'jewel' && node.kind !== 'jewel_socket') return;
    if (action === 'jewel' && isAlloc) { openJewel(skill); return; }
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
      if (action === 'refund') setRefundReceipt({ version: s.stateVersion + 1, points: response.result.removed.length, baseline: s.calc });
      else setRefundReceipt(null);
      if (jewelSocket !== null && response.result.removed.includes(jewelSocket)) {
        focusDetailsRef.current = true;
        setSelectedSkill(current => current ?? jewelSocket);
        s.setEditorDraft(`jewel:${jewelSocket}`, null); setJewelSocket(null);
      }
      setAttrPicker(null);
      setAttributeDraft(null);
      if (action !== 'refund' && node.kind === 'jewel_socket') openJewel(skill);
    } catch { setEditError('backend_unavailable'); }
    finally { editPending.current = false; }
  }, [byId, allocationMode, jewelSocket, importProblem, openJewel]);

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

  const plannedEdge = useMemo(() => plannedEdgeClass(plannedNodes, allocated), [plannedNodes, allocated]);

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
            className={`${e.active ? 'edge-active' : 'edge'}${plannedEdge(e.from, e.to)}`}
          />
        ))}
      </g>
    ),
    [edges, plannedEdge],
  );

  const previewEdgesEl = useMemo(() => {
    if (previewNodes.path.size === 0 && previewNodes.removed.size === 0) return null;
    const lines: ReactNode[] = [];
    for (let i = 0; i < edges.length; i++) {
      const e = edges[i];
      const isPreview = previewNodes.path.has(e.from) && previewNodes.path.has(e.to);
      const isRefund = previewNodes.removed.has(e.from) && previewNodes.removed.has(e.to);
      if (!isPreview && !isRefund) continue;
      lines.push(
        <line
          key={i}
          x1={e.x1}
          y1={e.y1}
          x2={e.x2}
          y2={e.y2}
          className={`${isPreview ? 'edge-preview' : ''}${isPreview && isRefund ? ' ' : ''}${isRefund ? 'edge-refund' : ''}`}
        />,
      );
    }
    return lines.length > 0 ? <g className="tree-edges tree-preview-edges">{lines}</g> : null;
  }, [edges, previewNodes]);

  const filledJewelSockets = useMemo(
    () => new Set(session.jewels.map((j) => j.socket_node)),
    [session.jewels],
  );

  // A node just allocated rings once. Not on the first render, and not when a whole build arrives at once.
  const previousAllocated = useRef<Set<number> | null>(null);
  const [pulse, setPulse] = useState<ReadonlySet<number>>(() => new Set());
  useEffect(() => {
    const before = previousAllocated.current;
    previousAllocated.current = allocated;
    if (!before) return;
    const added = [...allocated].filter(id => !before.has(id));
    if (added.length === 0 || added.length > 40) { setPulse(new Set()); return; }
    setPulse(new Set(added));
    const timer = setTimeout(() => setPulse(new Set()), 800);
    return () => clearTimeout(timer);
  }, [allocated]);

  const nodesGroupRef = useRef<SVGGElement | null>(null);
  const circlesBySkillRef = useRef<Map<number, SVGCircleElement>>(new Map());
  const previewHighlightedRef = useRef<SVGCircleElement[]>([]);

  const nodesEl = useMemo(() => {
    /** Heatmap fill: positive delta goes dark red -> bright gold (`sqrt` separates small gains); negative = cool blue; empty = unstyled. */
    const heatFill = (skill: number): string | undefined => {
      if (!heatStat || !heatData || allocated.has(skill)) return undefined;
      const delta = heatData.get(skill);
      if (delta === undefined || delta === 0) return undefined;
      if (delta < 0) return 'hsl(210 55% 32%)';
      const t = Math.sqrt(Math.min(1, delta / Math.max(1, heatMax)));
      return `hsl(${Math.round(20 + t * 25)} ${Math.round(70 + t * 30)}% ${Math.round(28 + t * 32)}%)`;
    };
    return (
      <g ref={nodesGroupRef} className="tree-nodes">
        {placed.map((node) => {
          const r = NODE_RADIUS[node.kind] ?? 40;
          const isAlloc = allocated.has(node.skill);
          const heat = heatFill(node.skill);
          return (
            <g key={node.skill}>
              <circle
                data-skill-id={node.skill}
                data-weapon-set={owner.get(node.skill) ?? (isAlloc ? 0 : undefined)}
                cx={node.x}
                cy={node.y}
                r={r}
                style={heat ? { fill: heat } : undefined}
                className={`node node-${node.kind}${node.ascendancy_id ? ' node-asc' : ''}${isAlloc ? ' node-allocated' : ''}${node.kind === 'jewel_socket' && filledJewelSockets.has(node.skill) ? ' node-jewel-filled' : ''}${searchHits?.has(node.skill) ? ' node-search-hit' : ''}${selectedSkill === node.skill ? ' node-selected' : ''}${freeNodes.has(node.skill) ? ' node-jewel-allocatable' : ''}${treeEffects?.nodes[String(node.skill)] ? ' node-transformed' : ''}${plannedNodes?.allocate.includes(node.skill) ? ' node-planned-add' : ''}${plannedNodes?.deallocate.includes(node.skill) ? ' node-planned-remove' : ''}${owner.has(node.skill) ? ` node-weapon-${owner.get(node.skill)}` : ''}`}
                onPointerEnter={(e) => {
                  if (dragRef.current?.moved) return;
                  hoverNode(node);
                  setHoverPos({ x: e.clientX, y: e.clientY });
                }}
                onPointerMove={(e) => {
                  // Pointer capture keeps sending events to the original node,
                  // even while dragging disables hit testing for other nodes.
                  if (!dragRef.current) setHoverPos({ x: e.clientX, y: e.clientY });
                }}
                onPointerLeave={leaveHover}
                onClick={(e) => handleNodeClick(node, e)}
              />
              {pulse.has(node.skill) && <circle className="node-pulse" cx={node.x} cy={node.y} r={r} />}
            </g>
          );
        })}
      </g>
    );
  }, [placed, allocated, pulse, filledJewelSockets, searchHits, selectedSkill, heatStat, heatData, heatMax, handleNodeClick, plannedNodes, freeNodes, treeEffects, owner]);

  const fullExtent = useMemo(() => treeExtent(placed), [placed]);

  const routeNodes = useMemo(() => ({
    add: placedRouteNodes(plannedNodes?.allocate ?? [], byId),
    remove: placedRouteNodes(plannedNodes?.deallocate ?? [], byId),
  }), [plannedNodes, byId]);

  useLayoutEffect(() => {
    const svg = svgRef.current;
    if (!routeScope || !svg) return;
    const { top, bottom, width, height } = svg.getBoundingClientRect();
    let topInset = 0, bottomInset = 0;
    const obstructions = panelRef.current?.querySelectorAll('.tree-bar-top, .tree-bar-view, .tree-side-toggle, .tree-edit-error, .asc-hint, .tree-route-preview');
    for (const obstruction of obstructions ?? []) {
      const rect = obstruction.getBoundingClientRect();
      if (rect.bottom <= top || rect.top >= bottom) continue;
      if (rect.top < top + height / 2) topInset = Math.max(topInset, rect.bottom - top);
      else bottomInset = Math.max(bottomInset, bottom - rect.top);
    }
    const nodes = routeScope === 'all' ? [...routeNodes.add, ...routeNodes.remove] : routeNodes[routeScope];
    const view = routeViewBox(nodes, width, height, topInset, bottomInset);
    if (view) {
      setHover(null);
      setCameraView(view);
    }
  }, [routePreview, routeScope, routeNodes, canvasSize]);

  useEffect(() => {
    if (!routePreview) return;
    const frame = requestAnimationFrame(() => {
      const canvas = svgRef.current?.closest('.tree-canvas');
      const rect = canvas?.getBoundingClientRect();
      const screen = canvas?.closest('.shell-main')?.getBoundingClientRect();
      if (rect && (rect.top < (screen?.top ?? 0) - 1 || rect.bottom > (screen?.bottom ?? window.innerHeight) + 1)) {
        canvas?.scrollIntoView({ block: 'center', behavior: matchMedia('(prefers-reduced-motion: reduce)').matches ? 'instant' : 'smooth' });
        routeControlsRef.current?.focus({ preventScroll: true });
      } else {
        const card = planner.containerRef.current?.querySelector('.is-previewed');
        card?.scrollIntoView({ block: 'nearest', inline: 'nearest' });
      }
    });
    return () => cancelAnimationFrame(frame);
  }, [routePreview, planner.containerRef]);

  // In Chinese UI, batch-translate hovered node name and mod lines via native catalog + mod templates
  // (async; verify the hover target still matches when late responses arrive).
  useEffect(() => {
    if (!hover) {
      setHoverStats(null);
      setHoverName(null);
      return;
    }
    const raw = (hover.stats ?? []).map(cleanLine);
    if (lang === 'en-US') {
      setHoverStats(raw);
      setHoverName(null);
      return;
    }
    const name = hover.name ?? hover.id;
    let cancelled = false;
    const generation = canvasGeneration.current;
    getBackend()
      .then(b => !cancelled && generation === canvasGeneration.current ? translatePassiveCard(b, hover, raw, lang) : null)
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

  useEffect(() => {
    const selected = ascendancyFocus.current;
    if (!selected) return;
    if (selected !== session.character?.ascendancy_name) {
      ascendancyFocus.current = null;
      return;
    }
    if (!ascExtent) return;
    ascendancyFocus.current = null;
    setViewBox(ascExtent);
  }, [ascendancyFocus, ascExtent, session.character?.ascendancy_name]);

  // Preview the full validated edit, including both weapon sets and dependent refunds.
  useEffect(() => {
    setHoverDiff(null); setHoverGain(null); setHoverCalculating(false);
    if (!hover || !hoverPreview?.result || session.busy || !session.calc || !editRequest) return;
    let cancelled = false;
    setHoverCalculating(true);
    const timer = setTimeout(() => {
      getBackend().then(backend => backend.calculateBuild({ ...treeAllocationRequest(editRequest, hoverPreview.result!.allocation), breakdowns: [] }))
        .then(next => {
          if (cancelled) return;
          setHoverDiff(diffStats(session.calc!.stats, next.stats));
          setHoverGain(treeStatDelta(session.calc, next, heatStat));
        }).catch(() => {}).finally(() => { if (!cancelled) setHoverCalculating(false); });
    }, 300);
    return () => { cancelled = true; clearTimeout(timer); };
  }, [hover, hoverPreview, session.stateVersion, session.busy, heatStat]);

  // Hotkeys S/D/I (or 1/2/3): select inside popover when open; directly allocate or switch hovered travel attribute node.
  useLayoutEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const target = e.target as HTMLElement | null;
      if (target && ['INPUT', 'TEXTAREA', 'SELECT'].includes(target.tagName)) return;
      // Hover shortcuts belong to the canvas (or the page body when nothing is focused), not tools.
      if (target && target !== document.body && !target.closest('.tree-canvas')) return;
      const choice = ATTRIBUTE_KEYMAP[e.key.toLowerCase()];
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
    // Capturing, so that the key is handled (and marked so) before the shell reads 1/2/3 as page numbers.
    window.addEventListener('keydown', onKey, true);
    return () => window.removeEventListener('keydown', onKey, true);
  }, [attrPicker, hover, actOnNode]);

  const view = viewBox ?? fullExtent;
  const hasCanvas = Boolean(!error && nodes && view);

  useLayoutEffect(() => {
    const map = new Map<number, SVGCircleElement>();
    const group = nodesGroupRef.current;
    if (group) {
      for (const circle of group.querySelectorAll<SVGCircleElement>('circle[data-skill-id]')) {
        map.set(Number(circle.dataset.skillId), circle);
      }
    }
    circlesBySkillRef.current = map;
    return () => {
      circlesBySkillRef.current.clear();
      previewHighlightedRef.current = [];
    };
  }, [placed, hasCanvas]);

  useLayoutEffect(() => {
    for (const el of previewHighlightedRef.current) {
      el.classList.remove('node-preview-add', 'node-preview-promote', 'node-preview-refund');
    }
    const next: SVGCircleElement[] = [];
    const seen = new Set<SVGCircleElement>();
    const mark = (skills: ReadonlySet<number>, cls: string) => {
      for (const skill of skills) {
        const el = circlesBySkillRef.current.get(skill)
          ?? nodesGroupRef.current?.querySelector<SVGCircleElement>(`circle[data-skill-id="${skill}"]`);
        if (!el) continue;
        el.classList.add(cls);
        if (!seen.has(el)) {
          seen.add(el);
          next.push(el);
        }
      }
    };
    mark(previewNodes.added, 'node-preview-add');
    mark(previewNodes.promoted, 'node-preview-promote');
    mark(previewNodes.removed, 'node-preview-refund');
    previewHighlightedRef.current = next;
  }, [previewNodes, nodesEl, hasCanvas]);

  const focusNodeOnTree = (node: PassiveNode) => {
    if (!view) return;
    setSelectedSkill(node.skill); setAttributeDraft(null); setAttrPicker(null);
    const w = Math.min(view.w, 6000), h = w / view.w * view.h;
    setViewBox({ x: node.x! - w / 2, y: node.y! - h / 2, w, h });
    svgRef.current?.scrollIntoView({ block: 'center' });
    svgRef.current?.focus({ preventScroll: true });
  };
  const ascendancyPoints = useMemo(() => {
    const shared = previewResult?.allocation.shared ?? [...allocated].filter(id => !owner.has(id));
    if (shared.some(id => !byId.has(id))) return null;
    return shared.filter(id => {
      const node = byId.get(id);
      return node?.ascendancy_id && node.kind !== 'ascendancy_start' && !implicitStarts.has(id);
    }).length;
  }, [previewResult, allocated, owner, byId, implicitStarts]);
  useTreeLocation(focusNode, consumedLocation,
    focusNode?.node === undefined ? undefined : byId.get(focusNode.node), Boolean(view), focusNodeOnTree);
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

  const onPointerMove = (e: React.PointerEvent<Element>) => {
    const drag = dragRef.current;
    const svg = svgRef.current;
    if (!drag || !svg || e.pointerId !== drag.pointerId) return;
    const dx = e.clientX - drag.startX;
    const dy = e.clientY - drag.startY;
    if (!drag.moved && Math.abs(dx) + Math.abs(dy) > 3) {
      drag.moved = true;
      // A confirmed drag shows .tree-drag-shield over the SVG, so moves no longer hit-test the
      // ~4900 circles. The circles then get no leave event, so clear the hover tooltip here.
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

  const finishDrag = (e: React.PointerEvent<Element>, cancelled = false) => {
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

  const onPanelKeyDown = (e: React.KeyboardEvent) => {
    if (e.key !== 'Escape' || (selectedSkill === null && !attrPicker)) return;
    e.preventDefault();
    setAttrPicker(null);
    closeDetails();
  };
  const modeEl = (
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
  );
  const searchEl = (
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
                // Pan to matched node while preserving zoom (narrowing only when zoomed too far out to see clusters).
                const w = Math.min(view.w, 14000);
                const h = (w / view.w) * view.h;
                setViewBox({ x: node.x! - w / 2, y: node.y! - h / 2, w, h });
              }}
            >
              {tt('tree.nextHit')}
            </AppButton>
          )}
        </span>
  );
  const heatEl = (
    <span className="tree-heat">
          <AppTooltip content={tt('tree.heatHint')}><span tabIndex={0}>{tt('tree.heat')}</span></AppTooltip>
          <AppSelectControl
            value={heatStat}
            aria-label={tt('tree.heat')}
            onChange={(e) => {
              cancelHeat();
              setHeatStat(e.target.value);
              setHeatData(null);
              setHeatVersion(-1);
            }}
          >
            <option value="">{tt('tree.heatOff')}</option>
            {heatStatOptions.map((id) => (
              <option key={id} value={id}>
                {treeStatLabel(lang, id)}
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
                    if (Number.isInteger(v) && v >= 1 && v <= 12) { cancelHeat(); setHeatDepth(v); setHeatData(null); setHeatComplete(false); }
                  }}
                />
              </label>
              <AppButton disabled={heatBusy || session.busy} onClick={runHeat}>
                {heatBusy
                  ? `${tt('tree.heatComputing')}${heatProgress ? ` ${heatProgress.done}/${heatProgress.total}` : ''}`
                  : tt('tree.heatRun')}
              </AppButton>
              {!heatBusy && !heatData && <span>{tt('tree.heatReady')}</span>}
              {heatBusy && <progress aria-label={tt('tree.heat')} value={heatProgress?.done} max={heatProgress?.total} />}
              {heatData && !heatComplete && <span role="status">{tt('tree.heatPartial')}</span>}
              {heatError && <span role="alert">{tt('tree.heatError')}</span>}
              <details className="tree-heat-legend"><summary className="disclosure">{treeStatLabel(lang, heatStat)}<span className="tree-heat-scale" aria-hidden="true" /></summary><p>{tt('tree.heatLegend').replace('{stat}', treeStatLabel(lang, heatStat))}</p></details>
              {heatBusy && <AppButton onClick={cancelHeat}>{tt('tree.heatCancel')}</AppButton>}
              {heatData && heatVersion !== session.stateVersion && (
                <span className="tree-hint">{tt('tree.heatStale')}</span>
              )}
            </>
          )}
        </span>
  );
  const classEl = (
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
                {classDisplayName(session.classNames, c.name, lang, 'classes')}
              </option>
            ))}
          </AppSelectControl>
        </label>
  );
  const ascEl = (
    <label className="tree-asc-picker">
          {tt('tree.ascendancy')}
          <AppSelectControl
            value={session.character?.ascendancy_name ?? ''}
            disabled={session.busy}
            onChange={event => {
              const nextAscendancy = event.target.value;
              ascendancyFocus.current = nextAscendancy || null;
              session.setCharacter({ ascendancy_name: nextAscendancy });
            }}
          >
            <option value="">{tt('build.none')}</option>
            {(session.treeMeta?.classes ?? [])
              .find((c) => c.name === session.character?.class_name)
              ?.ascendancies?.map((a) => (
                <option key={a.id} value={a.name}>
                  {classDisplayName(session.classNames, a.name, lang, 'ascendancies')}
                </option>
              ))}
          </AppSelectControl>
        </label>
  );
  const focusAscEl = ascExtent && (
    <AppButton onClick={() => setViewBox(ascExtent)}>{tt('tree.focusAsc')}</AppButton>
  );
  const resetEl = (
    <AppButton onClick={() => setViewBox(null)}>{tt('tree.reset')}</AppButton>
  );
  const countEl = <TreePointSummary current={pointSummary} preview={preview} ascendancy={ascendancyPoints}
    invalidReason={isImportProblem(preview?.reason) ? '' : treeEditError(preview?.reason ?? null, lang)} lang={lang} />;
  const routeScale = Math.min(canvasSize.width / view.w, canvasSize.height / view.h);
  const canvasEl = (
    // Drag moves and ends are handled on the canvas: once the shield covers the SVG, or if the captured
    // node is removed mid-drag, the next pointer events reach the shield instead of the SVG.
    <div className="tree-canvas" data-route-scope={routePreview ? routeScope ?? 'all' : undefined}
      onPointerMove={onPointerMove}
      onPointerUp={e => finishDrag(e)}
      onPointerCancel={e => finishDrag(e, true)}
      onLostPointerCapture={e => finishDrag(e, true)}>
        <svg
          ref={attachSvg}
          tabIndex={-1}
          viewBox={`${view.x} ${view.y} ${view.w} ${view.h}`}
          onWheel={onWheel}
          onPointerDown={onPointerDown}
          role="img"
          aria-label={tt('tree.title')}
        >
          <g ref={sceneRef} className="tree-scene">
          {ascExtent && (
            <circle
              className="asc-backdrop"
              cx={ascExtent.x + ascExtent.w / 2}
              cy={ascExtent.y + ascExtent.h / 2}
              r={Math.max(ascExtent.w, ascExtent.h) / 2}
            />
          )}
          {(treeEffects?.rings ?? []).filter(ring => ring.source === (jewelSocket ?? selectedSkill)).map(ring => {
            const center = byId.get(ring.center);
            if (center?.x === undefined || center.y === undefined) return null;
            const circle = (r: number) => `M ${center.x! + r} ${center.y} a ${r} ${r} 0 1 0 ${-2 * r} 0 a ${r} ${r} 0 1 0 ${2 * r} 0`;
            return <path key={`${ring.source}:${ring.center}`} className="jewel-radius" data-jewel-source={ring.source}
              fillRule="evenodd" d={`${circle(ring.outer)} ${ring.inner > 0 ? circle(ring.inner) : ''}`} />;
          })}
          {edgesEl}
          {previewEdgesEl}
          {nodesEl}
          {plannedNodes && routeScale > 0 && <g className="tree-route-markers" aria-hidden="true">
            {(['add', 'remove'] as const).flatMap(scope => routeNodes[scope].map(node =>
              <g key={node.skill} data-route-node={node.skill} data-change={scope}
                transform={`translate(${node.x} ${node.y}) scale(${1 / routeScale})`}>
                <circle r="9" />
                <path d={scope === 'add' ? 'M -4 0 H 4 M 0 -4 V 4' : 'M -4 0 H 4'} />
              </g>))}
          </g>}
          </g>
        </svg>
        {routePreview && (routeNodes.add.length > 0 || routeNodes.remove.length > 0) && <div className="tree-route-preview" ref={routeControlsRef} role="group" tabIndex={-1} aria-label={routeText.routePreview}>
          <strong>{routeText.routePreview}</strong>
          <div className="tree-route-scope">
            {(['all', 'add', 'remove'] as const).filter(scope => scope === 'all' || routeNodes[scope].length > 0).map(scope =>
              <AppButton key={scope} aria-pressed={routeScope === scope} data-scope={scope}
                onClick={() => setRouteFocus({ preview: routePreview, scope })}>
                {scope === 'all' ? routeText.focusAll : <>
                  <span aria-hidden="true">{scope === 'add' ? '+' : '−'}</span>
                  {(scope === 'add' ? routeText.addPoints : routeText.refundCount).replace('{count}', String(routeNodes[scope].length))}
                </>}
              </AppButton>)}
          </div>
          <AppButton className="tree-route-back" onClick={() => {
            const trigger = planner.containerRef.current?.querySelector<HTMLButtonElement>('.is-previewed [data-route-preview-trigger]');
            trigger?.scrollIntoView({ block: 'center' });
            trigger?.focus({ preventScroll: true });
          }}>{routeText.backToPlan}</AppButton>
          <AppButton className="tree-route-clear" onClick={() => { svgRef.current?.focus({ preventScroll: true }); planner.clearPreview(); }}>{routeText.clear}</AppButton>
        </div>}
        <div className="tree-drag-shield" aria-hidden="true" />
        {hover && !attrPicker && (
          <TreeTooltip
            node={hover}
            name={hoverName}
            stats={treeHoverLines(hoverStats ?? [], session.attributeChoices[String(hover.skill)], lang)}
            pos={hoverPos}
            canvasRef={svgRef}
            onPointerEnter={keepHover}
            onPointerLeave={leaveHover}
            onFocus={focusHover}
            onBlur={blurHover}
            benefit={(
                <div className="tooltip-benefit">
                  <span className="tooltip-benefit-title">
                    {allocated.has(hover.skill) ? tt('diff.ifDealloc') : tt('diff.ifAlloc')}
                  </span>
                  {allocated.has(hover.skill) && !hoverDiff && (hoverPreview?.result?.removed.length ?? 0) > 1 && <p>{tt('tree.downstream').replace('{count}', String(hoverPreview!.result!.removed.length - 1))}</p>}
                  {heatStat && !allocated.has(hover.skill) && <p>{tt('tree.pathGain').replace('{stat}', treeStatLabel(lang, heatStat)).replace('{value}',
                    hoverCalculating || !hoverPreview ? tt('tree.deltaPending') : hoverGain !== null ? powerGain(hoverGain, heatStat, lang) : tt('tree.deltaUnavailable'))}</p>}
                  {hoverDiff && <TreeHoverBenefits diffs={hoverDiff} lang={lang} refundCount={hoverPreview?.result?.removed.length} />}
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
        {!routePreview && !currentAscId &&
          ((session.treeMeta?.classes ?? []).find(
            (c) => c.name === session.character?.class_name,
          )?.ascendancies?.length ?? 0) > 0 && (
            <div className="asc-hint">{tt('tree.pickAscHint')}</div>
          )}
      </div>
  );
  const detailsEl = selectedNode && (
        <div className="tree-node-details" ref={detailsRef} tabIndex={-1}
          role="group" aria-label={tt('tree.nodeDetails')} data-skill-id={selectedNode.skill}>
          <header className="tree-node-details-header">
            <h2>{socketLabels.get(selectedNode.skill) ?? displayedDetails[0]}</h2>
            <AppButton onClick={closeDetails}>{tt('tree.closeDetails')}</AppButton>
          </header>
          <p className="tree-node-details-state">
            #{selectedNode.skill} · {tt(selectedAllocated ? 'tree.allocated' : 'tree.notAllocated')}
            {(selectedAllocated || owner.has(selectedNode.skill)) && <> · {tt(owner.get(selectedNode.skill) === 1 ? 'tree.set1' : owner.get(selectedNode.skill) === 2 ? 'tree.set2' : 'tree.shared')}</>}
          </p>
          {selectedAllocated && selectedNode.kind === 'jewel_socket' && (
            <p role="status">{socketContents[allocatedSockets.indexOf(selectedNode.skill)] || tt('tree.emptySocket')}</p>
          )}
          <ul className="tree-node-details-stats">
            {displayedDetails.slice(1).map((line, index) => <li key={index}><ValueText text={line} /></li>)}
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
          <TreeNodeChanges request={editRequest} preview={selectedPreview} baseline={session.calc}
            attributeChoices={selectedAttributeChoices}
            busy={session.busy} allocated={Boolean(selectedAllocated)} lang={lang} />
        </div>
      );
  const attrEl = (
    <div className="attr-distribute" role="group" aria-label={tt('tree.attrDistribute')}>
        <h3 className="tree-attributes-heading">{tt('tree.attributesHeading')}</h3>
        {allocatedAttrNodes.length > 0 && (
        <>
          <span className="attr-distribute-title">
            {tt('tree.attrSummary').replace('{count}', String(allocatedAttrNodes.length))}
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
                  // Trim the other two attributes on overflow so total stays <= allocated travel nodes.
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
            {tt('tree.attrUnassigned')}{colon(lang)}{allocatedAttrNodes.length - attrCounts.str - attrCounts.dex - attrCounts.int}
          </span>
          <details className="tree-shortcuts"><summary className="disclosure">{tt('tree.keyboardShortcuts')}</summary><p className="tree-hint">{tt('tree.attrHotkeys')}</p></details>
        </>
        )}
          <span className="attr-quest" role="group" aria-label={tt('tree.questAttr')}>
            <span>{tt('tree.questAttr')}</span>
            <span className="attr-quest-options">
            {(
              [
                ['str', "questAct 4Halls Of The DeadNgamahu's Test", '+5 to Strength'],
                ['dex', "questAct 4Halls Of The DeadTawhoa's Test", '+5 to Dexterity'],
                ['int', "questAct 4Halls Of The DeadTasalio's Test", '+5 to Intelligence'],
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
          </span>
      </div>
  );
  const warnEl = Boolean(treeEffects?.warnings.length) && <p className="tree-hint" role="status">{tt('tree.jewelSeedMissing')}</p>;
  const powerEl = (
    <NodePowerTable session={session} nodes={nodes ?? []} nodeNames={nodeNames} lang={lang}
        stat={heatStatOptions.includes(reportStat) ? reportStat : defaultReportStat(planner.goal.preset, heatStatOptions)} stats={heatStatOptions}
        onStat={setReportStat}
        onSearch={setReportSearch} visible={sideOpen}
        onFocus={focusNodeOnTree} />
  );
  const draftsEl = Object.keys(session.editorDrafts).filter(key => key.startsWith('jewel:')).map(key => (
        <AppButton key={key} onClick={() => openJewel(Number(key.slice(6)))}>
          {tt('editor.draft')} · {socketLabels.get(Number(key.slice(6))) ?? tt('tree.jewel')}
        </AppButton>
      ));
  const jewelEl = jewelEdit && (
        <div className="jewel-editor" ref={jewelRef} tabIndex={-1} role="group" aria-label={tt('tree.jewel')}>
          <header className="item-detail-header">
            <span className="item-slot">
              {socketLabels.get(jewelEdit.socket) ?? tt('tree.jewel')}
            </span>
            <span className="item-actions">
              <AppButton
                ref={applyJewelRef}
                disabled={session.busy || !allocated.has(jewelEdit.socket) || jewelEdit.draft === socketText
                  || (jewelEdit.draft !== '' && !jewelEdit.draft.trim())}
                onClick={() => applySocketJewel(jewelEdit.draft, jewelEdit.socket)}
              >
                {tt('items.apply')}
              </AppButton>
              {session.jewels.some((j) => j.socket_node === jewelEdit.socket) && (
                <AppButton
                  className="skill-remove"
                  disabled={session.busy}
                  onClick={() => applySocketJewel('', selectedSkill ?? jewelEdit.socket)}
                >
                  {tt('items.remove')}
                </AppButton>
              )}
              <AppButton
                disabled={session.busy}
                onClick={() => actOnNode(jewelEdit.socket, 'refund')}
              >
                {tt('tree.unallocSocket')}
              </AppButton>
              <AppButton
                disabled={session.busy || !jewelEdit.draft.trim()}
                onClick={() => session.saveLibraryItem('jewel', jewelEdit.draft)}
              >
                {tt('lib.save')}
              </AppButton>
              <AppButton disabled={session.busy} onClick={() => closeJewelEditor()}>{tt('items.cancel')}</AppButton>
            </span>
          </header>
          <p className="tree-hint">{tt('tree.jewelHint')}</p>
          <JewelDraftSources key={jewelEdit.socket} session={session} lang={lang}
            onDraft={text => {
              if (activeJewelEdit.current !== jewelEdit) return false;
              session.setEditorDraft(jewelDraftKey, text === socketText ? null : text);
              return true;
            }}
            onDraftReady={focusStagedJewel} />
          <NoteEditor
            value={session.annotations[`jewel:${jewelEdit.socket}`] ?? ''}
            onCommit={(text) => session.setAnnotation(`jewel:${jewelEdit.socket}`, text)}
            lang={lang}
          />
          {jewelLib.length > 0 && (
            <div className="jewel-library">
              {(() => {
                const value = jewelLib.find(entry => entry.text === jewelEdit.draft)?.id
                  ?? (!jewelEdit.draft ? '' : jewelEdit.draft === socketText ? '__current' : '__draft');
                return (
                  <AppSelect
                    ariaLabel={tt('items.switcher')}
                    value={value}
                    disabled={session.busy}
                    options={[
                      ...(value === '__draft' ? [{ value, label: tt('editor.draft') }] : []),
                      ...(socketText && !jewelLib.some((e) => e.text === socketText)
                        ? [{ value: '__current', label: tt('items.currentItem') }]
                        : []),
                      { value: '', label: tt('items.unequip') },
                      ...jewelLib.map((entry, i) => ({
                        value: entry.id,
                        label: jewelNames[i] || entry.name,
                      })),
                    ]}
                    onChange={(v) => {
                      const text = v === '__current' ? socketText : v === '' ? '' : jewelLib.find(entry => entry.id === v)?.text;
                      if (text !== undefined) session.setEditorDraft(jewelDraftKey, text === socketText ? null : text);
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
            placeholder={JEWEL_TEMPLATE}
            value={jewelEdit.draft}
            spellCheck={false}
            aria-label={tt('tree.jewel')}
            onChange={(e) => session.setEditorDraft(jewelDraftKey,
              e.target.value === socketText ? null : e.target.value)}
          />
        </div>
      );
  const socketPicker = allocatedSockets.length > 0 && <AppSelect ariaLabel={tt('tree.chooseSocket')} placeholder={tt('tree.chooseSocket')}
        value={jewelSocket === null ? '' : String(jewelSocket)} disabled={session.busy || importProblem}
        options={allocatedSockets.map((id, index) => ({ value: String(id), label: socketLabels.get(id)!,
          hint: socketContents[index] || tt('tree.emptySocket') }))}
        onChange={value => openJewel(Number(value))} />;
  const toolsBody = (
    <>
      {socketPicker && <div className="tree-side-socket-picker">{socketPicker}</div>}
      {jewelEl}
      {detailsEl}
      {attrEl}
      {warnEl}
      {powerEl}
      <TreeOptimizer planner={planner} />
      {draftsEl}
    </>
  );

  const toggleSide = () => {
    if (sideOpen) { setSideOpen(false); return; }
    setSideOpen(true);
    requestAnimationFrame(() => toolsRef.current?.focus({ preventScroll: true }));
  };
  return (
    <section ref={panelRef} className="ui-page tree-panel" aria-labelledby="tree-heading" style={{ '--tree-side-preference': `${sideWidth}rem` } as React.CSSProperties} data-side={sideOpen ? 'open' : 'closed'} onKeyDown={onPanelKeyDown}>
      <h2 id="tree-heading" className="sr-only">{tt('tree.title')}</h2>
      <div className="tree-stage">
        <div ref={trackBarBottom} className="tree-bar tree-bar-top" role="group" aria-label={tt('tree.findAndAllocate')}>
          {searchEl}
          {modeEl}
          {heatEl}
          {socketPicker && <div className="tree-bar-socket-picker">{socketPicker}</div>}
        </div>
        <AppButton className="tree-side-toggle" aria-expanded={sideOpen} aria-controls="tree-side" onClick={toggleSide}>
          {sideOpen ? <PanelRightClose aria-hidden="true" /> : <PanelRightOpen aria-hidden="true" />}
          {tt('tree.editOptions')}
        </AppButton>
        {(importProblem || disconnectedImport) && <p className="tree-edit-error tree-import-notice" role="status">{tt(importProblem ? 'tree.invalidImport' : 'tree.disconnectedImport')}</p>}
        {editError && !isImportProblem(editError) && <p className="tree-edit-error" role="alert">{treeEditError(editError, lang)}</p>}
        <div className="tree-bar tree-bar-view" role="group" aria-label={tt('tree.viewAndBuild')}>
          <TreeRefundNotice receipt={refundReceipt} session={session} lang={lang} onUndo={() => { session.undoTree(); setRefundReceipt(null); }} />
          {countEl}
          {classEl}
          {ascEl}
          {focusAscEl}
          {resetEl}
          {conversion}
        </div>
        {canvasEl}
      </div>
      {sideOpen && <SidebarResizeHandle panelRef={panelRef} width={sideWidth} label={tt('tree.resizeSidebar')} onCommit={commitSideWidth} />}
      <aside id="tree-side" className="tree-side" hidden={!sideOpen} aria-label={tt('tree.editOptions')}>
        <div className="tree-edit-tools" ref={toolsRef} tabIndex={-1}>
          {toolsBody}
          <details className="tree-legend-help"><summary className="disclosure">{tt('tree.weaponLegend')}</summary><p className="tree-hint">{tt('tree.weaponRules')}</p></details>
        </div>
      </aside>
    </section>
  );
}

/** Cursor-following node tooltip (offset bottom-right, flips near canvas right/bottom edges). */
function TreeTooltip({
  node,
  name,
  stats,
  pos,
  canvasRef,
  benefit,
  onPointerEnter, onPointerLeave, onFocus, onBlur,
}: {
  node: PassiveNode;
  /** Localized node name (present when matched in dictionary; null = show English name). */
  name: string | null;
  stats: string[];
  pos: { x: number; y: number };
  canvasRef: React.RefObject<SVGSVGElement | null>;
  benefit?: React.ReactNode;
  onPointerEnter: () => void; onPointerLeave: () => void;
  onFocus: () => void; onBlur: React.FocusEventHandler<HTMLDivElement>;
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
    <div className="tree-tooltip" role="tooltip" style={style} onPointerEnter={onPointerEnter} onPointerLeave={onPointerLeave} onFocus={onFocus} onBlur={onBlur}>
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
  if (isDisconnectedTree(reason)) return tt('tree.disconnectedImport');
  if (needsTreeConversion(reason)) return tt('tree.invalidImport');
  return tt('tree.noLegalPath');
}
