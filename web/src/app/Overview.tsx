import { EnergyTriggerNote } from '../components/config/EnergyTriggerNote';
import { useEffect, useState, type ReactNode } from 'react';
import { AlertTriangle, ArrowRight, BookmarkPlus, CircleAlert, CircleCheck, Download, Gem, Network, Search, Share2 } from 'lucide-react';
import { getBackend } from '../api/backend';
import type { AttributionResponse, GemCatalogEntry, SkillMetricsEntry } from '../api/types';
import { Button } from '../components/ui/button';
import type { BuildSession } from '../hooks/useBuildSession';
import { useMainMinion } from '../hooks/useMainMinion';
import { useSkillCatalog } from '../hooks/useSkillName';
import { headlineDpsStat, minionDpsNote } from '../lib/mainMinion';
import { classDisplayName } from '../lib/nativeDisplayNames';
import { bindT, damageTypeLabel, enemyTierLabel, grantedSourceLabel, slotLabel, statNameLabel, type Lang } from '../lib/i18n';
import { contributionLabel, removalChange } from '../lib/sourceContributions';
import { activeSkillEffects } from '../lib/skillSelection';
import { skillDisplayName } from '../lib/skillNames';
import { unmodelledMetaTriggerNote } from '../lib/triggerNotes';
import { formatNumber, formatStatValue, statMap, formatStat, statRowLabel } from '../lib/statDisplay';
import { UnincludedModifiers } from '../components/shared/UnincludedModifiers';
import { deriveFindings, type Finding } from './findings';
import { AnimatedNumber } from './motion/AnimatedNumber';
import { useCancellableJob } from './jobs/useCancellableJob';
import { useUpgradeGoal } from '../hooks/useUpgradeGoal';
import type { SettledCalc } from './motion/useSettledCalc';
import type { Route } from './routes';
import { colon } from './text';
import { calculatedSkillKey } from './calculatedSkill';
import { resolveEnemyTier } from './enemy';
import { requestSkillMetrics } from './pages/skills/useSkillMetrics';

const MAX_HIT = ['Physical', 'Fire', 'Cold', 'Lightning', 'Chaos'] as const;
const ATTRIBUTION_FIELDS = ['CombinedDPS', 'TotalEHP', 'Life'] as const;
const int = (value: number) => formatStatValue(value, 'int');

function describe(finding: Finding, lang: Lang, catalog: ReadonlyMap<string, GemCatalogEntry>): { title: string; detail: string } {
  const tt = bindT(lang);
  switch (finding.kind) {
    case 'no-main-skill': return { title: tt('overview.noSkillTitle'), detail: tt('overview.noSkillDetail') };
    case 'item-errors': return { title: tt('overview.unreadableItems', { count: finding.slots.length }), detail: finding.slots.map(slot => slotLabel(lang, slot)).join(' · ') };
    case 'spirit': return { title: tt('overview.spiritOvercommit', { amount: int(-finding.value) }), detail: tt('overview.spiritDetail') };
    case 'attribute': { const name = statNameLabel(lang, finding.attribute); return { title: tt('overview.attributeShortfall', { attribute: name, have: int(finding.have), need: int(finding.need) }), detail: tt('overview.attributeDetail') }; }
    case 'resist': { const name = damageTypeLabel(lang, finding.element); return finding.element === 'Chaos'
      ? { title: tt('overview.negativeResistance', { type: name, value: int(finding.value) }), detail: tt('overview.negativeResistanceDetail') }
      : { title: tt('overview.lowResistance', { type: name, value: int(finding.value) }), detail: tt('overview.resistanceShortfall', { missing: int(finding.missing) }) }; }
    case 'hit-chance': return { title: tt('overview.lowHitChance', { value: formatStatValue(finding.value, 'percent') }), detail: tt('overview.lowHitChanceDetail') };
    case 'net-regen': return {
      title: `${statNameLabel(lang, finding.stat)}${colon(lang)}${formatStatValue(finding.value, 'float2', lang).replace(/^-/, '−')}`,
      detail: tt('overview.recoveryDeficit'),
    };
    case 'unmodelled-meta-trigger': return { title: bindT(lang)('skills.unmodelledMetaTriggerTitle'), detail: unmodelledMetaTriggerNote(finding.trigger, lang, catalog) };
    case 'unsupported': return { title: `${tt('calcs.unsupported')}${colon(lang)}${finding.count}`, detail: tt('overview.unsupportedDetail') };
  }
}

export function Overview({ session, view, lang, navigate, focusStat, onSpirit, onImport, importNotice, onShare, onPlanTree, onPlanSupports }: {
  session: BuildSession; view: SettledCalc; lang: Lang; navigate: (route: Route) => void; focusStat: (id: string) => void;
  onSpirit?: () => void; onImport?: () => void; importNotice?: ReactNode; onShare?: () => void;
  onPlanTree?: () => void; onPlanSupports?: (group: number) => void;
}) {
  const tt = bindT(lang);
  const enemy = resolveEnemyTier(session.calcParams.enemy_tier, session.calcParams.config_inputs);
  const selection = calculatedSkillKey(session);
  const catalog = useSkillCatalog();
  const { goal, setGoal } = useUpgradeGoal();
  const build = session.workspace?.builds.find(entry => entry.id === session.workspace?.activeBuild);
  const stage = build?.stages.find(entry => entry.id === build.activeStage);
  const calc = view.calc;
  const stats = calc ? statMap(calc.stats) : new Map<string, number | null>();
  const value = (id: string) => { const item = stats.get(id); return typeof item === 'number' ? item : null; };
  const skillName = (id: string) => skillDisplayName(id, lang, catalog);
  const main = calc?.main_skill ?? null;
  const minion = useMainMinion(calc, lang);
  const findings = deriveFindings(calc, session.socketGroups.length > 0);
  const className = session.character?.ascendancy_name || session.character?.class_name || '';
  const classLabel = classDisplayName(session.classNames, className, lang);

  // Every active effect, calculated on its own: which skill actually carries the build.
  const [skillResult, setSkillResult] = useState<{ version: number; skills: SkillMetricsEntry[] | null } | null>(null);
  const hasSkills = session.socketGroups.length > 0;
  const skills = hasSkills ? skillResult?.skills : null;
  const skillsPending = hasSkills && !session.calcError && (session.busy || skillResult?.version !== session.stateVersion);
  useEffect(() => { if (!hasSkills) setSkillResult(null); }, [hasSkills]);
  useEffect(() => {
    if (session.busy || session.calcError || !hasSkills) return;
    let stale = false;
    const version = session.stateVersion;
    // The previous ranking stays up while a newer one is computed, so the list does not blink on each edit.
    const timer = setTimeout(() => {
      const result = requestSkillMetrics(session);
      if (!result) { setSkillResult({ version, skills: null }); return; }
      result.then(result => { if (!stale) setSkillResult({ version, skills: result.skills }); }).catch(() => { if (!stale) setSkillResult({ version, skills: null }); });
    }, 250);
    return () => { stale = true; clearTimeout(timer); };
    // eslint-disable-next-line react-hooks/exhaustive-deps -- the version number is the change signal.
  }, [session.stateVersion, session.busy, session.calcError, hasSkills]);
  // Each effect's own damage; the calculated summoning skill counts its minion's, which the per-effect metrics don't carry.
  const isCalculated = (entry: SkillMetricsEntry) => main?.group_index === entry.group_index && main.active_skill_index === entry.active_skill_index;
  const damageOf = (entry: SkillMetricsEntry) => minion && isCalculated(entry) ? minion.minion.dps : entry.metrics?.combined_dps ?? 0;
  const ranked = (skills ?? []).filter(entry => entry.status === 'calculated' && entry.metrics && (damageOf(entry) > 0 || entry.unmodelled_meta_trigger)).sort((a, b) => damageOf(b) - damageOf(a)).slice(0, 8);
  const top = ranked[0] ? damageOf(ranked[0]) : 0;
  const skillContext = (entry: SkillMetricsEntry) => {
    const group = session.socketGroups[entry.group_index];
    const repeated = ranked.some(other => other !== entry && other.skill_id === entry.skill_id);
    const supports = repeated ? group?.gems.filter(gem => gem.enabled !== false && catalog.get(gem.skill_id)?.is_support).slice(0, 2).map(gem => skillName(gem.skill_id)) ?? [] : [];
    const source = group?.source ? grantedSourceLabel(lang, group.source) : '';
    const weapon = group?.weapon_set ? `${tt('weapons.set')} ${group.weapon_set}` : '';
    const groupNumber = tt('overview.skillGroup').replace('{index}', String(entry.group_index + 1));
    const effect = ranked.some(other => other !== entry && other.group_index === entry.group_index && other.skill_id === entry.skill_id)
      ? tt('overview.skillEffect').replace('{index}', String(entry.active_skill_index)) : '';
    return [source, weapon, ...supports, groupNumber, effect].filter(Boolean).join(' · ');
  };
  const chooseSkill = (entry: SkillMetricsEntry) => {
    if (!isCalculated(entry)) session.inspectSkill(entry.group_index, entry.active_skill_index, skillResult?.version ?? session.stateVersion);
  };

  // What each equipped piece or skill group adds, by removing it and recalculating. On demand: it costs a recalculation per source.
  const job = useCancellableJob<{ version: number; selection: string; data: AttributionResponse }>();
  const contribution = job.result;
  const contributionBusy = job.running;
  const [field, setField] = useState<typeof ATTRIBUTION_FIELDS[number]>('CombinedDPS');
  // A summoner build measures its damage by the main minion.
  const statFor = (id: typeof ATTRIBUTION_FIELDS[number]) => id === 'CombinedDPS' ? headlineDpsStat(calc) : id;
  const computeContribution = () => {
    const request = session.currentRequest(); if (!request) return;
    const version = session.stateVersion;
    const fields = ATTRIBUTION_FIELDS.map(statFor);
    void job.start(async ({ signal }) => {
      const data = await (await getBackend()).attribution({ request, fields });
      if (signal.aborted) throw new DOMException('cancelled', 'AbortError');
      return { version, selection, data };
    });
  };
  const groupName = (index: number) => {
    const group = session.socketGroups[index];
    const effect = group && catalog.size ? activeSkillEffects(group, catalog)[(group.main_active_skill ?? 1) - 1] : undefined;
    return [contributionLabel(lang, 'socket_group', String(index)), effect && skillName(effect.skillId)].filter(Boolean).join(' · ');
  };
  const fieldStat = statFor(field);
  const entries = contribution ? contribution.data.entries.map(entry => ({ entry, delta: entry.deltas[fieldStat] ?? 0 })).filter(row => row.delta !== 0).sort((a, b) => Math.abs(b.delta) - Math.abs(a.delta)).slice(0, 10) : [];
  const biggest = Math.max(1, ...entries.map(row => Math.abs(row.delta)));
  const baseline = contribution?.data.baseline[fieldStat] ?? 0;

  const openFinding = (finding: Finding) => {
    if (finding.kind === 'resist' && finding.element !== 'Chaos') {
      setGoal({ ...goal, resistanceFirst: true, resistanceTarget: finding.value + finding.missing });
      navigate('trade');
    } else if (finding.kind === 'net-regen') focusStat(finding.stat);
    else if (finding.kind === 'spirit' && onSpirit) onSpirit();
    else navigate(finding.route);
  };

  if (session.socketGroups.length === 0 && !session.hasBuildContent) {
    return <section className="ov ov-empty" aria-labelledby="ov-title">
      <h1 id="ov-title" tabIndex={-1}>{tt('overview.importTitle')}</h1>
      {importNotice}
      <p>{tt('overview.importHint')}</p>
      <Button autoFocus onClick={onImport ?? (() => navigate('build'))}><Download />{tt('sidebar.importCreate')}</Button>
    </section>;
  }

  const weakest = MAX_HIT.map(type => [type, value(`${type}MaxHit`)] as const).filter((row): row is readonly [typeof MAX_HIT[number], number] => row[1] !== null).sort((a, b) => a[1] - b[1])[0];
  const spirit = value('Spirit'), spiritFree = value('SpiritUnreserved');
  const dash = (format: (n: number) => string) => (n: number | null) => n === null ? '—' : format(n);
  const life = value('Life'), shield = value('EnergyShield');
  const shieldLeads = shield !== null && life !== null && shield > life;
  const resource = shieldLeads ? 'EnergyShield' : 'Life';
  const otherResource = shieldLeads ? 'Life' : 'EnergyShield';
  const buildName = build?.name || tt('overview.untitledBuild');
  const spiritOver = spiritFree !== null && spiritFree < 0;
  const figures = [
    minion
      ? { id: `Minion:${minion.minion.index}:TotalDPS`, label: tt('sidebar.minionHitDps'), value: minion.minion.dps, format: dash(n => formatStatValue(n, 'float2')), sub: minionDpsNote(lang, minion.name, value('ActiveMinionLimit')), tone: 'dps' }
      : { id: 'CombinedDPS', label: tt('sidebar.combinedDps'), value: main?.combined_dps ?? null, format: dash(n => formatStatValue(n, 'float2')), sub: main ? skillName(main.skill_id) : tt('overview.noSkill'), tone: 'dps' },
    { id: 'TotalEHP', label: statRowLabel(lang, 'TotalEHP'), value: value('TotalEHP'), format: dash(int), sub: weakest ? tt('overview.weakestHit').replace('{type}', damageTypeLabel(lang, weakest[0])).replace('{value}', int(weakest[1])) : '', tone: 'ehp' },
    { id: resource, label: statRowLabel(lang, resource), value: value(resource), format: dash(int),
      sub: `${statRowLabel(lang, otherResource)} ${dash(int)(value(otherResource))} · ${statRowLabel(lang, 'Mana')} ${dash(int)(value('Mana'))}`, tone: 'life' },
    { id: 'SpiritUnreserved', label: spiritOver ? tt('overview.spiritOver') : statNameLabel(lang, 'SpiritUnreserved'), value: spiritOver ? -spiritFree : spiritFree, format: dash(int), sub: spirit === null ? '' : tt('overview.spiritLimit').replace('{value}', int(spirit)), tone: spiritOver ? 'bad' : 'plain' },
  ];

  return <section className="ov" data-stale={view.stale || undefined} aria-labelledby="ov-title">
    <header className="ov-head">
      <div><h1 id="ov-title" tabIndex={-1}>{buildName}</h1>
        <div className="ov-context">
          <span>{[classLabel === buildName ? '' : classLabel, session.character && `${tt('build.level')} ${session.character.level}`, stage?.name].filter(Boolean).join(' · ')}</span>
          <Button variant="link" onClick={() => navigate('config')}>{enemyTierLabel(lang, enemy)}</Button>
        </div>
      </div>
      {onShare && <Button variant="outline" onClick={onShare}><Share2 />{tt('overview.shareStage')}</Button>}
    </header>
    {importNotice}

    <dl className="ov-figures" aria-busy={session.busy}>
      {figures.map(figure => <div key={figure.id} className={`ov-figure is-${figure.tone}`}>
        <dt>{figure.label}</dt>
        <dd><button type="button" aria-label={`${figure.label}${colon(lang)}${figure.format(figure.value)} · ${tt('overview.viewCalculation')}`} onClick={() => focusStat(figure.id)}>
          <AnimatedNumber value={figure.value} format={figure.format} /><span className="ov-source">{tt('overview.sources')}</span>
        </button></dd>
        <small>{figure.sub}</small>
        {figure.tone === 'dps' && <EnergyTriggerNote calc={calc} groups={session.socketGroups} lang={lang} />}
        {figure.tone === 'dps' && !minion && <EnergyTriggerNote trigger={calc?.unmodelled_meta_trigger} catalog={catalog} lang={lang} />}
      </div>)}
    </dl>

    <div className="ov-actions" role="group" aria-label={tt('overview.nextActions')}>
      <Button variant="outline" onClick={onPlanTree ?? (() => navigate('tree'))}><Network />{tt('overview.planTree')}</Button>
      <Button variant="outline" onClick={() => main && onPlanSupports ? onPlanSupports(main.group_index) : navigate('skills')}><Gem />{tt('overview.adjustSkills')}</Button>
      <Button variant="outline" onClick={() => navigate('trade')}><Search />{tt('overview.findEquipment')}</Button>
      {build?.baseline ? <Button variant="ghost" onClick={() => { session.startBaselineComparison(); navigate('compare'); }}>{tt('compare.withBaseline')}</Button>
        : <Button variant="ghost" disabled={session.busy} onClick={session.saveComparisonBaseline}><BookmarkPlus />{tt('overview.keepBaseline')}</Button>}
    </div>

    <dl className="ov-resistances" aria-label={tt('overview.baseResistances')}>
      {(['Fire', 'Cold', 'Lightning', 'Chaos'] as const).map(type => <div key={type}>
        <dt>{statNameLabel(lang, `${type}Resist`)}</dt>
        <dd><button type="button" onClick={() => focusStat(`${type}Resist`)}>{value(`${type}Resist`) === null ? '—' : formatStatValue(value(`${type}Resist`), 'percent', lang)}</button></dd>
      </div>)}
    </dl>

    <div className="ov-cols">
      <div className="ov-col">
        <section className="ov-sec" aria-labelledby="ov-findings">
          <h2 id="ov-findings">{tt('overview.findings')}</h2>
          {!calc ? <p className="ov-muted">{session.busy ? tt('build.calculating') : tt('overview.calculationUnavailable')}</p>
            : findings.length === 0 ? <p className="ov-ok"><CircleCheck />{tt('overview.allClear')}</p>
              : <ul className="ov-findings">{findings.map((finding, index) => {
                const text = describe(finding, lang, catalog);
                const Icon = finding.severity === 'bad' ? CircleAlert : AlertTriangle;
                if (finding.kind === 'unsupported') return <li key={finding.kind} className="is-warn ov-finding-modifiers">
                  <Icon aria-hidden="true" />
                  <details className="ov-details">
                    <summary id="ov-unincluded" className="disclosure">{text.title}</summary>
                    <UnincludedModifiers lines={calc.unsupported_modifiers} lang={lang} items={session.items} />
                  </details>
                </li>;
                const action = finding.kind === 'resist' && finding.element !== 'Chaos' ? tt('overview.fixInUpgrades')
                  : finding.kind === 'net-regen' ? tt('overview.viewRecovery')
                  : finding.route === 'items' ? tt('overview.viewEquipment')
                  : finding.route === 'skills' ? tt('overview.viewSkills')
                  : tt('overview.viewConfig');
                return <li key={`${finding.kind}${index}`} className={`is-${finding.severity}`}>
                  <Icon aria-hidden="true" /><div><strong>{text.title}</strong><span>{text.detail}</span></div>
                  <Button variant="ghost" size="sm" onClick={() => openFinding(finding)}>{action}<ArrowRight /></Button>
                </li>;
              })}</ul>}
        </section>

        <section className="ov-sec" aria-labelledby="ov-skills" aria-busy={skillsPending}>
          <h2 id="ov-skills">{tt('overview.skillDamage')}</h2>
          <p className="ov-hint ov-skill-instruction">{tt('overview.selectSkillHint')}</p>
          {ranked.length === 0 ? <p className="ov-muted" role="status">{skillsPending ? tt('build.calculating')
            : session.calcError || (hasSkills && !skills) ? tt('overview.calculationUnavailable')
              : tt('overview.noDamage')}</p>
            : <ol className="ov-rank">{ranked.map(entry => { const current = isCalculated(entry); const damage = damageOf(entry);
              return <li key={`${entry.group_index}:${entry.active_skill_index}`}><button type="button" aria-pressed={current} disabled={skillsPending || !session.socketGroups[entry.group_index]?.enabled} onClick={() => chooseSkill(entry)}>
                <span className="ov-rank-name">{skillName(entry.skill_id)}{current && <span className="ov-current"><CircleCheck aria-hidden="true" />{tt('overview.currentSkill')}</span>}<small>{skillContext(entry)}{current && minion && ` · ${tt('skills.minionDps')}`}</small>
                  <EnergyTriggerNote trigger={entry.unmodelled_meta_trigger} catalog={catalog} lang={lang} /></span>
                <span className="ov-bar"><span style={{ width: top > 0 ? `${(damage / top) * 100}%` : '0%' }} /></span>
                <span className="ov-rank-value">{formatStatValue(damage, 'float2')}</span></button></li>; })}</ol>}
        </section>
      </div>

      <div className="ov-col">
        <section className="ov-sec" aria-labelledby="ov-contribution">
          <div className="ov-sec-head"><h2 id="ov-contribution">{tt('overview.removalImpact')}</h2>
            <div className="ov-toggle segmented" role="group" aria-label={tt('overview.measure')}>{ATTRIBUTION_FIELDS.map(id => <button key={id} type="button" aria-pressed={field === id} onClick={() => setField(id)}>{statNameLabel(lang, statFor(id))}</button>)}</div></div>
          <p className="ov-hint">{tt('calcs.attributionHint')}</p>
          {!contribution || contribution.version !== session.stateVersion
            ? <div className="ov-compute">
              {contribution && <p className="ov-muted">{contribution.selection !== selection ? tt('overview.skillAnalysisChanged') : tt('overview.contributionStale')}</p>}
              <div className="ov-compute-actions"><Button variant="outline" disabled={contributionBusy || !calc} onClick={computeContribution}>{contributionBusy ? tt('overview.measuring') : contribution ? tt('overview.measureAgain') : tt('calcs.runAttribution')}</Button>
                {contributionBusy && <Button variant="ghost" onClick={job.cancel}>{tt('common.cancel')}</Button>}</div>{job.error && <p className="ov-muted" role="alert">{job.error}</p>}</div>
            : entries.length === 0 ? <p className="ov-muted">{tt('overview.contributionEmpty')}</p>
              : <ol className="ov-rank ov-contrib">{entries.map(({ entry, delta }) => <li key={`${entry.kind}:${entry.id}`}><div>
                <span className="ov-rank-name">{entry.kind === 'socket_group' ? groupName(Number(entry.id)) : contributionLabel(lang, entry.kind, entry.id)}</span>
                <span className="ov-bar" data-sign={removalChange(delta) < 0 ? 'neg' : 'pos'}><span style={{ width: `${(Math.abs(delta) / biggest) * 100}%` }} /></span>
                <span className="ov-rank-value">{tt(delta > 0 ? 'calcs.removalLoss' : 'calcs.removalGain').replace('{value}', formatStat(fieldStat, Math.abs(delta), lang))}{baseline > 0 && <em>{tt('format.parentheticalSuffix', { detail: `${formatNumber((Math.abs(delta) / baseline) * 100, 1, 1)}%` })}</em>}</span></div></li>)}</ol>}
        </section>

        <section className="ov-sec" aria-labelledby="ov-trust">
          <h2 id="ov-trust">{tt('overview.coverage')}</h2>
          <dl className="ov-facts">
            <div><dt>{tt('overview.unreadableItemsCount')}</dt><dd className={calc?.item_errors.length ? 'is-bad' : undefined}>{calc ? calc.item_errors.length : '—'}</dd></div>
            <div><dt>{tt('overview.dataVersion')}</dt><dd>{calc?.provenance?.data_version ?? session.dataVersion ?? '—'}</dd></div>
            {calc?.provenance?.engine_version && <div><dt>{tt('overview.engineVersion')}</dt><dd>{calc.provenance.engine_version}</dd></div>}
          </dl>
          <p className="ov-hint">{tt('overview.modeledEffects')}</p>
        </section>
      </div>
    </div>
  </section>;
}
