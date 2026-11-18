import { Fragment, useState } from 'react';
import { bindT, type Lang } from '../../lib/i18n';
import { upgradeT } from '../../lib/upgradeText';
import { AppButton, AppInput } from '../shared/controls';
import { Dialog, DialogClose, DialogContent, DialogDescription, DialogTitle, DialogTrigger } from '../ui/dialog';

export interface RuneChoice {
  value: string;
  label: string;
  group?: string;
  effects: string[];
  searchText: string;
}

export function RuneSocketPicker({ lang, socket, destination, current, options, disabled, onApply }: {
  lang: Lang; socket: string; destination: string; current: RuneChoice; options: RuneChoice[];
  disabled: boolean; onApply: (name: string) => void;
}) {
  const tt = bindT(lang);
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState('');
  const [value, setValue] = useState(current.value);
  const terms = query.trim().toLowerCase().split(/\s+/).filter(Boolean);
  const visible = terms.length ? options.filter(option => terms.every(term => option.searchText.includes(term))) : options;
  const candidate = options.find(option => option.value === value);
  const changed = candidate && candidate.value !== current.value;
  const searchLabel = tt('items.augmentSearch');

  return <div className="app-select rune-socket">
    <Dialog open={open} onOpenChange={next => {
      if (next) { setQuery(''); setValue(current.value); }
      setOpen(next);
    }}>
      <DialogTrigger asChild>
        <AppButton role="combobox" aria-label={socket} disabled={disabled}>
          <span className="rune-socket-number" aria-hidden="true">{socket}</span>
          {current.label}
        </AppButton>
      </DialogTrigger>
      {open && <DialogContent className="rune-picker" showCloseButton={false}>
        <DialogTitle>{destination}</DialogTitle>
        <DialogDescription>
          {tt('items.augmentCurrent').replace('{name}', current.label)}
          {(current.effects.length > 0 || current.value) && <span className="rune-picker-effects">
            {current.effects.length ? current.effects.join(' · ') : tt('items.augmentEffectUnavailable')}
          </span>}
        </DialogDescription>
        <AppInput type="search" aria-label={searchLabel} placeholder={searchLabel} value={query} onChange={event => setQuery(event.target.value)} />
        <div className="rune-picker-options" role="group" aria-label={tt('items.augmentChoices')}>
          {visible.length === 0 && <p role="status">{upgradeT(lang, 'noMatchingAugment')}</p>}
          {visible.map((option, index) => <Fragment key={option.value}>
            {option.group && option.group !== visible[index - 1]?.group && <p className="rune-picker-group">{option.group}</p>}
            <label className="rune-picker-option">
              <input type="radio" name="rune-choice" value={option.value} checked={value === option.value} onChange={() => setValue(option.value)} />
              <span>{option.label}{option.effects[0] && <small>{option.effects[0]}</small>}</span>
            </label>
          </Fragment>)}
        </div>
        {changed && <div className="rune-picker-preview" aria-live="polite">
          <strong>{tt('items.augmentPreview').replace('{name}', candidate.label)}</strong>
          {candidate.effects.map((effect, index) => <span key={index}>{effect}</span>)}
        </div>}
        <div className="rune-picker-actions">
          <DialogClose asChild><AppButton>{tt('items.cancel')}</AppButton></DialogClose>
          <AppButton className="button-primary" disabled={disabled || !changed} onClick={() => {
            if (!changed) return;
            setOpen(false);
            onApply(candidate.value);
          }}>{tt(current.value ? (candidate?.value ? 'items.augmentReplace' : 'items.augmentRemove') : 'items.augmentInsert')}</AppButton>
        </div>
      </DialogContent>}
    </Dialog>
  </div>;
}
