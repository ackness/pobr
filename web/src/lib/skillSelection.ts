import type { GemCatalogEntry, SkillMetricsEntry, SocketGroupInput } from '../api/types';

/** PoB ordinals count all active effects, not gems or display/stat-set aliases. */
export function activeSkillEffects(group: SocketGroupInput, catalog: ReadonlyMap<string, Pick<GemCatalogEntry, 'active_effect_ids'>>) {
  return group.gems.flatMap((gem, gemIndex) => {
    if (!gem.skill_id) return [];
    const entry = catalog.get(gem.skill_id);
    const ids = entry ? entry.active_effect_ids : [gem.skill_id];
    return ids.map(skillId => ({ skillId, gemIndex }));
  });
}

/** Upgrade comparisons retain the exact effect, even when supports move or grant extra effects. */
export function preserveSkillSelection(group: SocketGroupInput, next: SocketGroupInput,
  catalog: ReadonlyMap<string, Pick<GemCatalogEntry, 'active_effect_ids'>>): SocketGroupInput | null {
  const effects = activeSkillEffects(group, catalog);
  const selectedIndex = (group.main_active_skill ?? 1) - 1;
  const selected = effects[selectedIndex];
  if (!selected) return null;
  const selectedGem = group.gems[selected.gemIndex];
  const preservedIndex = next.gems.indexOf(selectedGem);
  const gemIndex = preservedIndex >= 0 ? preservedIndex
    : next.gems[selected.gemIndex]?.skill_id === selectedGem.skill_id ? selected.gemIndex
    : next.gems.findIndex(gem => gem.skill_id === selectedGem.skill_id);
  if (gemIndex < 0) return null;
  const nextEffects = activeSkillEffects(next, catalog);
  const effectOffset = effects.slice(0, selectedIndex).filter(effect => effect.gemIndex === selected.gemIndex).length;
  const firstEffect = nextEffects.findIndex(effect => effect.gemIndex === gemIndex);
  const nextIndex = firstEffect + effectOffset;
  if (firstEffect < 0 || nextEffects[nextIndex]?.gemIndex !== gemIndex
    || nextEffects[nextIndex]?.skillId !== selected.skillId) return null;
  return { ...next, main_active_skill: nextIndex === selectedIndex ? group.main_active_skill : nextIndex + 1 };
}

/** Preserve the selected effect when removing another gem, including its granted effects. */
export function removeSkillGem(group: SocketGroupInput, gemIndex: number, catalog: ReadonlyMap<string, GemCatalogEntry>): SocketGroupInput {
  const effects = activeSkillEffects(group, catalog);
  const selected = (group.main_active_skill ?? 1) - 1;
  const remaining = effects.filter(effect => effect.gemIndex !== gemIndex);
  const selectedEffect = effects[selected];
  let mainActiveSkill = group.main_active_skill;
  if (selectedEffect) {
    const nextIndex = remaining.indexOf(selectedEffect);
    if (nextIndex >= 0) mainActiveSkill = nextIndex + 1;
    else {
      const preceding = effects.slice(0, selected).filter(effect => effect.gemIndex !== gemIndex).length;
      mainActiveSkill = remaining.length ? Math.max(1, preceding) : undefined;
    }
  }
  return { ...group, gems: group.gems.filter((_, index) => index !== gemIndex), main_active_skill: mainActiveSkill };
}

/**
 * A gem with its corrupted state set. The field is present only on a corrupted
 * gem, so an untouched gem and one toggled back compare and save the same.
 */
export function withGemCorrupted<Gem extends { corrupted?: boolean }>(gem: Gem, corrupted: boolean): Gem {
  const { corrupted: _state, ...rest } = gem;
  return (corrupted ? { ...rest, corrupted: true } : rest) as Gem;
}

/**
 * The default inspected skill when nothing is selected: the highest combined
 * (hit + damage over time) DPS among calculated effects of enabled groups.
 * Ties and all-zero builds keep the earliest row, so the choice is stable.
 */
export function highestDpsSkill(
  skills: SkillMetricsEntry[],
  groups: ReadonlyArray<{ enabled?: boolean } | undefined>,
): SkillMetricsEntry | null {
  let best: SkillMetricsEntry | null = null;
  for (const entry of skills) {
    if (entry.status !== 'calculated' || !entry.metrics || !groups[entry.group_index]?.enabled) continue;
    if (!best || entry.metrics.combined_dps > best.metrics!.combined_dps) best = entry;
  }
  return best;
}
