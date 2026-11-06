import type { BuildSession } from '../hooks/useBuildSession';
import { bindT, type Lang } from '../lib/i18n';

/** newBuild resets only the active stage. Describe its current contents before calling it. */
export function classChangeMessage(session: BuildSession, lang: Lang, stageName: string, className: string): string {
  const tt = bindT(lang);
  const lines = [tt('build.classResetLevel')];
  const count = (key: Parameters<typeof tt>[0], value: number) => {
    if (value) lines.push(tt(key).replace('{count}', String(value)));
  };
  if (session.character?.ascendancy_name) lines.push(tt('build.classResetAscendancy'));
  count('build.classResetPassives', session.allocatedNodes.length);
  count('build.classResetSkills', session.socketGroups.length);
  const build = session.workspace?.builds.find(entry => entry.id === session.workspace?.activeBuild);
  const saved = build?.stages.find(stage => stage.id === build.activeStage)?.saved.state;
  count('build.classResetItems', session.items.length + session.flasks.length + session.limbs.length + session.jewels.length
    + (saved?.equipmentJewels?.length ?? 0) + (session.weaponSwap?.alternate_items.length ?? 0));
  if (session.loadouts.length) lines.push(tt('build.classResetLoadouts'));
  if (session.notes || Object.keys(session.annotations).length) lines.push(tt('build.classResetNotes'));
  if (Object.keys(session.calcParams).length) lines.push(tt('build.classResetConfig'));
  if (Object.keys(session.editorDrafts).length) lines.push(tt('build.classResetDrafts'));
  return `${tt('build.confirmClassReset').replace('{class}', className)}\n${stageName}${lang === 'en-US' ? ':' : '：'}\n${lines.map(line => `• ${line}`).join('\n')}`;
}
