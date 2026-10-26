import { useEffect, useRef, useState } from 'react';
import { createRoot } from 'react-dom/client';
import { AppButton, AppInput, AppSelectControl, AppTextarea } from '../../src/components/shared/controls';
import { AppSelect } from '../../src/components/shared/AppSelect';
import { AppDialogsProvider, useAppDialogs } from '../../src/components/shared/AppDialogs';
import { UiProvider } from '../../src/components/shared/UiContext';
import { useSessionAction } from '../../src/components/shared/useSessionAction';
import { GemPicker } from '../../src/components/skills/GemPicker';
import type { BuildSession } from '../../src/hooks/useBuildSession';
import type { GemCatalogEntry } from '../../src/api/types';
import type { Lang } from '../../src/lib/i18n';
import '../../src/styles/global.css';
import '../../src/components/skills/skills.css';

const entries: GemCatalogEntry[] = [
  { skill_id: 'fire', name: 'Fireball', name_zh_cn: '火球', name_zh_tw: '火球', colour: 'int', tags: ['Spell', 'Projectile'] },
  { skill_id: 'ice', name: 'Ice Nova', name_zh_cn: '冰霜新星', name_zh_tw: '冰霜新星', colour: 'int', tags: ['Spell'] },
  { skill_id: 'lightning', name: 'Spark', name_zh_cn: '电球', name_zh_tw: '電球', colour: 'int', tags: ['Spell'] },
].map(entry => ({ ...entry, is_support: false, is_lineage: false, active_effect_ids: [entry.skill_id] })) as GemCatalogEntry[];

function Fixture() {
  const query = new URLSearchParams(location.search);
  const v2 = query.get('v2') !== 'false';
  const lang = (query.get('lang') ?? 'en-US') as Lang;
  const [scope, setScope] = useState('stage-a');
  const [revision, setRevision] = useState(0);
  const [result, setResult] = useState('idle');
  useEffect(() => {
    const change = () => setScope(value => value === 'stage-a' ? 'stage-b' : 'stage-a');
    const edit = () => setRevision(value => value + 1);
    document.addEventListener('fixture-stage', change);
    document.addEventListener('fixture-edit', edit);
    return () => { document.removeEventListener('fixture-stage', change); document.removeEventListener('fixture-edit', edit); };
  }, []);
  useEffect(() => { document.getElementById('root')!.className = v2 ? 'v2-root' : ''; }, [v2]);
  return <UiProvider v2={v2} lang={lang}><AppDialogsProvider scope={scope}>
    <Controls lang={lang} revision={revision} report={setResult} />
    <output aria-label="Result">{result}</output>
    <span aria-label="Stage">{scope}</span>
  </AppDialogsProvider></UiProvider>;
}
function Controls({ lang, revision, report }: { lang: Lang; revision: number; report: (value: string) => void }) {
  const dialogs = useAppDialogs(lang);
  const capture = useSessionAction({ stateVersion: revision } as BuildSession);
  const [selected, setSelected] = useState('');
  const [choice, setChoice] = useState('a');
  const [checked, setChecked] = useState(false);
  const [gem, setGem] = useState('none');
  const file = useRef<HTMLInputElement>(null);
  const text = useRef<HTMLTextAreaElement>(null);
  return <main style={{ padding: 24, display: 'grid', gap: 16, maxWidth: 700 }}>
    <AppButton onClick={async () => { const valid = capture(); const yes = await dialogs.confirm('Remove this item?'); report(yes && valid() ? 'removed' : 'cancelled'); }}>Remove</AppButton>
    <AppButton onClick={async () => { const value = await dialogs.prompt('Loadout name', 'Original'); report(value ?? 'cancelled'); }}>Rename</AppButton>
    <AppButton title="Delete the gem" aria-label="Delete the gem">×</AppButton>
    <label>Enabled<AppInput id="enabled" type="checkbox" name="enabled" checked={checked}
      onChange={event => { setChecked(event.target.checked); report(`${event.currentTarget instanceof HTMLInputElement}:${event.currentTarget.id}:${event.currentTarget.checked}`); }} /></label>
    <form onSubmit={event => { event.preventDefault(); report(String(new FormData(event.currentTarget).get('slot'))); }}>
      <label>Slot
      <AppSelectControl id="slot" name="slot" value={selected} onChange={event => { setSelected(event.target.value); report(`${event.currentTarget instanceof HTMLSelectElement}:${event.currentTarget.value}`); }}>
        <option value="">None</option><optgroup label="Gear"><option value="ring">Ring</option><option value="shield" disabled>Shield</option></optgroup>
      </AppSelectControl></label>
      <AppButton type="submit">Submit slot</AppButton>
    </form>
    <AppSelect ariaLabel="Choice" value={choice} onChange={setChoice} options={[{value: 'a', label: 'Alpha'}, {value: 'b', label: 'Beta', hint: 'Second option'}]} />
    <AppInput type="file" ref={file} hidden aria-label="File" onChange={event => report(event.target.files?.[0]?.name ?? 'empty')} />
    <AppButton onClick={() => file.current?.click()}>Choose file</AppButton>
    <AppTextarea ref={text} aria-label="Notes" />
    <AppButton onClick={() => text.current?.focus()}>Focus notes</AppButton>
    <GemPicker entries={entries} placeholder="Find a gem" disabled={false} lang={lang} onPick={setGem} />
    <output aria-label="Gem">{gem}</output>
  </main>;
}
createRoot(document.getElementById('root')!).render(<Fixture />);
