import { useUpgradeGoal } from '../../hooks/useUpgradeGoal';
import { upgradeT } from '../../lib/upgradeText';
import { ItemReplacementCheck } from './ItemReplacementCheck';
import { WeaponSetControl } from '../shared/WeaponSetControl';
import { SlotSymbol } from '../shared/SlotSymbol';
import { PageHeader } from '../shared/PageHeader';
import { useEffect, useMemo, useRef, useState } from 'react';
import { getBackend } from '../../api/backend';
import type { BuildSession } from '../../hooks/useBuildSession';
import { useItemDisplayNames, useLocalizedLines } from '../../hooks/useLocalizedLines';
import { useSkillName } from '../../hooks/useSkillName';
import { bindT, slotLabel, statNameLabel, type Lang, type UiKey } from '../../lib/i18n';
import { OBJECTIVE_PRESETS, scoreOf, type Objective } from '../../lib/optimize';
import { REALM_DEFAULT_LEAGUE, REALM_LEAGUES, TRADE_CURRENCIES, buildTradeUrl, gemTradeUrl, loadTradeLeagues, type TradeCurrency, type TradePriceCap, type TradeRealm } from '../../lib/trade';
import { affixPool, basesForSlot, categoryAffixPool, loadTradeCatalog, optimizeTradeAffixes, referenceBase, type TradeCatalog } from '../../lib/tradeOptimizer';
import { planGemUpgrades } from '../../lib/tradeMarket';
import { AppSelect } from '../shared/AppSelect';
import { CopyButton } from '../shared/CopyButton';
import { objectiveOf, ObjectiveEditor, OptimizerProgress } from '../shared/OptimizerControls';
import { statMap } from '../../lib/statDisplay';
import { rankUpgradePositions, type PositionAnalysis } from '../../lib/tradePriority';
import './trade.css';

const TRADE_OBJECTIVES = OBJECTIVE_PRESETS;
const TRADE_SLOTS = ['weapon1', 'weapon2', 'helmet', 'bodyarmour', 'gloves', 'boots', 'amulet', 'ring1', 'ring2', 'belt', 'Flask 1', 'Flask 2', 'Charm 1', 'Charm 2', 'Charm 3'];
const REALM_KEY = 'pobr-trade-realm';
const leagueKey = (realm: TradeRealm) => `pobr-trade-league-${realm}`;
const BUDGET_KEY = 'pobr-trade-budget';
const CURRENCY_KEY = 'pobr-trade-currency';
type SlotResult = PositionAnalysis;

/** Local build analysis produces official search links; login and buying stay on the market. */
export function TradePanel({ session, lang, focus, onSkills, onTree, initialItemText }: {
  session: BuildSession; lang: Lang; focus?: {slot:string; nonce:number};
  onSkills?: (group:number) => void; onTree?: () => void;
  initialItemText?: string;
}) {
  const tt = bindT(lang);
  const ut = (key: Parameters<typeof upgradeT>[1]) => upgradeT(lang, key);
  const { goal, setGoal } = useUpgradeGoal();
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
  const preset = goal.preset;
  const setPreset = (preset: string) => setGoal({ ...goal, preset });
  const [budget, setBudget] = useState(() => localStorage.getItem(BUDGET_KEY) ?? '100');
  const [currency, setCurrency] = useState<TradeCurrency>(() => {
    const saved = localStorage.getItem(CURRENCY_KEY);
    // The legacy "equiv" preference also selected Exalted Orb listings.
    return TRADE_CURRENCIES.find(option => option.value === saved)?.value ?? 'exalted';
  });
  const [running, setRunning] = useState<string | null>(null);
  const [progress, setProgress] = useState<{ done: number; total: number } | null>(null);
  const [results, setResults] = useState<Record<string, SlotResult>>({});
  const [broad, setBroad] = useState(false);
  const [requiredStats, setRequiredStats] = useState<Record<string, string[]>>({});
  const [showEmpty, setShowEmpty] = useState(false);
  const [includeUnique, setIncludeUnique] = useState(false);
  const resistanceFirst = goal.resistanceFirst ?? false;
  const resistanceTarget = goal.resistanceTarget ?? 75;
  const keepEhp = goal.keepEhp ?? true;
  const setResistanceFirst = (value:boolean) => setGoal({ ...goal, resistanceFirst:value });
  const setResistanceTarget = (value:number) => setGoal({ ...goal, resistanceTarget:value });
  const setKeepEhp = (value:boolean) => setGoal({ ...goal, keepEhp:value });
  const [allProgress, setAllProgress] = useState<{ done: number; total: number } | null>(null);
  const [overview, setOverview] = useState(false);
  const analysisRef = useRef<HTMLDivElement>(null);
  const goalRef = useRef<HTMLDivElement>(null);
  const abortRef = useRef<AbortController | null>(null);
  useEffect(() => { if (focus) setSelected(focus.slot); }, [focus]);
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
    abortRef.current?.abort(); setResults({}); setAllProgress(null); setRequiredStats({});
  }, [goal, session.currentRequest, categories]);
  useEffect(() => () => abortRef.current?.abort(), []);

  const slots = [...TRADE_SLOTS, ...jewelSockets.filter(node => session.allocatedNodes.includes(node)).map(node => `Jewel@${node}`)];
  const bySlot = new Map([...session.items, ...session.flasks,
    ...session.jewels.map(jewel => ({ slot: `Jewel@${jewel.socket_node}`, text: jewel.text })),
  ].map(item => [item.slot, item.text]));
  const slotNames = useItemDisplayNames(slots.map(slot => bySlot.get(slot)), lang);
  const labelOf = (slot: string) => slot === 'gems' ? tt('trade.gems') : slot.startsWith('Jewel@') ? `${tt('trade.jewelSocket')} ${slot.slice(6)}` : slotLabel(lang, slot);
  const baseOf = (slot: string) => catalog ? referenceBase(catalog, slot, bySlot.get(slot), categories[slot], session.character?.level) : undefined;
  const hasEquipment = slots.some(slot => bySlot.has(slot));
  const visibleSlots = slots.filter(slot => showEmpty || !hasEquipment || bySlot.has(slot) || slot === selected || slot.startsWith('Jewel@'));
  const priceCap = useMemo<TradePriceCap | undefined>(() => {
    const max = Number(budget);
    return Number.isFinite(max) && max > 0 ? { max, currency } : undefined;
  }, [budget, currency]);
  const objective = useMemo<Objective>(() => objectiveOf(goal,
    Object.fromEntries([...statMap(session.calc?.stats ?? [])].filter((entry): entry is [string, number] => entry[1] !== null))), [goal, session.calc]);
  const result = results[selected];
  const weights = result?.weights;
  const localized = useLocalizedLines(weights?.weighted.map(weight => weight.line) ?? [], lang);
  const situationalLines = useLocalizedLines(weights?.situational?.map(stat => stat.line) ?? [], lang);
  const selectedBase = selected === 'gems' ? undefined : baseOf(selected);
  const selectedName = selected === 'gems' ? skillName(session.socketGroups[mainGroup]?.gems[0]?.skill_id ?? '')
    : slotNames[slots.indexOf(selected)] || tt('trade.slotEmpty');
  const choices = catalog ? [...new Set(basesForSlot(catalog, selected).map(base => base.category))] : [];
  const searchUrl = result?.category && weights ? buildTradeUrl(league, weights.weighted, {
    realm, category: result.category, price: priceCap, maxLevel: session.character?.level, includeUnique,
    minimumWeight: broad ? 0 : weights.minimumWeight,
    requiredStats: requiredStats[selected],
  }) : null;
  const maxGain = Math.max(...(weights?.weighted.map(weight => weight.gain) ?? []), 1e-9);
  const disabled = running !== null || session.busy || !catalog;
  const number = (value: number, digits = 2) => value.toLocaleString(lang, { maximumFractionDigits: digits });

  const priorities = rankUpgradePositions(results, objective);
  const fullTargets = [...slots.filter(slot => (bySlot.has(slot) || slot.startsWith('Jewel@')) && baseOf(slot)),
    ...(session.socketGroups[mainGroup]?.enabled && !session.socketGroups[mainGroup]?.source ? ['gems'] : [])];
  const analyze = async (targets: string[], all = false) => {
    const request = session.currentRequest();
    if (!request || !catalog) return;
    const controller = new AbortController(); abortRef.current = controller;
    if (all) { setOverview(true); setAllProgress({ done: 0, total: targets.length }); }
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
            // PoB2 Item:Craft derives affix requirements as floor(mod.level * 0.8).
            const itemLevel = Math.min(100, Math.ceil(((request.character?.level ?? 1) + 1) / 0.8) - 1);
            next = { category: base.category, weights: await optimizeTradeAffixes({
              request, slot, base, pool: categoryAffixPool(catalog, base.category, itemLevel, request.character?.level), itemLevel,
              objective, combinations: all || overview, combinationPool: affixPool(catalog, base, itemLevel),
              maxEvaluations: all || overview ? 768 : undefined, beamWidth: 6, ...options,
            }) };
          }
          controller.signal.throwIfAborted();
          setResults(prev => ({ ...prev, [slot]: next }));
          if (all) setAllProgress({ done: targets.indexOf(slot) + 1, total: targets.length });
        } catch (error) {
          if (controller.signal.aborted) break;
          setResults(prev => ({ ...prev, [slot]: { error: error instanceof Error ? error.message : String(error) } }));
          if (all) setAllProgress({ done: targets.indexOf(slot) + 1, total: targets.length });
        }
      }
    } catch (error) {
      if (!controller.signal.aborted) throw error;
    } finally {
      if (abortRef.current === controller) { setRunning(null); setProgress(null); abortRef.current = null; }
    }
  };

  return <section className="ui-page trade-page" aria-labelledby="trade-heading">
    <PageHeader id="trade-heading" title={tt('trade.title')} description={tt('trade.hint')}>
      <span className="trade-local-badge"><span />{tt('trade.localBadge')}</span>
    </PageHeader>
    <div className="upgrade-evaluation">
      <div className="trade-objectives" role="group" aria-label={tt('opt.objective')} ref={goalRef} tabIndex={-1}>
        <span className="trade-field-label">{tt('opt.objective')}</span>
        <div>{TRADE_OBJECTIVES.map(entry => <button key={entry.id} aria-pressed={preset === entry.id}
          onClick={() => setPreset(entry.id)}>{tt(entry.labelKey as UiKey)}</button>)}</div>
      </div>
        {session.socketGroups.length > 0 && <div className="trade-skill-context"><span>{tt('sidebar.mainSkill')}</span>
          <AppSelect value={String(mainGroup)} ariaLabel={tt('trade.analysisSkill')} disabled={session.busy}
            options={session.socketGroups.flatMap((group, index) => group.enabled && group.gems.length ? [{ value: String(index), label: skillName(group.gems[0].skill_id) }] : [])}
            onChange={value => session.updateParams({ main_socket_group: Number(value) })} />
          <span className="trade-skill-hint">{tt('trade.skillContext')}</span></div>}
    <WeaponSetControl session={session} lang={lang} />
    <div className="trade-safety-settings">
      {preset === 'balanced' && <label><input type="checkbox" checked={keepEhp} onChange={event => setKeepEhp(event.target.checked)} />{tt('trade.keepEhp')}</label>}
      <label><input type="checkbox" checked={resistanceFirst} onChange={event => setResistanceFirst(event.target.checked)} />{tt('trade.resistanceFirst')}</label>
      {resistanceFirst && <label>{tt('trade.resistanceTarget')}<input type="number" min={0} max={90} value={resistanceTarget} aria-label={tt('trade.resistanceTarget')}
        onChange={event => { const value = Number(event.target.value); if (Number.isFinite(value) && value >= 0 && value <= 90) setResistanceTarget(value); }} />%</label>}
      <details className="trade-goal-help"><summary>{tt('trade.goalHelp')}</summary><p>{tt(preset === 'balanced' ? 'trade.balancedHint' : 'trade.resistanceHint')}</p></details>
    </div>
    <details className="upgrade-goal-details" open={!!goal.cStat}><summary>{ut('advancedGoal')}{goal.cStat ? ` · ${statNameLabel(lang, goal.cStat)}` : ''}</summary><div className="opt-controls"><ObjectiveEditor value={goal} onChange={setGoal} lang={lang} constraintsOnly /></div></details>
    </div>
    <div className="upgrade-paths">
      <button onClick={() => analysisRef.current?.scrollIntoView({block:'start'})}><SlotSymbol slot="helmet" /><strong>{ut('equipment')}</strong><span>{ut('equipmentHint')}</span></button>
      <button disabled={!onSkills || !session.socketGroups[mainGroup]} onClick={() => onSkills?.(mainGroup)}><SlotSymbol slot="gems" /><strong>{ut('supports')}</strong><span>{ut('supportsHint')}</span></button>
      <button disabled={!onTree} onClick={onTree}><SlotSymbol slot="Jewel@" /><strong>{ut('tree')}</strong><span>{ut('treeHint')}</span></button>
    </div>
    <p className="upgrade-shared-goal">{ut('sharedGoal')}</p>
    <ItemReplacementCheck session={session} lang={lang} objective={objective} catalog={catalog} jewelSockets={jewelSockets} initialItemText={initialItemText}
      goalLabel={tt(TRADE_OBJECTIVES.find(entry => entry.id === preset)!.labelKey as UiKey)}
      onEditGoal={() => { goalRef.current?.scrollIntoView({ block: 'start' }); goalRef.current?.focus({ preventScroll: true }); }} />
    <div className="trade-setup">
      <label className="trade-price-field"><span className="trade-field-label">{tt('trade.budget')}</span>
        <span className="trade-budget"><input type="number" min={0} aria-label={tt('trade.budget')} value={budget} placeholder={tt('trade.budgetAny')}
          onChange={event => { setBudget(event.target.value); localStorage.setItem(BUDGET_KEY, event.target.value); }} />
          <AppSelect value={currency} ariaLabel={tt('trade.currency')}
            options={TRADE_CURRENCIES.map(option => ({ value: option.value, label: tt(option.labelKey) }))}
            onChange={value => { setCurrency(value as TradeCurrency); localStorage.setItem(CURRENCY_KEY, value); }} /></span>
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
    <div className="trade-overview-toolbar">
      <div><h3>{tt('trade.overviewTitle')}</h3><p>{tt('trade.overviewHint')}</p></div>
      <button className="trade-primary trade-analyze-all" disabled={disabled || fullTargets.length === 0}
        onClick={() => void analyze(fullTargets, true)}>{tt('trade.analyzeAll')}</button>
      <label className="trade-unique"><input type="checkbox" checked={includeUnique} onChange={event => setIncludeUnique(event.target.checked)} />{tt('trade.includeUnique')}</label>
    </div>
    {overview && <section className="trade-priorities ui-card" aria-label={tt('trade.overviewTitle')}>
      <div className="trade-results-heading"><h4>{tt('trade.priorityOrder')}</h4>
        {allProgress && <span className="ui-badge">{allProgress.done} / {allProgress.total} {tt('trade.positions')}</span>}</div>
      <p className="trade-priority-hint">{tt('trade.priorityHint')}</p>
      {running && <div className="trade-progress"><span>{tt('trade.analyzingSlot')}: {labelOf(running)}</span>
        {progress && <OptimizerProgress {...progress} onCancel={() => abortRef.current?.abort()} lang={lang} />}</div>}
      <ol className="trade-priority-list">
        {priorities.map((entry, index) => <li key={entry.slot}>
          <button className="trade-priority-position" onClick={() => {
            setSelected(entry.slot); analysisRef.current?.scrollIntoView({ block: 'start' });
          }}>
            <span className="trade-rank">{String(index + 1).padStart(2, '0')}</span>
            <span className="trade-position-symbol"><SlotSymbol slot={entry.slot} /></span>
            <span className="trade-priority-name"><strong>{labelOf(entry.slot)}</strong><small>{entry.slot === 'gems' ? skillName(session.socketGroups[mainGroup]?.gems[0]?.skill_id ?? '') : slotNames[slots.indexOf(entry.slot)]}</small></span>
            <span className="trade-priority-delta"><strong className={entry.gain < 0 ? 'delta-neg' : 'trade-gain'}>{entry.gain >= 0 ? '+' : ''}{number(entry.gainPercent ?? entry.gain)}{entry.gainPercent === undefined ? '' : '%'}</strong><small>{tt('trade.referencePotential')}</small></span>
            <span aria-hidden>→</span>
          </button>
          <div className="trade-priority-defence"><span className={entry.dpsDelta < 0 ? 'delta-neg' : ''}>DPS {entry.dpsDelta > 0 ? '+' : ''}{number(entry.dpsDelta, 0)}</span><span className={entry.lifeDelta < 0 ? 'delta-neg' : ''}>{statNameLabel(lang, 'Life')} {entry.lifeDelta > 0 ? '+' : ''}{number(entry.lifeDelta, 0)}</span><span className={entry.ehpDelta < 0 ? 'delta-neg' : ''}>EHP {entry.ehpDelta > 0 ? '+' : ''}{number(entry.ehpDelta, 0)}</span>{resistanceFirst && <span>{tt('trade.resistanceGap')} {number(entry.deficit)}%</span>}</div>
        </li>)}
      </ol>
      {!running && priorities.length === 0 && <p className="trade-notice">{tt('trade.noPriority')}</p>}
      {Object.entries(results).some(([, entry]) => entry.error) && <p className="trade-notice">{tt('trade.partialAnalysis')}</p>}
    </section>}
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
      </aside>
      <div className="trade-analysis" ref={analysisRef}>
        <header className="trade-analysis-header">
          <div><span className="trade-section-label">{labelOf(selected)}</span><h3>{selectedName}</h3>
            <div className="trade-scope"><span>{selectedBase ? tt(`trade.category.${selectedBase.category}` as UiKey) : tt('trade.gems')}</span>
              {selected !== 'gems' && <><span>{tt('trade.allBases')}</span>{!includeUnique && <span>{tt('trade.nonUnique')}</span>}</>}<span>{tt('trade.levelLimit')} {session.character?.level ?? 1}</span></div>
          </div>
          <div className="trade-header-actions"><button className={weights || result?.gems ? 'trade-secondary' : 'trade-primary'} disabled={disabled || (selected !== 'gems' && !selectedBase) || (selected === 'gems' && (!session.socketGroups[mainGroup]?.enabled || !!session.socketGroups[mainGroup]?.source))}
            onClick={() => void analyze([selected])}>{tt(running === selected ? 'opt.running' : weights || result?.gems ? 'trade.recalculate' : 'trade.analyze')}</button>
          {searchUrl && league.trim() && <a className="trade-primary trade-market-link" href={searchUrl} target="_blank" rel="noreferrer">{tt('trade.browseMarket')}<span aria-hidden>↗</span></a>}</div>
        </header>
        {selected !== 'gems' && choices.length > 1 && <details className="trade-range">
          <summary>{tt('trade.changeType')}</summary><AppSelect value={selectedBase?.category ?? ''} ariaLabel={`${labelOf(selected)} ${tt('trade.category')}`}
            options={choices.map(category => ({ value: category, label: tt(`trade.category.${category}` as UiKey) }))}
            onChange={category => setCategories(prev => ({ ...prev, [selected]: category }))} />
        </details>}
        {!overview && running && progress && <div className="trade-progress"><span>{tt('trade.analyzingSlot')}: {labelOf(running)}</span>
          <OptimizerProgress {...progress} onCancel={() => abortRef.current?.abort()} lang={lang} /></div>}
        {result?.error && <div role="alert" className="trade-notice trade-error">{tt('trade.analysisFailed')}<details><summary>{tt('trade.errorDetails')}</summary>{result.error}</details></div>}
        {!weights && !result?.gems && running !== selected && <div className="trade-empty-state">
          <span className="trade-empty-symbol" aria-hidden><SlotSymbol slot={selected} /></span>
          <h4>{tt('trade.emptyTitle')}</h4><p>{tt(selected === 'gems' ? 'trade.gemHint' : 'trade.emptyDescription')}</p>
          <ol><li>{tt('trade.stepScore')}</li><li>{tt('trade.stepSearch')}</li><li>{tt('trade.stepBuy')}</li></ol>
        </div>}
        {weights && <>
          <section className="upgrade-score-reference ui-card" aria-label={ut('currentScore')}>
            <div><span>{ut('currentScore')}</span><strong>{weights.currentItemScore ? number(weights.currentItemScore.score, 3) : '—'}</strong>
              <small>{ut(!weights.currentItemScore ? 'noScore' : weights.currentItemScore.complete ? 'complete' : 'partial')}</small></div>
            <div className="upgrade-current-stats"><span>DPS <b>{number(weights.baseline.TotalDPS ?? 0)}</b></span><span>EHP <b>{number(weights.baseline.TotalEHP ?? 0)}</b></span></div>
            <p>{ut('scoreHint')}</p>
            {weights.scoreWarnings.includes('unmodeled-candidates') && <p>{ut('skippedUnmodeled')}</p>}
            {weights.scoreWarnings.includes('constraints-not-in-query') && <p className="upgrade-score-caution">{ut('marketConstraints')}</p>}
            {weights.currentItemScore && <details><summary>{ut('scoreDetails')} ({weights.currentItemScore.matchedLines}/{weights.currentItemScore.totalLines})</summary>
              <ul>{weights.currentItemScore.contributions.map((entry,index) => <li key={index}>{entry.line} · {number(entry.value)} × {number(entry.weight,3)} = {number(entry.score,3)}</li>)}</ul>
              {weights.currentItemScore.unscoredLines.length > 0 && <pre>{weights.currentItemScore.unscoredLines.join('\n')}</pre>}
            </details>}
          </section>
          <div className="trade-results-heading"><div><h4>{tt('trade.affixHeading')}</h4><p>{tt('trade.affixDescription')}</p></div>
            <span className="trade-count">{weights.weighted.length} {tt('trade.usefulAffixes')}</span></div>
          {weights.weighted.length === 0 ? <p className="trade-notice">{tt('trade.noWeights')}</p> : <div className="trade-score-table" role="table" aria-label={tt('trade.affixHeading')}>
            <div className="trade-score-row trade-score-labels" role="row"><span role="columnheader">{tt('trade.colLine')}</span><span role="columnheader">{tt('trade.priority')}</span><span role="columnheader">{ut('marginal')}</span><span role="columnheader">{tt('trade.marketWeight')}</span></div>
            {weights.weighted.map((weight, index) => <div key={weight.id} className="trade-score-row" role="row">
              <span className="trade-affix" role="cell"><span className="trade-rank">{String(index + 1).padStart(2, '0')}</span>{localized[index] ?? weight.line}</span>
              <span className="trade-priority" role="cell"><span className="trade-bar"><span style={{ width: `${weight.gain / maxGain * 100}%` }} /></span><b>{Math.round(weight.gain / maxGain * 100)}</b></span>
              <span className="trade-gain" role="cell">+{number(weight.gainPercent)}%</span><span className="trade-weight" role="cell">{number(weight.weight, 3)}</span>
            </div>)}
          </div>}
          {Boolean(weights.situational?.length) && <div className="trade-situational">
            <h4>{tt('trade.situational')}</h4><p>{tt('trade.situationalHint')}</p>
            {weights.situational?.map((stat, index) => <label key={stat.id}>
              <input type="checkbox" checked={(requiredStats[selected] ?? []).includes(stat.id)} onChange={event => setRequiredStats(prev => ({ ...prev,
                [selected]: event.target.checked ? [...(prev[selected] ?? []), stat.id] : (prev[selected] ?? []).filter(id => id !== stat.id),
              }))} />
              <span><strong>{situationalLines[index] ?? stat.line}</strong><small>{tt(`trade.mechanic.${stat.kind}`)}{stat.delta !== undefined ? ` +${number(stat.delta)}` : ''}</small></span>
            </label>)}
          </div>}
          {weights.combinations[0] && <details className="trade-method"><summary>{tt('trade.referenceItem')}</summary><pre className="trade-reference">{weights.combinations[0].text}</pre></details>}
          <details className="trade-method"><summary>{tt('trade.howItWorks')}</summary><p>{tt('trade.methodDescription')}</p>
            <p>{tt('trade.currentGoal')}: {(preset === 'balanced' ? tt('trade.balanced') : statNameLabel(lang, objective.stat))} {number(scoreOf(weights.baseline, objective))} · {weights.evaluated} {tt('trade.evaluations')}</p></details>
          {weights.unsupported.length > 0 && <details className="trade-notice trade-unsupported"><summary>{tt('trade.unsupportedHint')} ({weights.unsupported.length})</summary><pre>{weights.unsupported.join('\n')}</pre></details>}
          {searchUrl && league.trim() && <div className="trade-search-card">
            <div><span className="trade-section-label">{tt('trade.nextStep')}</span><h4>{tt('trade.searchReady')}</h4><p>{tt(realm === 'cn' ? 'trade.cnMarketHint' : 'trade.directMarketHint')}</p>
              <label className="trade-broad"><input type="checkbox" checked={broad} onChange={event => setBroad(event.target.checked)} />{tt('trade.broaderSearch')}</label></div>
            <div className="trade-search-actions"><a className="trade-primary trade-market-link" href={searchUrl} target="_blank" rel="noreferrer">{tt('trade.browseMarket')}<span aria-hidden>↗</span></a>
              <CopyButton text={searchUrl} label={tt('trade.copySearch')} lang={lang} /></div>
          </div>}
        </>}
        {result?.gems && <div className="trade-gems"><div className="trade-results-heading"><div><h4>{tt('trade.gemCandidates')}</h4>{onSkills && <button onClick={() => onSkills(mainGroup)}>{ut('autoSupports')} ↗</button>}<p>{tt('trade.gemHint')}</p></div></div>
          {result.gems.length === 0 && <p className="trade-notice">{tt('trade.noGemUpgrade')}</p>}
          {result.gems.map(plan => <article className="trade-gem-card" key={`${plan.gem.skill_id}:${plan.position}:${plan.level}:${plan.quality}`}>
            <span className="trade-position-symbol" aria-hidden><SlotSymbol slot="gems" /></span>
            <div><h4>{skillName(plan.gem.skill_id)} {plan.gem.is_lineage && <span className="gem-lineage-badge">{ut('lineage')}</span>}</h4>{plan.acquisition === 'skill-adjustment' && <small>{ut('adjustment')}</small>}<p>{tt('skills.level')} {plan.level} · {tt('skills.quality')} {plan.quality}%</p>
              <p>{session.socketGroups[plan.group]?.gems[plan.position]
                ? `${tt('trade.replacesGem')} ${skillName(session.socketGroups[plan.group].gems[plan.position].skill_id)}`
                : tt('trade.addsGem')}</p>
              {plan.gainPercent !== undefined && <span className={plan.gainPercent < 0 ? 'delta-neg' : 'trade-gain'}>{tt('trade.referenceGain')} {plan.gainPercent >= 0 ? '+' : ''}{number(plan.gainPercent)}%</span>}</div>
            <div className="upgrade-gem-stats">{['TotalDPS', 'TotalEHP'].map(stat => { const before = plan.baseline?.[stat] ?? 0, after = plan.stats?.[stat] ?? 0; return <span key={stat}>{stat === 'TotalDPS' ? 'DPS' : 'EHP'} <b className={after < before ? 'delta-neg' : 'delta-pos'}>{number(before)} → {number(after)}</b></span>; })}</div>
            {plan.acquisition !== 'skill-adjustment' && league.trim() && <a className="trade-secondary" href={gemTradeUrl({ realm, league, category: 'gem', price: priceCap, maxLevel: session.character?.level,
              gem: { name: realm === 'cn' ? cnSkillName(plan.gem.skill_id) : plan.gem.name, level: plan.level, quality: plan.quality } })} target="_blank" rel="noreferrer">{tt('trade.browseMarket')}</a>}
            <button disabled={session.busy} onClick={() => {
              const gems = plan.variant.socket_groups?.[plan.group]?.gems;
              if (gems) session.setSocketGroups(session.socketGroups.map((group,index) => index === plan.group ? {...group,gems} : group));
            }}>{ut('apply')}</button>
          </article>)}
        </div>}
      </div>
    </div>}
  </section>;
}
