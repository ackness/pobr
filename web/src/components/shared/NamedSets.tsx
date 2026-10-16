import { useState } from 'react';
import type { BuildSession } from '../../hooks/useBuildSession';
import { bindT, type Lang } from '../../lib/i18n';
import './namedSets.css';

export function NamedSets({ session, lang, kind = 'config' }: { session: BuildSession; lang: Lang; kind?: 'config' | 'skill' }) {
  const tt = bindT(lang);
  const sets = (kind === 'config' ? session.build?.config_sets : session.build?.skill_sets) ?? [{ id: 1, title: 'Default' }];
  const active = (kind === 'config' ? session.build?.active_config_set : session.build?.active_skill_set) ?? 1;
  const label = tt(kind === 'config' ? 'namedSets.config' : 'namedSets.skill');
  const [name, setName] = useState('');
  const [editing, setEditing] = useState<'create' | 'duplicate' | 'rename' | null>(null);
  return <section className="named-sets" aria-label={label}>
    <label>{label}
      <select aria-label={label} value={active} disabled={session.busy}
        onChange={event => { setEditing(null); void session.changeNamedSet(kind, 'select', Number(event.target.value)); }}>
        {sets.map(set => <option key={set.id} value={set.id}>{set.title}</option>)}
      </select>
    </label>
    {(['create', 'duplicate', 'rename'] as const).map(op => <button key={op} disabled={session.busy}
      onClick={() => { setEditing(op); setName(op === 'rename' ? sets.find(set => set.id === active)?.title ?? '' : ''); }}>
      {tt(`namedSets.${op}`)}
    </button>)}
    <button disabled={session.busy || sets.length < 2} onClick={() => {
      if (window.confirm(tt('namedSets.confirmDelete'))) { setEditing(null); void session.changeNamedSet(kind, 'remove', active); }
    }}>{tt('namedSets.remove')}</button>
    {editing && <form onSubmit={event => {
      event.preventDefault();
      if (name.trim()) { void session.changeNamedSet(kind, editing, active, name.trim()); setEditing(null); }
    }}>
      <input aria-label={tt('namedSets.name')} placeholder={tt('namedSets.name')} value={name} autoFocus
        onChange={event => setName(event.target.value)} />
      <button type="submit" disabled={session.busy || !name.trim()}>{tt('namedSets.save')}</button>
      <button type="button" onClick={() => setEditing(null)}>{tt('namedSets.cancel')}</button>
    </form>}
  </section>;
}
