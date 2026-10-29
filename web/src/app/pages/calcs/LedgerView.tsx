import { useEffect, useMemo, useRef, useState } from 'react';
import { AppButton, AppInput, AppSelectControl } from '../../../components/shared/controls';
import { getBackend } from '../../../api/backend';
import { formatApiError } from '../../../api/error';
import type { LineUsageLine, LineUsageResponse, LineUsageStatus } from '../../../api/types';
import type { BuildSession } from '../../../hooks/useBuildSession';
import { useSkillName } from '../../../hooks/useSkillName';
import { originKindLabel, originLabel, slotLabel, statNameLabel, type Lang } from '../../../lib/i18n';
import { words } from '../../text';
import {
  ATTENTION, NO_FILTER, SOURCE_GROUPS, STATUS_ORDER, filterLedger, lineText, sourceGroupLabel, statusHint, statusLabel,
  statusTone, summarise, toggleStatus, type LedgerFilter, type SourceGroup,
} from './ledger';

const PAGE = 80;

/** Lines are translated in batches the way Calcs does for modifier lines; English is shown as is. */
function useTranslated(texts: readonly string[], lang: Lang): (text: string) => string {
  const [cache, setCache] = useState<{ lang: Lang; map: Record<string, string> }>({ lang, map: {} });
  const key = texts.join('\n');
  useEffect(() => {
    if (lang === 'en-US') return;
    const known = cache.lang === lang ? cache.map : {};
    const pending = [...new Set(texts)].filter(text => text && !(text in known));
    if (pending.length === 0) return;
    let cancelled = false;
    getBackend().then(backend => backend.translateLines(pending, lang)).then(translated => {
      if (cancelled) return;
      setCache(previous => {
        const base = previous.lang === lang ? previous.map : {};
        const map = { ...base };
        pending.forEach((en, index) => { map[en] = translated[index] ?? en; });
        return { lang, map };
      });
    }).catch(() => {});
    return () => { cancelled = true; };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [key, lang]);
  return text => lang === 'en-US' || cache.lang !== lang ? text : cache.map[text] ?? text;
}

function sourceLabel(lang: Lang, line: LineUsageLine): string {
  const w = (en: string, cn: string, tw = cn) => words(lang, en, cn, tw);
  const id = line.source_id ?? '';
  if (id === 'quest') return w('Campaign reward', '任务奖励', '任務獎勵');
  if (id === 'config.custom') return w('Custom modifier', '自定义词条', '自訂詞綴');
  if (/^(item|flask|node|jewel|config|base)\./.test(id)) return originLabel(lang, id);
  return line.source_kind ? originKindLabel(lang, line.source_kind) : id;
}

export function LedgerView({ session, lang }: { session: BuildSession; lang: Lang }) {
  const w = (en: string, cn: string, tw = cn) => words(lang, en, cn, tw);
  const skillName = useSkillName(lang);
  const [done, setDone] = useState<{ version: number; response: LineUsageResponse } | null>(null);
  const [running, setRunning] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [filter, setFilter] = useState<LedgerFilter>(NO_FILTER);
  const [limit, setLimit] = useState(PAGE);
  const [open, setOpen] = useState<ReadonlySet<number>>(new Set());
  const sequence = useRef(0);
  const { stateVersion } = session;

  // A change to the BD makes the result an old one. Keep showing it, marked, and stop any check in flight.
  useEffect(() => { sequence.current++; setRunning(false); return () => { sequence.current++; }; }, [stateVersion]);

  const stale = done !== null && done.version !== stateVersion;
  const usage = done?.response.usage ?? null;
  const summary = useMemo(() => usage ? summarise(usage) : null, [usage]);
  const shown = useMemo(() => usage ? filterLedger(usage.lines, filter) : [], [usage, filter]);
  // Rows keep their place in the engine's list, so an opened row stays open while the filters change.
  const position = useMemo(() => new Map((usage?.lines ?? []).map((line, index) => [line, index])), [usage]);
  const visible = shown.slice(0, limit);
  const translate = useTranslated(visible.map(lineText), lang);

  const run = async () => {
    if (running || session.busy || !session.calc) return;
    const request = session.currentRequest();
    if (!request) return;
    const current = ++sequence.current;
    const version = stateVersion;
    setRunning(true); setError(null);
    try {
      // The check is heavier than a calculation and blocks the page while it runs; let the busy state paint first.
      await new Promise(resolve => setTimeout(resolve, 40));
      if (current !== sequence.current) return;
      const response = await (await getBackend()).lineUsage(request);
      if (current !== sequence.current) return;
      setDone({ version, response }); setFilter(NO_FILTER); setLimit(PAGE); setOpen(new Set());
    } catch (err) {
      if (current === sequence.current) setError(formatApiError(err));
    } finally {
      if (current === sequence.current) setRunning(false);
    }
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
        <h3 id="ledger-heading" tabIndex={-1}>{w('Line ledger', '词条账本', '詞綴帳本')}</h3>
        <p className="calcs-hint">{w('Every line of this BD and what the calculation did with it: counted, waiting for a condition, or lost.',
          '这个 BD 的每一行词条，以及计算对它做了什么：已计入、在等条件，还是丢失。',
          '這個 BD 的每一行詞綴，以及計算對它做了什麼：已計入、在等條件，還是遺失。')}</p>
      </div>
      <AppButton className="ledger-run" onClick={run} disabled={running || session.busy || !session.calc}>
        {running ? w('Checking…', '正在检查…', '正在檢查…') : done ? w('Check again', '重新检查', '重新檢查') : w('Check lines', '检查词条', '檢查詞綴')}
      </AppButton>
    </header>
    {running && <p className="calcs-hint" role="status">{w('Checking every line. This is slower than a calculation.', '正在逐行检查，比普通计算慢一些。', '正在逐行檢查，比一般計算慢一些。')}</p>}
    {error && <div className="calc-error" role="alert">{error}</div>}
    {!done && !running && !error && <p className="ledger-empty">{w('Check the lines to see what became of each one. It takes a few seconds.', '检查词条，看看每一行最终去了哪里。需要几秒钟。', '檢查詞綴，看看每一行最終去了哪裡。需要幾秒鐘。')}</p>}
    {done && summary && usage && <div className="ledger-result" data-stale={stale || undefined}>
      {stale && <p className="ledger-stale" role="status">{w('The BD changed after this check. Check again to refresh it.', '检查之后 BD 又有改动。重新检查即可刷新。', '檢查之後 BD 又有改動。重新檢查即可更新。')}</p>}
      <p className="ledger-headline">
        <strong>{summary.total}</strong> {w('lines', '行词条', '行詞綴')}
        <span aria-hidden="true"> · </span><strong>{summary.appliedPercent}%</strong> {w('applied', '已生效', '已生效')}
        <span aria-hidden="true"> · </span><strong className={summary.attention > 0 ? 'is-warn' : undefined}>{summary.attention}</strong> {w('need a look', '需要留意', '需要留意')}
        {usage.main_skill && <span className="ledger-skill">{w('Read for', '按此技能读取：', '依此技能讀取：')} {skillName(usage.main_skill)}</span>}
      </p>
      <div className="ledger-bar" role="img" aria-label={STATUS_ORDER.filter(status => summary.counts[status] > 0).map(status => `${statusLabel(lang, status)} ${summary.counts[status]}`).join(', ')}>
        {STATUS_ORDER.filter(status => summary.counts[status] > 0).map(status =>
          <span key={status} data-status={status} data-tone={statusTone(status)} style={{ flexGrow: summary.counts[status] }} />)}
      </div>
      <ul className="ledger-legend" aria-label={w('Filter by fate', '按去向筛选', '依去向篩選')}>
        {summary.attention > 0 && <li><button type="button" className="ledger-chip" data-tone="warn" aria-pressed={attentionOn}
          onClick={() => changeFilter({ ...filter, statuses: attentionOn ? null : new Set(ATTENTION) })}>
          {w('Needs a look', '需要留意', '需要留意')} <span>{summary.attention}</span></button></li>}
        {STATUS_ORDER.filter(status => summary.counts[status] > 0 || filter.statuses?.has(status)).map(status => <li key={status}>
          <button type="button" className="ledger-chip" data-tone={statusTone(status)} data-status={status}
            aria-pressed={filter.statuses?.has(status) ?? false}
            onClick={() => changeFilter({ ...filter, statuses: toggleStatus(filter.statuses, status) })}>
            <i aria-hidden="true" />{statusLabel(lang, status)} <span>{summary.counts[status]}</span></button></li>)}
      </ul>
      {single && <p className="ledger-hint">{statusHint(lang, single)}</p>}
      <div className="ledger-filters">
        <AppInput type="search" className="ledger-search" value={filter.query} aria-label={w('Search lines', '搜索词条', '搜尋詞綴')}
          placeholder={w('Search lines, items, nodes, stats…', '搜索词条、装备、节点、属性…', '搜尋詞綴、裝備、節點、屬性…')}
          onChange={event => changeFilter({ ...filter, query: event.target.value })} />
        {groups.length > 1 && <AppSelectControl className="ledger-group" aria-label={w('Source', '来源', '來源')} value={filter.group}
          onChange={event => changeFilter({ ...filter, group: event.target.value as SourceGroup | 'all' })}>
          <option value="all">{w('All sources', '全部来源', '全部來源')}</option>
          {groups.map(group => <option key={group} value={group}>{sourceGroupLabel(lang, group)} ({summary.groups[group]})</option>)}
        </AppSelectControl>}
        <span className="ledger-count" role="status">{w(`${shown.length} of ${summary.total} lines`, `${shown.length} / ${summary.total} 行`, `${shown.length} / ${summary.total} 行`)}</span>
      </div>
      {shown.length === 0 ? <div className="ledger-none" role="status">
        <p>{w('No lines match.', '没有符合条件的词条。', '沒有符合條件的詞綴。')}</p>
        <AppButton onClick={() => changeFilter(NO_FILTER)}>{w('Clear filters', '清除筛选', '清除篩選')}</AppButton>
      </div> : <ul className="ledger-list">
        {visible.map(line => {
          const index = position.get(line) ?? 0;
          const tone = statusTone(line.status);
          const expanded = open.has(index);
          const owner = [...new Set([line.holder, sourceLabel(lang, line)].filter(Boolean))].join(' · ');
          return <li key={index} className="ledger-row" data-status={line.status} data-tone={tone}>
            <span className="ledger-status"><i aria-hidden="true" />{statusLabel(lang, line.status)}</span>
            <div className="ledger-body">
              <p className="ledger-text">{translate(lineText(line))}</p>
              {owner && <p className="ledger-meta">{owner}</p>}
              {line.reason && <p className="ledger-reason">{line.reason}</p>}
              {line.used_in.length > 0 && <p className="ledger-meta">{w('Counts in', '计入', '計入')}: {line.used_in.map(id => statNameLabel(lang, id)).join(' · ')}</p>}
              {line.modifiers.length > 0 && <>
                <button type="button" className="ledger-toggle" aria-expanded={expanded} onClick={() => toggleRow(index)}>
                  {w('Modifiers', '词条项', '詞綴項')} ({line.modifiers.length})</button>
                {expanded && <table className="ledger-mods">
                  <thead><tr><th scope="col">{w('Name', '名称', '名稱')}</th><th scope="col">{w('Type', '类型', '類型')}</th><th scope="col">{w('Value', '数值', '數值')}</th><th scope="col">{w('State', '状态', '狀態')}</th></tr></thead>
                  <tbody>{line.modifiers.map((mod, modIndex) => <tr key={modIndex} data-status={mod.status}>
                    <td>{mod.name}</td><td>{mod.mod_type}</td>
                    <td className="mod-value">{String(mod.value)}{mod.applied !== null && mod.applied !== mod.value ? ` → ${mod.applied}` : ''}</td>
                    <td>{statusLabel(lang, mod.status as LineUsageStatus)}{mod.reason ? ` · ${mod.reason}` : ''}</td>
                  </tr>)}</tbody>
                </table>}
              </>}
            </div>
          </li>;
        })}
      </ul>}
      {shown.length > visible.length && <div className="ledger-more-row">
        <AppButton onClick={() => setLimit(limit + PAGE)}>{w(`Show ${Math.min(PAGE, shown.length - visible.length)} more`, `再显示 ${Math.min(PAGE, shown.length - visible.length)} 行`, `再顯示 ${Math.min(PAGE, shown.length - visible.length)} 行`)}</AppButton>
      </div>}
      {(needsWaiting.length > 0 || rejected.length > 0) && <div className="ledger-extra">
        {needsWaiting.length > 0 && <details>
          <summary>{w(`Conditions waiting to be set (${needsWaiting.length})`, `等待设置的条件（${needsWaiting.length}）`, `等待設定的條件（${needsWaiting.length}）`)}</summary>
          <ul>{needsWaiting.map(item => <li key={item.condition}><b>{item.condition}</b>
            <span>{item.config_option ? w(`Turn on “${item.config_option}” in Combat setup`, `在战斗配置中打开“${item.config_option}”`, `在戰鬥配置中開啟「${item.config_option}」`) : w('Nothing sets this', '没有任何来源能设置它', '沒有任何來源能設定它')}</span>
            <small>{w(`${item.modifiers.length} waiting`, `${item.modifiers.length} 条在等待`, `${item.modifiers.length} 條在等待`)}</small></li>)}</ul>
        </details>}
        {rejected.length > 0 && <details>
          <summary>{w(`Skill stats the engine did not use (${rejected.length})`, `引擎没有使用的技能 stat（${rejected.length}）`, `引擎沒有使用的技能 stat（${rejected.length}）`)}</summary>
          <ul>{rejected.map(item => <li key={`${item.owner}:${item.stat}`}><b>{item.stat}</b><span>{item.owner}</span>
            <small>{item.status === 'unmapped' ? w('Mapped, but rejected', '有映射但被拒绝', '有映射但被拒絕') : w('No mapping', '没有映射', '沒有映射')}{item.reason ? ` · ${item.reason}` : ''}</small></li>)}</ul>
        </details>}
      </div>}
      {done.response.item_errors.length > 0 && <ul className="ledger-errors">
        {done.response.item_errors.map((issue, index) => <li key={index} className="calc-error">{slotLabel(lang, issue.slot)}: {issue.message}</li>)}
      </ul>}
    </div>}
  </section>;
}
