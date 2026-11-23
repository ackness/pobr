import { ModifierText } from '../../../components/calcs/ModifierText';
import { ConfigFieldLink } from '../../../components/config/ConfigFieldLink';
import { bindT, modTypeLabel } from '../../../lib/i18n';
import { useEffect, useMemo, useState } from 'react';
import { useCancellableJob } from '../../jobs/useCancellableJob';
import { AppButton, AppInput, AppSelectControl } from '../../../components/shared/controls';
import { getBackend } from '../../../api/backend';
import { formatApiError } from '../../../api/error';
import type { LineUsageLine, LineUsageResponse, LineUsageStatus } from '../../../api/types';
import type { GameNameRequest } from '../../../lib/nativeDisplayNames';
import type { BuildSession } from '../../../hooks/useBuildSession';
import { useSkillName } from '../../../hooks/useSkillName';
import { originKindLabel, originLabel, slotLabel, statNameLabel, type Lang } from '../../../lib/i18n';
import { colon } from '../../text';
import { configLabel } from '../../../lib/configLabels';
import { ledgerReasonLabel, ownerLabel } from '../../../lib/diagnosticText';
import { lineText } from '../../../lib/continuedLines';
import type { ConfigOption } from '../../../api/types';
import {
  ATTENTION, NO_FILTER, SOURCE_GROUPS, STATUS_ORDER, filterLedger, ledgerDisplayText, displayedLedgerReason, reasonConfigField, sourceGroupLabel, statusHint, statusLabel,
  statusTone, summarise, toggleStatus, type LedgerFilter, type SourceGroup,
} from './ledger';

const PAGE = 80;
const NO_NODE_IDS: ReadonlyMap<number, string> = new Map();

/** Lines are translated in batches the way Calcs does for modifier lines; English is shown as is.
 * A text item goes through `translateLines`, a name of known identity through `translateNames`. */
function useTranslated(items: readonly (string | GameNameRequest)[], lang: Lang): (item: string | GameNameRequest) => string {
  const [cache, setCache] = useState<{ lang: Lang; map: Record<string, string> }>({ lang, map: {} });
  const id = (item: string | GameNameRequest) => typeof item === 'string' ? item : JSON.stringify(item);
  const english = (item: string | GameNameRequest) => typeof item === 'string' ? item : item.name;
  const key = items.map(id).join('\n');
  useEffect(() => {
    if (lang === 'en-US') return;
    const known = cache.lang === lang ? cache.map : {};
    const pending = [...new Map(items.map(item => [id(item), item])).values()]
      .filter(item => english(item) && !(id(item) in known));
    if (pending.length === 0) return;
    const texts = pending.filter(item => typeof item === 'string');
    const names = pending.filter(item => typeof item !== 'string');
    let cancelled = false;
    getBackend().then(backend => Promise.all([
      texts.length ? backend.translateLines(texts, lang) : [],
      names.length ? backend.translateNames(names, lang) : [],
    ])).then(([translatedTexts, translatedNames]) => {
      if (cancelled) return;
      setCache(previous => {
        const base = previous.lang === lang ? previous.map : {};
        const map = { ...base };
        texts.forEach((text, index) => { map[text] = lineText(text, translatedTexts[index]); });
        names.forEach((name, index) => { map[id(name)] = translatedNames[index] ?? name.name; });
        return { lang, map };
      });
    }).catch(() => {});
    return () => { cancelled = true; };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [key, lang]);
  return item => lang === 'en-US' || cache.lang !== lang ? english(item) : cache.map[id(item)] ?? english(item);
}

/** A holder is a passive node's name for `node.<skill>` sources and the item's base type otherwise
 * (`usage::holder_name`). An unknown node id leaves the name to the text path. */
function holderName(line: LineUsageLine, nodeIds: ReadonlyMap<number, string>): GameNameRequest | null {
  if (!line.holder) return null;
  if (!line.source_id?.startsWith('node.')) return { kind: 'base', name: line.holder };
  return { kind: 'passive', id: nodeIds.get(Number(line.source_id.slice(5))) ?? '', name: line.holder };
}

function sourceLabel(lang: Lang, line: LineUsageLine, configLabels: Readonly<Record<string, string>>): string {
  const tt = bindT(lang);
  const id = line.source_id ?? '';
  if (id === 'campaign.resistance_penalty') return tt('calcs.campaignResistancePenalty');
  if (id === 'quest') return tt('ledger.campaignReward');
  if (id === 'config.custom') return tt('ledger.customModifier');
  if (id.startsWith('config.') && configLabels[id.slice(7)]) return configLabels[id.slice(7)];
  if (id.startsWith('node.')) return originKindLabel(lang, 'PassiveNode');
  if (id.startsWith('config.')) return tt('ledger.unnamedConfig');
  if (/^(item|flask|base)\./.test(id)) return originLabel(lang, id);
  if (id.startsWith('jewel.')) return originKindLabel(lang, 'Jewel');
  const label = line.source_kind ? originKindLabel(lang, line.source_kind) : '';
  return label && label !== line.source_kind ? label : tt('ledger.unnamedSource');
}

export function LedgerView({ session, lang }: { session: BuildSession; lang: Lang }) {
  const tt = bindT(lang);
  const skillName = useSkillName(lang);
  const [done, setDone] = useState<{ version: number; response: LineUsageResponse; options: ConfigOption[]; nodeIds: ReadonlyMap<number, string> } | null>(null);
  const job = useCancellableJob<void>();
  const running = job.running;
  const error = job.error ? formatApiError(job.error) : null;
  const [filter, setFilter] = useState<LedgerFilter>({ ...NO_FILTER, showInternal: false });
  const [limit, setLimit] = useState(PAGE);
  const [open, setOpen] = useState<ReadonlySet<number>>(new Set());
  const { stateVersion } = session;

  // A change to the BD makes the result an old one. Keep showing it, marked, and stop any check in flight.
  useEffect(() => { job.cancel(); }, [stateVersion, job.cancel]);

  const stale = done !== null && done.version !== stateVersion;
  const usage = done?.response.usage ?? null;
  const summary = useMemo(() => usage ? summarise(usage) : null, [usage]);
  const shown = useMemo(() => usage ? filterLedger(usage.lines, filter) : [], [usage, filter]);
  // Rows keep their place in the engine's list, so an opened row stays open while the filters change.
  const position = useMemo(() => new Map((usage?.lines ?? []).map((line, index) => [line, index])), [usage]);
  const visible = shown.slice(0, limit);
  const configLabels = useMemo(() => Object.fromEntries((done?.options ?? [])
    .map(option => [option.var, configLabel(lang, option)])), [done?.options, lang]);
  const nodeIds = done?.nodeIds ?? NO_NODE_IDS;
  const translate = useTranslated(visible.flatMap(line => [ledgerDisplayText(line, lang, configLabels), holderName(line, nodeIds) ?? '']), lang);

  const run = () => {
    if (running || session.busy || !session.calc) return;
    const request = session.currentRequest();
    if (!request) return;
    const version = stateVersion;
    return job.start(async ({ signal }) => {
      // The check is heavier than a calculation and blocks the page while it runs; let the busy state paint first.
      await new Promise(resolve => setTimeout(resolve, 40));
      if (signal.aborted) return;
      const backend = await getBackend();
      const [response, options, nodes] = await Promise.all([backend.lineUsage(request), backend.loadConfigOptions(),
        // Node holders are named from the request's tree; without it they keep the text path.
        backend.loadPassiveTree(request.tree_version).catch(() => [])]);
      if (signal.aborted) return;
      setDone({ version, response, options, nodeIds: new Map(nodes.map(node => [node.skill, node.id])) }); setFilter({ ...NO_FILTER, showInternal: false }); setLimit(PAGE); setOpen(new Set());
    });
  };

  const changeFilter = (next: LedgerFilter) => { setFilter(next); setLimit(PAGE); };
  const sameSet = (a: ReadonlySet<LineUsageStatus> | null, b: ReadonlySet<LineUsageStatus>) => a !== null && a.size === b.size && [...b].every(status => a.has(status));
  const attentionOn = sameSet(filter.statuses, ATTENTION);
  const single = filter.statuses?.size === 1 ? [...filter.statuses][0] : null;
  const groups = summary ? SOURCE_GROUPS.filter(group => summary.groups[group] > 0) : [];
  const toggleRow = (index: number) => setOpen(previous => { const next = new Set(previous); if (next.has(index)) next.delete(index); else next.add(index); return next; });
  const needsWaiting = usage?.conditions.filter(item => item.modifiers.length > 0) ?? [];
  const rejected = usage?.gem_stats ?? [];

  return <section className="ledger" aria-labelledby="ledger-heading" aria-busy={running}>
    <header className="ledger-head">
      <div>
        <h3 id="ledger-heading" tabIndex={-1}>{tt('calcs.modifierLedger')}</h3>
        <p className="calcs-hint">{tt('ledger.description')}</p>
      </div>
      <AppButton className="ledger-run" onClick={run} disabled={running || session.busy || !session.calc}>
        {running ? tt('ledger.checking') : done ? tt('ledger.checkAgain') : tt('ledger.checkLines')}
      </AppButton>
    </header>
    {running && <p className="calcs-hint" role="status">{tt('ledger.checkingHint')}</p>}
    {error && <div className="calc-error" role="alert">{error}</div>}
    {!done && !running && !error && <p className="ledger-empty">{tt('ledger.checkHint')}</p>}
    {done && summary && usage && <div className="ledger-result" data-stale={stale || undefined}>
      {stale && <p className="ledger-stale" role="status">{tt('ledger.stale')}</p>}
      <p className="ledger-headline">
        <strong>{summary.total}</strong> {tt('ledger.lines')}
        <span aria-hidden="true"> · </span><strong>{summary.appliedPercent}%</strong> {tt('ledger.appliedCount')}
        <span aria-hidden="true"> · </span><strong className={summary.attention > 0 ? 'is-warn' : undefined}>{summary.attention}</strong> {tt('ledger.attentionCount')}
        {usage.main_skill && <span className="ledger-skill">{tt('ledger.readFor', { skill: skillName(usage.main_skill) })}</span>}
      </p>
      <div className="ledger-bar" role="img" aria-label={STATUS_ORDER.filter(status => summary.counts[status] > 0).map(status => `${statusLabel(lang, status)} ${summary.counts[status]}`).join(', ')}>
        {STATUS_ORDER.filter(status => summary.counts[status] > 0).map(status =>
          <span key={status} data-status={status} data-tone={statusTone(status)} style={{ flexGrow: summary.counts[status] }} />)}
      </div>
      <ul className="ledger-legend" aria-label={tt('ledger.filterByStatus')}>
        {summary.attention > 0 && <li><button type="button" className="ledger-chip" data-tone="warn" aria-pressed={attentionOn}
          onClick={() => changeFilter({ ...filter, statuses: attentionOn ? null : new Set(ATTENTION) })}>
          {tt('ledger.attention')} <span>{summary.attention}</span></button></li>}
        {STATUS_ORDER.filter(status => summary.counts[status] > 0 || filter.statuses?.has(status)).map(status => <li key={status}>
          <button type="button" className="ledger-chip" data-tone={statusTone(status)} data-status={status}
            aria-pressed={filter.statuses?.has(status) ?? false}
            onClick={() => changeFilter({ ...filter, statuses: toggleStatus(filter.statuses, status) })}>
            <i aria-hidden="true" />{statusLabel(lang, status)} <span>{summary.counts[status]}</span></button></li>)}
      </ul>
      {single && <p className="ledger-hint">{statusHint(lang, single)}</p>}
      <div className="ledger-filters">
        <AppInput type="search" className="ledger-search" value={filter.query} aria-label={tt('ledger.search')}
          placeholder={tt('ledger.searchPlaceholder')}
          onChange={event => changeFilter({ ...filter, query: event.target.value })} />
        {groups.length > 1 && <AppSelectControl className="ledger-group" aria-label={tt('calcs.source')} value={filter.group}
          onChange={event => changeFilter({ ...filter, group: event.target.value as SourceGroup | 'all' })}>
          <option value="all">{tt('ledger.allSources')}</option>
          {groups.map(group => <option key={group} value={group}>{sourceGroupLabel(lang, group)} ({summary.groups[group]})</option>)}
        </AppSelectControl>}
        <label className="ledger-internal"><AppInput type="checkbox" checked={!!filter.showInternal} onChange={event => changeFilter({ ...filter, showInternal: event.target.checked })} />{tt('calcs.internalEntries')}</label>
        <span className="ledger-count" role="status">{tt('ledger.resultCount', { shown: shown.length, total: summary.total })}</span>
      </div>
      {shown.length === 0 ? <div className="ledger-none" role="status">
        <p>{tt('ledger.empty')}</p>
        <AppButton onClick={() => changeFilter({ ...NO_FILTER, showInternal: filter.showInternal })}>{tt('ledger.clearFilters')}</AppButton>
      </div> : <ul className="ledger-list">
        {visible.map(line => {
          const index = position.get(line) ?? 0;
          const tone = statusTone(line.status);
          const expanded = open.has(index);
          const holder = holderName(line, nodeIds);
          const owner = [...new Set([holder ? translate(holder) : null, sourceLabel(lang, line, configLabels)].filter(Boolean))].join(' · ');
          const reason = displayedLedgerReason(line, lang, Object.fromEntries(Object.entries(usage.condition_configs ?? {}).map(([condition, field]) => [condition, configLabels[field]]).filter((entry): entry is [string, string] => !!entry[1])));
          const field = reasonConfigField(line, usage.condition_configs);
          const displayText = ledgerDisplayText(line, lang, configLabels);
          return <li key={index} className="ledger-row" data-status={line.status} data-tone={tone}>
            <span className="ledger-status"><i aria-hidden="true" />{statusLabel(lang, line.status)}</span>
            <div className="ledger-body">
              <div className="ledger-text"><ModifierText raw={displayText} translated={translate(displayText)} lang={lang} /></div>
              {owner && <p className="ledger-meta">{owner}{lang !== 'en-US' && /[A-Za-z]{3}/.test(owner) && <small className="modifier-fallback">{tt('calcs.untranslated')}</small>}</p>}
              {reason.text && <p className="ledger-reason">{reason.text}</p>}
              {field && <ConfigFieldLink field={field} lang={lang} />}
              <details className="ledger-raw"><summary className="disclosure">{tt('calcs.details')}</summary><pre>{JSON.stringify({ text: line.text, source: line.source_id, reason: line.reason, modifiers: line.modifiers }, null, 2)}</pre></details>
              {line.used_in.length > 0 && <p className="ledger-meta">{tt('ledger.countsIn', { metrics: line.used_in.map(id => statNameLabel(lang, id) === id ? tt('ledger.unnamedMetric') : statNameLabel(lang, id)).join(' · ') })}</p>}
              {line.modifiers.length > 0 && <>
                <button type="button" className="ledger-toggle" aria-expanded={expanded} onClick={() => toggleRow(index)}>
                  {tt('ledger.modifiers')} ({line.modifiers.length})</button>
                {expanded && <table className="ledger-mods">
                  <thead><tr><th scope="col">{tt('ledger.name')}</th><th scope="col">{tt('common.type')}</th><th scope="col">{tt('ledger.value')}</th><th scope="col">{tt('ledger.state')}</th></tr></thead>
                  <tbody>{line.modifiers.map((mod, modIndex) => <tr key={modIndex} data-status={mod.status}>
                    <td><ModifierText raw={mod.name} lang={lang} /></td><td>{modTypeLabel(lang, mod.mod_type)}</td>
                    <td className="mod-value">{typeof mod.value === 'boolean' ? (mod.value ? tt('ledger.enabled') : tt('ledger.disabled')) : typeof mod.value === 'number' ? String(mod.value) : tt('ledger.seeDetails')}{mod.applied !== null && mod.applied !== mod.value ? ` → ${mod.applied}` : ''}</td>
                    <td>{statusLabel(lang, mod.status as LineUsageStatus)}{mod.reason ? ` · ${displayedLedgerReason({ ...line, reason: mod.reason }, lang).text}` : ''}</td>
                  </tr>)}</tbody>
                </table>}
              </>}
            </div>
          </li>;
        })}
      </ul>}
      {shown.length > visible.length && <div className="ledger-more-row">
        <AppButton onClick={() => setLimit(limit + PAGE)}>{tt('ledger.showMore', { count: Math.min(PAGE, shown.length - visible.length) })}</AppButton>
      </div>}
      {(needsWaiting.length > 0 || rejected.length > 0) && <div className="ledger-extra">
        {needsWaiting.length > 0 && <details>
          <summary className="disclosure" data-disclosure-level="group">{tt('ledger.waitingConditions', { count: needsWaiting.length })}</summary>
          <ul>{needsWaiting.map(item => {
            const field = item.config_option ?? usage.condition_configs?.[item.condition];
            return <li key={item.condition}><b>{configLabels[field ?? ''] ?? tt('ledger.unmetCondition')}</b>
              <span>{field ? <ConfigFieldLink field={field} lang={lang} /> : tt('ledger.noConfigField')}</span>
              <details><summary className="disclosure">{tt('calcs.details')}</summary><pre>{item.condition}</pre></details>
              <small>{tt('ledger.waitingCount', { count: item.modifiers.length })}</small>
            </li>;
          })}</ul>
        </details>}
        {rejected.length > 0 && <details>
          <summary className="disclosure" data-disclosure-level="group">{tt('ledger.rejectedSkills', { count: rejected.length })}</summary>
          <ul>{rejected.map(item => <li key={`${item.owner}:${item.stat}`} title={`${item.stat} · ${item.owner}${item.reason ? ` · ${item.reason}` : ''}`}><span>{ownerLabel(item.owner, skillName) === item.owner ? tt('ledger.unnamedSkill') : ownerLabel(item.owner, skillName)}</span>
            <small>{item.status === 'unmapped' ? tt('ledger.unmapped') : tt('ledger.unrecognized')}{item.reason ? ` · ${ledgerReasonLabel(item.reason, lang) ?? tt('calcs.unknownReason')}` : ''}</small>
            <details><summary className="disclosure">{tt('calcs.details')}</summary><pre>{item.owner}{'\n'}{item.stat}{'\n'}{item.reason}</pre></details></li>)}</ul>
        </details>}
      </div>}
      {done.response.item_errors.length > 0 && <ul className="ledger-errors">
        {done.response.item_errors.map((issue, index) => <li key={index} className="calc-error">{slotLabel(lang, issue.slot)}{colon(lang)}{issue.message}</li>)}
      </ul>}
    </div>}
  </section>;
}
