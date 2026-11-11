import type { GemCatalogEntry, UnmodelledMetaTrigger } from '../api/types';
import { bindT, type Lang } from './i18n';
import { skillDisplayName } from './skillNames';

export function unmodelledMetaTriggerNote(trigger: UnmodelledMetaTrigger, lang: Lang, catalog: ReadonlyMap<string, GemCatalogEntry>): string {
  return bindT(lang)('skills.unmodelledMetaTriggerNote').replace('{gem}', skillDisplayName(trigger.meta_gem_id, lang, catalog));
}
