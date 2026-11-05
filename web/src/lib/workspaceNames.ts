import type { BuildStage, LocalBuild } from './buildWorkspace';
import type { Lang } from './i18n';
import { workspaceText } from './workspaceText';
import type { ClassNames } from '../api/types';
import { classDisplayName } from './nativeDisplayNames';

/** Display fallbacks never overwrite a player's saved names. */
export function buildDisplayName(build: Pick<LocalBuild, 'name'> | undefined, index: number, lang: Lang): string {
  return build?.name || workspaceText(lang).initialBuild.replace('{number}', String(index + 1));
}

export function stageDisplayName(stage: Pick<BuildStage, 'name'> | undefined, index: number, lang: Lang): string {
  return stage?.name || workspaceText(lang).initialStage.replace('{number}', String(index + 1));
}

export function importedBuildName(character: { class_name: string; ascendancy_name: string },
  names: ClassNames, lang: Lang, suppliedName?: string, plannerName?: string): string {
  return suppliedName?.trim() || plannerName?.trim()
    || classDisplayName(names, character.ascendancy_name || character.class_name, lang);
}
