import { Fragment, useEffect, useState } from 'react';
import { usePanelActive } from './UiContext';
import { Select, SelectContent, SelectGroup, SelectItem, SelectLabel, SelectTrigger, SelectValue } from '../ui/select';

export interface AppSelectOption {
  value: string;
  label: string;
  /** Optional section heading (rendered once per adjacent group, like native `<optgroup>`). */
  group?: string;
  /** Optional secondary description shown below the option label in the popover; hidden on the trigger button. */
  hint?: string;
}

interface Props {
  value: string;
  options: AppSelectOption[];
  onChange: (value: string) => void;
  disabled?: boolean;
  ariaLabel: string;
  /** Fallback text shown on the trigger button when no option matches `value`. */
  placeholder?: string;
}

/**
 * Shared dropdown selector with unified app styling (replaces native `<select>`, matching GemPicker's popover).
 * Keyboard: Up/Down moves highlight, Enter/Space opens or selects, Esc closes; clicking outside closes.
 */
export function AppSelect({ value, options, onChange, disabled, ariaLabel, placeholder }: Props) {
  const active = usePanelActive();
  const [open, setOpen] = useState(false);
  useEffect(() => { if (!active) setOpen(false); }, [active]);
  const current = options.find(option => option.value === value);
  return <div className="app-select">
    <Select open={open && active} onOpenChange={setOpen} value={current ? `value:${value}` : ''} disabled={disabled} onValueChange={next => onChange(next.slice(6))}>
      <SelectTrigger aria-label={ariaLabel} className="app-select-trigger"><SelectValue placeholder={placeholder}>{current?.label}</SelectValue></SelectTrigger>
      <SelectContent position="popper" align="start" className="app-select-content">
        <SelectGroup>{options.map((option, index) => <Fragment key={option.value}>
          {option.group && option.group !== options[index - 1]?.group && <SelectLabel>{option.group}</SelectLabel>}
          <SelectItem value={`value:${option.value}`} textValue={option.label} data-value={option.value}>
            <span className="app-select-option-copy"><span>{option.label}</span>{option.hint && <small>{option.hint}</small>}</span>
          </SelectItem>
        </Fragment>)}</SelectGroup>
      </SelectContent>
    </Select>
  </div>;
}
