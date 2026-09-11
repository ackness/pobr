import { useEffect, useMemo, useRef, useState } from 'react';
import { getBackend } from '../../api/backend';
import type { BuildSession } from '../../hooks/useBuildSession';
import { useItemDisplayNames, useLocalizedLines } from '../../hooks/useLocalizedLines';
import { bindT, slotLabel, statNameLabel, type Lang, type UiKey } from '../../lib/i18n';
import { OBJECTIVE_PRESETS } from '../../lib/optimize';
import {
  REALM_DEFAULT_LEAGUE,
  REALM_LEAGUES,
  buildTradeUrl,
  loadTradeLeagues,
  type TradePriceCap,
  type TradeRealm,
} from '../../lib/trade';
import { AppSelect } from '../shared/AppSelect';
import { CopyButton } from '../shared/CopyButton';
import { OptimizerProgress } from '../shared/OptimizerControls';
import { affixPool, basesForSlot, loadTradeCatalog, optimizeTradeAffixes, type TradeCatalog, type TradeBase, type TradeOptimization } from '../../lib/tradeOptimizer';
import { evaluateMarket, searchMarket, rankMarket, planGemUpgrades, evaluateGemMarket, type MarketRanking, type MarketUpgrade, type MarketQuery, type GemPlan } from '../../lib/tradeMarket';
import { readTradeExport, tradeExportBookmark, gemTradeUrl } from '../../lib/tradeExport';
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
  'belt', 'Flask 1', 'Flask 2', 'Charm 1', 'Charm 2', 'Charm 3',
];

const REALM_KEY = 'pobr-trade-realm';
const leagueKey = (realm: TradeRealm) => `pobr-trade-league-${realm}`;
const BUDGET_KEY = 'pobr-trade-budget';
const CURRENCY_KEY = 'pobr-trade-currency';

/** Retain the stored equiv key, now using explicit exalted quotes for value comparisons. */
type BudgetCurrency = 'equiv' | 'divine' | 'chaos';
const CURRENCY_OPTIONS: { value: BudgetCurrency; labelKey: UiKey }[] = [
  { value: 'equiv', labelKey: 'trade.curExalted' },
  { value: 'divine', labelKey: 'trade.curDivine' },
  { value: 'chaos', labelKey: 'trade.curChaos' },
];

type SlotResult = Partial<TradeOptimization> & { category?: string; market?: MarketRanking; total?: number; sampled?: number; searchMode?: string; error?: string; gemPlans?: GemPlan[] };

/** Category-constrained market search with full recalculation of each actual listing. */
export function TradePanel({ session, lang }: Props) {
  const tt = bindT(lang);
  const [catalog, setCatalog] = useState<TradeCatalog | null>(null);
  const [catalogError, setCatalogError] = useState(false);
  const [bases, setBases] = useState<Record<string, string>>({});
  const [itemLevel, setItemLevel] = useState(82);
  const [jewelSockets, setJewelSockets] = useState<number[]>([]);
  const [realm, setRealm] = useState<TradeRealm>(
    () => localStorage.getItem(REALM_KEY) === 'cn' ? 'cn' : 'intl',
  );
  const [league, setLeague] = useState(
    () => localStorage.getItem(leagueKey(realm)) ?? REALM_DEFAULT_LEAGUE[realm],
  );
  const [leagues, setLeagues] = useState(REALM_LEAGUES[realm]);
  const [leagueFallback, setLeagueFallback] = useState(false);
  const [preset, setPreset] = useState('dps');
  const [budget, setBudget] = useState(() => localStorage.getItem(BUDGET_KEY) ?? '100');
  const [currency, setCurrency] = useState<BudgetCurrency>(
    () => (localStorage.getItem(CURRENCY_KEY) as BudgetCurrency) ?? 'equiv',
  );
  const [sortBy, setSortBy] = useState('gain');
  const [gemGroup, setGemGroup] = useState(0);
  const [running, setRunning] = useState<string | null>(null);
  const [progress, setProgress] = useState<{ done: number; total: number } | null>(null);
  const [results, setResults] = useState<Record<string, SlotResult>>({});
  const abortRef = useRef<AbortController | null>(null);

  useEffect(() => {
    loadTradeCatalog().then(setCatalog).catch(() => setCatalogError(true));
    getBackend().then(backend => backend.loadPassiveTree()).then(nodes => {
      setJewelSockets(nodes.filter(node => node.kind === 'jewel_socket').map(node => node.skill));
    }).catch(() => {});
  }, []);
  useEffect(() => {
    let cancelled = false;
    setLeagues(REALM_LEAGUES[realm]);
    setLeagueFallback(false);
    loadTradeLeagues(realm).then(list => {
      if (cancelled) return;
      setLeagues(list);
      // Keep an explicitly stored league/custom name. Fresh sessions follow the live default.
      if (localStorage.getItem(leagueKey(realm)) === null) setLeague(list[0]);
    }).catch(() => { if (!cancelled) setLeagueFallback(true); });
    return () => { cancelled = true; };
  }, [realm]);
  useEffect(() => {
    abortRef.current?.abort();
    setResults({});
  }, [preset, session.currentRequest, bases, itemLevel, realm, league, budget, currency, gemGroup]);
  useEffect(() => () => abortRef.current?.abort(), []);

  const slots = [...TRADE_SLOTS, ...jewelSockets.filter(node => session.allocatedNodes.includes(node)).map(node => `Jewel@${node}`)];
  const labelOf = (slot: string) => slot.startsWith('Jewel@') ? `${tt('trade.jewelSocket')} ${slot.slice(6)}` : slotLabel(lang, slot);
  const bySlot = new Map([...session.items, ...session.flasks,
    ...session.jewels.map(jewel => ({ slot: `Jewel@${jewel.socket_node}`, text: jewel.text })),
  ].map(item => [item.slot, item.text]));
  const slotNames = useItemDisplayNames(
    slots.map((slot) => bySlot.get(slot)),
    lang,
  );

  // Prices and results are invalidated together when the market inputs change.
  const priceCap = useMemo<TradePriceCap | undefined>(() => {
    const max = Number(budget);
    if (!Number.isFinite(max) || max <= 0) return undefined;
    return { max, currency: currency === 'equiv' ? 'exalted' : currency };
  }, [budget, currency]);

  const urlOf = (result: SlotResult) => buildTradeUrl(league, result.weighted ?? [], {
    category: result.category!, realm, price: priceCap, minimumWeight: result.minimumWeight,
  });
  const baseOf = (slot: string): TradeBase | undefined => {
    if (!catalog) return undefined;
    const available = basesForSlot(catalog, slot);
    const lines = new Set((bySlot.get(slot) ?? '').split('\n').map(line => line.trim()));
    return available.find(base => base.name === bases[slot]) ?? available.find(base => lines.has(base.name)) ?? available[0];
  };

  // 词条明细本地化（flat 展开 → 按槽切回）。
  const detailLines = slots.map((slot) => {
    const r = results[slot];
    return r?.weighted ? r.weighted.map((w) => w.line) : [];
  });
  const localizedFlat = useLocalizedLines(detailLines.flat(), lang);
  const localizedOf = (slotIdx: number): string[] => {
    const offset = detailLines.slice(0, slotIdx).reduce((acc, l) => acc + l.length, 0);
    return localizedFlat.slice(offset, offset + detailLines[slotIdx].length);
  };

  const objective = (() => {
    const p = OBJECTIVE_PRESETS.find(entry => entry.id === preset) ?? OBJECTIVE_PRESETS[0];
    return { stat: p.stat, per: p.per, constraints: [] };
  })();
  const run = async (selected: string[]) => {
    const request = session.currentRequest();
    if (!request || !catalog) return;
    const controller = new AbortController();
    abortRef.current = controller;
    try {
      for (const slot of selected) {
        if (controller.signal.aborted) break;
        setRunning(slot);
        setProgress({ done: 0, total: 1 });
        setResults(prev => ({ ...prev, [slot]: {} }));
        const options = { signal: controller.signal, onProgress: (done: number, total: number) => setProgress({ done, total }) };
        try {
          if (slot === 'gems') {
            const plans = await planGemUpgrades(request, catalog, gemGroup, objective, options.signal, options.onProgress);
            setResults(prev => ({ ...prev, [slot]: { gemPlans: plans } }));
            let ranking: MarketRanking = { baseline: {}, upgrades: [], rejected: 0 };
            let total = 0, sampled = 0;
            for (const plan of plans) {
              const market = await searchMarket({ realm, league, category: 'gem', price: priceCap,
                maxLevel: session.character?.level, gem: { name: plan.gem.name, level: plan.level, quality: plan.quality } }, options.signal);
              const result = await evaluateGemMarket(request, plan, market, objective, options.signal);
              const ranked = rankMarket([...ranking.upgrades, ...result.upgrades]);
              ranking = { baseline: Object.keys(result.baseline).length ? result.baseline : ranking.baseline,
                upgrades: ranked.filter((entry, index) => ranked.findIndex(other => other.listing.id === entry.listing.id) === index),
                rejected: ranking.rejected + result.rejected };
              total += market.total; sampled += market.sampled;
              setResults(prev => ({ ...prev, [slot]: { gemPlans: plans, market: ranking, total, sampled } }));
            }
            setResults(prev => ({ ...prev, [slot]: { gemPlans: plans, market: ranking, total, sampled } }));
          } else {
            const base = baseOf(slot);
            if (!base) continue;
            const pool = [...new Map(catalog.bases.filter(entry => entry.category === base.category)
              .flatMap(entry => affixPool(catalog, entry, itemLevel)).map(mod => [mod.id, mod])).values()];
            const weights = await optimizeTradeAffixes({ request, slot, base, pool, itemLevel, objective,
              combinations: false, ...options });
            setResults(prev => ({ ...prev, [slot]: { ...weights, category: base.category } }));
            const market = await searchMarket({ realm, league, category: base.category, weighted: weights.weighted,
              price: priceCap, maxLevel: session.character?.level }, options.signal);
            const ranking = await evaluateMarket({ request, slot, market, objective, ...options });
            if (!controller.signal.aborted) setResults(prev => ({ ...prev, [slot]: {
              ...weights, category: base.category, market: ranking, total: market.total, sampled: market.sampled, searchMode: market.search_mode,
            } }));
          }
        } catch (err) {
          if (controller.signal.aborted) break;
          setResults(prev => ({ ...prev, [slot]: { ...prev[slot], error: err instanceof Error ? err.message : String(err) } }));
          // Stop a multi-slot scan on an upstream failure; no automatic retries or request storm.
          break;
        }
      }
    } finally {
      if (abortRef.current === controller) {
        setRunning(null); setProgress(null); abortRef.current = null;
      }
    }
  };
  const importCandidates = async (slot: string, file: File, plan?: GemPlan) => {
    const request = session.currentRequest();
    const result = results[slot];
    if (!request || !result || (!result.category && !plan)) return;
    const controller = new AbortController();
    abortRef.current = controller; setRunning(slot);
    try {
      if (file.size > 4 * 1024 * 1024) throw new Error('Trade export is too large.');
      const market = readTradeExport(await file.text(), { realm, league, category: plan ? 'gem' : result.category!, price: priceCap, maxLevel: session.character?.level,
        ...(plan ? { gem: { name: plan.gem.name, level: plan.level, quality: plan.quality } } : {}) });
      const ranking = plan ? await evaluateGemMarket(request, plan, market, objective, controller.signal)
        : await evaluateMarket({ request, slot, market, objective, signal: controller.signal,
          onProgress: (done, total) => setProgress({ done, total }) });
      controller.signal.throwIfAborted();
      const all = rankMarket([...(plan ? result.market?.upgrades ?? [] : []), ...ranking.upgrades]);
      const merged = { ...ranking, baseline: Object.keys(ranking.baseline).length ? ranking.baseline : result.market?.baseline ?? {}, upgrades: all.filter((entry, index) => all.findIndex(other => other.listing.id === entry.listing.id) === index) };
      setResults(prev => ({ ...prev, [slot]: { ...result, error: undefined, market: merged, sampled: market.sampled + (plan ? result.sampled ?? 0 : 0), total: market.total + (plan ? result.total ?? 0 : 0), searchMode: 'weighted' } }));
    } catch (error) {
      if (!controller.signal.aborted) setResults(prev => ({ ...prev, [slot]: { ...prev[slot], error: error instanceof Error ? error.message : String(error) } }));
    } finally {
      if (abortRef.current === controller) { setRunning(null); setProgress(null); abortRef.current = null; }
    }
  };
  const valueCurrency = sortBy === 'value' ? (currency === 'equiv' ? 'exalted' : currency) : undefined;
  const applyUpgrade = (upgrade: MarketUpgrade) => {
    const variant = upgrade.variant;
    if (variant.set_items) session.setItems([...session.items.filter(item => !variant.set_items!.some(next => next.slot === item.slot)), ...variant.set_items]);
    if (variant.jewels) session.setJewels(variant.jewels);
    if (variant.flasks) session.setFlasks(variant.flasks);
    if (variant.socket_groups) session.setSocketGroups(variant.socket_groups);
  };
  const renderMarket = (result: SlotResult) => result.market && <div className="trade-market">
    {result.searchMode === 'budget' && <p className="items-hint">{tt('trade.budgetSearch')}</p>}
    <p className="items-hint">{tt('trade.marketCoverage')}: {result.sampled ?? 0} / {result.total ?? 0}
      {result.market.rejected > 0 ? ` · ${tt('trade.rejected')}: ${result.market.rejected}` : ''}</p>
    {result.market.upgrades.length === 0 && <p>{tt('trade.noImprovement')}</p>}
    {rankMarket(result.market.upgrades, valueCurrency).slice(0, 10).map(upgrade =>
      <MarketCard key={upgrade.listing.id} upgrade={upgrade} baseline={result.market!.baseline} lang={lang}
        onApply={() => applyUpgrade(upgrade)} />)}
  </div>;
  const combined = rankMarket(Object.values(results).flatMap(result => result.market?.upgrades ?? []), valueCurrency);

  return (
    <section aria-labelledby="trade-heading">
      <h2 id="trade-heading" className="panel-heading">
        {tt('trade.title')}
      </h2>
      <p className="items-hint">{tt('trade.hint')}</p>

      {catalogError ? (
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
                value={leagues.includes(league) ? league : '__custom'}
                options={[
                  ...leagues.map((name) => ({
                    value: name,
                    label: name,
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
            {!leagues.includes(league) && (
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

          {leagueFallback && <p className="items-hint">{tt('trade.leagueFallback')}</p>}
          <label className="trade-ilvl">
            {tt('trade.itemLevel')}
            <input type="number" min={1} max={100} value={itemLevel}
              onChange={event => setItemLevel(Math.min(100, Math.max(1, Number(event.target.value) || 1)))} />
          </label>
          <p className="items-hint">{tt('trade.marketHint')}</p>
          <div className="opt-row">
            <label>{tt('trade.sort')} <select value={sortBy} onChange={event => setSortBy(event.target.value)}>
              <option value="gain">{tt('trade.sortGain')}</option><option value="value">{tt('trade.sortValue')}</option>
            </select></label>
            <button disabled={running !== null || session.busy || !catalog || !league.trim()}
              onClick={() => void run(slots.filter(slot => bySlot.has(slot)))}>{tt('trade.scanEquipped')}</button>
          </div>
          {Object.values(results).filter(result => result.market).length > 1 && combined.length > 0 && <details className="trade-overall" open>
            <summary>{tt('trade.overall')}</summary>
            {combined.slice(0, 5).map((upgrade, index) => {
              const source = Object.entries(results).find(([, result]) => result.market?.upgrades.includes(upgrade));
              return <div key={`${index}:${upgrade.listing.id}`}>
                <strong>#{index + 1} · {source?.[0] === 'gems' ? tt('trade.gems') : labelOf(source?.[0] ?? '')}</strong>
                <MarketCard upgrade={upgrade} baseline={source?.[1].market?.baseline ?? {}} lang={lang} onApply={() => applyUpgrade(upgrade)} />
              </div>;
            })}
          </details>}
          {!slots.some(slot => slot.startsWith('Jewel@')) && <p className="items-hint">{tt('trade.jewelHint')}</p>}
          <ul className="trade-slots">
            {slots.map((slot, idx) => {
              const text = bySlot.get(slot);
              const result = results[slot];
              const isRunning = running === slot;
              const base = baseOf(slot);
              const available = catalog ? basesForSlot(catalog, slot) : [];
              const categories = [...new Set(available.map(entry => entry.category))];
              return (
                <li key={slot} className="trade-slot-row">
                  <div className="trade-slot-main">
                    <span className="trade-slot-name">{labelOf(slot)}</span>
                    <span className={`trade-slot-item${text ? '' : ' is-empty'}`}>
                      {slotNames[idx] || tt('trade.slotEmpty')}
                    </span>
                    <select aria-label={`${labelOf(slot)} ${tt('trade.category')}`}
                      value={base?.category ?? ''} onChange={event => {
                        const next = available.find(entry => entry.category === event.target.value);
                        if (next) setBases(prev => ({ ...prev, [slot]: next.name }));
                      }}>
                      {categories.map(category => <option key={category} value={category}>
                        {tt(`trade.category.${category}` as UiKey)}
                      </option>)}
                    </select>
                    <select aria-label={`${labelOf(slot)} ${tt('trade.base')}`}
                      value={base?.name ?? ''} onChange={event => setBases(prev => ({ ...prev, [slot]: event.target.value }))}>
                      {available.filter(entry => entry.category === base?.category).map(entry =>
                        <option key={entry.name} value={entry.name}>{entry.name}</option>)}
                    </select>
                    <button
                      className="opt-run"
                      disabled={!base || session.busy || running !== null || !league.trim()}
                      onClick={() => void run([slot])}
                    >
                      {isRunning ? tt('opt.running') : tt('trade.findBetter')}
                    </button>
                    {result?.weighted && result.category && (
                      <>
                        <a href={urlOf(result)} target="_blank" rel="noreferrer">
                          <button>{tt('trade.openSite')}</button>
                        </a>
                        <CopyButton text={urlOf(result)} lang={lang} />
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
                  {result?.error && <p className="opt-error">{result.error}</p>}
                  {result?.weighted && result.category && <SignedInImport searchUrl={urlOf(result)} query={{
                    realm, league, category: result.category, price: priceCap, maxLevel: session.character?.level,
                  }} disabled={running !== null || session.busy} label={labelOf(slot)} lang={lang} onImport={file => void importCandidates(slot, file)} />}
                  {result && renderMarket(result)}
                  {result?.weighted && (
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
          <div className="trade-gems">
            <h3>{tt('trade.gems')}</h3><p className="items-hint">{tt('trade.gemHint')}</p>
            <select aria-label={tt('trade.gemGroup')} value={gemGroup} onChange={event => setGemGroup(Number(event.target.value))}>
              {session.socketGroups.map((group, index) => <option key={index} value={index} disabled={!group.enabled || Boolean(group.source)}>
                {index + 1}. {catalog?.gems?.find(gem => gem.skill_id === group.gems[0]?.skill_id)?.name ?? group.gems[0]?.skill_id}
              </option>)}
            </select>
            <button disabled={!catalog || running !== null || session.busy || !league.trim() || !session.socketGroups[gemGroup]?.enabled || Boolean(session.socketGroups[gemGroup]?.source)}
              onClick={() => void run(['gems'])}>{tt('trade.findBetter')}</button>
            {running === 'gems' && progress && <OptimizerProgress {...progress} onCancel={() => abortRef.current?.abort()} lang={lang} />}
            {results.gems?.error && <p className="opt-error">{results.gems.error}</p>}
            {results.gems?.gemPlans?.map(plan => {
              const query: MarketQuery = { realm, league, category: 'gem', price: priceCap, maxLevel: session.character?.level,
                gem: { name: plan.gem.name, level: plan.level, quality: plan.quality } };
              const url = gemTradeUrl(query);
              return <div key={`${plan.gem.skill_id}:${plan.level}:${plan.quality}`}>
                <a href={url} target="_blank" rel="noreferrer">{plan.gem.name} · {plan.level} / {plan.quality}%</a>
                <SignedInImport searchUrl={url} query={query} disabled={running !== null || session.busy} label={plan.gem.name} lang={lang}
                  onImport={file => void importCandidates('gems', file, plan)} />
              </div>;
            })}
            {results.gems && renderMarket(results.gems)}
          </div>
        </>
      )}
    </section>
  );
}

function MarketCard({ upgrade, baseline, lang, onApply }: {
  upgrade: MarketUpgrade; baseline: Record<string, number>; lang: Lang; onApply: () => void;
}) {
  const tt = bindT(lang);
  const signed = (value: number) => `${value >= 0 ? '+' : ''}${value.toLocaleString(lang, { maximumFractionDigits: 2 })}`;
  return <article className="trade-market-card">
    <div className="trade-slot-main"><strong>{upgrade.listing.item.name || upgrade.listing.item.baseType || upgrade.listing.item.typeLine}</strong>
      <span>{upgrade.listing.price.amount} {upgrade.listing.price.currency}</span>
      <strong className="opt-score">{tt('trade.gain')}: {signed(upgrade.gain)}</strong>
      {upgrade.listing.price.amount > 0 && <span>{(upgrade.gain / upgrade.listing.price.amount).toFixed(2)} / {upgrade.listing.price.currency}</span>}
      <button onClick={onApply}>{tt('trade.tryOn')}</button>
      <a href={upgrade.url} target="_blank" rel="noreferrer">{tt('trade.openSite')}</a>
      <CopyButton text={upgrade.text} lang={lang} />
    </div>
    <div className="trade-deltas">{['TotalDPS', 'Life', 'TotalEHP', 'FireResist', 'ColdResist', 'LightningResist', 'ChaosResist'].map(stat => {
      const before = baseline[stat] ?? 0, after = upgrade.stats[stat] ?? 0;
      const delta = after - before;
      return <span key={stat} className={delta < 0 ? 'opt-error' : ''}>{statNameLabel(lang, stat)}: {signed(delta)}
        {before !== 0 ? ` (${signed(delta / Math.abs(before) * 100)}%)` : ''}</span>;
    })}</div>
    <details><summary>{tt('trade.itemDetails')}</summary><pre>{upgrade.text}</pre></details>
    {!!upgrade.listing.item.requirements?.length && <p className="items-hint">{tt('trade.requirements')}: {upgrade.listing.item.requirements.map(req => `${req.name} ${req.values[0]?.[0] ?? ''}`).join(', ')}</p>}
    {upgrade.warnings.length > 0 && <details className="trade-warnings"><summary>{tt('trade.uncertain')} ({upgrade.warnings.length})</summary>
      <pre>{[...new Set(upgrade.warnings)].join('\n')}</pre></details>}
  </article>;
}

function SignedInImport({ searchUrl, query, disabled, label, lang, onImport }: {
  searchUrl: string; query: MarketQuery; disabled: boolean; label: string; lang: Lang; onImport: (file: File) => void;
}) {
  const tt = bindT(lang);
  const bookmark = tradeExportBookmark(searchUrl, query);
  return <details className="trade-export" open={query.realm === 'cn'}>
    <summary>{tt('trade.signedIn')}</summary><p className="items-hint">{tt('trade.exportHint')}</p>
    <a draggable ref={node => { node?.setAttribute('href', bookmark); }} onClick={event => event.preventDefault()}>{tt('trade.exportBookmark')}</a>
    <CopyButton label={tt('trade.copyBookmark')} text={bookmark} lang={lang} />
    <label>{tt('trade.importCandidates')}<input aria-label={`${label} ${tt('trade.importCandidates')}`} type="file" accept=".json,application/json"
      disabled={disabled} onChange={event => {
        const file = event.target.files?.[0]; event.target.value = '';
        if (file) onImport(file);
      }} /></label>
  </details>;
}
