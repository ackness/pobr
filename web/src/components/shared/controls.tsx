import { Children, Fragment, isValidElement, useRef, useState, type ComponentProps, type ReactElement, type ReactNode } from 'react';
import { Button } from '../ui/button';
import { Input } from '../ui/input';
import { Textarea } from '../ui/textarea';
import { Checkbox } from '../ui/checkbox';
import { Select, SelectContent, SelectItem, SelectLabel, SelectGroup, SelectTrigger, SelectValue } from '../ui/select';
import { Tooltip, TooltipContent, TooltipTrigger, TooltipProvider } from '../ui/tooltip';
import { useUi } from './UiContext';
import './controls.css';

/** Native props stay valid on both presentations, including refs and form events. */
export function AppTooltip({ content, children }: { content?: ReactNode; children: ReactElement }) {
  const { v2 } = useUi();
  if (!content) return children;
  return <TooltipProvider delayDuration={250}><Tooltip><TooltipTrigger asChild>{children}</TooltipTrigger>
    <TooltipContent className={v2 ? 'app-tooltip' : 'app-tooltip app-tooltip-legacy'}>{content}</TooltipContent>
  </Tooltip></TooltipProvider>;
}

export function AppButton({ title, children, className, ...props }: ComponentProps<'button'>) {
  const { v2 } = useUi();
  const button = v2 ? <Button variant="outline" className={`app-button ${className ?? ''}`} aria-label={title} {...props}>{children}</Button>
    : <button className={className} aria-label={title} {...props}>{children}</button>;
  return <AppTooltip content={title}>{title && props.disabled
    ? <span className="app-disabled-tooltip" tabIndex={0} aria-label={title}>{button}</span> : button}</AppTooltip>;
}

function CheckboxInput({ checked, defaultChecked, onChange, ...props }: ComponentProps<'input'>) {
  const [local, setLocal] = useState(defaultChecked ?? false);
  return <span className="app-checkbox">
    {/* The actual input owns focus, label activation, refs, forms and native events. */}
    <input {...props} type="checkbox" checked={checked ?? local} onChange={event => {
      setLocal(event.currentTarget.checked); onChange?.(event);
    }} />
    <Checkbox checked={checked ?? local} disabled={props.disabled} tabIndex={-1} aria-hidden="true" />
  </span>;
}

export function AppInput({ title, ...props }: ComponentProps<'input'>) {
  const { v2 } = useUi();
  const input = !v2 ? <input {...props} /> : props.type === 'checkbox' ? <CheckboxInput {...props} />
    : <Input {...props} className={`${props.className ?? ''} ${props.type === 'radio' || props.type === 'range' ? `app-input-${props.type}` : ''}`} />;
  return <AppTooltip content={title}>{input}</AppTooltip>;
}

export function AppTextarea({ title, ...props }: ComponentProps<'textarea'>) {
  const { v2 } = useUi();
  return <AppTooltip content={title}>{v2 ? <Textarea {...props} /> : <textarea {...props} />}</AppTooltip>;
}

interface Option { value: string; label: ReactNode; disabled?: boolean; group?: string }
function optionsFrom(children: ReactNode, group?: string, disabled = false): Option[] {
  return Children.toArray(children).flatMap(child => {
    if (!isValidElement<{ value?: string | number; label?: string; disabled?: boolean; children?: ReactNode }>(child)) return [];
    if (child.type === Fragment) return optionsFrom(child.props.children, group, disabled);
    if (child.type === 'optgroup') return optionsFrom(child.props.children, child.props.label, disabled || child.props.disabled);
    if (child.type !== 'option') return [];
    return [{ value: String(child.props.value ?? child.props.children ?? ''), label: child.props.label ?? child.props.children,
      disabled: disabled || child.props.disabled, group }];
  });
}

export function AppSelectControl(props: ComponentProps<'select'>) {
  const { v2 } = useUi();
  const { title, ...native } = props;
  if (!v2 || props.multiple || (props.size ?? 0) > 1) return <AppTooltip content={title}><select {...native} /></AppTooltip>;
  return <RadixSelectControl {...props} />;
}

function RadixSelectControl({ children, value, defaultValue, onChange, title, className, id, name, disabled,
  ref, style, 'aria-label': label, 'aria-labelledby': labelledBy, 'aria-describedby': description, ...props }: ComponentProps<'select'>) {
  const options = optionsFrom(children);
  const [local, setLocal] = useState(String(defaultValue ?? options.find(option => !option.disabled)?.value ?? ''));
  const native = useRef<HTMLSelectElement | null>(null);
  const selected = String(value ?? local);
  const entries = options.some(option => option.value === selected) ? options : [{ value: selected, label: selected || '—' }, ...options];
  const encode = (raw: string) => `value:${raw}`;
  return <>
    <Select value={encode(selected)} disabled={disabled} required={props.required} onValueChange={next => {
      const raw = next.slice(6);
      if (native.current) {
        native.current.value = raw;
        native.current.dispatchEvent(new Event('change', { bubbles: true }));
      }
    }}>
      <AppTooltip content={title}><SelectTrigger id={id} className={className} style={style} aria-label={label}
        aria-labelledby={labelledBy} aria-describedby={description} aria-invalid={props['aria-invalid']}
        aria-required={props.required} autoFocus={props.autoFocus}>
        <SelectValue>{entries.find(option => option.value === selected)?.label}</SelectValue>
      </SelectTrigger></AppTooltip>
      <SelectContent position="popper" align="start" className="app-select-content">
        <SelectGroup>{entries.map((option, index) => <Fragment key={`${option.value}:${index}`}>
          {option.group && option.group !== entries[index - 1]?.group && <SelectLabel>{option.group}</SelectLabel>}
          <SelectItem value={encode(option.value)} disabled={option.disabled}>{option.label}</SelectItem>
        </Fragment>)}</SelectGroup>
      </SelectContent>
    </Select>
    <select {...props} ref={element => {
      native.current = element;
      if (typeof ref === 'function') return ref(element);
      if (ref) ref.current = element;
    }} hidden aria-hidden="true" tabIndex={-1} name={name} disabled={disabled} value={selected} onChange={event => {
      setLocal(event.target.value); onChange?.(event);
    }}>{children}</select>
  </>;
}
