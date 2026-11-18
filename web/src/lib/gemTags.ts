import { LANGS, messages, type Lang, type Messages } from '../locales';

// Player-facing skill types in display order; internal mechanics are not shown.
const TAGS: readonly (keyof Messages['gemTag'])[] = [
  'Attack',
  'Spell',
  'Melee',
  'RangedAttack',
  'Projectile',
  'Area',
  'Duration',
  'DamageOverTime',
  'Physical',
  'Fire',
  'Cold',
  'Lightning',
  'Chaos',
  'Minion',
  'SummonsTotem',
  'Buff',
  'Channel',
  'Warcry',
  'AppliesCurse',
  'Bow',
  'Spear',
  'CrossbowSkill',
  'Slam',
  'Nova',
  'Shapeshift',
  'Persistent',
  'Meta',
];

export function gemTagLabels(tags: string[], lang: Lang, limit = 4): string[] {
  const owned = new Set(tags);
  const labels = messages(lang, 'gemTag');
  return TAGS.filter(type => owned.has(type)).slice(0, limit).map(type => labels[type]);
}

/** Search the raw skill types and every registered locale, not just the displayed labels. */
export function gemTagMatches(tags: string[], q: string): boolean {
  if (tags.some(tag => tag.toLowerCase().includes(q))) return true;
  const owned = new Set(tags);
  return TAGS.some(type => owned.has(type)
    && LANGS.some(lang => messages(lang, 'gemTag')[type].toLowerCase().includes(q)));
}
