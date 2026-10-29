import { useEffect, useState } from 'react';
import { AlertTriangle, ArrowRight, CircleAlert, CircleCheck, Download } from 'lucide-react';
import { getBackend } from '../api/backend';
import type { AttributionResponse, SkillMetricsEntry } from '../api/types';
import { Button } from '../components/ui/button';
import type { BuildSession } from '../hooks/useBuildSession';
import { useSkillCatalog } from '../hooks/useSkillName';
import { chineseScript } from '../lib/chinese';
import { damageTypeLabel, slotLabel, statNameLabel, type Lang } from '../lib/i18n';
import { activeSkillEffects } from '../lib/skillSelection';
import { skillDisplayName } from '../lib/skillNames';
import { formatStatValue, statMap } from '../lib/statDisplay';
import { deriveFindings, type Finding } from './findings';
import { AnimatedNumber } from './motion/AnimatedNumber';
import { useCancellableJob } from './jobs/useCancellableJob';
import { useUpgradeGoal } from '../hooks/useUpgradeGoal';
import type { SettledCalc } from './motion/useSettledCalc';
import type { Route } from './routes';
import { words } from './text';

const MAX_HIT = ['Physical', 'Fire', 'Cold', 'Lightning', 'Chaos'] as const;
const ATTRIBUTION_FIELDS = [['CombinedDPS', 'DPS'], ['TotalEHP', 'EHP'], ['Life', 'Life']] as const;
const int = (value: number) => formatStatValue(value, 'int');

function describe(finding: Finding, lang: Lang): { title: string; detail: string } {
  const w = (en: string, cn: string, tw?: string) => words(lang, en, cn, tw);
  switch (finding.kind) {
    case 'no-main-skill': return { title: w('No main skill is being calculated', '没有可计算的主技能', '沒有可計算的主技能'), detail: w('Choose a damaging skill in the side panel or on Skills.', '在侧栏或技能页选择一个造成伤害的技能。', '在側欄或技能頁選擇一個造成傷害的技能。') };
    case 'item-errors': return { title: w(`${finding.slots.length} item(s) could not be read`, `${finding.slots.length} 件装备无法解析`, `${finding.slots.length} 件裝備無法解析`), detail: finding.slots.map(slot => slotLabel(lang, slot)).join(' · ') };
    case 'spirit': return { title: w(`Spirit is over-committed by ${int(-finding.value)}`, `精魂超额保留 ${int(-finding.value)}`, `精魂超額保留 ${int(-finding.value)}`), detail: w('Reservations exceed your Spirit; the calculation does not switch skills off for you.', '技能保留总量超过精魂上限；计算不会替你关闭技能。', '技能保留總量超過精魂上限；計算不會替你關閉技能。') };
    case 'attribute': { const name = statNameLabel(lang, finding.attribute); return { title: w(`${name} ${int(finding.have)} of ${int(finding.need)} required`, `${name}不足：${int(finding.have)} / 需要 ${int(finding.need)}`, `${name}不足：${int(finding.have)} / 需要 ${int(finding.need)}`), detail: w('Some gear or a gem cannot be used at this attribute level.', '有装备或宝石的属性需求未满足。', '有裝備或寶石的屬性需求未滿足。') }; }
    case 'resist': { const name = damageTypeLabel(lang, finding.element); return finding.element === 'Chaos'
      ? { title: w(`${name} resistance is ${int(finding.value)}%`, `${name}抗性为 ${int(finding.value)}%`, `${name}抗性為 ${int(finding.value)}%`), detail: w('Negative resistance increases damage taken from this type.', '负抗性会增加该类型的承受伤害。', '負抗性會增加該類型的承受傷害。') }
      : { title: w(`${name} resistance ${int(finding.value)}%`, `${name}抗性 ${int(finding.value)}%`, `${name}抗性 ${int(finding.value)}%`), detail: w(`${int(finding.missing)}% short of the cap.`, `距上限还差 ${int(finding.missing)}%。`, `距上限還差 ${int(finding.missing)}%。`) }; }
    case 'hit-chance': return { title: w(`Hit chance ${formatStatValue(finding.value, 'percent')}`, `命中率 ${formatStatValue(finding.value, 'percent')}`, `命中率 ${formatStatValue(finding.value, 'percent')}`), detail: w('Attacks miss; accuracy is costing damage.', '攻击会落空，命中不足在损失伤害。', '攻擊會落空，命中不足在損失傷害。') };
    case 'net-regen': return { title: w(`Net recovery is ${formatStatValue(finding.value, 'float2')}/s`, `净恢复为 ${formatStatValue(finding.value, 'float2')}/秒`, `淨恢復為 ${formatStatValue(finding.value, 'float2')}/秒`), detail: w('Your own costs and degeneration outpace recovery.', '自身消耗与持续损失超过恢复速度。', '自身消耗與持續損失超過恢復速度。') };
    case 'unsupported': return { title: w(`${finding.count} modifier line(s) are not calculated`, `${finding.count} 条词条未参与计算`, `${finding.count} 條詞綴未參與計算`), detail: w('Their effect is missing from every number on this page.', '这些词条的效果不在本页任何数值中。', '這些詞綴的效果不在本頁任何數值中。') };
  }
}

export function Overview({ session, view, lang, navigate, focusStat }: { session: BuildSession; view: SettledCalc; lang: Lang; navigate: (route: Route) => void; focusStat: (id: string) => void }) {
  const w = (en: string, cn: string, tw?: string) => words(lang, en, cn, tw);
  const catalog = useSkillCatalog();
  const { goal, setGoal } = useUpgradeGoal();
  const build = session.workspace?.builds.find(entry => entry.id === session.workspace?.activeBuild);
  const stage = build?.stages.find(entry => entry.id === build.activeStage);
  const calc = view.calc;
  const stats = calc ? statMap(calc.stats) : new Map<string, number | null>();
  const value = (id: string) => { const item = stats.get(id); return typeof item === 'number' ? item : null; };
  const skillName = (id: string) => skillDisplayName(id, lang, catalog);
  const main = calc?.main_skill ?? null;
  const findings = deriveFindings(calc, session.socketGroups.length > 0);
  const className = session.character?.ascendancy_name || session.character?.class_name || '';
  const classLabel = lang === 'en-US' ? className : chineseScript((session.classNames.ascendancies[className] ?? session.classNames.classes[className]) ?? className, lang);

  // Every active effect, calculated on its own: which skill actually carries the build.
  const [skills, setSkills] = useState<SkillMetricsEntry[] | null>(null);
  const hasSkills = session.socketGroups.length > 0;
  useEffect(() => { if (!hasSkills) setSkills(null); }, [hasSkills]);
  useEffect(() => {
    if (session.busy || session.calcError || !hasSkills) return;
    let stale = false;
    // The previous ranking stays up while a newer one is computed, so the list does not blink on each edit.
    const timer = setTimeout(() => {
      const request = session.currentRequest(); if (!request) return;
      getBackend().then(backend => backend.skillMetrics(request)).then(result => { if (!stale) setSkills(result.skills); }).catch(() => { if (!stale) setSkills(null); });
    }, 250);
    return () => { stale = true; clearTimeout(timer); };
    // eslint-disable-next-line react-hooks/exhaustive-deps -- the version number is the change signal.
  }, [session.stateVersion, session.busy, session.calcError, hasSkills]);
  const ranked = (skills ?? []).filter(entry => entry.status === 'calculated' && entry.metrics && entry.metrics.combined_dps > 0).sort((a, b) => b.metrics!.combined_dps - a.metrics!.combined_dps).slice(0, 8);
  const top = ranked[0]?.metrics?.combined_dps ?? 0;
  const chooseSkill = (entry: SkillMetricsEntry) => {
    const group = session.socketGroups[entry.group_index];
    if (group && (group.main_active_skill ?? 1) !== entry.active_skill_index)
      session.setSocketGroups(session.socketGroups.map((item, index) => index === entry.group_index ? { ...item, main_active_skill: entry.active_skill_index } : item));
    session.updateParams({ main_socket_group: entry.group_index });
  };

  // What each equipped piece or skill group adds, by removing it and recalculating. On demand: it costs a recalculation per source.
  const job = useCancellableJob<{ version: number; data: AttributionResponse }>();
  const contribution = job.result;
  const contributionBusy = job.running;
  const [field, setField] = useState<typeof ATTRIBUTION_FIELDS[number][0]>('CombinedDPS');
  const computeContribution = () => {
    const request = session.currentRequest(); if (!request) return;
    const version = session.stateVersion;
    void job.start(async ({ signal }) => {
      const data = await (await getBackend()).attribution({ request, fields: ATTRIBUTION_FIELDS.map(([id]) => id) });
      if (signal.aborted) throw new DOMException('cancelled', 'AbortError');
      return { version, data };
    });
  };
  const groupName = (index: number) => {
    const group = session.socketGroups[index];
    const effect = group && catalog.size ? activeSkillEffects(group, catalog)[(group.main_active_skill ?? 1) - 1] : undefined;
    return `#${index + 1} ${effect ? skillName(effect.skillId) : ''}`.trim();
  };
  const entries = contribution ? contribution.data.entries.map(entry => ({ entry, delta: entry.deltas[field] ?? 0 })).filter(row => row.delta !== 0).sort((a, b) => Math.abs(b.delta) - Math.abs(a.delta)).slice(0, 10) : [];
  const biggest = Math.max(1, ...entries.map(row => Math.abs(row.delta)));
  const baseline = contribution?.data.baseline[field] ?? 0;

  if (session.socketGroups.length === 0 && !session.hasBuildContent) {
    return <section className="ov ov-empty" aria-labelledby="ov-title">
      <h1 id="ov-title">{w('Import a build to see what it can do', '导入一套 BD，看看它的真实表现', '匯入一套 BD，看看它的真實表現')}</h1>
      <p>{w('Paste a PoB2 code, open a game planner or China-server file, or start from a class. You will get damage, survivability, and what is holding the build back.', '粘贴 PoB2 代码或分享链接，或打开游戏规划器 / 国服文件，也可以从职业开始。你会看到伤害、生存，以及拖后腿的地方。', '貼上 PoB2 代碼或分享連結，或開啟遊戲規劃器 / 國服檔案，也可以從職業開始。你會看到傷害、生存，以及拖後腿的地方。')}</p>
      <Button onClick={() => navigate('build')}><Download />{w('Import or create a build', '导入或创建 BD', '匯入或建立 BD')}</Button>
    </section>;
  }

  const weakest = MAX_HIT.map(type => [type, value(`${type}MaxHit`)] as const).filter((row): row is readonly [typeof MAX_HIT[number], number] => row[1] !== null).sort((a, b) => a[1] - b[1])[0];
  const spirit = value('Spirit'), spiritFree = value('SpiritUnreserved');
  const dash = (format: (n: number) => string) => (n: number | null) => n === null ? '—' : format(n);
  const figures = [
    { id: 'CombinedDPS', label: w('Damage per second', '每秒伤害', '每秒傷害'), value: main?.combined_dps ?? null, format: dash(n => formatStatValue(n, 'float2')), sub: main ? skillName(main.skill_id) : w('No main skill', '未选择主技能', '未選擇主技能'), tone: 'dps' },
    { id: 'TotalEHP', label: w('Effective HP', '有效生命', '有效生命'), value: value('TotalEHP'), format: dash(int), sub: weakest ? w(`Weakest to ${damageTypeLabel(lang, weakest[0])}: ${int(weakest[1])}`, `最弱：${damageTypeLabel(lang, weakest[0])} 单次 ${int(weakest[1])}`, `最弱：${damageTypeLabel(lang, weakest[0])} 單次 ${int(weakest[1])}`) : '', tone: 'ehp' },
    { id: 'Life', label: w('Life', '生命', '生命'), value: value('Life'), format: dash(int), sub: `${w('ES', '护盾', '護盾')} ${int(value('EnergyShield') ?? 0)} · ${w('Mana', '魔力', '魔力')} ${int(value('Mana') ?? 0)}`, tone: 'life' },
    { id: 'SpiritUnreserved', label: w('Spirit left', '剩余精魂', '剩餘精魂'), value: spiritFree, format: dash(int), sub: spirit === null ? '' : w(`of ${int(spirit)}`, `共 ${int(spirit)}`, `共 ${int(spirit)}`), tone: spiritFree !== null && spiritFree < 0 ? 'bad' : 'plain' },
  ];

  return <section className="ov" data-stale={view.stale || undefined} aria-labelledby="ov-title">
    <header className="ov-head">
      <div><h1 id="ov-title">{build?.name || w('Untitled build', '未命名 BD', '未命名 BD')}</h1>
        <p>{[classLabel, session.character && `${w('Level', '等级', '等級')} ${session.character.level}`, stage?.name].filter(Boolean).join(' · ')}</p></div>
    </header>

    <dl className="ov-figures" aria-busy={session.busy}>
      {figures.map(figure => <div key={figure.id} className={`ov-figure is-${figure.tone}`}>
        <dt>{figure.label}</dt>
        <dd><button type="button" onClick={() => focusStat(figure.id)}><AnimatedNumber value={figure.value} format={figure.format} /></button></dd>
        <small>{figure.sub}</small>
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
                  return <Button variant="ghost" size="sm" onClick={() => { if (fixResist) { setGoal({ ...goal, resistanceFirst: true, resistanceTarget: goal.resistanceTarget ?? 75 }); navigate('trade'); } else navigate(finding.route); }}>
                    {fixResist ? w('Fix in Upgrades', '到提升页补抗性', '到提升頁補抗性') : finding.kind === 'net-regen' ? w('View recovery', '查看恢复', '查看恢復') : finding.kind === 'unsupported' ? w('View modifiers', '查看词条', '查看詞綴') : finding.route === 'items' ? w('View equipment', '查看装备', '查看裝備') : finding.route === 'skills' ? w('View skills', '查看技能', '查看技能') : w('View settings', '查看配置', '查看設定')}<ArrowRight /></Button>; })()}</li>; })}</ul>}
        </section>

        <section className="ov-sec" aria-labelledby="ov-skills">
          <h2 id="ov-skills">{w('Skill damage', '技能伤害', '技能傷害')}</h2>
          {ranked.length === 0 ? <p className="ov-muted">{w('No skill deals damage yet.', '暂时没有造成伤害的技能。', '暫時沒有造成傷害的技能。')}</p>
            : <ol className="ov-rank">{ranked.map(entry => { const metrics = entry.metrics!; const current = main?.group_index === entry.group_index && main.active_skill_index === entry.active_skill_index;
              return <li key={`${entry.group_index}:${entry.active_skill_index}`}><button type="button" aria-pressed={current} onClick={() => chooseSkill(entry)}>
                <span className="ov-rank-name">{skillName(entry.skill_id)}<small>{w(`Group ${entry.group_index + 1} · Effect ${entry.active_skill_index}`, `技能组 ${entry.group_index + 1} · 效果 ${entry.active_skill_index}`, `技能組 ${entry.group_index + 1} · 效果 ${entry.active_skill_index}`)}</small></span>
                <span className="ov-bar"><span style={{ width: `${(metrics.combined_dps / top) * 100}%` }} /></span>
                <span className="ov-rank-value">{formatStatValue(metrics.combined_dps, 'float2')}</span></button></li>; })}</ol>}
          <p className="ov-hint">{w('Each effect is calculated separately. Select one to use it for the current calculation.', '各效果分别计算，选择后用于当前计算。', '各效果分別計算，選取後用於目前計算。')}</p>
        </section>
      </div>

      <div className="ov-col">
        <section className="ov-sec" aria-labelledby="ov-contribution">
          <div className="ov-sec-head"><h2 id="ov-contribution">{w('What each piece contributes', '每件装备与技能的贡献', '每件裝備與技能的貢獻')}</h2>
            <div className="ov-toggle" role="group" aria-label={w('Measure', '指标', '指標')}>{ATTRIBUTION_FIELDS.map(([id, label]) => <button key={id} type="button" aria-pressed={field === id} onClick={() => setField(id)}>{label === 'DPS' ? w('DPS', '伤害', '傷害') : label === 'EHP' ? w('EHP', '有效生命', '有效生命') : w('Life', '生命', '生命')}</button>)}</div></div>
          {!contribution || contribution.version !== session.stateVersion
            ? <div className="ov-compute"><p className="ov-muted">{contribution ? w('The build changed since this was measured.', 'BD 已变化，结果已过期。', 'BD 已變化，結果已過期。') : w('See how removing each item or skill group changes the result.', '查看移除每件装备或技能组后，数值会如何变化。', '查看移除每件裝備或技能組後，數值會如何變化。')}</p>
              <div className="ov-compute-actions"><Button variant="outline" disabled={contributionBusy || !calc} onClick={computeContribution}>{contributionBusy ? w('Measuring…', '计算中…', '計算中…') : contribution ? w('Measure again', '重新计算', '重新計算') : w('Analyze contributions', '分析贡献', '分析貢獻')}</Button>
                {contributionBusy && <Button variant="ghost" onClick={job.cancel}>{w('Cancel', '取消', '取消')}</Button>}</div>{job.error && <p className="ov-muted" role="alert">{job.error}</p>}</div>
            : entries.length === 0 ? <p className="ov-muted">{w('Nothing changes this number.', '没有任何来源影响该数值。', '沒有任何來源影響該數值。')}</p>
              : <ol className="ov-rank ov-contrib">{entries.map(({ entry, delta }) => <li key={`${entry.kind}:${entry.id}`}><div>
                <span className="ov-rank-name">{entry.kind === 'socket_group' ? groupName(Number(entry.id)) : slotLabel(lang, entry.id)}</span>
                <span className="ov-bar" data-sign={delta < 0 ? 'neg' : 'pos'}><span style={{ width: `${(Math.abs(delta) / biggest) * 100}%` }} /></span>
                <span className="ov-rank-value">{delta > 0 ? '+' : '−'}{formatStatValue(Math.abs(delta), 'float2')}{baseline > 0 && <em> {((Math.abs(delta) / baseline) * 100).toFixed(1)}%</em>}</span></div></li>)}</ol>}
        </section>

        <section className="ov-sec" aria-labelledby="ov-trust">
          <h2 id="ov-trust">{w('Calculation coverage', '计算覆盖', '計算涵蓋範圍')}</h2>
          <dl className="ov-facts">
            <div><dt>{w('Lines not calculated', '未参与计算的词条', '未參與計算的詞綴')}</dt><dd className={calc?.unsupported_modifiers.length ? 'is-warn' : undefined}>{calc ? calc.unsupported_modifiers.length : '—'}</dd></div>
            <div><dt>{w('Items that failed to parse', '无法解析的装备', '無法解析的裝備')}</dt><dd className={calc?.item_errors.length ? 'is-bad' : undefined}>{calc ? calc.item_errors.length : '—'}</dd></div>
            <div><dt>{w('Game data', '游戏数据', '遊戲資料')}</dt><dd>{calc?.provenance?.data_version ?? session.dataVersion ?? '—'}</dd></div>
            {calc?.provenance?.engine_version && <div><dt>{w('Engine', '计算引擎', '計算引擎')}</dt><dd>{calc.provenance.engine_version}</dd></div>}
          </dl>
          {calc && calc.unsupported_modifiers.length > 0 && <details className="ov-details"><summary>{w('Show the lines', '查看这些词条', '查看這些詞綴')}</summary>
            <ul>{calc.unsupported_modifiers.map((line, index) => <li key={index}>{line}</li>)}</ul></details>}
          <p className="ov-hint">{w('Results reflect modeled effects. Review the uncalculated effects above.', '数值仅反映已支持的效果。未计算的内容见上方明细。', '數值僅反映已支援的效果。未計算的內容見上方明細。')}</p>
        </section>
      </div>
    </div>
  </section>;
}
