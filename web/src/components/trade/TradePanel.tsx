import { useEffect, useMemo, useRef, useState } from 'react';
import { getBackend } from '../../api/backend';
import type { TradeStatEntry, VariantInput } from '../../api/types';
import type { BuildSession } from '../../hooks/useBuildSession';
import { useItemDisplayNames, useLocalizedLines } from '../../hooks/useLocalizedLines';
import { bindT, slotLabel, type Lang, type UiKey } from '../../lib/i18n';
import { OBJECTIVE_PRESETS, evaluateVariants, scoreOf } from '../../lib/optimize';
import {
  REALM_DEFAULT_LEAGUE,
  REALM_LEAGUES,
  buildTradeUrl,
  lineValue,
  loadTradeMap,
  normalizeTradeLine,
  type TradePriceCap,
  type TradeRealm,
  type WeightedStat,
} from '../../lib/trade';
import { AppSelect } from '../shared/AppSelect';
import { CopyButton } from '../shared/CopyButton';
import { OptimizerProgress } from '../shared/OptimizerControls';
import './trade.css';

interface Props {
  session: BuildSession;
  lang: Lang;
}

/** 参与市集搜索的装备槽（PoB2 Trader 同序：武器 → 甲 → 首饰）。 */
const TRADE_SLOTS = [
  'weapon1',
  'weapon2',
  'helmet',
  'bodyarmour',
  'gloves',
  'boots',
  'amulet',
  'ring1',
  'ring2',
  'belt',
];

const REALM_KEY = 'pobr-trade-realm';
const leagueKey = (realm: TradeRealm) => `pobr-trade-league-${realm}`;
const BUDGET_KEY = 'pobr-trade-budget';
const CURRENCY_KEY = 'pobr-trade-currency';

/** 预算通货：equiv = 官方缺省「崇高石等价」（站方自动换算所有报价）。 */
type BudgetCurrency = 'equiv' | 'divine' | 'chaos';
const CURRENCY_OPTIONS: { value: BudgetCurrency; labelKey: UiKey }[] = [
  { value: 'equiv', labelKey: 'trade.curExalted' },
  { value: 'divine', labelKey: 'trade.curDivine' },
  { value: 'chaos', labelKey: 'trade.curChaos' },
];

type SlotResult = { weighted: WeightedStat[] } | { error: string };

/** 摘掉物品文本中与词条对应的那一行（比对剥 {…} 后的整行）。 */
function removeLine(text: string, target: string): string {
  const lines = text.split('\n');
  const idx = lines.findIndex((raw) => raw.replace(/\{[^}]*\}/g, '').trim() === target);
  if (idx < 0) return text;
  return [...lines.slice(0, idx), ...lines.slice(idx + 1)].join('\n');
}

/**
 * 市集页（PoB2 Trader 的对应物）：逐槽位生成「按对你的提升排序 + 预算上限」的
 * 官方市集搜索。权重 = 当前物品逐词条摘除重算的边际收益（与 PoB2 同口径）。
 */
export function TradePanel({ session, lang }: Props) {
  const tt = bindT(lang);
  const [tradeMap, setTradeMap] = useState<Record<string, TradeStatEntry> | null>(null);
  const [realm, setRealm] = useState<TradeRealm>(
    () => (localStorage.getItem(REALM_KEY) as TradeRealm) ?? 'intl',
  );
  const [league, setLeague] = useState(
    () => localStorage.getItem(leagueKey(realm)) ?? REALM_DEFAULT_LEAGUE[realm],
  );
  const [preset, setPreset] = useState('dps');
  const [budget, setBudget] = useState(() => localStorage.getItem(BUDGET_KEY) ?? '');
  const [currency, setCurrency] = useState<BudgetCurrency>(
    () => (localStorage.getItem(CURRENCY_KEY) as BudgetCurrency) ?? 'equiv',
  );
  const [running, setRunning] = useState<string | null>(null);
  const [progress, setProgress] = useState<{ done: number; total: number } | null>(null);
  const [results, setResults] = useState<Record<string, SlotResult>>({});
  const abortRef = useRef<AbortController | null>(null);

  useEffect(() => {
    loadTradeMap().then(setTradeMap);
  }, []);
  // 目标变了 → 已算的权重口径过期，清掉。
  useEffect(() => setResults({}), [preset]);

  const bySlot = new Map(session.items.map((item) => [item.slot, item.text]));
  const slotNames = useItemDisplayNames(
    TRADE_SLOTS.map((slot) => bySlot.get(slot)),
    lang,
  );

  // 预算：非法/空 = 不限；服务器/赛季/预算改动即时反映到已生成的链接（权重不用重算）。
  const priceCap = useMemo<TradePriceCap | undefined>(() => {
    const max = Number(budget);
    if (!Number.isFinite(max) || max <= 0) return undefined;
    return { max, ...(currency === 'equiv' ? {} : { currency }) };
  }, [budget, currency]);

  const urlOf = (weighted: WeightedStat[]) => buildTradeUrl(league, weighted, 0.7, realm, priceCap);

  // 词条明细本地化（flat 展开 → 按槽切回）。
  const detailLines = TRADE_SLOTS.map((slot) => {
    const r = results[slot];
    return r && 'weighted' in r ? r.weighted.map((w) => w.line) : [];
  });
  const localizedFlat = useLocalizedLines(detailLines.flat(), lang);
  const localizedOf = (slotIdx: number): string[] => {
    const offset = detailLines.slice(0, slotIdx).reduce((acc, l) => acc + l.length, 0);
    return localizedFlat.slice(offset, offset + detailLines[slotIdx].length);
  };

  const run = async (slot: string) => {
    const itemText = bySlot.get(slot);
    const request = session.currentRequest();
    if (!itemText || !request || !tradeMap) return;
    const backend = await getBackend();
    const classified = await backend.classifyItemLines(itemText);
    const mapped = classified
      .filter((l) => l.kind === 'explicit')
      .flatMap((l) => {
        const entry = tradeMap[normalizeTradeLine(l.text)];
        const value = lineValue(l.text);
        return entry && value !== null ? [{ line: l.text, entry, value }] : [];
      });
    if (mapped.length === 0) {
      setResults((prev) => ({ ...prev, [slot]: { error: tt('trade.noMapped') } }));
      return;
    }
    const presetDef = OBJECTIVE_PRESETS.find((p) => p.id === preset) ?? OBJECTIVE_PRESETS[0];
    const objective = { stat: presetDef.stat, per: presetDef.per, constraints: [] };
    const variants: VariantInput[] = mapped.map((m) => ({
      label: m.line,
      set_items: [{ slot, text: removeLine(itemText, m.line) }],
    }));
    const controller = new AbortController();
    abortRef.current = controller;
    setRunning(slot);
    setProgress({ done: 0, total: variants.length });
    try {
      const { baseline, results: variantStats } = await evaluateVariants({
        request,
        variants,
        signal: controller.signal,
        onProgress: (done, total) => setProgress({ done, total }),
      });
      const baseScore = scoreOf(baseline, objective);
      const weighted: WeightedStat[] = variantStats.flatMap((r) => {
        const m = mapped[r.index];
        if (!m || r.error) return [];
        // 边际收益：摘掉该词条后目标下降多少（≤0 = 对目标无贡献，不进权重）。
        const delta = baseScore - scoreOf(r.stats, objective);
        if (delta <= 0) return [];
        return [{ id: m.entry.id, weight: delta / m.value, line: m.line, value: m.value }];
      });
      setResults((prev) => ({
        ...prev,
        [slot]: weighted.length === 0 ? { error: tt('trade.noWeights') } : { weighted },
      }));
    } catch (err: unknown) {
      setResults((prev) => ({
        ...prev,
        [slot]: { error: err instanceof Error ? err.message : String(err) },
      }));
    } finally {
      setRunning(null);
      setProgress(null);
      abortRef.current = null;
    }
  };

  return (
    <section aria-labelledby="trade-heading">
      <h2 id="trade-heading" className="panel-heading">
        {tt('trade.title')}
      </h2>
      <p className="items-hint">{tt('trade.hint')}</p>

      {tradeMap && Object.keys(tradeMap).length === 0 ? (
        <p className="items-hint">{tt('trade.unavailable')}</p>
      ) : (
        <>
          <div className="opt-row trade-controls">
            <label>
              {tt('trade.realm')}
              <AppSelect
                value={realm}
                options={[
                  { value: 'intl', label: tt('trade.realmIntl') },
                  { value: 'cn', label: tt('trade.realmCn') },
                ]}
                onChange={(v) => {
                  const next = v as TradeRealm;
                  setRealm(next);
                  localStorage.setItem(REALM_KEY, next);
                  setLeague(localStorage.getItem(leagueKey(next)) ?? REALM_DEFAULT_LEAGUE[next]);
                }}
                ariaLabel={tt('trade.realm')}
              />
            </label>
            <label>
              {tt('trade.league')}
              <AppSelect
                value={REALM_LEAGUES[realm].includes(league) ? league : '__custom'}
                options={[
                  ...REALM_LEAGUES[realm].map((name, i) => ({
                    value: name,
                    label: i === 0 ? `${name}（${tt('trade.leagueSeason')}）` : name,
                  })),
                  { value: '__custom', label: tt('trade.leagueCustom') },
                ]}
                onChange={(v) => {
                  const next = v === '__custom' ? '' : v;
                  setLeague(next);
                  localStorage.setItem(leagueKey(realm), next);
                }}
                ariaLabel={tt('trade.league')}
              />
            </label>
            {!REALM_LEAGUES[realm].includes(league) && (
              <label>
                {tt('trade.leagueCustom')}
                <input
                  value={league}
                  placeholder={REALM_DEFAULT_LEAGUE[realm]}
                  onChange={(e) => {
                    setLeague(e.target.value);
                    localStorage.setItem(leagueKey(realm), e.target.value);
                  }}
                />
              </label>
            )}
            <label>
              {tt('opt.objective')}
              <AppSelect
                value={preset}
                options={OBJECTIVE_PRESETS.map((p) => ({
                  value: p.id,
                  label: tt(p.labelKey as UiKey),
                }))}
                onChange={setPreset}
                ariaLabel={tt('opt.objective')}
              />
            </label>
            <label>
              {tt('trade.budget')}
              <span className="trade-budget">
                <input
                  type="number"
                  min={0}
                  value={budget}
                  placeholder={tt('trade.budgetAny')}
                  onChange={(e) => {
                    setBudget(e.target.value);
                    localStorage.setItem(BUDGET_KEY, e.target.value);
                  }}
                />
                <AppSelect
                  value={currency}
                  options={CURRENCY_OPTIONS.map((c) => ({ value: c.value, label: tt(c.labelKey) }))}
                  onChange={(v) => {
                    setCurrency(v as BudgetCurrency);
                    localStorage.setItem(CURRENCY_KEY, v);
                  }}
                  ariaLabel={tt('trade.budget')}
                />
              </span>
            </label>
          </div>

          <ul className="trade-slots">
            {TRADE_SLOTS.map((slot, idx) => {
              const text = bySlot.get(slot);
              const result = results[slot];
              const isRunning = running === slot;
              return (
                <li key={slot} className="trade-slot-row">
                  <div className="trade-slot-main">
                    <span className="trade-slot-name">{slotLabel(lang, slot)}</span>
                    <span className={`trade-slot-item${text ? '' : ' is-empty'}`}>
                      {slotNames[idx] || tt('trade.slotEmpty')}
                    </span>
                    <button
                      className="opt-run"
                      disabled={!text || session.busy || running !== null || !league.trim()}
                      title={text ? undefined : tt('trade.emptyHint')}
                      onClick={() => void run(slot)}
                    >
                      {isRunning ? tt('opt.running') : tt('trade.findBetter')}
                    </button>
                    {result && 'weighted' in result && (
                      <>
                        <a href={urlOf(result.weighted)} target="_blank" rel="noreferrer">
                          <button>{tt('trade.openSite')}</button>
                        </a>
                        <CopyButton text={urlOf(result.weighted)} lang={lang} />
                      </>
                    )}
                  </div>
                  {isRunning && progress && (
                    <OptimizerProgress
                      done={progress.done}
                      total={progress.total}
                      onCancel={() => abortRef.current?.abort()}
                      lang={lang}
                    />
                  )}
                  {result && 'error' in result && <p className="opt-error">{result.error}</p>}
                  {result && 'weighted' in result && (
                    <details className="trade-details">
                      <summary>
                        {result.weighted.length} {tt('trade.details')}
                      </summary>
                      <table className="opt-table">
                        <thead>
                          <tr>
                            <th>{tt('trade.colLine')}</th>
                            <th>{tt('trade.colWeight')}</th>
                          </tr>
                        </thead>
                        <tbody>
                          {result.weighted.map((w, i) => (
                            <tr key={w.id}>
                              <td>{localizedOf(idx)[i] ?? w.line}</td>
                              <td className="opt-score">{w.weight.toFixed(3)}</td>
                            </tr>
                          ))}
                        </tbody>
                      </table>
                    </details>
                  )}
                </li>
              );
            })}
          </ul>
        </>
      )}
    </section>
  );
}
