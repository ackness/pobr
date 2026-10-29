import { useAppDialogs } from './AppDialogs';
import { useSessionAction } from './useSessionAction';
import { AppButton, AppInput, AppSelectControl } from './controls';
import { useState } from 'react';
import type { BuildSession } from '../../hooks/useBuildSession';
import { bindT, type Lang } from '../../lib/i18n';
import './namedSets.css';
import { DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuTrigger } from '../ui/dropdown-menu';
import { MoreHorizontal } from 'lucide-react';
import { words } from '../../app/text';

export function NamedSets({ session, lang, kind = 'config' }: { session: BuildSession; lang: Lang; kind?: 'config' | 'skill' }) {
  const tt = bindT(lang);
  const dialogs = useAppDialogs(lang);
  const captureAction = useSessionAction(session);
  const sets = (kind === 'config' ? session.build?.config_sets : session.build?.skill_sets) ?? [{ id: 1, title: 'Default' }];
  const active = (kind === 'config' ? session.build?.active_config_set : session.build?.active_skill_set) ?? 1;
  const label = tt(kind === 'config' ? 'namedSets.config' : 'namedSets.skill');
  const [name, setName] = useState('');
  const [editing, setEditing] = useState<'create' | 'duplicate' | 'rename' | null>(null);
  return <section className="named-sets" aria-label={label}>
    <label>{label}
      <AppSelectControl aria-label={label} value={active} disabled={session.busy}
        onChange={event => { setEditing(null); void session.changeNamedSet(kind, 'select', Number(event.target.value)); }}>
        {sets.map(set => <option key={set.id} value={set.id}>{set.title}</option>)}
      </AppSelectControl>
    </label>
    <DropdownMenu><DropdownMenuTrigger asChild><AppButton disabled={session.busy}
      aria-label={words(lang, `${label} actions`, `${label}操作`, `${label}操作`)}><MoreHorizontal aria-hidden="true" /></AppButton></DropdownMenuTrigger>
      <DropdownMenuContent align="end">
    {(['create', 'duplicate', 'rename'] as const).map(op => <DropdownMenuItem key={op}
      onSelect={() => { setEditing(op); setName(op === 'rename' ? sets.find(set => set.id === active)?.title ?? '' : ''); }}>
      {tt(`namedSets.${op}`)}
    </DropdownMenuItem>)}
    <DropdownMenuItem variant="destructive" disabled={sets.length < 2} onSelect={async () => {
      const valid = captureAction();
      if (await dialogs.confirm(tt('namedSets.confirmDelete')) && valid()) { setEditing(null); void session.changeNamedSet(kind, 'remove', active); }
    }}>{tt('namedSets.remove')}</DropdownMenuItem>
      </DropdownMenuContent></DropdownMenu>
    {editing && <form onSubmit={event => {
      event.preventDefault();
      if (name.trim()) { void session.changeNamedSet(kind, editing, active, name.trim()); setEditing(null); }
    }}>
      <AppInput aria-label={tt('namedSets.name')} placeholder={tt('namedSets.name')} value={name} autoFocus
        onChange={event => setName(event.target.value)} />
      <AppButton type="submit" disabled={session.busy || !name.trim()}>{tt('namedSets.save')}</AppButton>
      <AppButton type="button" onClick={() => setEditing(null)}>{tt('namedSets.cancel')}</AppButton>
    </form>}
  </section>;
}
