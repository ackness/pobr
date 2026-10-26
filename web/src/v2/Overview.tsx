import { ArrowRight, ArrowUpRight, Swords, Shield, Gem, Network, SlidersHorizontal, Import, Layers } from 'lucide-react';
import { Button } from '../components/ui/button';
import { Badge } from '../components/ui/badge';
import { Card } from '../components/ui/card';
import { statMap, formatStatValue } from '../lib/statDisplay';
import { skillDisplayName } from '../lib/skillNames';
import { useSkillCatalog } from '../hooks/useSkillName';
import { chineseScript } from '../lib/chinese';
import type { BuildSession } from '../hooks/useBuildSession';
import type { Lang } from '../lib/i18n';
import { words, pageNames } from './text';
import type { Route } from './routes';

export function Overview({ session, lang, navigate, focusStat }: { session: BuildSession; lang: Lang; navigate: (route: Route) => void; focusStat: (id: string) => void }) {
  const w = (en: string, cn: string, tw?: string) => words(lang, en, cn, tw);
  const catalog = useSkillCatalog();
  const build = session.workspace?.builds.find(entry => entry.id === session.workspace?.activeBuild);
  const stage = build?.stages.find(entry => entry.id === build.activeStage);
  const fresh = !session.busy && !session.calcError && session.calc !== null;
  const stats = fresh ? statMap(session.calc!.stats) : new Map<string, number | null>();
  const skill = fresh && session.calcParams.main_socket_group !== undefined ? session.calc?.main_skill : null;
  const number = (value: number | null | undefined) => value == null ? '—' : formatStatValue(value, 'float2', lang);
  const className = session.character?.ascendancy_name || session.character?.class_name || '';
  const classLabel = lang === 'en-US' ? className : chineseScript((session.classNames.ascendancies[className] ?? session.classNames.classes[className]) ?? className, lang);
  const steps = [
    { route: 'skills' as const, icon: Gem, title: w('Start with your main skill', '确定你的核心技能', '確定你的核心技能'), text: w('Choose active effects, supports and weapon bindings.', '选择主动技能、辅助组合与武器绑定。', '選擇主動技能、輔助組合與武器綁定。') },
    { route: 'items' as const, icon: Swords, title: w('Shape your equipment', '完善装备搭配', '完善裝備搭配'), text: w('Edit items, compare candidates and keep drafts.', '编辑装备、比较候选，保留尚未应用的草稿。', '編輯裝備、比較候選，保留尚未套用的草稿。') },
    { route: 'config' as const, icon: SlidersHorizontal, title: w('Set the fight', '设定真实战斗条件', '設定真實戰鬥條件'), text: w('Check enemy, charges and conditions before comparing.', '先确认敌人、充能与生效条件，再判断提升。', '先確認敵人、充能與生效條件，再判斷提升。') },
  ];
  return <section className="v2-overview" aria-labelledby="overview-title">
    <div className="v2-hero"><div className="v2-hero-copy">
      <div className="v2-eyebrow"><span className="v2-gold-line" />{w('YOUR BUILD, IN FOCUS', '专注于你的下一次提升', '專注於你的下一次提升')}</div>
      <h1 id="overview-title">{build?.name || w('Your next build starts here.', '下一套构筑，从这里开始。', '下一套構築，從這裡開始。')}</h1>
      <p>{w('Understand what changes. Know what to improve.', '看清每次改动，找到下一步方向。', '看清每次改動，找到下一步方向。')}</p>
      <div className="v2-hero-meta"><Badge variant="outline">{classLabel || w('Character', '角色')} · {w('Level', '等级', '等級')} {session.character?.level}</Badge><Badge variant="outline"><Layers />{stage?.name || w('Current stage', '当前阶段', '目前階段')}</Badge></div>
      <div className="v2-hero-actions"><Button onClick={() => navigate(session.socketGroups.length ? 'trade' : 'build')}><ArrowUpRight />{session.socketGroups.length ? pageNames[lang].trade : w('Import or create a build', '导入或创建构筑', '匯入或建立構築')}</Button><Button variant="outline" onClick={() => navigate('tree')}><Network />{pageNames[lang].tree}</Button></div>
    </div><div className="v2-hero-emblem" aria-hidden="true"><Network /><span>PATH OF EXILE II</span><strong>PoBR</strong></div></div>
    <div className="v2-section-label"><h2>{w('At a glance', '当前表现', '目前表現')}</h2><span>{session.busy ? w('Calculating…', '计算中…', '計算中…') : w('Under the current combat setup', '基于当前战斗配置', '基於目前戰鬥配置')}</span></div>
    <div className="v2-metrics">
      {[
        { id: 'CombinedDPS', icon: Swords, label: w('Combined DPS', '综合每秒伤害', '綜合每秒傷害'), value: skill?.combined_dps, detail: skill ? skillDisplayName(skill.skill_id, lang, catalog) : w('Select a main skill', '请先选择主技能', '請先選擇主技能'), color: 'gold' },
        { id: 'Life', icon: Shield, label: w('Life', '生命'), value: stats.get('Life'), detail: w('Maximum life', '生命上限'), color: 'life' },
        { id: 'EnergyShield', icon: Shield, label: w('Energy shield', '能量护盾', '能量護盾'), value: stats.get('EnergyShield'), detail: w('Maximum energy shield', '能量护盾上限', '能量護盾上限'), color: 'es' },
        { id: 'Mana', icon: Gem, label: w('Mana', '魔力'), value: stats.get('Mana'), detail: w('Maximum mana', '魔力上限'), color: 'mana' },
      ].map(metric => <Card key={metric.id} className={`v2-metric v2-metric-${metric.color}`}><Button variant="ghost" className="v2-metric-button" onClick={() => focusStat(metric.id)}><span className="v2-metric-label"><metric.icon />{metric.label}<ArrowUpRight /></span><strong>{number(metric.value)}</strong><span className="v2-metric-detail">{metric.detail}</span></Button></Card>)}
    </div>
    <div className="v2-section-label"><h2>{w('Build with intention', '规划你的构筑', '規劃你的構築')}</h2><Button variant="link" onClick={() => navigate('build')}><Import />{w('Manage & import', '管理与导入', '管理與匯入')}</Button></div>
    <div className="v2-workflow">{steps.map((step, index) => <Card key={step.route} className="v2-workflow-card"><div className="v2-workflow-heading"><step.icon /><span>0{index + 1}</span></div><h3>{step.title}</h3><p>{step.text}</p><Button variant="ghost" onClick={() => navigate(step.route)}>{pageNames[lang][step.route]}<ArrowRight /></Button></Card>)}</div>
    <Card className="v2-review-strip"><div><h3>{w('A bigger number needs an explanation.', '提升，要有据可查。', '提升，要有據可查。')}</h3><p>{w('Inspect the calculation, then compare saved stages before committing to an upgrade.', '展开计算来源，并用阶段对比检查每一项变化。', '展開計算來源，並用階段比較檢查每一項變化。')}</p></div><div><Button variant="outline" onClick={() => navigate('calcs')}>{pageNames[lang].calcs}<ArrowRight /></Button><Button variant="outline" onClick={() => navigate('compare')}>{pageNames[lang].compare}<ArrowRight /></Button></div></Card>
  </section>;
}
