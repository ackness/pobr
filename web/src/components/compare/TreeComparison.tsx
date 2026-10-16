import { memo, useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { getBackend } from '../../api/backend';
import { formatApiError } from '../../api/error';
import type { CalculateBuildResponse, PassiveNode, PassiveTreeMeta } from '../../api/types';
import { cleanLine, useLocalizedLines } from '../../hooks/useLocalizedLines';
import { compareTreeAllocations, resolvedTreeVersion, treeAllocationTotals, type OverlayNode, type StageState } from '../../lib/compare';
import { bindT, statNameLabel, type Lang } from '../../lib/i18n';
import { NODE_RADIUS, hasPosition, passiveTreeEdges, treeExtent, type ViewBox } from '../../lib/treeGeometry';

interface Props {
  a: StageState;
  b: StageState;
  before?: CalculateBuildResponse;
  after?: CalculateBuildResponse;
  dataVersion: string | null;
  meta: PassiveTreeMeta | null;
  lang: Lang;
}

export function TreeComparison(props: Props) {
  const { a, b, dataVersion, lang } = props;
  const tt = bindT(lang);
  const versionA = resolvedTreeVersion(a.treeVersion, dataVersion);
  const versionB = resolvedTreeVersion(b.treeVersion, dataVersion);
  const key = JSON.stringify([versionA, versionB, dataVersion]);
  const [loaded, setLoaded] = useState<{ key: string; a?: PassiveNode[]; b?: PassiveNode[]; error?: string }>();
  useEffect(() => {
    let cancelled = false;
    getBackend().then(async backend => {
      const [nodesA, nodesB] = await Promise.all([backend.loadPassiveTree(a.treeVersion), backend.loadPassiveTree(b.treeVersion)]);
      if (!cancelled) setLoaded({ key, a: nodesA, b: nodesB });
    }).catch(error => { if (!cancelled) setLoaded({ key, error: formatApiError(error) }); });
    return () => { cancelled = true; };
  }, [key]);
  const current = loaded?.key === key ? loaded : undefined;
  return <section className="compare-trees" aria-label={tt('compare.treeOverlay')}>
    <h3>{tt('compare.treeOverlay')}</h3>
    <p>A · {versionA ?? tt('compare.defaultVersion')} / B · {versionB ?? tt('compare.defaultVersion')}</p>
    {versionA !== versionB && <p className="compare-tree-warning" role="note">{tt('compare.treeMismatch')}</p>}
    {!current && <p role="status">{tt('compare.treeLoading')}</p>}
    {current?.error && <p role="alert">{current.error}</p>}
    {current?.a && current.b && <LoadedTreeComparison key={key} {...props} nodesA={current.a} nodesB={current.b} sameVersion={versionA === versionB} />}
  </section>;
}

type LoadedProps = Props & { nodesA: PassiveNode[]; nodesB: PassiveNode[]; sameVersion: boolean };

function LoadedTreeComparison({ a, b, before, after, nodesA, nodesB, sameVersion, meta, lang }: LoadedProps) {
  const tt = bindT(lang);
  const overlay = useMemo(() => compareTreeAllocations(a, b, nodesA, nodesB, sameVersion), [a, b, nodesA, nodesB, sameVersion]);
  const totals = useMemo(() => [treeAllocationTotals(a, nodesA), treeAllocationTotals(b, nodesB)], [a, b, nodesA, nodesB]);
  const visible = useMemo(() => {
    const asc = new Set<string>();
    for (const [state, nodes] of [[a, nodesA], [b, nodesB]] as const) {
      const character = meta?.classes.find(entry => entry.name === state.character.class_name);
      const id = character?.ascendancies?.find(entry => entry.name === state.character.ascendancy_name)?.id;
      if (id) asc.add(id);
      const allocated = new Set(state.allocatedNodes);
      for (const node of nodes) if (node.ascendancy_id && allocated.has(node.skill)) asc.add(node.ascendancy_id);
    }
    return overlay.nodes.filter(entry => hasPosition(entry.node) && (!entry.node.ascendancy_id || asc.has(entry.node.ascendancy_id)));
  }, [overlay, a, b, nodesA, nodesB, meta]);
  const full = useMemo(() => treeExtent(visible.map(entry => entry.node)), [visible]);
  const focus = useMemo(() => treeExtent(visible.filter(entry => entry.side !== 'none').map(entry => entry.node), 500, 2000) ?? full, [visible, full]);
  const [view, setView] = useState<ViewBox | null>(null);
  const [selectedKey, setSelectedKey] = useState('');
  const selected = overlay.nodes.find(entry => entry.key === selectedKey);
  const inspectable = overlay.nodes.filter(entry => entry.side !== 'none' || entry.jewelA || entry.jewelB || entry.key === selectedKey);
  const [inspectLabel, ...inspectNames] = useLocalizedLines([tt('compare.inspectNode'), ...inspectable.map(entry => entry.node.name ?? `#${entry.node.skill}`)], lang);
  const edges = useMemo(() => {
    const aa = new Set(a.allocatedNodes), bb = new Set(b.allocatedNodes);
    const catalogA = visible.flatMap(entry => entry.a ? [entry.a] : []);
    const catalogB = visible.flatMap(entry => entry.b ? [entry.b] : []);
    const groups = sameVersion ? [['both', catalogA] as const] : [['a', catalogA] as const, ['b', catalogB] as const];
    return groups.flatMap(([side, nodes]) => passiveTreeEdges(nodes).map(edge => {
      const inA = side !== 'b' && aa.has(edge.from) && aa.has(edge.to);
      const inB = side !== 'a' && bb.has(edge.from) && bb.has(edge.to);
      return { ...edge, side: inA ? inB ? 'both' : 'a' : inB ? 'b' : 'none' };
    }));
  }, [a, b, visible, sameVersion]);
  const paths = useMemo(() => (['none', 'a', 'b', 'both'] as const).map(side => ({ side,
    d: edges.filter(edge => edge.side === side).map(edge => `M${edge.x1},${edge.y1}L${edge.x2},${edge.y2}`).join(' '),
  })), [edges]);
  const svg = useRef<SVGSVGElement>(null);
  const drag = useRef<{ x: number; y: number; view: ViewBox; moved: boolean } | null>(null);
  const suppressClick = useRef(false);
  const currentView = view ?? focus;
  const zoom = (scale: number) => {
    if (!currentView || !full) return;
    const w = Math.max(400, Math.min(full.w * 1.5, currentView.w * scale));
    const factor = w / currentView.w;
    setView({ x: currentView.x + currentView.w * (1 - factor) / 2,
      y: currentView.y + currentView.h * (1 - factor) / 2, w, h: currentView.h * factor });
  };
  const select = useCallback((key: string) => setSelectedKey(key), []);
  const count = (side: string) => overlay.nodes.filter(entry => entry.side === side).length;
  const sockets = [...new Set([...a.jewels.map(jewel => jewel.socket_node), ...b.jewels.map(jewel => jewel.socket_node)])].sort((x, y) => x - y);
  const tableRow = (key: string, label: string, values: (number | null | undefined)[]) => <tr key={key} data-total={key}>
    <th scope="row">{label}</th>{values.map((value, index) => <td key={index}>{typeof value === 'number' && Number.isFinite(value) ? value.toLocaleString(lang) : '—'}</td>)}
  </tr>;
  return <>
    <ul className="compare-tree-legend" aria-label={tt('compare.legend')}>
      {(['a', 'b', 'both'] as const).map(side => <li key={side} data-side={side}>
        <span aria-hidden="true">{side === 'a' ? '●' : side === 'b' ? '■' : '◎'}</span>{' '}
        {tt(side === 'a' ? 'compare.removed' : side === 'b' ? 'compare.added' : 'compare.both')} ({count(side)})
      </li>)}
    </ul>
    {[overlay.unplacedA, overlay.unplacedB].map((ids, index) => ids.length > 0 && <details className="compare-tree-warning" key={index} open>
      <summary>{index ? 'B' : 'A'} · {tt('compare.unplaced')} ({ids.length})</summary><p>{ids.join(', ')}</p>
    </details>)}
    {currentView && full ? <>
      <div className="compare-tree-tools">
        <button type="button" onClick={() => setView(focus)}>{tt('compare.fitAllocations')}</button>
        <button type="button" onClick={() => setView(full)}>{tt('compare.wholeTree')}</button>
        <button type="button" aria-label={tt('compare.zoomIn')} onClick={() => zoom(.7)}>+</button>
        <button type="button" aria-label={tt('compare.zoomOut')} onClick={() => zoom(1 / .7)}>−</button>
      </div>
      <p className="compare-hint">{tt('compare.canvasHint')}</p>
      <svg ref={svg} className="compare-tree-canvas" role="group" tabIndex={0} aria-label={tt('compare.treeOverlay')}
        viewBox={`${currentView.x} ${currentView.y} ${currentView.w} ${currentView.h}`}
        onKeyDown={event => {
          if (event.target !== event.currentTarget) return;
          const shifts: Record<string, [number, number]> = { ArrowLeft: [-1, 0], ArrowRight: [1, 0], ArrowUp: [0, -1], ArrowDown: [0, 1] };
          if (shifts[event.key]) {
            event.preventDefault();
            const [x, y] = shifts[event.key];
            setView({ ...currentView, x: currentView.x + x * currentView.w * .1, y: currentView.y + y * currentView.h * .1 });
          } else if (event.key === '+' || event.key === '-') { event.preventDefault(); zoom(event.key === '+' ? .7 : 1 / .7); }
        }}
        onPointerDown={event => {
          if (event.button !== 0) return;
          drag.current = { x: event.clientX, y: event.clientY, view: currentView, moved: false };
          suppressClick.current = false;
          event.currentTarget.setPointerCapture(event.pointerId);
        }}
        onPointerMove={event => {
          const start = drag.current, matrix = svg.current?.getScreenCTM();
          if (!start || !matrix) return;
          const dx = event.clientX - start.x, dy = event.clientY - start.y;
          if (Math.hypot(dx, dy) > 4) start.moved = true;
          if (start.moved) setView({ ...start.view, x: start.view.x - dx / matrix.a, y: start.view.y - dy / matrix.d });
        }}
        onPointerUp={event => {
          suppressClick.current = drag.current?.moved ?? false;
          drag.current = null;
          if (event.currentTarget.hasPointerCapture(event.pointerId)) event.currentTarget.releasePointerCapture(event.pointerId);
        }}
        onPointerCancel={() => { drag.current = null; suppressClick.current = true; }}
        onClick={event => {
          if (suppressClick.current) return;
          // Pointer capture retargets clicks to the SVG. Hit-test the released position.
          const element = document.elementFromPoint(event.clientX, event.clientY)?.closest('[data-overlay-key]');
          if (element && svg.current?.contains(element)) select(element.getAttribute('data-overlay-key')!);
        }}>
        <TreeScene nodes={visible} paths={paths} selectedKey={selectedKey} select={select} />
      </svg>
    </> : <p role="note">{tt('compare.noCoordinates')}</p>}
    <label className="compare-tree-inspect">{inspectLabel}<select value={selected?.key ?? ''} onChange={event => {
      select(event.target.value);
      const node = overlay.nodes.find(entry => entry.key === event.target.value)?.node;
      if (node && hasPosition(node) && currentView) setView({ ...currentView, x: node.x - currentView.w / 2, y: node.y - currentView.h / 2 });
    }}><option value="">—</option>{inspectable.map((entry, index) => <option key={entry.key} value={entry.key}>
      {entry.side === 'both' ? 'A+B' : entry.side === 'a' ? 'A' : entry.side === 'b' ? 'B' : entry.a && entry.b ? 'A/B' : entry.a ? 'A' : 'B'} · #{entry.node.skill} · {inspectNames[index]}
    </option>)}</select></label>
    {selected && <div className="compare-pair compare-node-details" role="group" aria-label={tt('tree.nodeDetails')}>
      <NodeDetails node={selected.a} state={a} output={before} side="A" lang={lang} />
      <NodeDetails node={selected.b} state={b} output={after} side="B" lang={lang} />
    </div>}
    <div className="compare-table-wrap"><table className="compare-tree-totals"><caption>{tt('compare.treeTotals')}</caption>
      <thead><tr><th scope="col">{tt('compare.summary')}</th><th scope="col">A</th><th scope="col">B</th></tr></thead><tbody>
        {tableRow('passive', tt('compare.passivePoints'), totals.map(total => total.passive))}
        {tableRow('ascendancy', tt('compare.ascendancyPoints'), totals.map(total => total.ascendancy))}
        {(['Str', 'Dex', 'Int'] as const).map(id => tableRow(id, statNameLabel(lang, id), [before, after].map(output => output?.stats.find(stat => stat.id === id)?.value)))}
        {(['str', 'dex', 'int'] as const).map(id => tableRow(`choice-${id}`, `${tt('tree.attributeChoice')} · ${statNameLabel(lang, id[0].toUpperCase() + id.slice(1))}`, totals.map(total => total.attributes[id])))}
        {tableRow('choice-unassigned', tt('compare.unassigned'), totals.map(total => total.attributes.unassigned))}
      </tbody></table></div>
    <p className="compare-hint">{tt('compare.totalsHint')}</p>
    {totals.map((total, index) => total.unknown.length > 0 && <p className="compare-tree-warning" key={index}>{index ? 'B' : 'A'} · {tt('compare.unknownNodes')}: {total.unknown.join(', ')}</p>)}
    <h4>{tt('compare.socketJewels')}</h4>
    {!sockets.length && <p>{tt('compare.noJewels')}</p>}
    {sockets.map(id => <article className="compare-difference" key={id} data-socket={id}>
      <h4>#{id}</h4><div className="compare-pair">{[a, b].map((state, index) => <div key={index}>
        <strong>{index ? 'B' : 'A'} · {tt(state.allocatedNodes.includes(id) ? 'compare.allocated' : 'compare.inactiveJewel')}</strong>
        <pre>{state.jewels.find(jewel => jewel.socket_node === id)?.text ?? tt('compare.notSet')}</pre>
      </div>)}</div>
    </article>)}
    <details><summary>{tt('compare.passives')}</summary>{(['a', 'b', 'both'] as const).map(side => <p key={side}>
      {tt(side === 'a' ? 'compare.removed' : side === 'b' ? 'compare.added' : 'compare.both')}: {overlay.nodes.filter(entry => entry.side === side).map(entry => entry.node.skill).join(', ') || '—'}
    </p>)}</details>
  </>;
}

const TreeScene = memo(function TreeScene({ nodes, paths, selectedKey, select }: {
  nodes: OverlayNode[]; paths: { side: string; d: string }[]; selectedKey: string; select: (key: string) => void;
}) {
  return <>
    {paths.map(path => <path className="compare-tree-edge" key={path.side} data-side={path.side} d={path.d} />)}
    {[...nodes].sort((a, b) => Number(a.side !== 'none') - Number(b.side !== 'none')).map(entry => {
      const { node, side, key } = entry;
      const r = NODE_RADIUS[node.kind] ?? 40;
      return <g key={key} transform={`translate(${node.x},${node.y})`} className="compare-tree-node"
        data-side={side} data-skill-id={node.skill} data-overlay-key={key} data-selected={key === selectedKey || undefined}
        tabIndex={side === 'none' && !entry.jewelA && !entry.jewelB ? undefined : 0} role="button"
        aria-label={`${node.name ?? node.skill} · #${node.skill} · ${side === 'both' ? 'A+B' : side === 'a' ? 'A' : side === 'b' ? 'B' : '—'}`}
        onKeyDown={event => { if (event.key === 'Enter' || event.key === ' ') { event.preventDefault(); event.stopPropagation(); select(key); } }}>
        <title>{node.name ?? `#${node.skill}`} · #{node.skill}</title>
        {side === 'b' ? <rect x={-r} y={-r} width={r * 2} height={r * 2} /> : <circle r={r} />}
        {side === 'both' && <circle className="compare-tree-ring" r={r * 1.55} />}
        {entry.jewelA && <path data-jewel="a" d={`M${-r / 2},${-r / 2}l${r / 2},${r / 2}l${-r / 2},${r / 2}l${-r / 2},${-r / 2}Z`} />}
        {entry.jewelB && <path data-jewel="b" d={`M${r / 2},${-r / 2}l${r / 2},${r / 2}l${-r / 2},${r / 2}l${-r / 2},${-r / 2}Z`} />}
      </g>;
    })}
  </>;
});

function NodeDetails({ node, state, output, side, lang }: {
  node?: PassiveNode; state: StageState; output?: CalculateBuildResponse; side: string; lang: Lang;
}) {
  const tt = bindT(lang);
  const change = node && output?.tree_effects?.nodes[String(node.skill)];
  const lines = node ? [change?.name ?? node.name ?? `#${node.skill}`,
    ...(change?.replace ? change.stats : [...(node.stats ?? []), ...(change?.stats ?? [])])] : [];
  const translated = useLocalizedLines(lines, lang);
  return <div><strong>{side}</strong>{!node ? <p>{tt('compare.notSet')}</p> : <>
    <h4>{cleanLine(translated[0] ?? lines[0])} · #{node.skill}</h4>
    <p>{tt(state.allocatedNodes.includes(node.skill) ? 'compare.allocated' : 'compare.unallocated')}</p>
    <ul>{translated.slice(1).map((line, index) => <li key={index}>{cleanLine(line)}</li>)}</ul>
    {state.attributeChoices[String(node.skill)] && <p>{tt('tree.attributeChoice')}: {statNameLabel(lang, { str: 'Str', dex: 'Dex', int: 'Int' }[state.attributeChoices[String(node.skill)]])}</p>}
    {change && <p className="compare-hint">{tt('compare.jewelEffect')}</p>}
    {state.jewels.find(jewel => jewel.socket_node === node.skill) && <pre>{state.jewels.find(jewel => jewel.socket_node === node.skill)!.text}</pre>}
  </>}</div>;
}
