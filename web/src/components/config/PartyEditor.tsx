import { useRef, useState } from 'react';
import { getBackend } from '../../api/backend';
import type { PartyMember } from '../../api/types';
import type { BuildSession } from '../../hooks/useBuildSession';

const destinations = ['PartyMemberStats', 'Aura', 'Curse', 'Warcry Skills', 'Link Skills', 'EnemyConditions', 'EnemyMods'];

export function PartyEditor({ session, lang }: { session: BuildSession; lang: string }) {
  const currentRevision = useRef(session.stateVersion);
  currentRevision.current = session.stateVersion;
  const [code, setCode] = useState('');
  const [error, setError] = useState('');
  const [importing, setImporting] = useState(false);
  const zh = lang !== 'en-US';
  const party = session.calcParams.party ?? { members: [], exports: {} };
  const update = (members: PartyMember[]) => session.updateParams({ party: { ...party, members } });
  const change = (id: string, patch: Partial<PartyMember>) => update(party.members.map(m => m.id === id ? { ...m, ...patch } : m));
  const add = (member?: PartyMember) => update([...party.members, { ...(member ?? { name: zh ? '队友' : 'Party member', enabled: true, blocks: {}, warnings: [] }), id: crypto.randomUUID() }]);
  return <details className="party-editor">
    <summary>{zh ? '队伍' : 'Party'} ({party.members.filter(m => m.enabled).length})</summary>
    <p>{zh ? '队友的光环、诅咒和其他效果会计入计算。每个文本块保留导出时的效果强度。' : 'Party auras, curses and other effects contribute to calculations. Blocks keep the effect strength from their export.'}</p>
    <label>{zh ? '队友构筑码' : 'Member build code'}<textarea aria-label="Member build code" value={code} onChange={e => setCode(e.target.value)} /></label>
    <button disabled={importing || !code.trim()} onClick={async () => {
      setImporting(true); setError('');
      try { const backend = await getBackend(); const member = await backend.importPartyMember(code.trim()); if (currentRevision.current !== session.stateVersion) { setError(zh ? '构筑已更改，请重新导入。' : 'The build changed. Import the member again.'); return; } add(member); setCode(''); }
      catch (e) { setError(String(e)); }
      finally { setImporting(false); }
    }}>{zh ? '导入队友' : 'Import member'}</button>
    <button onClick={() => add()}>{zh ? '添加队友' : 'Add member'}</button>
    {error && <p role="alert">{error}</p>}
    {party.members.map(member => <fieldset key={member.id}>
      <legend>{member.name}</legend>
      <label>{zh ? '名称' : 'Name'}<input aria-label="Member name" value={member.name} onChange={e => change(member.id, { name: e.target.value })} /></label>
      <label><input aria-label="Enable member" type="checkbox" checked={member.enabled} onChange={e => change(member.id, { enabled: e.target.checked })} />{zh ? '启用' : 'Enabled'}</label>
      <button onClick={() => update(party.members.filter(m => m.id !== member.id))}>{zh ? '移除队友' : 'Remove member'}</button>
      {member.warnings.map((warning, i) => <p key={i} role="status">{warning}</p>)}
      {destinations.map(destination => <details key={destination} open={!!member.blocks[destination]}>
        <summary>{destination}</summary>
        <textarea aria-label={`${member.name} ${destination}`} rows={5} value={member.blocks[destination] ?? ''}
          onChange={e => change(member.id, { blocks: { ...member.blocks, [destination]: e.target.value } })} />
      </details>)}
    </fieldset>)}
  </details>;
}
