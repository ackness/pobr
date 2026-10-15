import type { GemCatalogEntry } from '../../api/types';
import type { BuildSession } from '../../hooks/useBuildSession';
import { bindT, type Lang } from '../../lib/i18n';
import { activeSkillEffects } from '../../lib/skillSelection';
import { AppSelect } from '../shared/AppSelect';
import './skills.css';

/** A form belongs to the gem supplying the selected active effect, not to its ordinal. */
export function SkillFormControls({ session, lang, groupIndex, catalog }: {
  session: BuildSession; lang: Lang; groupIndex: number; catalog: ReadonlyMap<string, GemCatalogEntry>;
}) {
  const tt = bindT(lang);
  const group = session.socketGroups[groupIndex];
  const effect = group && activeSkillEffects(group, catalog)[(group.main_active_skill ?? 1) - 1];
  const gem = effect && group.gems[effect.gemIndex];
  const forms = gem && catalog.get(gem.skill_id)?.effect_forms?.find(row => row.skill_id === effect.skillId)?.stat_sets;
  if (!gem || !forms?.length) return null;
  const form = forms.find(row => row.index === (gem.stat_set_index ?? forms[0].index)) ?? forms[0];
  const stage = form.stage;
  const raw = stage && session.calcParams.config_inputs[stage.config_input];
  return <div className="skill-form-controls" onClick={event => event.stopPropagation()}>
    {forms.length > 1 && <label>{tt('skills.form')}
      <AppSelect ariaLabel={tt('skills.form')} value={String(gem.stat_set_index ?? forms[0].index)}
        options={forms.map(row => ({ value: String(row.index), label: row.label || `${tt('skills.defaultForm')} ${row.index}` }))}
        disabled={session.busy} onChange={value => session.setSocketGroups(session.socketGroups.map((item, index) => index === groupIndex
          ? { ...item, gems: item.gems.map((valueGem, indexGem) => indexGem === effect.gemIndex ? { ...valueGem, stat_set_index: Number(value) } : valueGem) } : item))} />
    </label>}
    {stage && <label>{tt('skills.stages')}
      <input type="number" min={stage.min} max={stage.max} step={1} disabled={session.busy}
        aria-label={tt('skills.stages')} value={typeof raw === 'number' ? raw : ''} placeholder={String(stage.default)}
        onChange={event => {
          const value = event.target.value;
          if (!value) session.setConfigInput(stage.config_input, null);
          else if (Number.isInteger(Number(value)) && Number(value) >= stage.min && Number(value) <= stage.max)
            session.setConfigInput(stage.config_input, Number(value));
        }} />
      {typeof raw === 'number' && <button disabled={session.busy} onClick={() => session.setConfigInput(stage.config_input, null)}>{tt('skills.stageDefault')}</button>}
    </label>}
  </div>;
}
