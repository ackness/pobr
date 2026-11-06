import { colon } from './text';
import { useEffect, useState } from 'react';
import { useMainMinion } from '../hooks/useMainMinion';
import { useSkillName } from '../hooks/useSkillName';
import { Button } from '../components/ui/button';
import { bindT, type Lang } from '../lib/i18n';
import { classDisplayName } from '../lib/nativeDisplayNames';
import { headlineDpsStat, minionDpsNote } from '../lib/mainMinion';
import { formatStatValue } from '../lib/statDisplay';
import { UnincludedModifiers } from '../components/shared/UnincludedModifiers';
import type { Route } from './routes';
import type { ImportSummary } from './importReview';

export function ImportReviewNotice({ route, lang, summary, onDismiss, onReview }: {
  route: Route; lang: Lang; summary: ImportSummary; onDismiss: () => void; onReview: () => void;
}) {
  const tt = bindT(lang);
  const skillName = useSkillName(lang);
  const minion = useMainMinion(summary.calc, lang);
  const [expanded, setExpanded] = useState(false);
  useEffect(() => { if (route === 'config') onDismiss(); }, [route, onDismiss]);
  if (route === 'config') return null;
  const { character, calc } = summary;
  const className = character && classDisplayName(summary.classNames, character.ascendancy_name || character.class_name, lang);
  const stat = headlineDpsStat(calc);
  const damage = minion ? minion.minion.dps : calc?.stats.find(entry => entry.id === stat)?.value;
  return <div className="shell-notice import-summary" role="note" aria-label={tt('build.importSummary')}>
    <div className="import-summary-body">
      <strong>{tt('build.importedCharacter').replace('{class}', className || tt('build.none')).replace('{level}', String(character?.level ?? '—'))}</strong>
      <p>{tt('build.calculatedSkill')}{colon(lang)}{calc?.main_skill ? skillName(calc.main_skill.skill_id) : tt(calc ? 'sidebar.noMainSkill' : summary.failed ? 'build.summaryFailed' : 'build.summaryPending')}
        {damage !== undefined && <>{' · '}{tt(minion ? 'sidebar.minionHitDps' : 'sidebar.combinedDps')} <span className="import-summary-number">{formatStatValue(damage, 'float2')}</span></>}</p>
      {minion && <p>{minionDpsNote(lang, minion.name, calc?.stats.find(entry => entry.id === 'ActiveMinionLimit')?.value)}</p>}
      {calc && <p>{tt('build.summaryModifiers').replace('{count}', String(calc.unsupported_modifiers.length))}{' '}
        <a href="#import-summary-modifiers" aria-expanded={expanded} onClick={event => { event.preventDefault(); setExpanded(value => !value); }}>{tt('build.summaryView')}</a></p>}
      {expanded && calc && <div id="import-summary-modifiers"><UnincludedModifiers lines={calc.unsupported_modifiers} lang={lang} /></div>}
      <p>{tt('config.importReview')}{' '}<a href="/config" onClick={event => { event.preventDefault(); onReview(); }}>{tt('config.review')}</a></p>
    </div>
    <Button variant="ghost" onClick={onDismiss}>{tt('config.reviewDismiss')}</Button>
  </div>;
}
