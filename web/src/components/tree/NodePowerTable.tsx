import { AppButton, AppInput, AppSelectControl } from '../shared/controls';
import { useLayoutEffect, useMemo, useRef, useState } from 'react';
import type { AttributeChoice, PassiveNode } from '../../api/types';
import type { BuildSession } from '../../hooks/useBuildSession';
import { formatApiError } from '../../api/error';
import { statNameLabel, type Lang } from '../../lib/i18n';
import { passivePlanningContext, withTravelAttributes } from '../../lib/passivePlanner';
import { evaluateNodePower, filterPowerRows, powerCandidates, rankPowerRows, type PowerRow, type PowerSort } from '../../lib/nodePower';
import { powerLabels } from './nodePowerLabels';
import './nodePower.css';

interface Props {
  session: BuildSession;
  nodes: PassiveNode[];
  lang: Lang;
  stat: string;
  stats: string[];
  onStat: (stat: string) => void;
  onFocus: (node: PassiveNode) => void;
}
const PAGE_SIZE = 50;

export function NodePowerTable({ session, nodes, lang, stat, stats, onStat, onFocus }: Props) {
  const t = powerLabels[lang];
  const [open, setOpen] = useState(false);
  const [search, setSearch] = useState('');
  const [small, setSmall] = useState(false);
  const [attribute, setAttribute] = useState<AttributeChoice>('int');
  const [sort, setSort] = useState<PowerSort>('perPoint');
  const [ascending, setAscending] = useState(false);
  const [page, setPage] = useState(0);
  const [completed, setCompleted] = useState<{ key: string; rows: Map<number, PowerRow>; limits: string[] } | null>(null);
  const [progress, setProgress] = useState<{ done: number; total: number } | null>(null);
  const [message, setMessage] = useState<'cancelled' | 'complete' | null>(null);
  const [error, setError] = useState<string | null>(null);
  const abort = useRef<AbortController | null>(null);
  const request = useMemo(() => session.currentRequest(), [session.currentRequest, session.stateVersion]);
  const ascendancy = session.treeMeta?.classes.flatMap(c => c.ascendancies ?? []).find(a => a.name === session.character?.ascendancy_name)?.id;
  const key = JSON.stringify([request, session.stateVersion, session.dataVersion, session.calc?.tree_effects, stat, attribute]);
  const latest = useRef(key);
  latest.current = key;
  const context = useMemo(() => passivePlanningContext(nodes, session.allocatedNodes, session.character?.class_name,
    session.weaponSwap?.exclusive_nodes.flat() ?? [], session.jewels.map(j => j.socket_node), ascendancy, session.calc?.tree_effects),
  [nodes, session.allocatedNodes, session.character?.class_name, session.weaponSwap, session.jewels, ascendancy, session.calc?.tree_effects]);
  const candidates = useMemo(() => powerCandidates(nodes, context, session.allocatedNodes, ascendancy), [nodes, context, session.allocatedNodes, ascendancy]);
  const results = completed?.key === key ? completed : null;
  const filtered = filterPowerRows(candidates.map(row => results?.rows.get(row.node.skill) ?? row), search, small);
  const ranked = rankPowerRows(filtered, sort, ascending);
  const pending = filtered.filter(row => row.status === 'pending').length;
  const currentPage = Math.min(page, Math.max(0, Math.ceil(ranked.length / PAGE_SIZE) - 1));
  const visible = ranked.slice(currentPage * PAGE_SIZE, (currentPage + 1) * PAGE_SIZE);

  useLayoutEffect(() => {
    abort.current?.abort(); abort.current = null;
    setProgress(null); setCompleted(null); setMessage(null); setError(null); setPage(0);
  }, [key]);
  useLayoutEffect(() => () => { abort.current?.abort(); }, []);
  const cancel = () => { abort.current?.abort(); abort.current = null; setProgress(null); setMessage('cancelled'); };
  const run = async () => {
    if (!request || progress || session.busy || !stat || !pending) return;
    const controller = new AbortController();
    abort.current = controller;
    setProgress({ done: 0, total: Math.min(128, pending) }); setMessage(null); setError(null);
    try {
      await evaluateNodePower({ request: withTravelAttributes(request, nodes, attribute), rows: filtered, stat,
        signal: controller.signal, onBatch: (rows, done, total, limits) => {
          if (controller.signal.aborted || latest.current !== key || abort.current !== controller) return;
          setCompleted(previous => ({ key, limits, rows: new Map([...(previous?.key === key ? previous.rows : []), ...rows.map(row => [row.node.skill, row] as const)]) }));
          setProgress({ done, total });
        } });
      if (!controller.signal.aborted && latest.current === key && abort.current === controller) setMessage('complete');
    } catch (err) {
      if (!controller.signal.aborted && latest.current === key) setError(formatApiError(err));
    } finally {
      if (abort.current === controller) { abort.current = null; setProgress(null); }
    }
  };
  const changeSort = (next: PowerSort) => {
    setAscending(next === sort ? !ascending : next === 'node' || next === 'kind' || next === 'points'); setSort(next); setPage(0);
  };
  const number = (value: number | null) => value === null ? '—' : value.toLocaleString(lang, { maximumFractionDigits: 2 });
  return <details className="node-power" open={open} onToggle={event => {
    const next = event.currentTarget.open; setOpen(next); if (!next && abort.current) cancel();
  }}>
    <summary>{t.title}</summary>
    {open && <>
      <p>{t.hint}</p>
      <div className="node-power-controls">
        <label>{t.stat}<AppSelectControl value={stat} onChange={e => onStat(e.target.value)}>{stats.map(id => <option key={id} value={id}>{statNameLabel(lang, id)}</option>)}</AppSelectControl></label>
        <label>{t.attribute}<AppSelectControl value={attribute} onChange={e => setAttribute(e.target.value as AttributeChoice)}>{(['str', 'dex', 'int'] as const).map(id => <option key={id} value={id}>{t[id]}</option>)}</AppSelectControl></label>
        <label>{t.search}<AppInput type="search" value={search} onChange={e => { if (abort.current) cancel(); setSearch(e.target.value); setPage(0); }} /></label>
        <label className="node-power-small"><AppInput type="checkbox" checked={small} onChange={e => { if (abort.current) cancel(); setSmall(e.target.checked); setPage(0); }} />{t.small}</label>
      </div>
      <div className="node-power-actions">
        <AppButton disabled={session.busy || Boolean(session.calcError) || !session.calc || !pending || Boolean(progress) || !stats.includes(stat)} onClick={run}>{t.run}</AppButton>
        {progress && <AppButton onClick={cancel}>{t.cancel}</AppButton>}
        <AppButton disabled={Boolean(progress) || !results} onClick={() => { setCompleted(null); setMessage(null); }}>{t.refresh}</AppButton>
      </div>
      <p role="status">{progress ? `${progress.done} / ${progress.total}` : message ? t[message] : ''}</p>
      <p>{t.results}: {filtered.length} · {t.evaluated}: {filtered.filter(r => r.status === 'ready').length} · {t.remaining}: {pending}</p>
      {error && <p role="alert">{error}</p>}
      {results && results.limits.length > 0 && <details><summary>{t.limits} ({results.limits.length})</summary><ul>{results.limits.map(line => <li key={line}>{line}</li>)}</ul></details>}
      <div className="node-power-scroll" tabIndex={0} role="region" aria-label={t.title}>
        <table aria-label={t.title}>
          <thead><tr>{(['node', 'perPoint', 'points', 'gain', 'kind', 'single'] as const).map(id => <th key={id} aria-sort={sort === id ? ascending ? 'ascending' : 'descending' : 'none'}><AppButton onClick={() => changeSort(id)}>{t[id]}</AppButton></th>)}</tr></thead>
          <tbody>{visible.map(row => <tr key={row.node.skill} data-node-id={row.node.skill} data-status={row.status}>
            <td><AppButton onClick={() => onFocus(row.node)} disabled={row.node.x === undefined || row.node.y === undefined}>{row.node.name ?? row.node.id}</AppButton><small>#{row.node.skill}{row.node.ascendancy_id ? ` · ${t.asc}` : ''}</small>
              {row.status !== 'ready' && <small>{t[row.status]}</small>}
              {row.diagnostics.length > 0 && <details><summary>{t[row.status]}</summary>{row.diagnostics.map((text, i) => <p key={i}>{text}</p>)}</details>}
            </td>
            <td>{number(row.perPoint)}</td><td>{row.path?.length ?? '—'}</td><td>{number(row.gain)}</td>
            <td>{t[row.node.kind as 'normal' | 'notable' | 'keystone']}</td><td>{number(row.single)}</td>
          </tr>)}</tbody>
        </table>
        {visible.length === 0 && <p>{t.empty}</p>}
      </div>
      <div className="node-power-actions"><AppButton disabled={currentPage === 0} onClick={() => setPage(currentPage - 1)}>{t.previous}</AppButton><span>{t.page} {currentPage + 1} / {Math.max(1, Math.ceil(ranked.length / PAGE_SIZE))}</span><AppButton disabled={(currentPage + 1) * PAGE_SIZE >= ranked.length} onClick={() => setPage(currentPage + 1)}>{t.next}</AppButton></div>
      <p>{t.scope}</p>
    </>}
  </details>;
}
