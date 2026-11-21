/**
 * Skill name localization hook: gem catalog localized name -> prettified ID fallback.
 * Shared by the sidebar main-skill selector and the Calcs skills view.
 */

import { useEffect, useMemo, useState } from 'react';
import { getBackend } from '../api/backend';
import type { GemCatalogEntry } from '../api/types';
import { skillDisplayName } from '../lib/skillNames';
import type { Lang } from '../lib/i18n';

export function useSkillCatalog(): ReadonlyMap<string, GemCatalogEntry> {
  const [catalog, setCatalog] = useState<GemCatalogEntry[]>([]);
  useEffect(() => {
    let cancelled = false;
    getBackend()
      .then((b) => b.gemCatalog())
      .then(entries => { if (!cancelled) setCatalog(entries); })
      .catch(() => {});
    return () => { cancelled = true; };
  }, []);
  return useMemo(() => new Map(catalog.map((e) => [e.skill_id, e])), [catalog]);
}

export function useSkillName(lang: Lang): (skillId: string) => string {
  const byId = useSkillCatalog();
  return (skillId: string) => skillDisplayName(skillId, lang, byId);
}
