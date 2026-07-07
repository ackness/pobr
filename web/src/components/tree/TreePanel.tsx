import { useEffect, useMemo, useRef, useState } from 'react';
import { getBackend } from '../../api/backend';
import type { AttributeChoice, PassiveNode } from '../../api/types';
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
  const [hoverPos, setHoverPos] = useState<{ x: number; y: number }>({ x: 0, y: 0 });
  const [hoverStats, setHoverStats] = useState<string[] | null>(null);
  /** 属性小点三选一弹窗（node + 屏幕坐标）。 */
  const [attrPicker, setAttrPicker] = useState<{ node: PassiveNode; x: number; y: number } | null>(
    null,
  );
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

  /** 属性小点判定（`+5 to any Attribute`）。 */
  const isAttrNode = (node: PassiveNode) =>
    node.name === 'Attribute' &&
    (node.stats ?? []).some((line) => line.includes('any') && line.includes('Attribute'));

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

  // 中文界面下把 hover 节点词条经模板反查翻成简中（异步，结果晚到时校验仍是当前节点）。
  useEffect(() => {
    if (!hover) {
      setHoverStats(null);
      return;
    }
    const chosen = session.attributeChoices[String(hover.skill)];
    const chosenLine = chosen ? [`→ ${tt(`tree.attr.${chosen}` as Parameters<typeof tt>[0])}`] : [];
    const raw = [...(hover.stats ?? []), ...chosenLine.map((l) => l)].map((line) =>
      line.replace(/\[([^\]|]*)\|([^\]]*)\]/g, '$2').replace(/\[([^\]]*)\]/g, '$1'),
    );
    if (lang === 'en-US' || raw.length === 0) {
      setHoverStats(raw);
      return;
    }
    let cancelled = false;
    getBackend()
      .then((b) => b.translateLines(raw))
      .then((translated) => !cancelled && setHoverStats(translated))
      .catch(() => !cancelled && setHoverStats(raw));
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

  /** 点选加点/取消（拖拽平移不触发）；属性小点加点前先弹三选一。 */
  const onNodeClick = (node: PassiveNode, e: React.MouseEvent) => {
    if (dragRef.current?.moved) return;
    if (!allocated.has(node.skill) && isAttrNode(node)) {
      const rect = svgRef.current!.getBoundingClientRect();
      setAttrPicker({ node, x: e.clientX - rect.left, y: e.clientY - rect.top });
      return;
    }
    setAttrPicker(null);
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
          {ascExtent && (
            <circle
              className="asc-backdrop"
              cx={ascExtent.x + ascExtent.w / 2}
              cy={ascExtent.y + ascExtent.h / 2}
              r={Math.max(ascExtent.w, ascExtent.h) / 2}
            />
          )}
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
                className={`node node-${node.kind}${node.ascendancy_id ? ' node-asc' : ''}${allocated.has(node.skill) ? ' node-allocated' : ''}`}
                onPointerEnter={(e) => {
                  setHover(node);
                  setHoverPos({ x: e.clientX, y: e.clientY });
                }}
                onPointerMove={(e) => setHoverPos({ x: e.clientX, y: e.clientY })}
                onPointerLeave={() => setHover((h) => (h?.skill === node.skill ? null : h))}
                onClick={(e) => onNodeClick(node, e)}
              />
            ))}
          </g>
        </svg>
        {hover && !attrPicker && (
          <TreeTooltip node={hover} stats={hoverStats ?? []} pos={hoverPos} canvasRef={svgRef} />
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
  stats,
  pos,
  canvasRef,
}: {
  node: PassiveNode;
  stats: string[];
  pos: { x: number; y: number };
  canvasRef: React.RefObject<SVGSVGElement | null>;
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
      <strong className={`tooltip-name kind-${node.kind}`}>{node.name ?? node.id}</strong>
      {stats.map((line, i) => (
        <div key={i} className="tooltip-stat">
          {line}
        </div>
      ))}
    </div>
  );
}
