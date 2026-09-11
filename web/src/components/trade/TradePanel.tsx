import { useEffect, useMemo, useRef, useState } from 'react';
import { getBackend } from '../../api/backend';
import type { BuildSession } from '../../hooks/useBuildSession';
import { useItemDisplayNames, useLocalizedLines } from '../../hooks/useLocalizedLines';
import { useSkillName } from '../../hooks/useSkillName';
import { bindT, slotLabel, statNameLabel, type Lang, type UiKey } from '../../lib/i18n';
import { OBJECTIVE_PRESETS, scoreOf } from '../../lib/optimize';
import { REALM_DEFAULT_LEAGUE, REALM_LEAGUES, buildTradeUrl, gemTradeUrl, loadTradeLeagues, type TradePriceCap, type TradeRealm } from '../../lib/trade';
import { basesForSlot, categoryAffixPool, loadTradeCatalog, optimizeTradeAffixes, referenceBase, type TradeCatalog, type TradeOptimization } from '../../lib/tradeOptimizer';
import { planGemUpgrades, type GemPlan } from '../../lib/tradeMarket';
import { AppSelect } from '../shared/AppSelect';
import { CopyButton } from '../shared/CopyButton';
import { OptimizerProgress } from '../shared/OptimizerControls';
import './trade.css';

const TRADE_SLOTS = ['weapon1', 'weapon2', 'helmet', 'bodyarmour', 'gloves', 'boots', 'amulet', 'ring1', 'ring2', 'belt', 'Flask 1', 'Flask 2', 'Charm 1', 'Charm 2', 'Charm 3'];
const REALM_KEY = 'pobr-trade-realm';
const leagueKey = (realm: TradeRealm) => `pobr-trade-league-${realm}`;
const BUDGET_KEY = 'pobr-trade-budget';
const CURRENCY_KEY = 'pobr-trade-currency';
type Currency = 'equiv' | 'divine' | 'chaos';
type SlotResult = { category?: string; weights?: TradeOptimization; gems?: GemPlan[]; error?: string };

/** Local build analysis produces official search links; login and buying stay on the market. */
export function TradePanel({ session, lang }: { session: BuildSession; lang: Lang }) {
  const tt = bindT(lang);
  const skillName = useSkillName(lang);
  const cnSkillName = useSkillName('zh-CN');
  const [catalog, setCatalog] = useState<TradeCatalog | null>(null);
  const [catalogError, setCatalogError] = useState(false);
  const [jewelSockets, setJewelSockets] = useState<number[]>([]);
  const [selected, setSelected] = useState('weapon1');
  const [categories, setCategories] = useState<Record<string, string>>({});
  const [realm, setRealm] = useState<TradeRealm>(() => localStorage.getItem(REALM_KEY) === 'cn' ? 'cn' : 'intl');
  const [league, setLeague] = useState(() => localStorage.getItem(leagueKey(realm)) ?? REALM_DEFAULT_LEAGUE[realm]);
  const [leagues, setLeagues] = useState(REALM_LEAGUES[realm]);
  const [leagueFallback, setLeagueFallback] = useState(false);
  const [preset, setPreset] = useState('dps');
  const [budget, setBudget] = useState(() => localStorage.getItem(BUDGET_KEY) ?? '100');
  const [currency, setCurrency] = useState<Currency>(() => {
    const saved = localStorage.getItem(CURRENCY_KEY);
    return saved === 'divine' || saved === 'chaos' ? saved : 'equiv';
  });
  const [running, setRunning] = useState<string | null>(null);
  const [progress, setProgress] = useState<{ done: number; total: number } | null>(null);
  const [results, setResults] = useState<Record<string, SlotResult>>({});
  const [broad, setBroad] = useState(false);
  const [showEmpty, setShowEmpty] = useState(false);
  const abortRef = useRef<AbortController | null>(null);
  const mainGroup = session.calcParams.main_socket_group ?? session.calc?.main_skill?.group_index ?? 0;

  useEffect(() => {
    loadTradeCatalog().then(setCatalog).catch(() => setCatalogError(true));
    getBackend().then(backend => backend.loadPassiveTree()).then(nodes => {
      setJewelSockets(nodes.filter(node => node.kind === 'jewel_socket').map(node => node.skill));
    }).catch(() => {});
  }, []);
  useEffect(() => {
    let cancelled = false;
    setLeagues(REALM_LEAGUES[realm]); setLeagueFallback(false);
    loadTradeLeagues(realm).then(list => {
      if (cancelled) return;
      setLeagues(list);
      if (localStorage.getItem(leagueKey(realm)) === null) setLeague(list[0]);
    }).catch(() => { if (!cancelled) setLeagueFallback(true); });
    return () => { cancelled = true; };
  }, [realm]);
  // Market-only changes update links immediately; they do not rerun local calculations.
  useEffect(() => {
    abortRef.current?.abort(); setResults({});
  }, [preset, session.currentRequest, categories]);
  useEffect(() => () => abortRef.current?.abort(), []);

  const slots = [...TRADE_SLOTS, ...jewelSockets.filter(node => session.allocatedNodes.includes(node)).map(node => `Jewel@${node}`)];
  const bySlot = new Map([...session.items, ...session.flasks,
    ...session.jewels.map(jewel => ({ slot: `Jewel@${jewel.socket_node}`, text: jewel.text })),
  ].map(item => [item.slot, item.text]));
  const slotNames = useItemDisplayNames(slots.map(slot => bySlot.get(slot)), lang);
  const labelOf = (slot: string) => slot === 'gems' ? tt('trade.gems') : slot.startsWith('Jewel@') ? `${tt('trade.jewelSocket')} ${slot.slice(6)}` : slotLabel(lang, slot);
  const baseOf = (slot: string) => catalog ? referenceBase(catalog, slot, bySlot.get(slot), categories[slot]) : undefined;
  const hasEquipment = slots.some(slot => bySlot.has(slot));
  const visibleSlots = slots.filter(slot => showEmpty || !hasEquipment || bySlot.has(slot) || slot === selected || slot.startsWith('Jewel@'));
  const priceCap = useMemo<TradePriceCap | undefined>(() => {
    const max = Number(budget);
    return Number.isFinite(max) && max > 0 ? { max, currency: currency === 'equiv' ? 'exalted' : currency } : undefined;
  }, [budget, currency]);
  const objective = useMemo(() => {
    const chosen = OBJECTIVE_PRESETS.find(entry => entry.id === preset) ?? OBJECTIVE_PRESETS[0];
    return { stat: chosen.stat, per: chosen.per, constraints: [] };
  }, [preset]);
  const result = results[selected];
  const weights = result?.weights;
  const localized = useLocalizedLines(weights?.weighted.map(weight => weight.line) ?? [], lang);
  const selectedBase = selected === 'gems' ? undefined : baseOf(selected);
  const selectedName = selected === 'gems' ? skillName(session.socketGroups[mainGroup]?.gems[0]?.skill_id ?? '')
    : slotNames[slots.indexOf(selected)] || tt('trade.slotEmpty');
  const choices = catalog ? [...new Set(basesForSlot(catalog, selected).map(base => base.category))] : [];
  const searchUrl = result?.category && weights ? buildTradeUrl(league, weights.weighted, {
    realm, category: result.category, price: priceCap, maxLevel: session.character?.level,
    minimumWeight: broad ? 0 : weights.minimumWeight,
  }) : null;
  const maxGain = Math.max(...(weights?.weighted.map(weight => weight.gain) ?? []), 1e-9);
  const disabled = running !== null || session.busy || !catalog;
  const number = (value: number, digits = 2) => value.toLocaleString(lang, { maximumFractionDigits: digits });

  const analyze = async (targets: string[]) => {
    const request = session.currentRequest();
    if (!request || !catalog) return;
    const controller = new AbortController(); abortRef.current = controller;
    try {
      for (const slot of targets) {
        controller.signal.throwIfAborted();
        setRunning(slot); setProgress({ done: 0, total: 1 });
        const options = { signal: controller.signal, onProgress: (done: number, total: number) => setProgress({ done, total }) };
        try {
          let next: SlotResult;
          if (slot === 'gems') {
            next = { gems: await planGemUpgrades(request, catalog, mainGroup, objective, options.signal, options.onProgress) };
          } else {
            const base = baseOf(slot);
            if (!base) continue;
            next = { category: base.category, weights: await optimizeTradeAffixes({
              request, slot, base, pool: categoryAffixPool(catalog, base.category), itemLevel: 100,
              objective, combinations: false, ...options,
            }) };
          }
          controller.signal.throwIfAborted();
          setResults(prev => ({ ...prev, [slot]: next }));
        } catch (error) {
          if (controller.signal.aborted) break;
          setResults(prev => ({ ...prev, [slot]: { error: error instanceof Error ? error.message : String(error) } }));
        }
      }
    } catch (error) {
      if (!controller.signal.aborted) throw error;
    } finally {
      if (abortRef.current === controller) { setRunning(null); setProgress(null); abortRef.current = null; }
    }
  };

  return <section className="trade-page" aria-labelledby="trade-heading">
    <header className="trade-heading">
      <div><span className="trade-eyebrow">{tt('trade.eyebrow')}</span><h2 id="trade-heading">{tt('trade.title')}</h2>
        <p>{tt('trade.hint')}</p></div>
      <span className="trade-local-badge"><span />{tt('trade.localBadge')}</span>
    </header>
    <div className="trade-setup">
      <div className="trade-objectives" role="group" aria-label={tt('opt.objective')}>
        <span className="trade-field-label">{tt('opt.objective')}</span>
        <div>{OBJECTIVE_PRESETS.map(entry => <button key={entry.id} aria-pressed={preset === entry.id}
          onClick={() => setPreset(entry.id)}>{tt(entry.labelKey as UiKey)}</button>)}</div>
      </div>
      <label className="trade-price-field"><span className="trade-field-label">{tt('trade.budget')}</span>
        <span className="trade-budget"><input type="number" min={0} aria-label={tt('trade.budget')} value={budget} placeholder={tt('trade.budgetAny')}
          onChange={event => { setBudget(event.target.value); localStorage.setItem(BUDGET_KEY, event.target.value); }} />
          <AppSelect value={currency} ariaLabel={tt('trade.currency')} options={[
            { value: 'equiv', label: tt('trade.curExalted') }, { value: 'divine', label: tt('trade.curDivine') }, { value: 'chaos', label: tt('trade.curChaos') },
          ]} onChange={value => { setCurrency(value as Currency); localStorage.setItem(CURRENCY_KEY, value); }} /></span>
      </label>
      <div className="trade-market-fields">
        <label><span className="trade-field-label">{tt('trade.realm')}</span><AppSelect value={realm} ariaLabel={tt('trade.realm')}
          options={[{ value: 'intl', label: tt('trade.realmIntl') }, { value: 'cn', label: tt('trade.realmCn') }]}
          onChange={value => { const next = value as TradeRealm; setRealm(next); localStorage.setItem(REALM_KEY, next);
            setLeague(localStorage.getItem(leagueKey(next)) ?? REALM_DEFAULT_LEAGUE[next]); }} /></label>
        <label><span className="trade-field-label">{tt('trade.league')}</span><AppSelect value={leagues.includes(league) ? league : '__custom'} ariaLabel={tt('trade.league')}
          options={[...leagues.map(name => ({ value: name, label: name })), { value: '__custom', label: tt('trade.leagueCustom') }]}
          onChange={value => { const next = value === '__custom' ? '' : value; setLeague(next); localStorage.setItem(leagueKey(realm), next); }} /></label>
        {!leagues.includes(league) && <input aria-label={tt('trade.leagueCustom')} value={league} placeholder={REALM_DEFAULT_LEAGUE[realm]}
          onChange={event => { setLeague(event.target.value); localStorage.setItem(leagueKey(realm), event.target.value); }} />}
      </div>
    </div>
    {leagueFallback && <p className="trade-notice">{tt('trade.leagueFallback')}</p>}
    {catalogError ? <p role="alert" className="trade-notice">{tt('trade.unavailable')}</p> : <div className="trade-workspace">
      <aside className="trade-position-panel">
        <div className="trade-section-label">{tt('trade.chooseSlot')}</div>
        <nav aria-label={tt('trade.chooseSlot')} className="trade-position-list">
          {visibleSlots.map(slot => <button key={slot} className="trade-position" aria-pressed={selected === slot} onClick={() => setSelected(slot)}>
            <span className="trade-position-symbol" aria-hidden><SlotSymbol slot={slot} /></span>
            <span><strong>{labelOf(slot)}</strong><small>{slotNames[slots.indexOf(slot)] || tt('trade.slotEmpty')}</small></span>
            {results[slot]?.weights && <span className="trade-position-done" aria-label={tt('trade.analyzed')}>+</span>}
          </button>)}
          <button className="trade-position" aria-pressed={selected === 'gems'} onClick={() => setSelected('gems')}>
            <span className="trade-position-symbol" aria-hidden><SlotSymbol slot="gems" /></span><span><strong>{tt('trade.gems')}</strong><small>{tt('trade.gemSubtitle')}</small></span>
          </button>
        </nav>
        {hasEquipment && <button className="trade-text-button" onClick={() => setShowEmpty(!showEmpty)}>{tt(showEmpty ? 'trade.hideEmpty' : 'trade.showEmpty')}</button>}
        <button className="trade-analyze-all" disabled={disabled || !hasEquipment} onClick={() => void analyze(slots.filter(slot => bySlot.has(slot)))}>{tt('trade.analyzeAll')}</button>
      </aside>
      <div className="trade-analysis">
        <header className="trade-analysis-header">
          <div><span className="trade-section-label">{labelOf(selected)}</span><h3>{selectedName}</h3>
            <div className="trade-scope"><span>{selectedBase ? tt(`trade.category.${selectedBase.category}` as UiKey) : tt('trade.gems')}</span>
              {selected !== 'gems' && <span>{tt('trade.allBases')}</span>}<span>{tt('trade.levelLimit')} {session.character?.level ?? 1}</span></div>
          </div>
          <div className="trade-header-actions"><button className={weights || result?.gems ? 'trade-secondary' : 'trade-primary'} disabled={disabled || (selected !== 'gems' && !selectedBase) || (selected === 'gems' && (!session.socketGroups[mainGroup]?.enabled || !!session.socketGroups[mainGroup]?.source))}
            onClick={() => void analyze([selected])}>{tt(running === selected ? 'opt.running' : weights || result?.gems ? 'trade.recalculate' : 'trade.analyze')}</button>
          {searchUrl && league.trim() && <a className="trade-primary trade-market-link" href={searchUrl} target="_blank" rel="noreferrer">{tt('trade.browseMarket')}<span aria-hidden>↗</span></a>}</div>
        </header>
        {session.socketGroups.length > 0 && <div className="trade-skill-context"><span>{tt('sidebar.mainSkill')}</span>
          <AppSelect value={String(mainGroup)} ariaLabel={tt('trade.analysisSkill')} disabled={session.busy}
            options={session.socketGroups.flatMap((group, index) => group.enabled && group.gems.length ? [{ value: String(index), label: skillName(group.gems[0].skill_id) }] : [])}
            onChange={value => session.updateParams({ main_socket_group: Number(value) })} />
          <span className="trade-skill-hint">{tt('trade.skillContext')}</span></div>}
        {selected !== 'gems' && choices.length > 1 && <details className="trade-range">
          <summary>{tt('trade.changeType')}</summary><AppSelect value={selectedBase?.category ?? ''} ariaLabel={`${labelOf(selected)} ${tt('trade.category')}`}
            options={choices.map(category => ({ value: category, label: tt(`trade.category.${category}` as UiKey) }))}
            onChange={category => setCategories(prev => ({ ...prev, [selected]: category }))} />
        </details>}
        {running && progress && <div className="trade-progress"><span>{tt('trade.analyzingSlot')}: {labelOf(running)}</span>
          <OptimizerProgress {...progress} onCancel={() => abortRef.current?.abort()} lang={lang} /></div>}
        {result?.error && <div role="alert" className="trade-notice trade-error">{tt('trade.analysisFailed')}<details><summary>{tt('trade.errorDetails')}</summary>{result.error}</details></div>}
        {!weights && !result?.gems && running !== selected && <div className="trade-empty-state">
          <span className="trade-empty-symbol" aria-hidden><SlotSymbol slot={selected} /></span>
          <h4>{tt('trade.emptyTitle')}</h4><p>{tt(selected === 'gems' ? 'trade.gemHint' : 'trade.emptyDescription')}</p>
          <ol><li>{tt('trade.stepScore')}</li><li>{tt('trade.stepSearch')}</li><li>{tt('trade.stepBuy')}</li></ol>
        </div>}
        {weights && <>
          <div className="trade-results-heading"><div><h4>{tt('trade.affixHeading')}</h4><p>{tt('trade.affixDescription')}</p></div>
            <span className="trade-count">{weights.weighted.length} {tt('trade.usefulAffixes')}</span></div>
          {weights.weighted.length === 0 ? <p className="trade-notice">{tt('trade.noWeights')}</p> : <div className="trade-score-table" role="table" aria-label={tt('trade.affixHeading')}>
            <div className="trade-score-row trade-score-labels" role="row"><span role="columnheader">{tt('trade.colLine')}</span><span role="columnheader">{tt('trade.priority')}</span><span role="columnheader">{tt('trade.referenceGain')}</span><span role="columnheader">{tt('trade.marketWeight')}</span></div>
            {weights.weighted.map((weight, index) => <div key={weight.id} className="trade-score-row" role="row">
              <span className="trade-affix" role="cell"><span className="trade-rank">{String(index + 1).padStart(2, '0')}</span>{localized[index] ?? weight.line}</span>
              <span className="trade-priority" role="cell"><span className="trade-bar"><span style={{ width: `${weight.gain / maxGain * 100}%` }} /></span><b>{Math.round(weight.gain / maxGain * 100)}</b></span>
              <span className="trade-gain" role="cell">+{number(weight.gainPercent)}%</span><span className="trade-weight" role="cell">{number(weight.weight, 3)}</span>
            </div>)}
          </div>}
          <details className="trade-method"><summary>{tt('trade.howItWorks')}</summary><p>{tt('trade.methodDescription')}</p>
            <p>{tt('trade.currentGoal')}: {statNameLabel(lang, objective.stat)} {number(scoreOf(weights.baseline, objective))} · {weights.evaluated} {tt('trade.evaluations')}</p></details>
          {weights.unsupported.length > 0 && <details className="trade-notice trade-unsupported"><summary>{tt('trade.unsupportedHint')} ({weights.unsupported.length})</summary><pre>{weights.unsupported.join('\n')}</pre></details>}
          {searchUrl && league.trim() && <div className="trade-search-card">
            <div><span className="trade-section-label">{tt('trade.nextStep')}</span><h4>{tt('trade.searchReady')}</h4><p>{tt(realm === 'cn' ? 'trade.cnMarketHint' : 'trade.directMarketHint')}</p>
              <label className="trade-broad"><input type="checkbox" checked={broad} onChange={event => setBroad(event.target.checked)} />{tt('trade.broaderSearch')}</label></div>
            <div className="trade-search-actions"><a className="trade-primary trade-market-link" href={searchUrl} target="_blank" rel="noreferrer">{tt('trade.browseMarket')}<span aria-hidden>↗</span></a>
              <CopyButton text={searchUrl} label={tt('trade.copySearch')} lang={lang} /></div>
          </div>}
        </>}
        {result?.gems && <div className="trade-gems"><div className="trade-results-heading"><div><h4>{tt('trade.gemCandidates')}</h4><p>{tt('trade.gemHint')}</p></div></div>
          {result.gems.length === 0 && <p className="trade-notice">{tt('trade.noGemUpgrade')}</p>}
          {result.gems.map(plan => <article className="trade-gem-card" key={`${plan.gem.skill_id}:${plan.position}:${plan.level}:${plan.quality}`}>
            <span className="trade-position-symbol" aria-hidden><SlotSymbol slot="gems" /></span>
            <div><h4>{skillName(plan.gem.skill_id)}</h4><p>{tt('skills.level')} {plan.level} · {tt('skills.quality')} {plan.quality}%</p>
              <p>{session.socketGroups[plan.group]?.gems[plan.position]
                ? `${tt('trade.replacesGem')} ${skillName(session.socketGroups[plan.group].gems[plan.position].skill_id)}`
                : tt('trade.addsGem')}</p>
              {plan.gainPercent !== undefined && <span className="trade-gain">{tt('trade.referenceGain')} +{number(plan.gainPercent)}%</span>}</div>
            {league.trim() && <a className="trade-secondary" href={gemTradeUrl({ realm, league, category: 'gem', price: priceCap, maxLevel: session.character?.level,
              gem: { name: realm === 'cn' ? cnSkillName(plan.gem.skill_id) : plan.gem.name, level: plan.level, quality: plan.quality } })} target="_blank" rel="noreferrer">{tt('trade.browseMarket')}</a>}
          </article>)}
        </div>}
      </div>
    </div>}
  </section>;
}

function SlotSymbol({ slot }: { slot: string }) {
  const path = slot === 'gems' || slot.startsWith('Jewel@') ? 'M12 3 3 10l9 11 9-11-9-7Zm-9 7h18M8 6l4 15 4-15'
    : slot.startsWith('weapon') ? 'M5 3c17 2 17 16 0 18M5 3l5 9-5 9M3 12h18m-4-3 4 3-4 3'
    : slot.startsWith('ring') || slot === 'amulet' ? 'M8 5l4-3 4 3-4 4-4-4Zm1 4a7 7 0 1 0 6 0'
    : /^(Flask|Charm)/.test(slot) ? 'M9 3h6m-5 0v6L6 16v4h12v-4l-4-7V3M8 15h8'
    : 'M12 3 4 6v6c0 5 8 9 8 9s8-4 8-9V6l-8-3Z';
  return <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.35" strokeLinecap="round" strokeLinejoin="round"><path d={path} /></svg>;
}
