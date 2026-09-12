import { Fragment, useEffect, useId, useLayoutEffect, useRef, useState } from 'react';

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
  const [open, setOpen] = useState(false);
  const [highlight, setHighlight] = useState(0);
  const rootRef = useRef<HTMLDivElement | null>(null);
  const listRef = useRef<HTMLUListElement | null>(null);
  const triggerRef = useRef<HTMLButtonElement | null>(null);
  const listId = useId();
  const isOpen = open && !disabled;

  const selectedIdx = options.findIndex((o) => o.value === value);
  const current = selectedIdx >= 0 ? options[selectedIdx] : null;

  useLayoutEffect(() => {
    if (!isOpen) return;
    const position = () => {
      const list = listRef.current;
      if (!list) return;
      list.style.left = '0px';
      const rect = list.getBoundingClientRect();
      const bounds = rootRef.current?.closest('main')?.getBoundingClientRect();
      const left = (bounds?.left ?? 0) + 16;
      const right = (bounds?.right ?? document.documentElement.clientWidth) - 16;
      list.style.left = `${Math.max(left - rect.left, Math.min(0, right - rect.right))}px`;
    };
    position();
    window.addEventListener('resize', position);
    return () => window.removeEventListener('resize', position);
  }, [isOpen]);

  useEffect(() => {
    if (!isOpen) return;
    setHighlight(Math.max(0, selectedIdx));
    listRef.current?.focus({ preventScroll: true });
    listRef.current?.querySelector('[aria-selected="true"]')?.scrollIntoView({ block: 'nearest' });
    const onDown = (e: PointerEvent) => {
      if (rootRef.current && !rootRef.current.contains(e.target as Node)) setOpen(false);
    };
    document.addEventListener('pointerdown', onDown);
    return () => document.removeEventListener('pointerdown', onDown);
    // selectedIdx 只作打开瞬间的初始高亮，不随外部变化重置。
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [isOpen]);

  useEffect(() => {
    if (disabled) setOpen(false);
  }, [disabled]);

  const pick = (option: AppSelectOption) => {
    triggerRef.current?.focus({ preventScroll: true });
    onChange(option.value);
    setOpen(false);
  };

  const onKeyDown = (e: React.KeyboardEvent) => {
    if (disabled) return;
    if (e.key === 'Escape') {
      e.preventDefault();
      setOpen(false);
      triggerRef.current?.focus({ preventScroll: true });
      return;
    }
    if (!open) {
      if (e.key === 'ArrowDown' || e.key === 'ArrowUp' || e.key === 'Enter' || e.key === ' ') {
        e.preventDefault();
        setOpen(true);
      }
      return;
    }
    if (['ArrowDown', 'ArrowUp', 'Home', 'End'].includes(e.key)) {
      e.preventDefault();
      const next = e.key === 'Home' ? 0 : e.key === 'End' ? options.length - 1
        : e.key === 'ArrowDown' ? highlight + 1 : highlight - 1;
      const clamped = Math.max(0, Math.min(options.length - 1, next));
      setHighlight(clamped);
      listRef.current
        ?.querySelector(`[data-idx="${clamped}"]`)
        ?.scrollIntoView({ block: 'nearest' });
    } else if (e.key === 'Enter' || e.key === ' ') {
      e.preventDefault();
      if (options[highlight]) pick(options[highlight]);
    }
  };

  return (
    <div className="app-select" ref={rootRef} onKeyDown={onKeyDown}
      onBlur={event => {
        if (!event.currentTarget.contains(event.relatedTarget)) setOpen(false);
      }}>
      <button
        ref={triggerRef}
        type="button"
        className="app-select-trigger"
        aria-haspopup="listbox"
        aria-expanded={isOpen}
        aria-controls={isOpen ? listId : undefined}
        aria-label={ariaLabel}
        disabled={disabled}
        onClick={() => setOpen(!open)}
      >
        <span className="app-select-value">{current?.label ?? placeholder ?? ''}</span>
        <span className="app-select-caret" aria-hidden>
          ▾
        </span>
      </button>
      {isOpen && (
        <ul className="app-select-panel" role="listbox" ref={listRef} id={listId}
          tabIndex={-1} aria-label={ariaLabel}
          aria-activedescendant={options[highlight] ? `${listId}-${highlight}` : undefined}>
          {options.map((option, idx) => (
            <Fragment key={`${option.value}-${idx}`}>
              {option.group && option.group !== options[idx - 1]?.group && (
                <li className="app-select-group" role="presentation">
                  {option.group}
                </li>
              )}
              <li
                id={`${listId}-${idx}`}
                role="option"
                aria-selected={option.value === value}
                data-idx={idx}
                data-value={option.value}
                className={`app-select-option${idx === highlight ? ' is-highlight' : ''}${
                  option.value === value ? ' is-selected' : ''
                }`}
                onPointerEnter={() => setHighlight(idx)}
                onMouseDown={event => event.preventDefault()}
                onClick={() => pick(option)}
              >
                {option.label}
                {option.hint && <span className="app-select-hint">{option.hint}</span>}
              </li>
            </Fragment>
          ))}
        </ul>
      )}
    </div>
  );
}
