import { AppButton, AppInput, AppTextarea } from '../shared/controls';
import { useRef, useState } from 'react';
import { getBackend } from '../../api/backend';
import type { PartyMember } from '../../api/types';
import type { BuildSession } from '../../hooks/useBuildSession';
import type { Lang } from '../../lib/i18n';
import { createId } from '../../lib/createId';
import { workflowLabel } from './workflowLabels';

const destinations = ['PartyMemberStats', 'Aura', 'Curse', 'Warcry Skills', 'Link Skills', 'EnemyConditions', 'EnemyMods'] as const;

export function PartyEditor({ session, lang }: { session: BuildSession; lang: Lang }) {
  const currentRevision = useRef(session.stateVersion);
  currentRevision.current = session.stateVersion;
  const [code, setCode] = useState('');
  const [error, setError] = useState('');
  const [importing, setImporting] = useState(false);
  const label = (key: Parameters<typeof workflowLabel>[1]) => workflowLabel(lang, key);
  const party = session.calcParams.party ?? { members: [], exports: {} };
  const update = (members: PartyMember[]) => session.updateParams({ party: { ...party, members } });
  const change = (id: string, patch: Partial<PartyMember>) => update(party.members.map(m => m.id === id ? { ...m, ...patch } : m));
  const add = (member?: PartyMember) => update([...party.members, { ...(member ?? { name: label('member'), enabled: true, blocks: {}, warnings: [] }), id: createId() }]);
  return <details className="party-editor" open>
    <summary>{label('party')} ({party.members.filter(m => m.enabled).length})</summary>
    <p>{label('partyHint')}</p>
    <label>{label('memberCode')}<AppTextarea disabled={session.busy || importing} aria-label={label('memberCode')} value={code} onChange={e => setCode(e.target.value)} /></label>
    <AppButton disabled={session.busy || importing || !code.trim()} onClick={async () => {
      setImporting(true); setError('');
      try { const backend = await getBackend(); const member = await backend.importPartyMember(code.trim()); if (currentRevision.current !== session.stateVersion) { setError(label('changed')); return; } add(member); setCode(''); }
      catch (e) { setError(String(e)); }
      finally { setImporting(false); }
    }}>{label('importMember')}</AppButton>
    <AppButton disabled={session.busy || importing} onClick={() => add()}>{label('addMember')}</AppButton>
    {error && <p role="alert">{error}</p>}
    {party.members.map(member => <fieldset key={member.id}>
      <legend>{member.name}</legend>
      <label>{label('name')}<AppInput disabled={session.busy || importing} aria-label={label('name')}
        key={member.name} defaultValue={member.name}
        onKeyDown={event => { if (event.key === 'Enter') event.currentTarget.blur(); }}
        onBlur={event => { if (event.target.value !== member.name) change(member.id, { name: event.target.value }); }} /></label>
      <label><AppInput disabled={session.busy || importing} aria-label={label('enabled')} type="checkbox" checked={member.enabled} onChange={e => change(member.id, { enabled: e.target.checked })} />{label('enabled')}</label>
      <AppButton disabled={session.busy || importing} onClick={() => update(party.members.filter(m => m.id !== member.id))}>{label('remove')}</AppButton>
      {member.warnings.map((warning, i) => <p key={i} role="status">{warning}</p>)}
      {destinations.map(destination => <details key={destination} open={!!member.blocks[destination]}>
        <summary>{label(destination)}</summary>
        <AppTextarea disabled={session.busy || importing} aria-label={`${member.name} ${label(destination)}`} rows={5}
          key={member.blocks[destination] ?? ''} defaultValue={member.blocks[destination] ?? ''}
          onBlur={event => {
            if (event.target.value !== (member.blocks[destination] ?? '')) {
              change(member.id, { blocks: { ...member.blocks, [destination]: event.target.value } });
            }
          }} />
      </details>)}
    </fieldset>)}
  </details>;
}
