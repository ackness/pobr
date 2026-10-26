import { useEffect, useState } from 'react';
import type { ConfigInputValue } from '../../api/types';
import type { Lang } from '../../lib/i18n';
import { AppInput, AppSelectControl } from '../shared/controls';
import { workflowLabel } from './workflowLabels';

type ValueType = 'unset' | 'boolean' | 'number' | 'string';

/** Raw editing is deliberately typed: "0", 0, false and absence round-trip separately. */
export function RawConfigValue({ name, value, lang, disabled, onChange }: {
  name: string;
  value: ConfigInputValue | undefined;
  lang: Lang;
  disabled?: boolean;
  onChange: (value: ConfigInputValue | null) => void;
}) {
  const type: ValueType = value === undefined ? 'unset' : typeof value as Exclude<ValueType, 'unset'>;
  const [draft, setDraft] = useState(String(value ?? ''));
  useEffect(() => setDraft(String(value ?? '')), [value]);
  return <div className="config-raw-value">
    <AppSelectControl className="w-full" aria-label={`${workflowLabel(lang, 'type')}: ${name}`} value={type} disabled={disabled}
      onChange={event => {
        const next = event.target.value as ValueType;
        onChange(next === 'unset' ? null : next === 'boolean' ? value === true
          : next === 'number' ? (Number.isFinite(Number(value)) ? Number(value) : 0) : String(value ?? ''));
      }}>
      {(['unset', 'boolean', 'number', 'string'] as const).map(kind => <option key={kind} value={kind}>{workflowLabel(lang, kind)}</option>)}
    </AppSelectControl>
    {type === 'boolean' ? <AppSelectControl className="w-full" aria-label={name} value={String(value)} disabled={disabled}
      onChange={event => onChange(event.target.value === 'true')}>
      <option value="true">{workflowLabel(lang, 'true')}</option>
      <option value="false">{workflowLabel(lang, 'false')}</option>
    </AppSelectControl> : <AppInput aria-label={name} type={type === 'number' ? 'number' : 'text'} step="any"
      disabled={disabled || type === 'unset'} value={draft} onChange={event => setDraft(event.target.value)}
      placeholder={type === 'unset' ? workflowLabel(lang, 'unset') : undefined}
      onKeyDown={event => { if (event.key === 'Enter') event.currentTarget.blur(); }}
      onBlur={() => {
        if (type === 'string' && draft !== value) onChange(draft);
        else if (type === 'number') {
          if (draft.trim() && Number.isFinite(Number(draft))) {
            if (Number(draft) !== value) onChange(Number(draft));
          } else setDraft(String(value));
        }
      }} />}
  </div>;
}
