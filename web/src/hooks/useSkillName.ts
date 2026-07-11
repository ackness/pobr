/**
 * 技能名本地化 hook：宝石目录中文名 → 回退 id 美化。
 * 侧边栏主技能下拉与 Calcs 页技能视图共用。
 */

import { useEffect, useMemo, useState } from 'react';
import { getBackend } from '../api/backend';
import type { GemCatalogEntry } from '../api/types';
import { gemDisplayName } from '../components/skills/GemPicker';
import { prettySkillId } from '../components/skills/SkillsPanel';
import type { Lang } from '../lib/i18n';

export function useSkillName(lang: Lang): (skillId: string) => string {
  const [catalog, setCatalog] = useState<GemCatalogEntry[]>([]);
  useEffect(() => {
    getBackend()
      .then((b) => b.gemCatalog())
      .then(setCatalog)
      .catch(() => {});
  }, []);
  const byId = useMemo(() => new Map(catalog.map((e) => [e.skill_id, e])), [catalog]);
  return (skillId: string) => {
    const entry = byId.get(skillId);
    return entry ? gemDisplayName(entry, lang) : prettySkillId(skillId);
  };
}
