import { useEffect, useMemo, useRef, useState } from 'react';
import { getBackend } from '../../api/backend';
import type { PassiveNode } from '../../api/types';
import type { BuildSession } from '../../hooks/useBuildSession';
import { bindT, type Lang } from '../../lib/i18n';
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

interface ViewBox {
  x: number;
  y: number;
  w: number;
  h: number;
}

/** 天赋树查看器：SVG 渲染 + 已加点高亮 + 缩放平移 / hover 词条 + 点选加点重算。 */
export function TreePanel({ session, lang }: Props) {
  const tt = bindT(lang);
  const [nodes, setNodes] = useState<PassiveNode[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [hover, setHover] = useState<PassiveNode | null>(null);
  const [viewBox, setViewBox] = useState<ViewBox | null>(null);
  const dragRef = useRef<{ x: number; y: number; moved: boolean } | null>(null);
  const svgRef = useRef<SVGSVGElement | null>(null);

  useEffect(() => {
    getBackend()
      .then((b) => b.loadPassiveTree())
      .then(setNodes)
      .catch((err) => setError(String(err)));
  }, []);

  const allocated = useMemo(() => new Set(session.allocatedNodes), [session.allocatedNodes]);

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

  const view = viewBox ?? fullExtent;

  if (error) return <div className="calc-error">{error}</div>;
  if (!nodes || !view) return <div className="empty-hint">{tt('tree.loading')}</div>;

  const onWheel = (e: React.WheelEvent<SVGSVGElement>) => {
    const factor = e.deltaY > 0 ? 1.15 : 1 / 1.15;
    const rect = svgRef.current!.getBoundingClientRect();
    const px = view.x + ((e.clientX - rect.left) / rect.width) * view.w;
    const py = view.y + ((e.clientY - rect.top) / rect.height) * view.h;
    const w = Math.min(Math.max(view.w * factor, 800), (fullExtent?.w ?? 1) * 2);
    const h = (w / view.w) * view.h;
    setViewBox({ x: px - ((px - view.x) / view.w) * w, y: py - ((py - view.y) / view.h) * h, w, h });
  };

  const onPointerDown = (e: React.PointerEvent<SVGSVGElement>) => {
    dragRef.current = { x: e.clientX, y: e.clientY, moved: false };
    (e.target as Element).setPointerCapture?.(e.pointerId);
  };

  const onPointerMove = (e: React.PointerEvent<SVGSVGElement>) => {
    if (!dragRef.current) return;
    const rect = svgRef.current!.getBoundingClientRect();
    const dx = ((e.clientX - dragRef.current.x) / rect.width) * view.w;
    const dy = ((e.clientY - dragRef.current.y) / rect.height) * view.h;
    if (Math.abs(e.clientX - dragRef.current.x) + Math.abs(e.clientY - dragRef.current.y) > 3) {
      dragRef.current.moved = true;
    }
    dragRef.current = { ...dragRef.current, x: e.clientX, y: e.clientY };
    setViewBox({ ...view, x: view.x - dx, y: view.y - dy });
  };

  const onPointerUp = () => {
    // 保留 moved 标记到 click 事件之后（click 在 pointerup 后触发）。
    setTimeout(() => {
      dragRef.current = null;
    }, 0);
  };

  /** 点选加点/取消（拖拽平移不触发）。 */
  const onNodeClick = (node: PassiveNode) => {
    if (dragRef.current?.moved) return;
    session.toggleNode(node.skill);
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
        <button onClick={() => setViewBox(null)}>{tt('tree.reset')}</button>
      </div>
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
          <g className="tree-nodes">
            {placed.map((node) => (
              <circle
                key={node.skill}
                cx={node.x}
                cy={node.y}
                r={NODE_RADIUS[node.kind] ?? 40}
                className={`node node-${node.kind}${allocated.has(node.skill) ? ' node-allocated' : ''}`}
                onPointerEnter={() => setHover(node)}
                onPointerLeave={() => setHover((h) => (h?.skill === node.skill ? null : h))}
                onClick={() => onNodeClick(node)}
              />
            ))}
          </g>
        </svg>
        {hover && (
          <div className="tree-tooltip" role="tooltip">
            <strong className={`tooltip-name kind-${hover.kind}`}>{hover.name ?? hover.id}</strong>
            {(hover.stats ?? []).map((line, i) => (
              <div key={i} className="tooltip-stat">
                {line.replace(/\[([^\]|]*)\|([^\]]*)\]/g, '$2').replace(/\[([^\]]*)\]/g, '$1')}
              </div>
            ))}
          </div>
        )}
      </div>
    </section>
  );
}
