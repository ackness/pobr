import { Children, Fragment, isValidElement, useState, type ChangeEvent, type ComponentProps, type ReactNode } from 'react';
import { Button } from '../ui/button';
import { Input } from '../ui/input';
import { Textarea } from '../ui/textarea';
import { Checkbox } from '../ui/checkbox';
import { Select, SelectContent, SelectItem, SelectLabel, SelectGroup, SelectTrigger, SelectValue } from '../ui/select';
import { Tooltip, TooltipContent, TooltipTrigger } from '../ui/tooltip';
import { useUi } from './UiContext';

/** Shared presentation adapters preserve the existing controlled form contract. */
export function AppButton({ title, children, ...props }: ComponentProps<'button'>) {
  const { v2 } = useUi();
  if (!v2) return <button title={title} {...props}>{children}</button>;
  const button = <Button variant="outline" {...props}>{children}</Button>;
  return title ? <Tooltip><TooltipTrigger asChild>{button}</TooltipTrigger><TooltipContent>{title}</TooltipContent></Tooltip> : button;
}

export function AppInput(props: ComponentProps<'input'>) {
  const { v2 } = useUi();
  if (!v2) return <input {...props} />;
  if (props.type === 'checkbox') {
    const { checked, defaultChecked, onChange, id, name, disabled, className, 'aria-label': label, 'aria-describedby': description } = props;
    return <Checkbox id={id} name={name} disabled={disabled} className={className}
      checked={checked} defaultChecked={defaultChecked} aria-label={label} aria-describedby={description}
      onCheckedChange={next => {
        const target = { checked: next === true, value: String(props.value ?? 'on'), name: name ?? '', id: id ?? '', type: 'checkbox' };
        onChange?.({ target, currentTarget: target } as ChangeEvent<HTMLInputElement>);
      }} />;
  }
  return <Input {...props} />;
}

export function AppTextarea(props: ComponentProps<'textarea'>) {
  const { v2 } = useUi();
  return v2 ? <Textarea {...props} /> : <textarea {...props} />;
}

interface Option { value: string; label: ReactNode; disabled?: boolean; group?: string }
function optionsFrom(children: ReactNode, group?: string): Option[] {
  return Children.toArray(children).flatMap(child => {
    if (!isValidElement<{ value?: string | number; label?: string; disabled?: boolean; children?: ReactNode }>(child)) return [];
    if (child.type === Fragment) return optionsFrom(child.props.children, group);
    if (child.type === 'optgroup') return optionsFrom(child.props.children, child.props.label);
    if (child.type !== 'option') return [];
    return [{ value: String(child.props.value ?? child.props.children ?? ''), label: child.props.children,
      disabled: child.props.disabled, group }];
  });
}

export function AppSelectControl({ children, value, defaultValue, onChange, title, className, id, name, disabled,
  'aria-label': label, 'aria-describedby': description, ...props }: ComponentProps<'select'>) {
  const { v2 } = useUi();
  const [local, setLocal] = useState(String(defaultValue ?? ''));
  if (!v2) return <select {...props} className={className} id={id} name={name} disabled={disabled}
    aria-label={label} aria-describedby={description} title={title} value={value} defaultValue={defaultValue} onChange={onChange}>{children}</select>;
  const options = optionsFrom(children);
  const selected = String(value ?? local);
  const entries = options.some(option => option.value === selected) ? options : [{ value: selected, label: selected || '—' }, ...options];
  const encode = (raw: string) => `value:${raw}`;
  return <Select value={encode(selected)} name={name} disabled={disabled} onValueChange={next => {
    const raw = next.slice(6); setLocal(raw);
    const target = { value: raw, name: name ?? '', id: id ?? '', selectedIndex: options.findIndex(option => option.value === raw) };
    onChange?.({ target, currentTarget: target } as ChangeEvent<HTMLSelectElement>);
  }}>
    <SelectTrigger id={id} className={className} aria-label={label} aria-describedby={description}><SelectValue /></SelectTrigger>
    <SelectContent position="popper">
      <SelectGroup>{entries.map((option, index) => <Fragment key={`${option.value}:${index}`}>
        {option.group && option.group !== entries[index - 1]?.group && <SelectLabel>{option.group}</SelectLabel>}
        <SelectItem value={encode(option.value)} disabled={option.disabled}>{option.label}</SelectItem>
      </Fragment>)}</SelectGroup>
    </SelectContent>
  </Select>;
}
