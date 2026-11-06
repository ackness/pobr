import { useEffect, useRef, useState } from 'react';
import { AlertTriangle, ArrowRight, CircleAlert, CircleCheck, Download } from 'lucide-react';
import { getBackend } from '../api/backend';
import type { AttributionResponse, SkillMetricsEntry } from '../api/types';
import { Button } from '../components/ui/button';
import type { BuildSession } from '../hooks/useBuildSession';
import { useMainMinion } from '../hooks/useMainMinion';
import { useSkillCatalog } from '../hooks/useSkillName';
import { headlineDpsStat, minionDpsNote } from '../lib/mainMinion';
import { classDisplayName } from '../lib/nativeDisplayNames';
import { bindT, damageTypeLabel, slotLabel, statNameLabel, type Lang } from '../lib/i18n';
import { contributionLabel, removalChange } from '../lib/sourceContributions';
import { activeSkillEffects } from '../lib/skillSelection';
import { skillDisplayName } from '../lib/skillNames';
import { formatNumber, formatStatValue, statFormat, statMap } from '../lib/statDisplay';
import { UnincludedModifiers } from '../components/shared/UnincludedModifiers';
import { deriveFindings, type Finding } from './findings';
import { AnimatedNumber } from './motion/AnimatedNumber';
import { useCancellableJob } from './jobs/useCancellableJob';
import { useUpgradeGoal } from '../hooks/useUpgradeGoal';
import type { SettledCalc } from './motion/useSettledCalc';
import type { Route } from './routes';
import { words } from './text';
import { calculatedSkillKey } from './calculatedSkill';

const MAX_HIT = ['Physical', 'Fire', 'Cold', 'Lightning', 'Chaos'] as const;
const ATTRIBUTION_FIELDS = ['CombinedDPS', 'TotalEHP', 'Life'] as const;
const int = (value: number) => formatStatValue(value, 'int');

function describe(finding: Finding, lang: Lang): { title: string; detail: string } {
  const w = (en: string, cn: string, tw?: string) => words(lang, en, cn, tw);
  switch (finding.kind) {
    case 'no-main-skill': return { title: w('No calculated skill selected', '尚未选择计算技能', '尚未選擇計算技能'), detail: w('Choose a damaging skill in the side panel or on Skills.', '在侧栏或技能页选择一个造成伤害的技能。', '在側欄或技能頁選擇一個造成傷害的技能。') };
    case 'item-errors': return { title: w(`${finding.slots.length} item(s) could not be read`, `${finding.slots.length} 件装备无法解析`, `${finding.slots.length} 件裝備無法解析`), detail: finding.slots.map(slot => slotLabel(lang, slot)).join(' · ') };
    case 'spirit': return { title: w(`Spirit is over-committed by ${int(-finding.value)}`, `精魂超额保留 ${int(-finding.value)}`, `精魂超額保留 ${int(-finding.value)}`), detail: w('Reservations exceed your Spirit; the calculation does not switch skills off for you.', '技能保留总量超过精魂上限；计算不会替你关闭技能。', '技能保留總量超過精魂上限；計算不會替你關閉技能。') };
    case 'attribute': { const name = statNameLabel(lang, finding.attribute); return { title: w(`${name} ${int(finding.have)} of ${int(finding.need)} required`, `${name}不足：${int(finding.have)} / 需要 ${int(finding.need)}`, `${name}不足：${int(finding.have)} / 需要 ${int(finding.need)}`), detail: w('Some gear or a gem cannot be used at this attribute level.', '有装备或宝石的属性需求未满足。', '有裝備或寶石的屬性需求未滿足。') }; }
    case 'resist': { const name = damageTypeLabel(lang, finding.element); return finding.element === 'Chaos'
      ? { title: w(`${name} resistance is ${int(finding.value)}%`, `${name}抗性为 ${int(finding.value)}%`, `${name}抗性為 ${int(finding.value)}%`), detail: w('Negative resistance increases damage taken from this type.', '负抗性会增加该类型的承受伤害。', '負抗性會增加該類型的承受傷害。') }
      : { title: w(`${name} resistance ${int(finding.value)}%`, `${name}抗性 ${int(finding.value)}%`, `${name}抗性 ${int(finding.value)}%`), detail: w(`${int(finding.missing)}% short of the cap.`, `距上限还差 ${int(finding.missing)}%。`, `距上限還差 ${int(finding.missing)}%。`) }; }
    case 'hit-chance': return { title: w(`Hit chance ${formatStatValue(finding.value, 'percent')}`, `命中率 ${formatStatValue(finding.value, 'percent')}`, `命中率 ${formatStatValue(finding.value, 'percent')}`), detail: w('Attacks miss; accuracy is costing damage.', '攻击会落空，命中不足在损失伤害。', '攻擊會落空，命中不足在損失傷害。') };
    case 'net-regen': return { title: w(`Net recovery is ${formatStatValue(finding.value, 'float2')}/s`, `净恢复为 ${formatStatValue(finding.value, 'float2')}/秒`, `淨恢復為 ${formatStatValue(finding.value, 'float2')}/秒`), detail: w('Your own costs and degeneration outpace recovery.', '自身消耗与持续损失超过恢复速度。', '自身消耗與持續損失超過恢復速度。') };
    case 'unsupported': return { title: `${bindT(lang)('calcs.unsupported')}${lang === 'en-US' ? ': ' : '：'}${finding.count}`, detail: w('Their effect is missing from every number on this page.', '这些词缀的效果不在本页任何数值中。', '這些詞綴的效果不在本頁任何數值中。') };
  }
}

export function Overview({ session, view, lang, navigate, focusStat, onSpirit }: { session: BuildSession; view: SettledCalc; lang: Lang; navigate: (route: Route) => void; focusStat: (id: string) => void; onSpirit?: () => void }) {
  const w = (en: string, cn: string, tw?: string) => words(lang, en, cn, tw);
  const tt = bindT(lang);
  const modifierDetails = useRef<HTMLDetailsElement>(null);
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
      const request = session.currentRequest();
      if (!request) { setSkillResult({ version, skills: null }); return; }
      getBackend().then(backend => backend.skillMetrics(request)).then(result => { if (!stale) setSkillResult({ version, skills: result.skills }); }).catch(() => { if (!stale) setSkillResult({ version, skills: null }); });
    }, 250);
    return () => { stale = true; clearTimeout(timer); };
    // eslint-disable-next-line react-hooks/exhaustive-deps -- the version number is the change signal.
  }, [session.stateVersion, session.busy, session.calcError, hasSkills]);
  // Each effect's own damage; the calculated summoning skill counts its minion's, which the per-effect metrics don't carry.
  const isCalculated = (entry: SkillMetricsEntry) => main?.group_index === entry.group_index && main.active_skill_index === entry.active_skill_index;
  const damageOf = (entry: SkillMetricsEntry) => minion && isCalculated(entry) ? minion.minion.dps : entry.metrics?.combined_dps ?? 0;
  const ranked = (skills ?? []).filter(entry => entry.status === 'calculated' && entry.metrics && damageOf(entry) > 0).sort((a, b) => damageOf(b) - damageOf(a)).slice(0, 8);
  const top = ranked[0] ? damageOf(ranked[0]) : 0;
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
      setGoal({ ...goal, resistanceFirst: true, resistanceTarget: goal.resistanceTarget ?? 75 });
      navigate('trade');
    } else if (finding.kind === 'net-regen') focusStat('TotalNetRegen');
    else if (finding.kind === 'unsupported') {
      const details = modifierDetails.current;
      if (details) {
        details.open = true;
        details.scrollIntoView({ block: 'center', behavior: 'smooth' });
        details.querySelector('summary')?.focus();
      }
    } else if (finding.kind === 'spirit' && onSpirit) onSpirit();
    else navigate(finding.route);
  };

  if (session.socketGroups.length === 0 && !session.hasBuildContent) {
    return <section className="ov ov-empty" aria-labelledby="ov-title">
      <h1 id="ov-title">{w('Import a build to see what it can do', '导入一套 BD，看看它的真实表现', '匯入一套 BD，看看它的真實表現')}</h1>
      <p>{w('Paste a PoB2 code, open a game planner or China-server file, or start from a class. You will get damage, survivability, and what is holding the build back.', '粘贴 PoB2 代码或分享链接，或打开游戏规划器 / 国服文件，也可以从职业开始。你会看到伤害、生存，以及拖后腿的地方。', '貼上 PoB2 代碼或分享連結，或開啟遊戲規劃器 / 國服檔案，也可以從職業開始。你會看到傷害、生存，以及拖後腿的地方。')}</p>
      <Button autoFocus onClick={() => navigate('build')}><Download />{w('Import or create a build', '导入或创建 BD', '匯入或建立 BD')}</Button>
    </section>;
  }

  const weakest = MAX_HIT.map(type => [type, value(`${type}MaxHit`)] as const).filter((row): row is readonly [typeof MAX_HIT[number], number] => row[1] !== null).sort((a, b) => a[1] - b[1])[0];
  const spirit = value('Spirit'), spiritFree = value('SpiritUnreserved');
  const dash = (format: (n: number) => string) => (n: number | null) => n === null ? '—' : format(n);
  const life = value('Life'), shield = value('EnergyShield');
  const lifeAndShield = life !== null && shield !== null && shield > life
    ? `${tt('overview.lifeAndShield')} ${int(life + shield)} · ` : '';
  const spiritOver = spiritFree !== null && spiritFree < 0;
  const figures = [
    minion
      ? { id: `Minion:${minion.minion.index}:TotalDPS`, label: tt('sidebar.minionHitDps'), value: minion.minion.dps, format: dash(n => formatStatValue(n, 'float2')), sub: minionDpsNote(lang, minion.name, value('ActiveMinionLimit')), tone: 'dps' }
      : { id: 'CombinedDPS', label: tt('sidebar.combinedDps'), value: main?.combined_dps ?? null, format: dash(n => formatStatValue(n, 'float2')), sub: main ? skillName(main.skill_id) : w('No calculated skill', '未选择计算技能', '未選擇計算技能'), tone: 'dps' },
    { id: 'TotalEHP', label: w('Effective HP', '有效生命', '有效生命'), value: value('TotalEHP'), format: dash(int), sub: weakest ? tt('overview.weakestHit').replace('{type}', damageTypeLabel(lang, weakest[0])).replace('{value}', int(weakest[1])) : '', tone: 'ehp' },
    { id: 'Life', label: w('Life', '生命', '生命'), value: value('Life'), format: dash(int), sub: `${lifeAndShield}${w('ES', '护盾', '護盾')} ${int(value('EnergyShield') ?? 0)} · ${w('Mana', '魔力', '魔力')} ${int(value('Mana') ?? 0)}`, tone: 'life' },
    { id: 'SpiritUnreserved', label: spiritOver ? tt('overview.spiritOver') : statNameLabel(lang, 'SpiritUnreserved'), value: spiritOver ? -spiritFree : spiritFree, format: dash(int), sub: spirit === null ? '' : tt('overview.spiritLimit').replace('{value}', int(spirit)), tone: spiritOver ? 'bad' : 'plain' },
  ];

  return <section className="ov" data-stale={view.stale || undefined} aria-labelledby="ov-title">
    <header className="ov-head">
      <div><h1 id="ov-title">{build?.name || w('Untitled build', '未命名 BD', '未命名 BD')}</h1>
        <p>{[classLabel, session.character && `${w('Level', '等级', '等級')} ${session.character.level}`, stage?.name].filter(Boolean).join(' · ')}</p></div>
    </header>

    <dl className="ov-figures" aria-busy={session.busy}>
      {figures.map(figure => <div key={figure.id} className={`ov-figure is-${figure.tone}`}>
        <dt>{figure.label}</dt>
        <dd><button type="button" onClick={() => focusStat(figure.id)}><AnimatedNumber value={figure.value} format={figure.format} /><span className="ov-source">{tt('overview.sources')}</span></button></dd>
        <small>{figure.sub}</small>
      </div>)}
    </dl>

    <dl className="ov-resistances" aria-label={tt('overview.baseResistances')}>
      {(['Fire', 'Cold', 'Lightning', 'Chaos'] as const).map(type => <div key={type}>
        <dt>{statNameLabel(lang, `${type}Resist`)}</dt>
        <dd><button type="button" onClick={() => focusStat(`${type}Resist`)}>{value(`${type}Resist`) === null ? '—' : formatStatValue(value(`${type}Resist`), 'percent', lang)}</button></dd>
      </div>)}
    </dl>

    <div className="ov-cols">
      <div className="ov-col">
        <section className="ov-sec" aria-labelledby="ov-findings">
          <h2 id="ov-findings">{w('Needs attention', '需要关注', '需要關注')}</h2>
          {!calc ? <p className="ov-muted">{session.busy ? w('Calculating…', '计算中…', '計算中…') : w('The calculation is unavailable.', '当前计算不可用。', '目前計算無法使用。')}</p>
            : findings.length === 0 ? <p className="ov-ok"><CircleCheck />{w('Resistances, Spirit, attributes and accuracy all check out under this setup.', '在当前设定下，抗性、精魂、属性与命中均无明显问题。', '在目前設定下，抗性、精魂、屬性與命中均無明顯問題。')}</p>
              : <ul className="ov-findings">{findings.map((finding, index) => { const text = describe(finding, lang); const Icon = finding.severity === 'bad' ? CircleAlert : AlertTriangle;
                return <li key={`${finding.kind}${index}`} className={`is-${finding.severity}`}><Icon aria-hidden="true" /><div><strong>{text.title}</strong><span>{text.detail}</span></div>
                  {(() => {
                  // A resistance shortfall is fixed in Upgrades with the goal already set to "resistances first".
                  const fixResist = finding.kind === 'resist' && finding.element !== 'Chaos';
                  return <Button variant="ghost" size="sm" onClick={() => openFinding(finding)}>
                    {fixResist ? w('Fix in Upgrades', '到提升页补抗性', '到提升頁補抗性') : finding.kind === 'net-regen' ? w('View recovery', '查看恢复', '查看恢復') : finding.kind === 'unsupported' ? w('View modifiers', '查看词缀', '查看詞綴') : finding.route === 'items' ? w('View equipment', '查看装备', '查看裝備') : finding.route === 'skills' ? w('View skills', '查看技能', '查看技能') : w('View settings', '查看配置', '查看設定')}<ArrowRight /></Button>; })()}</li>; })}</ul>}
        </section>

        <section className="ov-sec" aria-labelledby="ov-skills" aria-busy={skillsPending}>
          <h2 id="ov-skills">{w('Skill damage', '技能伤害', '技能傷害')}</h2>
          <p className="ov-hint ov-skill-instruction">{tt('overview.selectSkillHint')}</p>
          {ranked.length === 0 ? <p className="ov-muted" role="status">{skillsPending ? w('Calculating…', '计算中…', '計算中…')
            : session.calcError || (hasSkills && !skills) ? w('The calculation is unavailable.', '当前计算不可用。', '目前計算無法使用。')
              : w('No skill deals damage yet.', '暂时没有造成伤害的技能。', '暫時沒有造成傷害的技能。')}</p>
            : <ol className="ov-rank">{ranked.map(entry => { const current = isCalculated(entry); const damage = damageOf(entry);
              return <li key={`${entry.group_index}:${entry.active_skill_index}`}><button type="button" aria-pressed={current} disabled={skillsPending || !session.socketGroups[entry.group_index]?.enabled} onClick={() => chooseSkill(entry)}>
                <span className="ov-rank-name">{skillName(entry.skill_id)}{current && <span className="ov-current"><CircleCheck aria-hidden="true" />{tt('overview.currentSkill')}</span>}<small>{w(`Group ${entry.group_index + 1} · Effect ${entry.active_skill_index}`, `技能组 ${entry.group_index + 1} · 效果 ${entry.active_skill_index}`, `技能組 ${entry.group_index + 1} · 效果 ${entry.active_skill_index}`)}{current && minion && ` · ${tt('skills.minionDps')}`}</small></span>
                <span className="ov-bar"><span style={{ width: `${(damage / top) * 100}%` }} /></span>
                <span className="ov-rank-value">{formatStatValue(damage, 'float2')}</span></button></li>; })}</ol>}
        </section>
      </div>

      <div className="ov-col">
        <section className="ov-sec" aria-labelledby="ov-contribution">
          <div className="ov-sec-head"><h2 id="ov-contribution">{tt('calcs.attribution')}</h2>
            <div className="ov-toggle" role="group" aria-label={w('Measure', '指标', '指標')}>{ATTRIBUTION_FIELDS.map(id => <button key={id} type="button" aria-pressed={field === id} onClick={() => setField(id)}>{statNameLabel(lang, statFor(id))}</button>)}</div></div>
          <p className="ov-hint">{tt('calcs.attributionHint')}</p>
          {!contribution || contribution.version !== session.stateVersion
            ? <div className="ov-compute"><p className="ov-muted">{contribution ? contribution.selection !== selection ? tt('overview.skillAnalysisChanged') : w('The build changed since this was measured.', 'BD 已变化，结果已过期。', 'BD 已變化，結果已過期。') : w('See how removing each item or skill group changes the result.', '查看移除每件装备或技能组后，数值会如何变化。', '查看移除每件裝備或技能組後，數值會如何變化。')}</p>
              <div className="ov-compute-actions"><Button variant="outline" disabled={contributionBusy || !calc} onClick={computeContribution}>{contributionBusy ? w('Measuring…', '计算中…', '計算中…') : contribution ? w('Measure again', '重新计算', '重新計算') : tt('calcs.runAttribution')}</Button>
                {contributionBusy && <Button variant="ghost" onClick={job.cancel}>{w('Cancel', '取消', '取消')}</Button>}</div>{job.error && <p className="ov-muted" role="alert">{job.error}</p>}</div>
            : entries.length === 0 ? <p className="ov-muted">{w('Nothing changes this number.', '没有任何来源影响该数值。', '沒有任何來源影響該數值。')}</p>
              : <ol className="ov-rank ov-contrib">{entries.map(({ entry, delta }) => <li key={`${entry.kind}:${entry.id}`}><div>
                <span className="ov-rank-name">{entry.kind === 'socket_group' ? groupName(Number(entry.id)) : contributionLabel(lang, entry.kind, entry.id)}</span>
                <span className="ov-bar" data-sign={removalChange(delta) < 0 ? 'neg' : 'pos'}><span style={{ width: `${(Math.abs(delta) / biggest) * 100}%` }} /></span>
                <span className="ov-rank-value">{tt(delta > 0 ? 'calcs.removalLoss' : 'calcs.removalGain').replace('{value}', formatStatValue(Math.abs(delta), statFormat(fieldStat), lang))}{baseline > 0 && <em>{lang === 'en-US' ? ' (' : '（'}{formatNumber((Math.abs(delta) / baseline) * 100, 1, 1)}%{lang === 'en-US' ? ')' : '）'}</em>}</span></div></li>)}</ol>}
        </section>

        <section className="ov-sec" aria-labelledby="ov-trust">
          <h2 id="ov-trust">{w('Calculation coverage', '计算覆盖', '計算涵蓋範圍')}</h2>
          <dl className="ov-facts">
            <div><dt>{tt('calcs.unsupported')}</dt><dd className={calc?.unsupported_modifiers.length ? 'is-warn' : undefined}>{calc ? calc.unsupported_modifiers.length : '—'}</dd></div>
            <div><dt>{w('Items that failed to parse', '无法解析的装备', '無法解析的裝備')}</dt><dd className={calc?.item_errors.length ? 'is-bad' : undefined}>{calc ? calc.item_errors.length : '—'}</dd></div>
            <div><dt>{w('Game data', '游戏数据', '遊戲資料')}</dt><dd>{calc?.provenance?.data_version ?? session.dataVersion ?? '—'}</dd></div>
            {calc?.provenance?.engine_version && <div><dt>{w('Engine', '计算引擎', '計算引擎')}</dt><dd>{calc.provenance.engine_version}</dd></div>}
          </dl>
          {calc && calc.unsupported_modifiers.length > 0 && <details ref={modifierDetails} className="ov-details"><summary>{w('Show the modifiers', '查看这些词缀', '查看這些詞綴')}</summary>
            <UnincludedModifiers lines={calc.unsupported_modifiers} lang={lang} items={session.items} /></details>}
          <p className="ov-hint">{w('Results reflect modeled effects. Review the uncalculated effects above.', '数值仅反映已支持的效果。未计算的内容见上方明细。', '數值僅反映已支援的效果。未計算的內容見上方明細。')}</p>
        </section>
      </div>
    </div>
  </section>;
}
