import { jewelSocketNames } from '../tree/jewelSocketNames';
import { usePassiveNodeNames } from '../tree/passiveNodeNames';
import type { PassiveNode } from '../../api/types';
import { messages } from '../../lib/i18n';
import { useJobBeacon } from '../../app/jobs/useJobBeacon';
import { usePreview } from '../shared/PreviewContext';
import { changesFromRecords } from '../../app/preview/fromRecords';
import { readPreference, uiStorageKey, writePreference } from '../../lib/sessionStorage';
import { AppButton, AppInput } from '../shared/controls';
import { useUpgradeGoal } from '../../hooks/useUpgradeGoal';
import { upgradeT } from '../../lib/upgradeText';
import { resolveDamageObjective } from '../../lib/mainMinion';
import { useHeadlineDamage } from '../../hooks/useMainMinion';
import { TradeAnalysisProgress, type SlotProgress } from './TradeAnalysisProgress';
import { ItemReplacementCheck } from './ItemReplacementCheck';
import { WeaponSetControl } from '../shared/WeaponSetControl';
import { SlotSymbol } from '../shared/SlotSymbol';
import { PageHeader } from '../shared/PageHeader';
import { words, colon } from '../../app/text';
import { useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react';
import { getBackend } from '../../api/backend';
import type { BuildSession } from '../../hooks/useBuildSession';
import { useItemDisplayNames, useLocalizedLines } from '../../hooks/useLocalizedLines';
import { lineText } from '../../lib/continuedLines';
import { useSkillCatalog } from '../../hooks/useSkillName';
import { activeSkillEffects } from '../../lib/skillSelection';
import { skillDisplayName } from '../../lib/skillNames';
import { bindT, slotLabel, statNameLabel, type Lang, type UiKey } from '../../lib/i18n';
import { OBJECTIVE_PRESETS, scoreOf, type Objective } from '../../lib/optimize';
import { REALM_DEFAULT_LEAGUE, REALM_LEAGUES, TRADE_CURRENCIES, buildTradeUrl, gemTradeUrl, loadTradeLeagues, type TradeCurrency, type TradePriceCap, type TradeRealm } from '../../lib/trade';
import { affixPool, basesForSlot, categoryAffixPool, categorySearchMods, loadTradeCatalog, optimizeTradeAffixes, referenceBase, jewelSearchType, type JewelSearchType, type TradeCatalog } from '../../lib/tradeOptimizer';
import { planGemUpgrades } from '../../lib/tradeMarket';
import { AppSelect } from '../shared/AppSelect';
import { CopyButton } from '../shared/CopyButton';
import { objectiveOf, ObjectiveEditor } from '../shared/OptimizerControls';
import { formatNumber, statMap, formatStat } from '../../lib/statDisplay';
import { UnincludedModifiers } from '../shared/UnincludedModifiers';
import { rankUpgradePositions, type PositionAnalysis } from '../../lib/tradePriority';
import { hasReusableTradeAnalysis, hasUsableTradeWeights, reusableTradeCount, type CachedTradeAnalysis, type TradeAnalysisMode } from '../../lib/tradeAnalysis';
import { tradeAnalysisText } from '../../lib/tradeAnalysisText';
import './trade.css';

const TRADE_OBJECTIVES = OBJECTIVE_PRESETS;
const PRIMARY_GOALS = ['dps', 'ehp', 'balanced'];
const TRADE_SLOTS = ['weapon1', 'weapon2', 'helmet', 'bodyarmour', 'gloves', 'boots', 'amulet', 'ring1', 'ring2', 'belt', 'Flask 1', 'Flask 2', 'Charm 1', 'Charm 2', 'Charm 3'];
const REALM_KEY = 'pobr-trade-realm';
const leagueKey = (realm: TradeRealm) => `pobr-trade-league-${realm}`;
const BUDGET_KEY = 'pobr-trade-budget';
const CURRENCY_KEY = 'pobr-trade-currency';
type SlotResult = PositionAnalysis;

/** Local build analysis produces official search links; login and buying stay on the market. */
export function TradePanel({ session, lang, active, focus, onSkills, onTree, initialItemText }: {
  session: BuildSession; lang: Lang; active: boolean; focus?: {slot:string; nonce:number; jewelType?: JewelSearchType};
  onSkills?: (group:number) => void; onTree?: (node?: number) => void;
  initialItemText?: string;
}) {
  const preferenceKey = (key: string) => uiStorageKey(key);
  const tt = bindT(lang);
  const flowT = (key: Parameters<typeof tradeAnalysisText>[1]) => tradeAnalysisText(lang, key);
  const ut = (key: Parameters<typeof upgradeT>[1]) => upgradeT(lang, key);
  const { goal, setGoal } = useUpgradeGoal();
  const damage = useHeadlineDamage(session.calc, lang);
  const { stat: damageStat, label: damageLabel } = damage;
  const goalLabel = presetLabel(goal.preset);
  function presetLabel(id: string) {
    return id === 'dps' ? damageLabel : id === 'ehp' ? statNameLabel(lang, 'TotalEHP')
      : id === 'balanced' ? flowT('balancedGoal') : tt(TRADE_OBJECTIVES.find(entry => entry.id === id)!.labelKey as UiKey);
  }
  const skillCatalog = useSkillCatalog();
  const skillName = (skillId: string) => skillDisplayName(skillId, lang, skillCatalog);
  const cnSkillName = (skillId: string) => skillDisplayName(skillId, 'zh-CN', skillCatalog);
  const [catalog, setCatalog] = useState<TradeCatalog | null>(null);
  const [catalogError, setCatalogError] = useState(false);
  const [treeNodes, setTreeNodes] = useState<PassiveNode[]>([]);
  const jewelSockets = useMemo(() => treeNodes.filter(node => node.kind === 'jewel_socket').map(node => node.skill), [treeNodes]);
  const namedTreeNodes = useMemo(() => treeNodes.map(node => {
    const change = session.calc?.tree_effects?.nodes[String(node.skill)];
    return change ? { ...node, name: change.name } : node;
  }), [treeNodes, session.calc?.tree_effects]);
  const landmarks = useMemo(() => namedTreeNodes.filter(node => ['notable', 'keystone'].includes(node.kind) && !node.ascendancy_id), [namedTreeNodes]);
  const passiveNames = usePassiveNodeNames(landmarks, lang, active);
  const socketLabels = jewelSocketNames(namedTreeNodes, passiveNames, lang);
  const jewelText = messages(lang, 'jewelPlanner');
  const [selected, setSelected] = useState('weapon1');
  const [jewelTypes, setJewelTypes] = useState<Record<string, JewelSearchType>>({});
  const [categories, setCategories] = useState<Record<string, string>>({});
  const [realm, setRealm] = useState<TradeRealm>(() => readPreference(preferenceKey(REALM_KEY)) === 'cn' ? 'cn' : 'intl');
  const [league, setLeague] = useState(() => readPreference(preferenceKey(leagueKey(realm))) ?? REALM_DEFAULT_LEAGUE[realm]);
  const [leagues, setLeagues] = useState(REALM_LEAGUES[realm]);
  const [leagueFallback, setLeagueFallback] = useState(false);
  const preset = goal.preset;
  const setPreset = (preset: string) => setGoal({ ...goal, preset });
  const [budget, setBudget] = useState(() => readPreference(preferenceKey(BUDGET_KEY)) ?? '100');
  const [currency, setCurrency] = useState<TradeCurrency>(() => {
    const saved = readPreference(preferenceKey(CURRENCY_KEY));
    return TRADE_CURRENCIES.find(option => option.value === saved)?.value ?? 'equiv';
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
  const [allProgress, setAllProgress] = useState<SlotProgress | null>(null);
  useJobBeacon(running !== null);
  const preview = usePreview();
  const [overview, setOverview] = useState(false);
  const [interrupted, setInterrupted] = useState(false);
  const analysisRef = useRef<HTMLElement>(null);
  const marketRef = useRef<HTMLElement>(null);
  const handledFocus = useRef<number | undefined>(undefined);
  const goalRef = useRef<HTMLDivElement>(null);
  const pasteRef = useRef<HTMLDivElement>(null);
  const abortRef = useRef<AbortController | null>(null);
  const cacheRef = useRef<Record<string, CachedTradeAnalysis<SlotResult>>>({});
  const leagueChoices = useRef<Partial<Record<TradeRealm, string>>>({});
  const chooseLeague = (next: string) => {
    leagueChoices.current[realm] = next;
    setLeague(next);
    writePreference(preferenceKey(leagueKey(realm)), next);
  };
  useLayoutEffect(() => {
    if (!focus || !active || handledFocus.current === focus.nonce) return;
    setSelected(focus.slot);
    if (focus.jewelType) setJewelTypes(prev => ({ ...prev, [focus.slot]: focus.jewelType! }));
  }, [focus, active]);
  useEffect(() => {
    if (!focus || !active || handledFocus.current === focus.nonce) return;
    // The shell resets its scroll on navigation. Follow that reset on the next frame.
    const frame = requestAnimationFrame(() => {
      handledFocus.current = focus.nonce;
      marketRef.current?.scrollIntoView({ block: 'start' });
      marketRef.current?.focus({ preventScroll: true });
    });
    return () => cancelAnimationFrame(frame);
  }, [focus, active]);
  const mainGroup = session.calcParams.main_socket_group;
  const selectedGroup = mainGroup === undefined ? undefined : session.socketGroups[mainGroup];
  const selectedEffect = selectedGroup && skillCatalog.size
    ? activeSkillEffects(selectedGroup, skillCatalog)[(selectedGroup.main_active_skill ?? 1) - 1] : undefined;
  const inspectedName = selectedEffect ? skillName(selectedEffect.skillId) : tt('sidebar.unselected');
  const calculatedSkill = session.calc?.main_skill;
  const hasSkillTarget = mainGroup !== undefined && !!selectedGroup?.enabled && !!calculatedSkill
    && calculatedSkill.group_index === mainGroup
    && calculatedSkill.active_skill_index === (selectedGroup?.main_active_skill ?? 1);

  useEffect(() => {
    loadTradeCatalog().then(setCatalog).catch(() => setCatalogError(true));
    getBackend().then(backend => backend.loadPassiveTree(session.treeVersion)).then(nodes => {
      setTreeNodes(nodes);
    }).catch(() => {});
  }, [session.treeVersion]);
  useEffect(() => {
    let cancelled = false;
    setLeagues(REALM_LEAGUES[realm]); setLeagueFallback(false);
    loadTradeLeagues(realm).then(list => {
      if (cancelled) return;
      setLeagues(list);
      // A user choice wins even when storage could not save it.
      if (leagueChoices.current[realm] === undefined && readPreference(preferenceKey(leagueKey(realm))) === null) setLeague(list[0]);
    }).catch(() => { if (!cancelled) setLeagueFallback(true); });
    return () => { cancelled = true; };
  }, [realm]);
  // Invalidate before async work can publish against a newly committed build or goal,
  // including while hidden. Market-only changes still update links without recalculation.
  useLayoutEffect(() => {
    abortRef.current?.abort(); abortRef.current = null;
    cacheRef.current = {};
    setRunning(null); setProgress(null); setResults({}); setAllProgress(null); setInterrupted(false); setRequiredStats({});
  }, [goal, session.currentRequest, categories, jewelTypes, damageStat]);
  useLayoutEffect(() => {
    if (active || !abortRef.current) return;
    abortRef.current.abort(); abortRef.current = null;
    setRunning(null); setProgress(null); setInterrupted(true);
  }, [active]);
  useLayoutEffect(() => () => { abortRef.current?.abort(); abortRef.current = null; }, []);
  useLayoutEffect(() => {
    if (selected.startsWith('Jewel@') && !session.allocatedNodes.includes(Number(selected.slice(6)))) {
      setSelected('weapon1');
    }
  }, [selected, session.allocatedNodes]);

  const slots = [...TRADE_SLOTS, ...jewelSockets.filter(node => session.allocatedNodes.includes(node)).map(node => `Jewel@${node}`)];
  const bySlot = new Map([...session.items, ...session.flasks,
    ...session.jewels.map(jewel => ({ slot: `Jewel@${jewel.socket_node}`, text: jewel.text })),
  ].map(item => [item.slot, item.text]));
  const slotNames = useItemDisplayNames(slots.map(slot => bySlot.get(slot)), lang);
  const labelOf = (slot: string) => slot === 'gems' ? tt('trade.gems') : slot.startsWith('Jewel@') ? socketLabels.get(Number(slot.slice(6))) ?? tt('trade.jewelSocket') : slotLabel(lang, slot);
  const baseOf = (slot: string) => catalog ? referenceBase(catalog, slot, bySlot.get(slot), categories[slot], session.character?.level, jewelTypes[slot]) : undefined;
  const hasEquipment = slots.some(slot => bySlot.has(slot));
  const visibleSlots = slots.filter(slot => showEmpty || !hasEquipment || bySlot.has(slot) || slot === selected || slot.startsWith('Jewel@'));
  const priceCap = useMemo<TradePriceCap | undefined>(() => {
    const max = Number(budget);
    return Number.isFinite(max) && max > 0 ? { max, currency } : undefined;
  }, [budget, currency]);
  const objective = useMemo<Objective>(() => {
    const next = objectiveOf(goal, Object.fromEntries([...statMap(session.calc?.stats ?? [])]
      .filter((entry): entry is [string, number] => entry[1] !== null)));
    return resolveDamageObjective(next, session.calc);
  }, [goal, session.calc, damageStat]);
  const needsSkill = ['TotalDPS', 'CombinedDPS', 'MainMinionDPS'].includes(objective.stat) || !!objective.per
    || objective.constraints.some(constraint => ['ActionRate', 'ManaCost', 'TotalDPS', 'CritChance'].includes(constraint.stat));
  const canAnalyze = !!session.calc && !session.calcError && (!needsSkill || hasSkillTarget);
  const result = results[selected];
  const weights = result?.weights;
  const localized = useLocalizedLines(weights?.weighted.map(weight => weight.line) ?? [], lang);
  const situationalLines = useLocalizedLines(weights?.situational?.map(stat => stat.line) ?? [], lang);
  const selectedBase = selected === 'gems' ? undefined : baseOf(selected);
  const selectedName = selected === 'gems' ? inspectedName
    : slotNames[slots.indexOf(selected)] || tt('trade.slotEmpty');
  const choices = catalog ? [...new Set(basesForSlot(catalog, selected).map(base => base.category))] : [];
  const hasScores = hasUsableTradeWeights(weights?.weighted);
  const searchUrl = result?.category && weights && hasScores ? buildTradeUrl(league, weights.weighted, {
    realm, category: result.category, price: priceCap, maxLevel: session.character?.level, includeUnique,
    minimumWeight: broad ? 0 : weights.minimumWeight,
    requiredStats: requiredStats[selected],
  }) : null;
  const searchReady = !!searchUrl && !!league.trim();
  const maxGain = Math.max(...(weights?.weighted.map(weight => weight.gain) ?? []), 1e-9);
  const disabled = !active || running !== null || session.busy || !catalog || !canAnalyze;

  const priorities = rankUpgradePositions(results, objective, damageStat);
  const fullTargets = slots.filter(slot => (bySlot.has(slot) || slot.startsWith('Jewel@')) && baseOf(slot));
  const reusableAllCount = reusableTradeCount(fullTargets, cacheRef.current, 'all');
  const allComplete = fullTargets.length > 0 && reusableAllCount === fullTargets.length;
  const cancelAnalysis = () => {
    if (!abortRef.current) return;
    abortRef.current.abort(); abortRef.current = null;
    setRunning(null); setProgress(null); setInterrupted(true);
  };
  const analyze = async (targets: string[], all = false, force = false) => {
    if (!active || !canAnalyze || session.busy) return;
    const request = session.currentRequest();
    if (!request || !catalog) return;
    const controller = new AbortController(); abortRef.current = controller;
    const mode: TradeAnalysisMode = all || overview ? 'all' : 'single';
    const completed = all && !force ? reusableTradeCount(targets, cacheRef.current, 'all') : 0;
    if (all) setOverview(true);
    setAllProgress({ done: completed, total: targets.length, measuredMs: 0, measuredSlots: 0 });
    setInterrupted(false);
    let done = completed;
    let measuredMs = 0;
    let measuredSlots = 0;
    try {
      for (const slot of targets) {
        controller.signal.throwIfAborted();
        if (all && !force && hasReusableTradeAnalysis(slot, cacheRef.current[slot], 'all')) continue;
        setRunning(slot); setProgress({ done: 0, total: 1 });
        const options = { signal: controller.signal, onProgress: (done: number, total: number) => {
          if (abortRef.current === controller && !controller.signal.aborted) setProgress({ done, total });
        } };
        const startedAt = performance.now();
        let succeeded = false;
        try {
          let next: SlotResult;
          if (slot === 'gems') {
            if (mainGroup === undefined || !hasSkillTarget) continue;
            next = { gems: await planGemUpgrades(request, catalog, mainGroup, objective, options.signal, options.onProgress) };
          } else {
            const base = baseOf(slot);
            if (!base) continue;
            // PoB2 Item:Craft derives affix requirements as floor(mod.level * 0.8).
            const itemLevel = Math.min(100, Math.ceil(((request.character?.level ?? 1) + 1) / 0.8) - 1);
            next = { category: base.category, weights: await optimizeTradeAffixes({
              request, slot, base, pool: categoryAffixPool(catalog, base.category, itemLevel, request.character?.level, slot.startsWith('Jewel@') ? jewelSearchType(base) : undefined), itemLevel,
              searchMods: categorySearchMods(catalog, base.category, itemLevel, slot.startsWith('Jewel@') ? jewelSearchType(base) : undefined),
              objective, combinations: mode === 'all', combinationPool: affixPool(catalog, base, itemLevel),
              maxEvaluations: mode === 'all' ? 768 : undefined, beamWidth: 6, ...options,
            }) };
          }
          controller.signal.throwIfAborted();
          if (abortRef.current !== controller) break;
          cacheRef.current[slot] = { result: next, mode };
          setResults(prev => ({ ...prev, [slot]: next }));
          succeeded = true;
        } catch (error) {
          if (controller.signal.aborted || abortRef.current !== controller) break;
          delete cacheRef.current[slot];
          setResults(prev => ({ ...prev, [slot]: { error: error instanceof Error ? error.message : String(error) } }));
        } finally {
          if (abortRef.current === controller && !controller.signal.aborted) {
            if (succeeded) {
              measuredMs += performance.now() - startedAt;
              measuredSlots++;
            }
            setAllProgress({ done: ++done, total: targets.length, measuredMs, measuredSlots });
          }
        }
      }
    } catch (error) {
      if (!controller.signal.aborted) throw error;
    } finally {
      if (abortRef.current === controller) { setRunning(null); setProgress(null); abortRef.current = null; }
    }
  };

  const editGoal = () => { goalRef.current?.scrollIntoView({ block: 'start' }); goalRef.current?.focus({ preventScroll: true }); };
  const deltaClass = (value: number) => value < 0 ? 'delta-neg' : value > 0 ? 'delta-pos' : '';
  const goalBlock = <>
    <div className="upgrade-evaluation">
      <div className="trade-objectives" role="group" aria-label={tt('opt.objective')} ref={goalRef} tabIndex={-1}>
        <div className="segmented">{PRIMARY_GOALS.map(id => <AppButton key={id} aria-pressed={preset === id}
          onClick={() => setPreset(id)}>{presetLabel(id)}</AppButton>)}</div>
      </div>
      {damage.note && <p className="trade-priority-hint">{damage.note}</p>}
        {session.socketGroups.length > 0 && <div className="trade-skill-context"><span>{words(lang, 'Calculated skill', '计算技能', '計算技能')}</span>
          <AppSelect value={mainGroup === undefined ? '' : String(mainGroup)} ariaLabel={tt('trade.analysisSkill')} disabled={session.busy}
            placeholder={tt('sidebar.unselected')}
            options={session.socketGroups.flatMap((group, index) => {
              if (!group.enabled || !group.gems.length) return [];
              const effect = skillCatalog.size ? activeSkillEffects(group, skillCatalog)[(group.main_active_skill ?? 1) - 1] : undefined;
              return [{ value: String(index), label: `${index + 1}. ${effect ? skillName(effect.skillId) : tt('calcs.unavailable')}` }];
            })}
            onChange={value => session.updateParams({ main_socket_group: Number(value) })} />
          <span className="trade-skill-hint">{tt('trade.skillContext')}</span></div>}
    {!session.busy && !hasSkillTarget && <p className="trade-notice" role="status">{tt('sidebar.noMainSkill')}</p>}
    <WeaponSetControl session={session} lang={lang} />
    <div className="trade-safety-settings">
      {preset === 'balanced' && <label><AppInput type="checkbox" checked={keepEhp} onChange={event => setKeepEhp(event.target.checked)} />{flowT('keepEhp')}</label>}
      <label><AppInput type="checkbox" checked={resistanceFirst} onChange={event => setResistanceFirst(event.target.checked)} />{tt('trade.resistanceFirst')}</label>
      {resistanceFirst && <label>{tt('trade.resistanceTarget')}<AppInput type="number" min={0} max={90} value={resistanceTarget} aria-label={tt('trade.resistanceTarget')}
        onChange={event => { const value = Number(event.target.value); if (Number.isFinite(value) && value >= 0 && value <= 90) setResistanceTarget(value); }} />%</label>}
      <details className="trade-goal-help"><summary className="disclosure">{tt('trade.goalHelp')}</summary><p>{preset === 'balanced' ? flowT('balancedHint') : tt('trade.resistanceHint')}</p>{preset === 'balanced' && <p>{flowT('balancedAlgorithm')}</p>}</details>
    </div>
    <details className="upgrade-goal-details" open={!!goal.cStat || !PRIMARY_GOALS.includes(preset)}><summary className="disclosure">{flowT('advancedGoal')}{goal.cStat ? ` · ${statNameLabel(lang, goal.cStat)}` : ''}</summary><div className="trade-objectives"><div className="segmented">{TRADE_OBJECTIVES.filter(entry => !PRIMARY_GOALS.includes(entry.id)).map(entry =>
      <AppButton key={entry.id} aria-pressed={preset === entry.id} onClick={() => setPreset(entry.id)}>{presetLabel(entry.id)}</AppButton>)}</div></div><div className="opt-controls"><ObjectiveEditor value={goal} onChange={setGoal} lang={lang} constraintsOnly /></div></details>
    </div>
  </>;
  const pathsBlock = <>
    <div className="upgrade-paths">
      <AppButton aria-pressed className="upgrade-equipment-path" onClick={() => { analysisRef.current?.scrollIntoView({ block: 'start' }); analysisRef.current?.focus({ preventScroll: true }); }}><SlotSymbol slot="helmet" /><strong>{flowT('equipmentHere')}</strong></AppButton>
      <AppButton disabled={!onTree} onClick={() => onTree?.()}><SlotSymbol slot="Jewel@" /><strong>{flowT('passivesPage')}</strong></AppButton>
      <AppButton disabled={!onSkills || !selectedGroup} onClick={() => { if (mainGroup !== undefined) onSkills?.(mainGroup); }}><SlotSymbol slot="gems" /><strong>{flowT('supportsPage')}</strong></AppButton>
    </div>
    <p className="upgrade-shared-goal">{ut('sharedGoal')}</p>
  </>;
  const replacementBlock = <>
    <ItemReplacementCheck session={session} lang={lang} active={active && canAnalyze} objective={objective} catalog={catalog} jewelSockets={jewelSockets} initialItemText={initialItemText} socketLabel={id => socketLabels.get(id) ?? tt('trade.jewelSocket')}
      goalLabel={goalLabel} damage={damage} actionClassName="trade-secondary"
      onEditGoal={editGoal} />
  </>;
  const marketBlock = <>
    <div className="trade-setup">
      <label className="trade-price-field"><span className="trade-field-label">{tt('trade.budget')}</span>
        <span className="trade-budget"><AppInput type="number" min={0} aria-label={tt('trade.budget')} value={budget} placeholder={tt('trade.budgetAny')}
          onChange={event => { setBudget(event.target.value); writePreference(preferenceKey(BUDGET_KEY), event.target.value); }} />
          <AppSelect value={currency} ariaLabel={tt('trade.currency')}
            options={TRADE_CURRENCIES.map(option => ({ value: option.value, label: tt(option.labelKey) }))}
            onChange={value => { setCurrency(value as TradeCurrency); writePreference(preferenceKey(CURRENCY_KEY), value); }} /></span>
      </label>
      <div className="trade-market-fields">
        <label><span className="trade-field-label">{tt('trade.realm')}</span><AppSelect value={realm} ariaLabel={tt('trade.realm')}
          options={[{ value: 'intl', label: tt('trade.realmIntl') }, { value: 'cn', label: tt('trade.realmCn') }]}
          onChange={value => { const next = value as TradeRealm; setRealm(next); writePreference(preferenceKey(REALM_KEY), next);
            setLeague(leagueChoices.current[next] ?? readPreference(preferenceKey(leagueKey(next))) ?? REALM_DEFAULT_LEAGUE[next]); }} /></label>
        <label><span className="trade-field-label">{tt('trade.league')}</span><AppSelect value={leagues.includes(league) ? league : '__custom'} ariaLabel={tt('trade.league')}
          options={[...leagues.map(name => ({ value: name, label: name })), { value: '__custom', label: tt('trade.leagueCustom') }]}
          onChange={value => chooseLeague(value === '__custom' ? '' : value)} /></label>
        {!leagues.includes(league) && <AppInput aria-label={tt('trade.leagueCustom')} value={league} placeholder={REALM_DEFAULT_LEAGUE[realm]}
          onChange={event => chooseLeague(event.target.value)} />}
      </div>
    </div>
  </>;
  const toolbarBlock = <>
    <div className="trade-overview-toolbar">
      <div><p>{flowT('analysisScope')}</p></div>
      {!running && <AppButton className="trade-primary trade-analyze-all" disabled={disabled || fullTargets.length === 0}
        onClick={() => void analyze(fullTargets, true, allComplete)}>{allComplete ? tradeAnalysisText(lang, 'refresh') : reusableAllCount > 0 ? tradeAnalysisText(lang, 'resume') : tt('trade.analyzeAll')}</AppButton>}

    </div>
  </>;
  const prioritiesBlock = <>
    {overview && <section className="trade-priorities" aria-label={tt('trade.overviewTitle')}>
      <div className="trade-results-heading"><h4>{tt('trade.priorityOrder')}</h4>
        {allProgress && <span className="ui-badge" role="status">{reusableAllCount} / {fullTargets.length} {tt('trade.positions')}</span>}</div>
      <p className="trade-priority-hint">{flowT('analysisHint')}</p>
      <div className="trade-priority-table-wrap"><table className="trade-priority-table">
        <thead><tr><th>{flowT('position')}</th><th>{flowT('balancedScore')}</th><th>{damageLabel}</th><th>{statNameLabel(lang, 'TotalEHP')}</th><th><span className="sr-only">{flowT('goMarket')}</span></th></tr></thead>
        <tbody>{priorities.map(entry => <tr key={entry.slot}>
          <th scope="row"><strong>{labelOf(entry.slot)}</strong><small>{slotNames[slots.indexOf(entry.slot)]}</small></th>
          {[entry.balancedPercent, entry.dpsPercent, entry.ehpPercent].map((value, index) => <td key={index} className={value === undefined ? '' : deltaClass(value)}>
            {value === undefined ? '—' : `${value >= 0 ? '+' : ''}${formatNumber(value, 1)}%`}</td>)}
          <td><AppButton className="trade-priority-position" onClick={() => {
            setSelected(entry.slot); marketRef.current?.scrollIntoView({ block: 'start' }); marketRef.current?.focus({ preventScroll: true });
          }}>{flowT('goMarket')}<span className="sr-only"> · {labelOf(entry.slot)}</span></AppButton></td>
        </tr>)}</tbody>
      </table></div>
      {!running && priorities.length === 0 && <p className="trade-notice">{tt('trade.noPriority')}</p>}
      {Object.entries(results).some(([, entry]) => entry.error) && <p className="trade-notice">{tt('trade.partialAnalysis')}</p>}
    </section>}
  </>;
  const fallbackBlock = <>
    {leagueFallback && <p className="trade-notice">{tt('trade.leagueFallback')}</p>}
  </>;
  const workspaceBlock = <>
    {catalogError ? <p role="alert" className="trade-notice">{tt('trade.unavailable')}</p> : <div className="trade-workspace">
      <aside className="trade-position-panel">
        <div className="trade-section-label">{tt('trade.chooseSlot')}</div>
        <nav aria-label={tt('trade.chooseSlot')} className="trade-position-list">
          {visibleSlots.map(slot => <AppButton key={slot} className="trade-position" data-slot-id={slot} aria-pressed={selected === slot} onClick={() => setSelected(slot)}>
            <span className="trade-position-symbol" aria-hidden><SlotSymbol slot={slot} /></span>
            <span><strong>{labelOf(slot)}</strong><small>{slotNames[slots.indexOf(slot)] || tt('trade.slotEmpty')}</small></span>
            {results[slot]?.weights && <span className="trade-position-done" title={tt('trade.analyzed')}>{results[slot].weights!.weighted.some(weight => weight.gain > 0) ? jewelText.upgradeAvailable : tt('trade.analyzed')}</span>}
          </AppButton>)}
          <AppButton className="trade-position" data-slot-id="gems" aria-pressed={selected === 'gems'} onClick={() => setSelected('gems')}>
            <span className="trade-position-symbol" aria-hidden><SlotSymbol slot="gems" /></span><span><strong>{tt('trade.gems')}</strong><small>{tt('trade.gemSubtitle')}</small></span>
          </AppButton>
        </nav>
        {hasEquipment && <AppButton className="trade-text-button" onClick={() => setShowEmpty(!showEmpty)}>{tt(showEmpty ? 'trade.hideEmpty' : 'trade.showEmpty')}</AppButton>}
      </aside>
      <div className="trade-analysis">
        <header className="trade-analysis-header">
          <div><span className="trade-section-label">{labelOf(selected)}</span><h3>{selectedName}</h3>
            <div className="trade-scope"><span>{selectedBase ? tt(`trade.category.${selectedBase.category}` as UiKey) : tt('trade.gems')}</span>
              {selected !== 'gems' && <><span>{tt('trade.allBases')}</span>{!includeUnique && <span>{tt('trade.nonUnique')}</span>}</>}<span>{tt('trade.levelLimit')} {session.character?.level ?? 1}</span></div>
          </div>
          <div className="trade-header-actions">{!running && <AppButton className="trade-secondary" disabled={disabled || (selected !== 'gems' && !selectedBase) || (selected === 'gems' && (!hasSkillTarget || !!selectedGroup?.source))}
            onClick={() => void analyze([selected])}>{tt(weights || result?.gems ? 'trade.recalculate' : 'trade.analyze')}</AppButton>}</div>
        </header>
        {selected.startsWith('Jewel@') && <AppButton disabled={!onTree} onClick={() => onTree?.(Number(selected.slice(6)))}>{jewelText.locate}</AppButton>}
        {selected.startsWith('Jewel@') && selectedBase && <div className="trade-range">
          <AppSelect value={jewelSearchType(selectedBase)} ariaLabel={tt('trade.jewelType')}
            options={[{ value: 'base', label: tt('trade.baseJewel') }, { value: 'radius', label: tt('trade.radiusJewel') }]}
            onChange={value => setJewelTypes(prev => ({ ...prev, [selected]: value as JewelSearchType }))} />
          {selectedBase.radius && <p>{tt('trade.radiusHint')}</p>}
        </div>}
        {selected !== 'gems' && choices.length > 1 && <details className="trade-range">
          <summary className="disclosure">{tt('trade.changeType')}</summary><AppSelect value={selectedBase?.category ?? ''} ariaLabel={`${labelOf(selected)} ${tt('trade.category')}`}
            options={choices.map(category => ({ value: category, label: tt(`trade.category.${category}` as UiKey) }))}
            onChange={category => setCategories(prev => ({ ...prev, [selected]: category }))} />
        </details>}
        {result?.error && <div role="alert" className="trade-notice trade-error">{tt('trade.analysisFailed')}<details><summary className="disclosure">{tt('trade.errorDetails')}</summary>{result.error}</details></div>}
        {!weights && !result?.gems && running !== selected && <div className="trade-empty-state">
          <span className="trade-empty-symbol" aria-hidden><SlotSymbol slot={selected} /></span>
          <h4>{tt('trade.emptyTitle')}</h4><p>{tt(selected === 'gems' ? 'trade.gemHint' : 'trade.emptyDescription')}</p>
        </div>}
        {weights && <>
          <section className="upgrade-score-reference ui-card" aria-label={ut('currentScore')}>
            <div><span>{ut('currentScore')}</span><strong>{weights.currentItemScore ? formatNumber(weights.currentItemScore.score, 0) : '—'}</strong>
              <small>{ut(!weights.currentItemScore ? 'noScore' : weights.currentItemScore.complete ? 'complete' : 'partial')}</small></div>
            <div className="upgrade-current-stats"><span>{damageLabel} <b>{weights.baseline[damageStat] === undefined ? '—' : formatStat(damageStat, weights.baseline[damageStat], lang)}</b></span><span>{statNameLabel(lang, 'TotalEHP')} <b>{formatStat('TotalEHP', weights.baseline.TotalEHP ?? 0, lang)}</b></span></div>
            <p>{ut('scoreHint')}</p>
            {weights.scoreWarnings.includes('unmodeled-candidates') && <p>{ut('skippedUnmodeled')}</p>}
            {weights.scoreWarnings.includes('constraints-not-in-query') && <p className="upgrade-score-caution">{ut('marketConstraints')}</p>}
            {weights.currentItemScore && <details><summary className="disclosure">{ut('scoreDetails')}</summary><p>{ut('scoredLines')}{colon(lang)}{weights.currentItemScore.matchedLines}/{weights.currentItemScore.totalLines}</p>
              <ul>{weights.currentItemScore.contributions.map((entry,index) => <li key={index}>{entry.line} · {formatNumber(entry.value, 2)} × {formatNumber(entry.weight, 3)} = {formatNumber(entry.score, 3)}</li>)}</ul>
              {weights.currentItemScore.unscoredLines.length > 0 && <pre>{weights.currentItemScore.unscoredLines.join('\n')}</pre>}
            </details>}
          </section>
          <div className="trade-results-heading"><div><h4>{tt('trade.affixHeading')}</h4><p>{tt('trade.affixDescription')}</p></div>
            <span className="trade-count">{weights.weighted.length} {tt('trade.usefulAffixes')}</span></div>
          {!hasScores ? <div className="trade-notice" role="status"><p>{tt('trade.noEquipmentScores')}</p><AppButton onClick={editGoal}>{tt('trade.adjustTarget')}</AppButton></div> : <div className="trade-score-table" role="table" aria-label={tt('trade.affixHeading')}>
            <div className="trade-score-row trade-score-labels" role="row"><span role="columnheader">{tt('trade.colLine')}</span><span role="columnheader">{tt('trade.priority')}</span><span role="columnheader">{ut('marginal')}</span></div>
            {weights.weighted.map((weight, index) => <div key={weight.id} className="trade-score-row" role="row">
              <span className="trade-affix" role="cell"><span className="trade-rank">{String(index + 1).padStart(2, '0')}</span>{lineText(weight.line, localized[index])}</span>
              <span className="trade-priority" role="cell"><span className="trade-bar"><span style={{ width: `${weight.gain / maxGain * 100}%` }} /></span><b>{Math.round(weight.gain / maxGain * 100)}</b></span>
              <span className="trade-gain" role="cell">+{formatNumber(weight.gainPercent, 1, 1)}%</span>
            </div>)}
          </div>}
          {Boolean(weights.situational?.length) && <div className="trade-situational">
            <h4>{tt('trade.situational')}</h4><p>{tt('trade.situationalHint')}</p>
            {weights.situational?.map((stat, index) => <label key={stat.id}>
              <AppInput type="checkbox" checked={(requiredStats[selected] ?? []).includes(stat.id)} onChange={event => setRequiredStats(prev => ({ ...prev,
                [selected]: event.target.checked ? [...(prev[selected] ?? []), stat.id] : (prev[selected] ?? []).filter(id => id !== stat.id),
              }))} />
              <span><strong>{lineText(stat.line, situationalLines[index])}</strong><small>{tt(`trade.mechanic.${stat.kind}`)}{stat.delta !== undefined ? ` +${formatNumber(stat.delta, 2)}` : ''}</small></span>
            </label>)}
          </div>}
          {weights.combinations[0] && <details className="trade-method"><summary className="disclosure">{tt('trade.referenceItem')}</summary><pre className="trade-reference">{weights.combinations[0].text}</pre></details>}
          <details className="trade-method"><summary className="disclosure">{ut('weightDetails')}</summary><p>{tt('trade.methodDescription')}</p>
            <ul>{weights.weighted.map((weight, index) => <li key={weight.id}>{lineText(weight.line, localized[index])} · {tt('trade.marketWeight')}{colon(lang)}{formatNumber(weight.weight, 3)}</li>)}</ul>
            <p>{tt('trade.currentGoal')}{colon(lang)}{goalLabel} {formatNumber(scoreOf(weights.baseline, objective), 2)} · {weights.evaluated} {tt('trade.evaluations')}</p></details>
          {weights.unsupported.length > 0 && <details className="trade-notice trade-unsupported"><summary className="disclosure">{tt('trade.unsupportedHint')} ({weights.unsupported.length})</summary><UnincludedModifiers lines={weights.unsupported} lang={lang} items={session.items} /></details>}
        </>}
        {result?.gems && <div className="trade-gems"><div className="trade-results-heading"><div><h4>{tt('trade.gemCandidates')}</h4>{onSkills && mainGroup !== undefined && <AppButton onClick={() => onSkills(mainGroup)}>{ut('autoSupports')} →</AppButton>}<p>{tt('trade.gemHint')}</p></div></div>
          {result.gems.length === 0 && <p className="trade-notice">{tt('trade.noGemUpgrade')}</p>}
          {result.gems.map(plan => {
            // Resting on a candidate shows what it would change, from the stats already calculated for it.
            const showPlan = preview && plan.stats && plan.baseline ? (event: { currentTarget: Element }) => preview.showChanges(skillName(plan.gem.skill_id), changesFromRecords(plan.baseline!, plan.stats!), event.currentTarget) : undefined;
            return <article className="trade-gem-card" key={`${plan.gem.skill_id}:${plan.position}:${plan.level}:${plan.quality}`}
              onMouseEnter={showPlan} onMouseLeave={showPlan ? preview!.clear : undefined} onFocus={showPlan} onBlur={showPlan ? preview!.clear : undefined}>
            <span className="trade-position-symbol" aria-hidden><SlotSymbol slot="gems" /></span>
            <div><h4>{skillName(plan.gem.skill_id)} {plan.gem.is_lineage && <span className="gem-lineage-badge">{ut('lineage')}</span>}</h4>{plan.acquisition === 'skill-adjustment' && <small>{ut('adjustment')}</small>}<p>{tt('skills.level')} {plan.level} · {tt('skills.quality')} {plan.quality}%</p>
              <p>{session.socketGroups[plan.group]?.gems[plan.position]
                ? `${tt('trade.replacesGem')} ${skillName(session.socketGroups[plan.group].gems[plan.position].skill_id)}`
                : tt('trade.addsGem')}</p>
              {plan.gainPercent !== undefined && <span className={plan.gainPercent < 0 ? 'delta-neg' : 'trade-gain'}>{tt('trade.referenceGain')} {plan.gainPercent >= 0 ? '+' : ''}{formatNumber(plan.gainPercent, 2)}%</span>}</div>
            <div className="upgrade-gem-stats">{[damage.stat, 'TotalEHP'].map(stat => {
              const before = plan.baseline?.[stat] ?? 0, after = plan.stats?.[stat] ?? 0;
              return <span key={stat} data-stat={stat}>{stat === damage.stat ? damage.label : statNameLabel(lang, stat)} <b className={after < before ? 'delta-neg' : 'delta-pos'}>{formatStat(stat, before, lang)} → {formatStat(stat, after, lang)}</b></span>;
            })}{damage.note && <small>{damage.note}</small>}</div>
            {plan.acquisition !== 'skill-adjustment' && league.trim() && <a className="trade-secondary" href={gemTradeUrl({ realm, league, category: 'gem', price: priceCap, maxLevel: session.character?.level,
              gem: { name: realm === 'cn' ? cnSkillName(plan.gem.skill_id) : plan.gem.name, level: plan.level, quality: plan.quality } })} target="_blank" rel="noreferrer">{tt('trade.browseMarket')}</a>}
            <AppButton disabled={session.busy} onClick={() => {
              const candidate = plan.variant.socket_groups?.[plan.group];
              if (candidate) session.setSocketGroups(session.socketGroups.map((group,index) => index === plan.group
                ? { ...group, gems: candidate.gems, main_active_skill: candidate.main_active_skill } : group));
            }}>{ut('apply')}</AppButton>
          </article>;
          })}
        </div>}
      </div>
    </div>}
  </>;
  const heading = <PageHeader id="trade-heading" title={tt('trade.title')} description={tt('trade.hint')}>
      <span className="trade-local-badge"><span />{tt('trade.localBadge')}</span>
    </PageHeader>;
  const currencyLabel = tt(TRADE_CURRENCIES.find(option => option.value === currency)?.labelKey ?? 'trade.currency');
  const marketSummary = [priceCap ? `${formatNumber(priceCap.max, 2)} ${currencyLabel}` : tt('trade.budgetAny'), tt(realm === 'cn' ? 'trade.realmCn' : 'trade.realmIntl'), league.trim() || '—'].join(' · ');
  const step = (id: string, index: number, title: string) => <h3 className="up-title" id={id}><span className="up-num" aria-hidden="true">{index}</span>{title}</h3>;
  return <section className="ui-page trade-page" aria-labelledby="trade-heading">
    {heading}
    <section className="up-step up-goal" aria-labelledby="up-goal">
      {step('up-goal', 1, flowT('goalTitle'))}
      {goalBlock}
    </section>
    <section className="up-step up-method" aria-labelledby="up-method">
      {step('up-method', 2, flowT('methodTitle'))}
      {pathsBlock}
    </section>
    <section className="up-step up-gain" aria-labelledby="up-gain" ref={analysisRef} tabIndex={-1}>
      {step('up-gain', 3, flowT('analysisTitle'))}
      {toolbarBlock}
      {interrupted && <p className="trade-notice" role="status">{flowT('stopped')}</p>}
      {running && allProgress && <TradeAnalysisProgress slots={allProgress} position={labelOf(running)} lang={lang}
        fraction={progress && progress.total > 0 ? progress.done / progress.total : 0} onStop={cancelAnalysis} />}
      {prioritiesBlock}
    </section>
    <section className="up-step up-market" aria-labelledby="up-market" ref={marketRef} tabIndex={-1}>
      {step('up-market', 4, flowT('marketTitle'))}
      <div className="trade-market-context"><strong>{labelOf(selected)}</strong><span>{flowT('comparisonGoal').replace('{goal}', goalLabel)}</span>
        <p>{flowT('baselineItem').replace('{item}', selectedName)}</p></div>
      {fallbackBlock}
      <details className="trade-market"><summary className="disclosure" data-disclosure-level="section"><span>{words(lang, 'Market', '市集', '市集')}</span><small>{marketSummary}</small></summary>{marketBlock}
      <label className="trade-unique"><AppInput type="checkbox" checked={includeUnique} onChange={event => setIncludeUnique(event.target.checked)} />{tt('trade.includeUnique')}</label>
      </details>
      {searchReady && searchUrl && weights && <div className="trade-search-card">
        <div><p>{flowT('searchPrepared')}</p><p>{tt(realm === 'cn' ? 'trade.cnMarketHint' : 'trade.directMarketHint')}</p>
          <p className="trade-login">{ut('searchLogin')}</p>
          {!broad && <p className="trade-minimum" role="status">{weights.minimumWeight > 0
            ? <>{ut('searchMinimum')}{colon(lang)}<b>{formatNumber(weights.minimumWeight, 0)}</b> · {ut(weights.currentItemScore?.complete ? 'searchMinimumExact' : 'searchMinimumPartial')}</>
            : ut('searchNoMinimum')}</p>}
          <label className="trade-broad"><AppInput type="checkbox" checked={broad} onChange={event => setBroad(event.target.checked)} />{tt('trade.broaderSearch')}</label></div>
        <div className="trade-search-actions"><a className="trade-primary trade-market-link" href={searchUrl} target="_blank" rel="noreferrer">{flowT('openSearch')}<span aria-hidden>↗</span></a>
          <CopyButton text={searchUrl} label={tt('trade.copySearch')} lang={lang} /></div>
      </div>}
      {!searchReady && selected !== 'gems' && <div className="trade-search-pending"><p>{flowT(!league.trim() ? 'chooseLeague' : 'searchPending')}</p><AppButton className="trade-primary" disabled>{flowT('openSearch')}</AppButton></div>}
      {workspaceBlock}
      <div className="up-paste" ref={pasteRef}>{replacementBlock}</div>
    </section>
  </section>;
}
