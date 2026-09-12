import { useState } from 'react';
import { AppSelect } from '../shared/AppSelect';
import { useLocalizedLines } from '../../hooks/useLocalizedLines';
import type { AugmentSelection, ReplacementAugments } from '../../lib/replacementAugments';
import type { Lang } from '../../lib/i18n';
import { upgradeT } from '../../lib/upgradeText';

export function ReplacementAugmentPicker({ plan, level, lang, disabled, onChange }: {
  plan: ReplacementAugments; level: number; lang: Lang; disabled: boolean; onChange: (value: AugmentSelection) => void;
}) {
  const ut = (key: Parameters<typeof upgradeT>[1]) => upgradeT(lang, key);
  const [query, setQuery] = useState('');
  const effects = useLocalizedLines(plan.info.options.flatMap(option => option.lines), lang);
  let effectOffset = 0;
  const options = plan.info.options.map(option => {
    const hint = effects.slice(effectOffset, effectOffset + option.lines.length).join('\n');
    effectOffset += option.lines.length;
    return {
    value: option.name, label: (lang === 'zh-CN' ? option.name_zh_cn : lang === 'zh-TW' ? option.name_zh_tw : option.name) || option.name,
    group: ut(option.kind === 'Idol' ? 'augmentIdol' : option.is_soul_core ? 'augmentCore' : 'augmentRune'),
    hint, level: option.required_level ?? 0, limit: option.limit, limitKey: option.limit_id ?? option.name,
  }; }).filter(option => option.level <= level)
    .sort((a, b) => a.group.localeCompare(b.group, lang) || a.label.localeCompare(b.label, lang));
  const filtered = options.filter(option => `${option.label} ${option.value} ${option.hint}`.toLocaleLowerCase().includes(query.trim().toLocaleLowerCase()));
  const custom = (sockets: number, runes = plan.runes) => onChange({ mode: 'custom', sockets, runes: runes.slice(0, sockets) });
  if (!plan.info.sockets && !plan.info.max_sockets) return null;
  return <section className="replacement-augments" aria-label={ut('augmentTitle')}>
    <div className="replacement-augment-heading"><h4>{ut('augmentTitle')}</h4><span>{plan.sockets} / {plan.info.max_sockets}</span></div>
    {!plan.info.editable ? <p className="replacement-augment-note">{ut('augmentLocked')}</p> : <>
      <div className="replacement-augment-modes" role="group" aria-label={ut('augmentTitle')}>
        {(['inherit', 'original', 'custom'] as const).map(mode => <button key={mode} disabled={disabled}
          aria-pressed={plan.selection.mode === mode} onClick={() => mode === 'custom' ? custom(plan.sockets || Math.min(1, plan.info.max_sockets)) : onChange({ mode })}>
          {ut(mode === 'inherit' ? 'augmentInherit' : mode === 'original' ? 'augmentOriginal' : 'augmentCustom')}</button>)}
      </div>
      <p className="replacement-augment-note">{ut(plan.source === 'inherited' ? 'augmentInherited' : plan.source === 'custom' ? 'augmentSimulation' : plan.info.runes.some(Boolean) ? 'augmentPreserved' : 'augmentEmpty')}</p>
      {plan.selection.mode !== 'custom' && plan.runes.some(Boolean) && <div className="replacement-augment-summary">
        {plan.runes.filter(Boolean).map((name, index) => <span key={index}>{options.find(option => option.value === name)?.label ?? name}</span>)}
      </div>}
      {plan.selection.mode === 'custom' && <>
        <div className="replacement-socket-count"><span>{ut('augmentSocket')} · {plan.sockets}</span><div>
          <button aria-label={ut('augmentRemove')} disabled={disabled || plan.sockets <= 0} onClick={() => custom(plan.sockets - 1)}>−</button>
          <button aria-label={ut('augmentAdd')} disabled={disabled || plan.sockets >= plan.info.max_sockets} onClick={() => custom(plan.sockets + 1)}>+</button>
        </div></div>
        {!!plan.sockets && <>
          <input type="search" className="replacement-augment-search" aria-label={ut('augmentSearch')} placeholder={ut('augmentSearch')} value={query} onChange={event => setQuery(event.target.value)} />
          {!filtered.length && <p>{ut('noMatchingAugment')}</p>}
          {Array.from({ length: plan.sockets }, (_, index) => {
            const name = plan.runes[index] ?? '';
            const chosen = options.find(option => option.value === name);
            const choices = [{ value: '', label: ut('augmentEmptySocket') }, ...(chosen && !filtered.includes(chosen) ? [chosen] : []), ...filtered.filter(option =>
              option.limit === undefined || plan.runes.filter((rune, i) => i !== index &&
                (plan.info.options.find(entry => entry.name === rune)?.limit_id ?? rune) === option.limitKey).length < option.limit)];
            return <div className="replacement-socket" key={index}>
              <span className={`replacement-socket-icon${name ? ' is-filled' : ''}`} aria-hidden>{index + 1}</span>
              <div><AppSelect value={name} options={choices} ariaLabel={`${ut('augmentSocket')} ${index + 1}`} disabled={disabled}
                onChange={value => { const runes = Array.from({ length: plan.sockets }, (_, i) => i === index ? value : plan.runes[i] ?? ''); custom(plan.sockets, runes); }} />
                {chosen && <p className="replacement-socket-effect">{chosen.hint}</p>}</div>
            </div>;
          })}
        </>}
      </>}
    </>}
    {plan.addedSockets > 0 && <p className="replacement-augment-assumption">+{plan.addedSockets} · {ut('augmentAdded')}</p>}
    {plan.source === 'inherited' && <p className="replacement-augment-note">{ut('augmentSimulation')}</p>}
    {!!plan.skipped.length && <p className="replacement-augment-assumption">{ut('augmentSkipped')}: {plan.skipped.join(', ')}</p>}
    {plan.limitWarning && <p className="replacement-augment-assumption">{ut('augmentLimitUnknown')}</p>}
  </section>;
}
