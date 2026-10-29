import { Fragment, useEffect, useState } from 'react';
import { usePanelActive } from './UiContext';
import { Select, SelectContent, SelectGroup, SelectItem, SelectLabel, SelectTrigger, SelectValue } from '../ui/select';

export interface AppSelectOption {
  value: string;
  label: string;
  /** 可选分组标题（相邻同组只渲染一次，对应原生 optgroup）。 */
  group?: string;
  /** 可选副行说明（面板选项内 label 下方的弱化小字；触发按钮不显示）。 */
  hint?: string;
}

interface Props {
  value: string;
  options: AppSelectOption[];
  onChange: (value: string) => void;
  disabled?: boolean;
  ariaLabel: string;
  /** 无匹配 value 时触发按钮上显示的文案。 */
  placeholder?: string;
}

/**
 * 应用统一风格的下拉选择器（替代系统原生 select；面板视觉与 GemPicker 一致）。
 * 键盘：↑↓ 移动高亮、Enter/Space 打开或选中、Esc 关闭；点击组件外关闭。
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
          <SelectItem value={`value:${option.value}`} textValue={option.label}>
            <span className="app-select-option-copy"><span>{option.label}</span>{option.hint && <small>{option.hint}</small>}</span>
          </SelectItem>
        </Fragment>)}</SelectGroup>
      </SelectContent>
    </Select>
  </div>;
}
