import { formatApiError } from '../../api/error';
import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react';
import { getBackend } from '../../api/backend';
import type { AttributeChoice, PassiveNode, TreeArt } from '../../api/types';
import type { BuildSession } from '../../hooks/useBuildSession';
import { useItemDisplayNames } from '../../hooks/useLocalizedLines';
import { bindT, statNameLabel, type Lang } from '../../lib/i18n';
import {
  buildPassiveGraph,
  classStartSkill,
  deallocateNode,
  shortestAllocationPath,
} from '../../lib/passiveGraph';
import { previewDiff, type DiffEntry } from '../../lib/compare';
import { DiffList } from '../shared/DiffList';
import { AppSelect } from '../shared/AppSelect';
import { NoteEditor } from '../shared/NoteEditor';
import { TreeOptimizer } from './TreeOptimizer';
import './tree.css';

interface Props {
  session: BuildSession;
  lang: Lang;
}

const NODE_RADIUS: Record<string, number> = {
  normal: 40,
  notable: 65,
  keystone: 90,
  mastery: 50,
  jewel_socket: 55,
  ascendancy_start: 60,
};

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

interface ViewBox {
  x: number;
  y: number;
  w: number;
  h: number;
}

/**
 * 钳制视口，防止把树平移到视野外露出大片空白。天赋树近似圆形，故约束视口中心到
 * 树中心的距离 ≤ 内切圆半径 −半视口（缩得比树大→锁中心；放大后可到边缘看边缘节点）。
 * 用圆形而非矩形，避免拖到（本就是空的）包围盒四角。
 */
function clampView(v: ViewBox, extent: ViewBox): ViewBox {
  const tcx = extent.x + extent.w / 2;
  const tcy = extent.y + extent.h / 2;
  const radius = Math.min(extent.w, extent.h) / 2;
  const half = Math.max(v.w, v.h) / 2;
  const maxOff = Math.max(0, radius - half * 0.5); // 允许边缘节点接近视口中心
  const dx = v.x + v.w / 2 - tcx;
  const dy = v.y + v.h / 2 - tcy;
  const dist = Math.hypot(dx, dy);
  if (dist <= maxOff || dist === 0) return v;
  const cx = tcx + (dx / dist) * maxOff;
  const cy = tcy + (dy / dist) * maxOff;
  return { ...v, x: cx - v.w / 2, y: cy - v.h / 2 };
}

/** 属性小点判定（`+5 to any Attribute`）。 */
function isAttrNode(node: PassiveNode): boolean {
  return (
    node.name === 'Attribute' &&
    (node.stats ?? []).some((line) => line.includes('any') && line.includes('Attribute'))
  );
}

const JEWEL_TEMPLATE = 'Rarity: RARE\nMy Jewel\nEmerald\n+50 to maximum Life';

/** 天赋树查看器：SVG 渲染 + 已加点高亮 + 缩放平移 / hover 词条 + 点选加点重算。 */
export function TreePanel({ session, lang }: Props) {
  const tt = bindT(lang);
  const [nodes, setNodes] = useState<PassiveNode[] | null>(null);
  const [art, setArt] = useState<TreeArt | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [hover, setHover] = useState<PassiveNode | null>(null);
  const [hoverPos, setHoverPos] = useState<{ x: number; y: number }>({ x: 0, y: 0 });
  const [hoverStats, setHoverStats] = useState<string[] | null>(null);
  /** hover 节点名的中文翻译（词典命中才有；null = 显示英文原名）。 */
  const [hoverName, setHoverName] = useState<string | null>(null);
  /** 节点搜索串（名称/词条子串匹配，高亮命中节点）。 */
  const [search, setSearch] = useState('');
  /** 「下一个命中」轮转下标（搜索串变化时归零）。 */
  const [hitIndex, setHitIndex] = useState(0);
  /** 属性小点三选一弹窗（node + 屏幕坐标）。 */
  const [attrPicker, setAttrPicker] = useState<{ node: PassiveNode; x: number; y: number } | null>(
    null,
  );
  /** 珠宝插槽编辑器（正在编辑的插槽节点 id + 草稿）。 */
  const [jewelEdit, setJewelEdit] = useState<{ socket: number; draft: string } | null>(null);
  /** hover 节点的加点/取消收益（防抖重算；按 stateVersion 失效的缓存）。 */
  const [hoverDiff, setHoverDiff] = useState<DiffEntry[] | null>(null);
  const diffCacheRef = useRef<{ version: number; map: Map<number, DiffEntry[]> }>({
    version: -1,
    map: new Map(),
  });
  const [viewBox, setViewBox] = useState<ViewBox | null>(null);
  const dragRef = useRef<{
    x: number;
    y: number;
    startX: number;
    startY: number;
    moved: boolean;
  } | null>(null);
  const svgRef = useRef<SVGSVGElement | null>(null);
  /** 滚轮缩放的 rAF 合帧暂存（一帧最多提交一次 viewBox）。 */
  const wheelPendingRef = useRef<ViewBox | null>(null);
  const wheelRafRef = useRef<number | null>(null);
  /** 拖动提交后待复位 CSS transform（与新 viewBox 同帧清除，防闪跳）。 */
  const transformResetRef = useRef(false);
  useLayoutEffect(() => {
    if (!transformResetRef.current) return;
    transformResetRef.current = false;
    const svg = svgRef.current;
    if (svg) {
      svg.style.transform = '';
      svg.classList.remove('is-dragging');
    }
  }, [viewBox]);

  useEffect(() => {
    getBackend()
      .then((b) => Promise.all([b.loadPassiveTree(), b.loadTreeArt()]))
      .then(([n, a]) => {
        setNodes(n);
        setArt(a); // null = 美术未生成，回退纯圆点渲染。
      })
      .catch((err) => setError(formatApiError(err)));
  }, []);

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
    setHeatBusy(true);
    getBackend()
      .then((b) => b.nodePower(request, heatStat, heatDepth))
      .then((res) => {
        setHeatData(new Map(res.entries.map((e) => [e.skill, e.delta])));
        setHeatVersion(version);
      })
      .catch(() => setHeatData(null))
      .finally(() => setHeatBusy(false));
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
    const present = new Set((session.calc?.stats ?? []).map((s) => s.id));
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
  const graph = useMemo(() => buildPassiveGraph(nodes ?? []), [nodes]);
  const startSkill = useMemo(
    () => classStartSkill(nodes ?? [], session.character?.class_name),
    [nodes, session.character?.class_name],
  );

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

  const edges = useMemo(() => {
    const out: { x1: number; y1: number; x2: number; y2: number; active: boolean }[] = [];
    // GGG 的 `connections` 是**单向 out 边**（每条只出现一次，方向任意）——
    // 不能按 id 大小去重（会丢一半边），用无向键 seen 集合。
    const seen = new Set<string>();
    for (const node of placed) {
      for (const target of node.connections ?? []) {
        const other = byId.get(target);
        if (!other) continue;
        const key =
          node.skill < target ? `${node.skill}:${target}` : `${target}:${node.skill}`;
        if (seen.has(key)) continue;
        seen.add(key);
        // 飞升与主树分区渲染在同一平面；跨区连线跳过（坐标相距过远的伪边）。
        if ((node.ascendancy_id ?? null) !== (other.ascendancy_id ?? null)) continue;
        out.push({
          x1: node.x!,
          y1: node.y!,
          x2: other.x!,
          y2: other.y!,
          active: allocated.has(node.skill) && allocated.has(target),
        });
      }
    }
    return out;
  }, [placed, byId, allocated]);

  // ── 渲染性能：节点/连线子树 memo 化 ─────────────────────────────────────
  // 拖拽/缩放/悬停每帧都 setState；若子树不 memo，React 每帧要 diff 近万个
  // SVG 元素（拖动卡顿的根因）。memo 后这些帧只更新 svg viewBox 与 tooltip。
  // 点击回调经 sessionRef 取最新会话，避免 memo 闭包捕获过期 state。
  const sessionRef = useRef(session);
  sessionRef.current = session;

  /**
   * 点选加点/取消（拖拽平移不触发）；属性小点弹三选一；珠宝插槽开编辑器。
   *
   * 加点走最短路径：点一个未加点的远端节点会把沿途节点一并点亮（PoB2 语义）。
   * 相邻节点的路径长度为 1，交互与寻路接入前逐字一致。取消加点走级联清理——
   * `deallocateNode` 在图模型解释不了当前加点全集时自动退化为单点取消，
   * 导入的真实 build 常含模型外连接，不会被误删整棵树。
   */
  const handleNodeClick = useCallback(
    (node: PassiveNode, e: React.MouseEvent) => {
      if (dragRef.current?.moved) return;
      const s = sessionRef.current;
      const allocated = new Set(s.allocatedNodes);
      const isAlloc = allocated.has(node.skill);

      // 珠宝插槽：点击恒为「（必要时先接上）开编辑器」，不走取消分支。
      if (node.kind === 'jewel_socket') {
        if (!isAlloc) {
          s.setAllocatedNodes([
            ...s.allocatedNodes,
            ...shortestAllocationPath(graph, allocated, startSkill, node.skill),
          ]);
        }
        const existing = s.jewels.find((j) => j.socket_node === node.skill);
        setJewelEdit({ socket: node.skill, draft: existing?.text ?? JEWEL_TEMPLATE });
        return;
      }

      if (isAlloc) {
        setAttrPicker(null);
        s.setAllocatedNodes([...deallocateNode(graph, allocated, startSkill, node.skill)]);
        return;
      }

      const path = shortestAllocationPath(graph, allocated, startSkill, node.skill);
      // 相邻单点仍弹三选一（原行为）；多步路径穿过的属性小点只点亮、不定选择，
      // 比例交给工具栏的批量调配面板（choice 留空＝引擎语义无贡献）。
      if (path.length <= 1 && isAttrNode(node)) {
        const rect = svgRef.current!.getBoundingClientRect();
        setAttrPicker({ node, x: e.clientX - rect.left, y: e.clientY - rect.top });
        return;
      }
      setAttrPicker(null);
      s.setAllocatedNodes([...s.allocatedNodes, ...path]);
    },
    [graph, startSkill],
  );

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
            className={e.active ? 'edge-active' : 'edge'}
          />
        ))}
      </g>
    ),
    [edges],
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
                cx={node.x}
                cy={node.y}
                r={r}
                style={heat ? { fill: heat } : undefined}
                className={`node node-${node.kind}${icon || frame ? ' node-art' : ''}${node.ascendancy_id ? ' node-asc' : ''}${isAlloc ? ' node-allocated' : ''}${node.kind === 'jewel_socket' && filledJewelSockets.has(node.skill) ? ' node-jewel-filled' : ''}${searchHits?.has(node.skill) ? ' node-search-hit' : ''}`}
                onPointerEnter={(e) => {
                  setHover(node);
                  setHoverPos({ x: e.clientX, y: e.clientY });
                }}
                onPointerMove={(e) => setHoverPos({ x: e.clientX, y: e.clientY })}
                onPointerLeave={() => setHover((h) => (h?.skill === node.skill ? null : h))}
                onClick={(e) => handleNodeClick(node, e)}
              />
            </g>
          );
        })}
      </g>
    );
  }, [placed, allocated, filledJewelSockets, searchHits, heatStat, heatData, heatMax, handleNodeClick, art]);

  const fullExtent = useMemo((): ViewBox | null => {
    if (placed.length === 0) return null;
    const xs = placed.map((n) => n.x!);
    const ys = placed.map((n) => n.y!);
    const minX = Math.min(...xs);
    const maxX = Math.max(...xs);
    const minY = Math.min(...ys);
    const maxY = Math.max(...ys);
    const pad = 400;
    return { x: minX - pad, y: minY - pad, w: maxX - minX + pad * 2, h: maxY - minY + pad * 2 };
  }, [placed]);

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
    getBackend()
      .then((b) => b.translateLines([name, ...raw]))
      .then((translated) => {
        if (cancelled) return;
        setHoverName(translated[0] !== name ? translated[0] : null);
        setHoverStats(translated.slice(1));
      })
      .catch(() => {
        if (cancelled) return;
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

  // hover 收益：把「该节点加点/取消后」的请求再算一次，差异进 tooltip。
  // 300ms 防抖；结果按 stateVersion 缓存（编辑后全量失效）。
  useEffect(() => {
    setHoverDiff(null);
    if (!hover || session.busy || !session.calc) return;
    const cache = diffCacheRef.current;
    if (cache.version !== session.stateVersion) {
      cache.version = session.stateVersion;
      cache.map.clear();
    }
    const cached = cache.map.get(hover.skill);
    if (cached) {
      setHoverDiff(cached);
      return;
    }
    const request = session.currentRequest();
    if (!request) return;
    const skill = hover.skill;
    const timer = setTimeout(() => {
      const has = session.allocatedNodes.includes(skill);
      const nodes = has
        ? session.allocatedNodes.filter((n) => n !== skill)
        : [...session.allocatedNodes, skill];
      previewDiff({ ...request, allocated_nodes: nodes }, session.calc!)
        .then((diffs) => {
          cache.map.set(skill, diffs);
          setHoverDiff((current) => (hover.skill === skill ? diffs : current));
        })
        .catch(() => {});
    }, 300);
    return () => clearTimeout(timer);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [hover, session.stateVersion, session.busy]);

  // 快捷键 S/D/I（或 1/2/3）：弹窗打开时选择；hover 属性小点时直接加点或改选。
  useEffect(() => {
    const KEYMAP: Record<string, AttributeChoice> = {
      s: 'str', d: 'dex', i: 'int', '1': 'str', '2': 'dex', '3': 'int',
    };
    const onKey = (e: KeyboardEvent) => {
      const target = e.target as HTMLElement | null;
      if (target && ['INPUT', 'TEXTAREA', 'SELECT'].includes(target.tagName)) return;
      const choice = KEYMAP[e.key.toLowerCase()];
      if (!choice) return;
      if (attrPicker) {
        session.toggleNode(attrPicker.node.skill, choice);
        setAttrPicker(null);
        e.preventDefault();
        return;
      }
      if (hover && isAttrNode(hover)) {
        if (allocated.has(hover.skill)) {
          // 已加点：仅改选（不取消）。
          session.setAttributeChoices({
            ...session.attributeChoices,
            [String(hover.skill)]: choice,
          });
        } else {
          session.toggleNode(hover.skill, choice);
        }
        e.preventDefault();
      }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  });


  const view = viewBox ?? fullExtent;

  if (error) return <div className="calc-error">{error}</div>;
  if (!nodes || !view) return <div className="empty-hint">{tt('tree.loading')}</div>;

  // ── 平移/缩放的高性能路径 ────────────────────────────────────────────────
  // viewBox 每帧改动会强制整树（~1 万 SVG 元素）重新栅格化；拖动期间只写
  // CSS transform（GPU 合成器，与元素数量无关），pointerup 一次性提交
  // viewBox（transform 复位在 useLayoutEffect 里与新 viewBox 同帧，防闪跳）。
  // 滚轮缩放本就需要重绘内容，走 rAF 合帧（一帧最多一次栅格化）。

  const onWheel = (e: React.WheelEvent<SVGSVGElement>) => {
    const factor = e.deltaY > 0 ? 1.15 : 1 / 1.15;
    const rect = svgRef.current!.getBoundingClientRect();
    const base = wheelPendingRef.current ?? view;
    const px = base.x + ((e.clientX - rect.left) / rect.width) * base.w;
    const py = base.y + ((e.clientY - rect.top) / rect.height) * base.h;
    const w = Math.min(Math.max(base.w * factor, 800), (fullExtent?.w ?? 1) * 2);
    const h = (w / base.w) * base.h;
    wheelPendingRef.current = {
      x: px - ((px - base.x) / base.w) * w,
      y: py - ((py - base.y) / base.h) * h,
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
    dragRef.current = { x: e.clientX, y: e.clientY, startX: e.clientX, startY: e.clientY, moved: false };
    (e.target as Element).setPointerCapture?.(e.pointerId);
  };

  const onPointerMove = (e: React.PointerEvent<SVGSVGElement>) => {
    const drag = dragRef.current;
    const svg = svgRef.current;
    if (!drag || !svg) return;
    drag.x = e.clientX;
    drag.y = e.clientY;
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
      svg.style.transform = `translate3d(${dx}px, ${dy}px, 0)`;
    }
  };

  const onPointerUp = () => {
    const drag = dragRef.current;
    const svg = svgRef.current;
    if (drag?.moved && svg) {
      const rect = svg.getBoundingClientRect();
      const dxUnits = ((drag.x - drag.startX) / rect.width) * view.w;
      const dyUnits = ((drag.y - drag.startY) / rect.height) * view.h;
      const panned = { ...view, x: view.x - dxUnits, y: view.y - dyUnits };
      setViewBox(fullExtent ? clampView(panned, fullExtent) : panned);
      transformResetRef.current = true;
    }
    // 保留 moved 标记到 click 事件之后（click 在 pointerup 后触发）。
    setTimeout(() => {
      dragRef.current = null;
    }, 0);
  };

  return (
    <section className="tree-panel" aria-labelledby="tree-heading">
      <div className="tree-toolbar">
        <h2 id="tree-heading" className="panel-heading">
          {tt('tree.title')}
        </h2>
        <span className="tree-count">
          {session.allocatedNodes.length} {tt('tree.allocated')}
        </span>
        <span className="tree-hint">
          {tt('tree.hint')}
        </span>
        <span className="tree-search">
          <input
            type="search"
            placeholder={tt('tree.search')}
            value={search}
            onChange={(e) => {
              setSearch(e.target.value);
              setHitIndex(0);
            }}
            aria-label={tt('tree.search')}
          />
          {searchHits && (
            <span className="tree-search-count">
              {searchHits.size} {tt('tree.matches')}
            </span>
          )}
          {searchHits && searchHits.size > 0 && (
            <button
              onClick={() => {
                const sorted = [...searchHits].sort((a, b) => a - b);
                const node = byId.get(sorted[hitIndex % sorted.length]);
                setHitIndex((i) => i + 1);
                if (!node) return;
                // 保持当前缩放（过远时收到能看清节点簇的档位）平移到命中节点。
                const w = Math.min(view.w, 14000);
                const h = (w / view.w) * view.h;
                setViewBox({ x: node.x! - w / 2, y: node.y! - h / 2, w, h });
              }}
            >
              {tt('tree.nextHit')}
            </button>
          )}
        </span>
        <span className="tree-heat" title={tt('tree.heatHint')}>
          {tt('tree.heat')}
          <select
            value={heatStat}
            aria-label={tt('tree.heat')}
            onChange={(e) => {
              setHeatStat(e.target.value);
              if (!e.target.value) setHeatData(null);
            }}
          >
            <option value="">{tt('tree.heatOff')}</option>
            {heatStatOptions.map((id) => (
              <option key={id} value={id}>
                {statNameLabel(lang, id)}
              </option>
            ))}
          </select>
          {heatStat && (
            <>
              <label className="tree-heat-depth">
                {tt('tree.heatDepth')}
                <input
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
              <button disabled={heatBusy || session.busy} onClick={runHeat}>
                {heatBusy ? tt('tree.heatComputing') : tt('tree.heatRun')}
              </button>
              {heatData && heatVersion !== session.stateVersion && (
                <span className="tree-hint">{tt('tree.heatStale')}</span>
              )}
            </>
          )}
        </span>
        <label className="tree-asc-picker">
          {tt('build.class')}
          <select
            value={session.character?.class_name ?? ''}
            disabled={session.busy}
            onChange={(e) => session.newBuild(e.target.value, '')}
          >
            {(session.treeMeta?.classes ?? []).map((c) => (
              <option key={c.name} value={c.name}>
                {lang !== 'en-US' ? (session.classNames.classes[c.name] ?? c.name) : c.name}
              </option>
            ))}
          </select>
        </label>
        <label className="tree-asc-picker">
          {tt('tree.ascendancy')}
          <select
            value={session.character?.ascendancy_name ?? ''}
            disabled={session.busy}
            onChange={(e) => session.setCharacter({ ascendancy_name: e.target.value })}
          >
            <option value="">{tt('build.none')}</option>
            {(session.treeMeta?.classes ?? [])
              .find((c) => c.name === session.character?.class_name)
              ?.ascendancies?.map((a) => (
                <option key={a.id} value={a.name}>
                  {lang !== 'en-US'
                    ? (session.classNames.ascendancies[a.name] ?? a.name)
                    : a.name}
                </option>
              ))}
          </select>
        </label>
        {ascExtent && (
          <button onClick={() => setViewBox(ascExtent)}>{tt('tree.focusAsc')}</button>
        )}
        <button onClick={() => setViewBox(null)}>{tt('tree.reset')}</button>
      </div>
      <div className="attr-distribute" role="group" aria-label={tt('tree.attrDistribute')}>
        {allocatedAttrNodes.length > 0 && (
        <>
          <span className="attr-distribute-title">
            {tt('tree.attrDistribute')}（{allocatedAttrNodes.length}）
          </span>
          {(['str', 'dex', 'int'] as AttributeChoice[]).map((key) => (
            <label key={key} className={`attr-count attr-${key}`}>
              {tt(`tree.attr.${key}` as Parameters<typeof tt>[0])}
              <input
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
                  <input
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
              <input
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
      {heatData && (
        <TreeOptimizer
          session={session}
          lang={lang}
          heatData={heatData}
          nodeLabel={(skill) => byId.get(skill)?.name ?? `#${skill}`}
        />
      )}
      {jewelEdit && (
        <div className="jewel-editor" role="group" aria-label={tt('tree.jewel')}>
          <header className="item-detail-header">
            <span className="item-slot">
              {tt('tree.jewel')} · #{jewelEdit.socket}
            </span>
            <span className="item-actions">
              <button
                disabled={session.busy}
                onClick={() => {
                  const rest = session.jewels.filter((j) => j.socket_node !== jewelEdit.socket);
                  session.setJewels([...rest, { socket_node: jewelEdit.socket, text: jewelEdit.draft }]);
                  setJewelEdit(null);
                }}
              >
                {tt('items.apply')}
              </button>
              {session.jewels.some((j) => j.socket_node === jewelEdit.socket) && (
                <button
                  className="skill-remove"
                  disabled={session.busy}
                  onClick={() => {
                    session.setJewels(session.jewels.filter((j) => j.socket_node !== jewelEdit.socket));
                    setJewelEdit(null);
                  }}
                >
                  {tt('items.remove')}
                </button>
              )}
              <button
                disabled={session.busy}
                onClick={() => {
                  if (allocated.has(jewelEdit.socket)) session.toggleNode(jewelEdit.socket);
                  session.setJewels(session.jewels.filter((j) => j.socket_node !== jewelEdit.socket));
                  setJewelEdit(null);
                }}
              >
                {tt('tree.unallocSocket')}
              </button>
              <button
                disabled={session.busy}
                onClick={() => session.saveLibraryItem('jewel', jewelEdit.draft)}
              >
                {tt('lib.save')}
              </button>
              <button onClick={() => setJewelEdit(null)}>{tt('items.cancel')}</button>
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
                        setJewelEdit({ ...jewelEdit, draft: JEWEL_TEMPLATE });
                        return;
                      }
                      const entry = jewelLib.find((x) => x.id === v);
                      if (!entry) return;
                      session.setJewels([
                        ...rest,
                        { socket_node: jewelEdit.socket, text: entry.text },
                      ]);
                      setJewelEdit({ ...jewelEdit, draft: entry.text });
                    }}
                  />
                );
              })()}
            </div>
          )}
          <textarea
            rows={7}
            value={jewelEdit.draft}
            spellCheck={false}
            aria-label={tt('tree.jewel')}
            onChange={(e) => setJewelEdit({ ...jewelEdit, draft: e.target.value })}
          />
        </div>
      )}
      <div className="tree-canvas">
        <svg
          ref={svgRef}
          viewBox={`${view.x} ${view.y} ${view.w} ${view.h}`}
          onWheel={onWheel}
          onPointerDown={onPointerDown}
          onPointerMove={onPointerMove}
          onPointerUp={onPointerUp}
          role="img"
          aria-label={tt('tree.title')}
        >
          <defs>
            {/* 技能图标裁成圆形（美术是方图，避免方角戳出外框） */}
            <clipPath id="tree-icon-clip" clipPathUnits="objectBoundingBox">
              <circle cx="0.5" cy="0.5" r="0.5" />
            </clipPath>
          </defs>
          {ascExtent && (
            <circle
              className="asc-backdrop"
              cx={ascExtent.x + ascExtent.w / 2}
              cy={ascExtent.y + ascExtent.h / 2}
              r={Math.max(ascExtent.w, ascExtent.h) / 2}
            />
          )}
          {edgesEl}
          {nodesEl}
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
              <button
                key={choice}
                role="menuitem"
                className={`attr-choice attr-${choice}`}
                onClick={() => {
                  session.toggleNode(attrPicker.node.skill, choice);
                  setAttrPicker(null);
                }}
              >
                {tt(`tree.attr.${choice}` as Parameters<typeof tt>[0])}
                <kbd>{choice === 'str' ? 'S' : choice === 'dex' ? 'D' : 'I'}</kbd>
              </button>
            ))}
            <button className="attr-cancel" onClick={() => setAttrPicker(null)}>
              ×
            </button>
          </div>
        )}
        {!currentAscId &&
          ((session.treeMeta?.classes ?? []).find(
            (c) => c.name === session.character?.class_name,
          )?.ascendancies?.length ?? 0) > 0 && (
            <div className="asc-hint">{tt('tree.pickAscHint')}</div>
          )}
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
