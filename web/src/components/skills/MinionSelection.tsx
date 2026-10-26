import { AppInput } from '../shared/controls';
import { useEffect, useState } from 'react';
import { getBackend } from '../../api/backend';
import type { MinionCatalog } from '../../api/types';
import type { BuildSession } from '../../hooks/useBuildSession';
import { bindT, type Lang } from '../../lib/i18n';
import { AppSelect } from '../shared/AppSelect';
import './skills.css';

let catalogPromise: Promise<MinionCatalog> | undefined;
export function MinionSelection({ session, lang, groupIndex, gemIndex, gemName }: {
  session: BuildSession; lang: Lang; groupIndex: number; gemIndex: number; gemName: string;
}) {
  const tt = bindT(lang);
  const [catalog, setCatalog] = useState<MinionCatalog>();
  const [failed, setFailed] = useState(false);
  const [query, setQuery] = useState('');
  useEffect(() => {
    let active = true;
    catalogPromise ??= getBackend().then(backend => backend.minionCatalog()).catch(error => { catalogPromise = undefined; throw error; });
    catalogPromise.then(value => { if (active) setCatalog(value); }).catch(() => { if (active) setFailed(true); });
    return () => { active = false; };
  }, []);
  const group = session.socketGroups[groupIndex];
  const gem = group?.gems[gemIndex];
  const summoner = catalog?.summoners.find(row => row.skill_id === gem?.skill_id);
  if (!gem || !summoner) return failed ? <p className="opt-error">{tt('minion.catalogError')}</p> : null;
  const allowed = catalog!.monsters.filter(monster => summoner.minion_ids.includes(monster.id)
    || summoner.choice === 'spectre' && monster.spectre
    || summoner.choice === 'beast' && monster.spectre && monster.beast);
  const id = gem.minion ?? summoner.minion_ids[0];
  const monster = catalog!.monsters.find(row => row.id === id);
  const choices = monster && !allowed.some(row => row.id === monster.id) ? [monster, ...allowed] : allowed;
  const selectedSkill = Math.min(Math.max(gem.minion_skill ?? 1, 1), monster?.skills.length || 1);
  const update = (patch: { minion?: string; minion_skill?: number }) => session.setSocketGroups(session.socketGroups.map((item, index) => index === groupIndex
    ? { ...item, gems: item.gems.map((value, indexGem) => indexGem === gemIndex ? { ...value, ...patch } : value) } : item));
  const visible = choices.filter(row => row.id === id || row.name.toLowerCase().includes(query.trim().toLowerCase()));
  return <div className="minion-controls" role="group" aria-label={`${tt('minion.title')}: ${gemName}`}>
    <h4>{gemName} · {tt('minion.title')}</h4>
    {choices.length > 8 && <AppInput type="search" value={query} placeholder={tt('minion.search')} aria-label={tt('minion.search')}
      onChange={event => setQuery(event.target.value)} />}
    <label>{tt('minion.identity')}<AppSelect ariaLabel={tt('minion.identity')} value={id ?? ''}
      placeholder={id ? tt('minion.unavailable') : tt('minion.choose')} disabled={session.busy}
      options={visible.map(row => ({ value: row.id, label: row.name }))}
      onChange={value => { update({ minion: value, minion_skill: undefined }); setQuery(''); }} /></label>
    {monster && (monster.skills.length ? <label>{tt('minion.skill')}<AppSelect ariaLabel={tt('minion.skill')}
      value={String(selectedSkill)} disabled={session.busy}
      options={monster.skills.map(skill => ({ value: String(skill.index), label: `${skill.index} · ${skill.name || tt('minion.skillFallback')}` }))}
      onChange={value => update({ minion_skill: Number(value) })} /></label> : <p>{tt('minion.noSkills')}</p>)}
  </div>;
}
