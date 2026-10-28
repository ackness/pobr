import { useEffect, useMemo, useState, type ReactNode } from 'react';
import { ChevronRight, CircleAlert, CircleCheck, Pin } from 'lucide-react';
import type { EnemyTier, MainSkillInfo } from '../api/types';
import type { BuildSession } from '../hooks/useBuildSession';
import { useSkillCatalog } from '../hooks/useSkillName';
import { activeSkillEffects } from '../lib/skillSelection';
import { skillDisplayName } from '../lib/skillNames';
import { STAT_SECTIONS, formatStatValue, statMap } from '../lib/statDisplay';
import { detailKind } from '../lib/statDetails';
import { bindT, damageTypeLabel, enemyTierLabel, statNameLabel, type Lang } from '../lib/i18n';
import { AppSelect } from '../components/shared/AppSelect';
import { deriveFindings } from './findings';
import { AnimatedNumber } from './motion/AnimatedNumber';
import { resolveEnemyTier } from './enemy';
import { statColor, statFormat, statLabel } from './inspector/statId';
import { computeStatChanges, type StatChange } from './motion/statChanges';
import type { SettledCalc } from './motion/useSettledCalc';
import type { Route } from './routes';
import { words } from './text';

const COLLAPSED_KEY = 'pobr-v2-rail-collapsed';
const TIERS: EnemyTier[] = ['none', 'boss', 'pinnacle', 'uber'];
const DAMAGE_COLOR: Record<string, string> = {
  Physical: 'var(--color-physical)', Fire: 'var(--color-fire)', Cold: 'var(--color-cold)',
  Lightning: 'var(--color-lightning)', Chaos: 'var(--color-chaos)',
};
/** Sections people open on purpose; the rest of the rail is what an edit usually moves. */
const DEFAULT_COLLAPSED = ['Recovery', 'Skill mechanics', 'Attributes'];
const CHIP_MS = 4000;

function readCollapsed(): string[] {
  try {
    const raw = localStorage.getItem(COLLAPSED_KEY);
    if (raw === null) return DEFAULT_COLLAPSED;
    const value = JSON.parse(raw); return Array.isArray(value) ? value.filter(item => typeof item === 'string') : DEFAULT_COLLAPSED;
  } catch { return DEFAULT_COLLAPSED; }
}

/** A value that is only meaningful for a moment after an edit, such as the "+5.8%" next to the headline. */
function useRecent<T>(value: T | undefined, ms: number): T | undefined {
  const [shown, setShown] = useState<T | undefined>(undefined);
  useEffect(() => {
    setShown(value);
    if (value === undefined) return;
    const timer = setTimeout(() => setShown(undefined), ms);
    return () => clearTimeout(timer);
  }, [value, ms]);
  return shown;
}

function percentText(change: StatChange) {
  if (change.percent === null) return `${change.delta > 0 ? '+' : '−'}${formatStatValue(Math.abs(change.delta), 'float2')}`;
  return `${change.percent > 0 ? '+' : '−'}${Math.abs(change.percent).toFixed(1)}%`;
}

interface RowProps { label: string; value: ReactNode; color?: string; change?: StatChange; onOpen?: () => void; pin?: { on: boolean; toggle: () => void; label: string } }
/** One result. If an edit just moved it, a tinted wash says so; if there is a calculation behind it, it opens; a pin keeps it at the top. */
function Row({ label, value, color, change, onOpen, pin }: RowProps) {
  const content = <>
    {change && <span key={`${change.before}>${change.after}`} className="rail-flash" data-tone={change.tone} aria-hidden="true" />}
    <span className="rail-label">{label}</span><span className="rail-value" style={color ? { color } : undefined}>{value}</span></>;
  const row = onOpen ? <button type="button" className="rail-row is-link" onClick={onOpen}>{content}</button> : <div className="rail-row">{content}</div>;
  // The pin is for the pointer; the keyboard pins from the inspector, so it adds no tab stops to a long list.
  return pin ? <div className="rail-item">{row}<button type="button" className="rail-pin" tabIndex={-1} aria-pressed={pin.on} aria-label={pin.label} onClick={pin.toggle}><Pin /></button></div> : row;
}

interface Shared { session: BuildSession; view: SettledCalc; lang: Lang; onStatClick: (id: string) => void }

function Headline({ session, view, lang, onStatClick, changes, onNavigate }: Shared & { changes: Map<string, StatChange>; onNavigate: (route: Route) => void }) {
  const tt = bindT(lang);
  const w = (en: string, cn: string, tw?: string) => words(lang, en, cn, tw);
  const catalog = useSkillCatalog();
  const calc = view.calc;
  const groups = session.socketGroups;
  const selected = session.calcParams.main_socket_group;
  const mainSkill: MainSkillInfo | null = calc && selected !== undefined ? calc.main_skill : null;
  const findings = useMemo(() => deriveFindings(calc, groups.length > 0), [calc, groups.length]);
  const chip = useRecent(changes.get('CombinedDPS'), CHIP_MS);
  const enemy = resolveEnemyTier(session.calcParams.enemy_tier, session.calcParams.config_inputs);
  const name = (skillId: string) => skillDisplayName(skillId, lang, catalog);
  const open = (id: string) => calc && detailKind(calc, id) !== 'none' ? () => onStatClick(id) : undefined;
  const total = mainSkill?.hit_damage.reduce((sum, part) => sum + part.avg, 0) ?? 0;
  const parts = mainSkill ? [...mainSkill.hit_damage].filter(part => part.avg > 0).sort((a, b) => b.avg - a.avg) : [];
  const toOverview = () => { onNavigate('overview'); requestAnimationFrame(() => requestAnimationFrame(() => document.getElementById('ov-findings')?.scrollIntoView({ block: 'start', behavior: 'smooth' }))); };
  const numberOf = (value: number) => (shown: number | null) => formatStatValue(shown ?? value, 'float2');
  return <div className="rail-headline">
    {groups.length > 0 && <AppSelect ariaLabel={tt('sidebar.mainSkill')} value={selected === undefined ? '' : String(selected)} placeholder={tt('sidebar.unselected')}
      disabled={session.busy} onChange={value => session.updateParams({ main_socket_group: Number(value) })}
      options={groups.map((group, index) => {
        const effect = catalog.size ? activeSkillEffects(group, catalog)[(group.main_active_skill ?? 1) - 1] : undefined;
        return { value: String(index), label: `${index + 1}. ${effect ? name(effect.skillId) : tt('calcs.unavailable')}${group.enabled ? '' : ` ${tt('sidebar.disabledGroup')}`}` };
      })} />}
    <div className="rail-scenario"><span>{w('Against', '对象', '對象')}</span>
      <AppSelect ariaLabel={w('Enemy tier', '敌人档位', '敵人檔位')} value={enemy === 'custom' ? '' : enemy} placeholder={w('Custom', '自定义', '自訂')} disabled={session.busy}
        onChange={tier => session.updateParams({ enemy_tier: tier as EnemyTier })} options={TIERS.map(tier => ({ value: tier, label: enemyTierLabel(lang, tier) }))} /></div>
    {calc && groups.length > 0 && <button type="button" className="rail-health" data-state={findings.length === 0 ? 'ok' : findings.some(item => item.severity === 'bad') ? 'bad' : 'warn'} onClick={toOverview}>
      {findings.length === 0 ? <CircleCheck /> : <CircleAlert />}
      <span>{findings.length === 0 ? w('No obvious problems', '没有明显问题', '沒有明顯問題') : w(`${findings.length} to look at`, `${findings.length} 项需要关注`, `${findings.length} 項需要關注`)}</span>
      <ChevronRight /></button>}
    {groups.length === 0 && <p className="rail-empty">{w('Add a skill to see damage.', '添加技能后即可查看伤害。', '新增技能後即可查看傷害。')}</p>}
    {groups.length > 0 && !mainSkill && <p className="rail-empty">{view.stale && !calc ? w('Calculating…', '计算中…', '計算中…') : tt('sidebar.noMainSkill')}</p>}
    {mainSkill && mainSkill.group_index !== selected && <p className="rail-empty">{tt('sidebar.computedSkill')}: {name(mainSkill.skill_id)}</p>}
    {mainSkill && <>
      <div className="rail-hero">
        <span className="rail-hero-label">{tt('sidebar.combinedDps')}</span>
        <div className="rail-hero-line">
          {open('CombinedDPS')
            ? <button type="button" className="rail-hero-value is-link" onClick={open('CombinedDPS')}><AnimatedNumber value={mainSkill.combined_dps} format={numberOf(mainSkill.combined_dps)} /></button>
            : <strong className="rail-hero-value"><AnimatedNumber value={mainSkill.combined_dps} format={numberOf(mainSkill.combined_dps)} /></strong>}
          {chip && <span className="rail-chip" data-tone={chip.tone} role="status">{percentText(chip)}</span>}
        </div>
      </div>
      <Row label={tt('sidebar.hitDps')} change={changes.get('TotalDPS')} value={<AnimatedNumber value={mainSkill.hit_dps} format={numberOf(mainSkill.hit_dps)} />} onOpen={open('TotalDPS')} />
      {mainSkill.dot_dps > 0 && <Row label={tt('sidebar.dotDps')} change={changes.get('TotalDotDPS')} value={<AnimatedNumber value={mainSkill.dot_dps} format={numberOf(mainSkill.dot_dps)} />} onOpen={open('TotalDotDPS')} />}
      {total > 0 && <div className="rail-share" role="list" aria-label={tt('sidebar.damageShare')}>
        <div className="rail-share-bar" aria-hidden="true">{parts.map(part => <span key={part.damage_type} style={{ flexGrow: part.avg, background: DAMAGE_COLOR[part.damage_type] ?? 'var(--primary)' }} />)}</div>
        {parts.map(part => <div className="rail-share-row" role="listitem" key={part.damage_type}>
          <span className="rail-share-dot" style={{ background: DAMAGE_COLOR[part.damage_type] ?? 'var(--primary)' }} />
          <span className="rail-label">{damageTypeLabel(lang, part.damage_type)}</span>
          <span className="rail-value">{((part.avg / total) * 100).toFixed(1)}%</span>
        </div>)}
      </div>}
    </>}
  </div>;
}

function Minion({ view, lang, onStatClick }: Pick<Shared, 'view' | 'lang' | 'onStatClick'>) {
  const tt = bindT(lang);
  const minion = view.calc?.minions?.find(row => row.index === view.calc?.main_minion_index);
  if (!minion) return null;
  const values: Array<[string, number]> = [['TotalDPS', minion.dps], ['Life', minion.life], ['EnergyShield', minion.energy_shield], ['Armour', minion.armour], ['Evasion', minion.evasion]];
  return <section className="rail-section"><h3 className="rail-title">{tt('minion.title')} · {minion.name || tt('minion.unavailable')}</h3>
    {values.map(([name, value]) => <Row key={name} label={name === 'TotalDPS' ? tt('sidebar.hitDps') : statNameLabel(lang, name)} value={formatStatValue(value, 'float2')}
      onOpen={detailKind(minion, name) !== 'none' ? () => onStatClick(`Minion:${minion.index}:${name}`) : undefined} />)}
  </section>;
}

/** The numbers every edit changes. They stay on screen so cause and effect are never a page apart. */
export function ResultsRail({ session, view, lang, onStatClick, onNavigate, pinned, onTogglePin }: Shared & { onNavigate: (route: Route) => void; pinned: readonly string[]; onTogglePin: (id: string) => void }) {
  const calc = view.calc;
  const values = useMemo(() => calc ? statMap(calc.stats) : null, [calc]);
  const changes = useMemo(() => computeStatChanges(view.previous?.stats ?? null, calc?.stats ?? []), [view.previous, calc]);
  const [collapsed, setCollapsed] = useState(readCollapsed);
  const toggle = (key: string) => setCollapsed(previous => {
    const next = previous.includes(key) ? previous.filter(item => item !== key) : [...previous, key];
    try { localStorage.setItem(COLLAPSED_KEY, JSON.stringify(next)); } catch { /* A remembered fold is a convenience only. */ }
    return next;
  });
  const pinFor = (id: string) => ({ on: pinned.includes(id), toggle: () => onTogglePin(id), label: `${words(lang, pinned.includes(id) ? 'Unpin' : 'Pin', pinned.includes(id) ? '取消固定' : '固定', pinned.includes(id) ? '取消固定' : '固定')}: ${statLabel(lang, id)}` });
  const pinnedRows = values ? pinned.filter(id => values.has(id)) : [];
  const version = [calc?.provenance?.data_version ?? session.dataVersion, calc?.provenance?.engine_version].filter(Boolean).join(' · ');
  return <aside id="character-stats" className="v2-rail-content" data-stale={view.stale || undefined} aria-label={words(lang, 'Character stats', '角色属性', '角色屬性')}>
    <Headline session={session} view={view} lang={lang} onStatClick={onStatClick} changes={changes} onNavigate={onNavigate} />
    <Minion view={view} lang={lang} onStatClick={onStatClick} />
    {calc && <button type="button" className="rail-link" onClick={() => onStatClick('skill-metrics')}>{bindT(lang)('calcs.skillMetrics')}<ChevronRight /></button>}
    {pinnedRows.length > 0 && <section className="rail-section rail-pinned"><h3 className="rail-title"><span>{words(lang, 'Pinned', '已固定', '已固定')}</span></h3>
      {pinnedRows.map(id => <Row key={id} label={statLabel(lang, id)} color={statColor(id)} change={changes.get(id)} pin={pinFor(id)}
        value={<AnimatedNumber value={values?.get(id)} format={shown => formatStatValue(shown, statFormat(id), lang)} />}
        onOpen={calc && detailKind(calc, id) !== 'none' ? () => onStatClick(id) : undefined} />)}</section>}
    {STAT_SECTIONS.map(section => {
      if (section.requiresSkill && (session.calcParams.main_socket_group === undefined || !calc?.main_skill)) return null;
      const rows = section.rows.filter(row => {
        if (!values) return false;
        const value = values.get(row.id);
        return value !== undefined && !(row.hideZero && (value === 0 || value === null));
      });
      if (rows.length === 0) return null;
      const key = section.title['en-US'];
      const folded = collapsed.includes(key);
      return <section key={key} className="rail-section" data-folded={folded || undefined}>
        <h3 className="rail-title"><button type="button" aria-expanded={!folded} onClick={() => toggle(key)}><ChevronRight />{section.title[lang]}</button></h3>
        <div className="rail-fold" data-open={!folded || undefined} inert={folded || undefined}><div>
          {rows.map(row => <Row key={row.id} label={row.label[lang]} color={row.colorVar ? `var(--${row.colorVar})` : undefined} change={changes.get(row.id)} pin={pinFor(row.id)}
            value={<AnimatedNumber value={values?.get(row.id)} format={shown => formatStatValue(shown, row.format, lang)} />}
            onOpen={calc && detailKind(calc, row.id) !== 'none' ? () => onStatClick(row.id) : undefined} />)}
        </div></div>
      </section>;
    })}
    {(version || session.calcMs !== null) && <footer className="rail-foot">
      <span>{version}</span>{session.calcMs !== null && <span>{Math.max(1, Math.round(session.calcMs))} ms</span>}</footer>}
  </aside>;
}
