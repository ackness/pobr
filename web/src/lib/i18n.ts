import { configLabelTranslation } from './configLabels';
/** UI copy only (src/locales/<lang>.json). Game names and modifier text use the versioned localization data. */

import { STAT_SECTIONS, statRowLabel } from './statDisplay';
import { lookup, messages, translated, type Lang, type Messages } from '../locales';

export { LANGS, isLang, lookup, messages, translated, type Lang } from '../locales';

export type UiKey = keyof Messages['ui'];
type MessageValues = Readonly<Record<string, string | number>>;

/** Replace named placeholders once, without interpreting text inside the inserted values. */
export function formatMessage(template: string, values: MessageValues): string {
  return template.replace(/\{(\w+)\}/g, (match, key: string) => Object.hasOwn(values, key) ? String(values[key]) : match);
}

/** Look up UI copy. */
export function t(lang: Lang, key: UiKey, values?: MessageValues): string {
  const template = messages(lang, 'ui')[key];
  if (template === undefined) return key;
  return values ? formatMessage(template, values) : template;
}

/** Convenience component binder: `const tt = bindT(lang)`. */
export function bindT(lang: Lang): (key: UiKey, values?: MessageValues) => string {
  return (key, values) => t(lang, key, values);
}

/** Localize existing progress messages at the UI boundary; unknown diagnostics
 * retain their original text. No change to the session or backend contract. */
export function bootMessageLabel(lang: Lang, message: string): string {
  const exact = lookup(lang, 'bootMessage', message);
  if (exact) return exact;
  const progress = /^加载游戏数据 (\d+)\/(\d+)…$/.exec(message);
  if (progress) {
    return t(lang, 'boot.loadingGameData', { loaded: progress[1], total: progress[2] });
  }
  return message;
}

/** Localize structured backend boot errors on the UI thread; raw diagnostics stay intact. */
export function bootErrorLabel(lang: Lang, error: unknown): string {
  const raw = error instanceof Error ? error.message : String(error);
  const json = raw.startsWith('Error: ') ? raw.slice('Error: '.length) : raw;
  if (json.startsWith('{')) {
    try {
      const parsed = JSON.parse(json) as {
        bootError?: unknown;
        error?: unknown;
        expected?: unknown;
        actual?: unknown;
      };
      if (parsed.bootError === 'wasmNotBuilt' && typeof parsed.error === 'string') {
        return t(lang, 'bootError.wasmNotBuilt', { error: parsed.error });
      }
      if (
        parsed.bootError === 'schemaMismatch'
        && typeof parsed.expected === 'number'
        && typeof parsed.actual === 'number'
      ) {
        return t(lang, 'bootError.schemaMismatch', {
          expected: String(parsed.expected),
          actual: String(parsed.actual),
        });
      }
    } catch {
      // Not a structured boot error; preserve the raw diagnostic below.
    }
  }
  return String(error);
}

// ---------------------------------------------------------------------------
// Dynamic keyed labels (slots, config sections, enemy tiers, aggregated stats) —
// keys come from game data, outside `ui`; unknown keys fall back verbatim.
// ---------------------------------------------------------------------------

/** Canonical equipment slot id -> localized slot label. */
export function slotLabel(lang: Lang, slotId: string): string {
  const utility = /^(flask|charm)([1-3])$/i.exec(slotId);
  const canonical = utility ? `${utility[1].toLowerCase() === 'flask' ? 'Flask' : 'Charm'} ${utility[2]}` : slotId;
  return lookup(lang, 'slot', canonical) ?? slotId;
}

/**
 * Granted skill-group source badge (PoB2 auto-generated groups such as
 * `source="Item:14:Plague Edge, Akoyan Spear"` or `"Tree:11641"`) -> short badge label;
 * manual groups without a source return null. Duplicate skills usually come from granted sources.
 */
export function grantedSourceLabel(
  lang: Lang,
  source: string | null | undefined,
): string | null {
  if (!source) return null;
  const [kind, , detail] = source.split(':');
  if (kind === 'Item') {
    const name = detail?.split(',')[0]?.trim();
    return name ? t(lang, 'granted.fromNamedItem', { name }) : t(lang, 'granted.fromItem');
  }
  if (kind === 'Tree') {
    return t(lang, 'granted.fromTree');
  }
  return source;
}

/** Config section name (raw catalog name) -> localized label. */
export function configSectionLabel(lang: Lang, section: string): string {
  return lookup(lang, 'configSection', section) ?? section;
}

/** Enemy tier -> localized label. */
export function enemyTierLabel(lang: Lang, tier: string): string {
  return lookup(lang, 'enemyTier', tier) ?? tier;
}

/** Modifier origin category (`SourceKind` name) -> localized label. */
export function originKindLabel(lang: Lang, kind: string): string {
  return lookup(lang, 'originKind', kind) ?? kind;
}

/** Display only; accepts JSON enum names and uppercase trace labels. */
export function modTypeLabel(lang: Lang, type: string): string {
  return lookup(lang, 'modType', type.toUpperCase()) ?? type;
}

/** Localize structural source IDs, keeping unknown suffixes available verbatim. */
export function originLabel(lang: Lang, source: string): string {
  const [kind, id, ...suffix] = source.split('.');
  if (kind === 'item' && id) {
    const affixKind: Record<string, string> = { explicit: 'ItemAffix', implicit: 'ItemImplicit', enchant: 'ItemEnchant', quality: 'ItemQuality' };
    return [slotLabel(lang, id), ...suffix.map(part => affixKind[part] ? originKindLabel(lang, affixKind[part]) : part)].join(' · ');
  }
  if (kind === 'config' && id) {
    return configLabelTranslation(lang, id) ?? source;
  }
  if (kind === 'flask' && id) return [slotLabel(lang, id), ...suffix].join(' · ');
  if ((kind === 'node' || kind === 'jewel') && id) {
    return `${originKindLabel(lang, kind === 'node' ? 'PassiveNode' : 'Jewel')} ${[id, ...suffix].join('.')}`;
  }
  if (kind === 'base' && id) {
    const alias: Record<string, string> = { MaximumLife: 'Life', MaximumMana: 'Mana' };
    const label = statNameLabel(lang, alias[id] ?? id);
    return `${originKindLabel(lang, 'CharacterBase')} · ${label}`;
  }
  return source;
}

/** Aggregated stat name (breakdown key, attribution field, or diff row) -> localized label.
 * Fallback chain: breakdown names (`modName`) -> sidebar display catalog label -> supplementary
 * translations outside sidebar (`statExtra`, long-tail diff/Calcs entries) -> `detail` -> raw ID. */
export function statNameLabel(lang: Lang, id: string): string {
  const utility = /^(Flask|Charm)([1-3])(.+)$/.exec(id);
  if (utility) {
    const [, kind, slot, field] = utility;
    const label = lookup(lang, 'detail', `UtilityField${field}`);
    if (label) {
      return t(lang, kind === 'Flask' ? 'stat.flaskMetric' : 'stat.charmMetric', { slot, label });
    }
  }
  const skillContribution = /^SkillGroup(\d+)Effect(\d+)DPS$/.exec(id);
  if (skillContribution) {
    const [, group, effect] = skillContribution;
    return t(lang, 'stat.skillContribution', { group, effect });
  }
  const fromMods = lookup(lang, 'modName', id);
  if (fromMods) return fromMods;
  for (const section of STAT_SECTIONS) {
    const row = section.rows.find((r) => r.id === id);
    if (row) return statRowLabel(lang, row.id);
  }
  // statExtra only translates: English keeps reading detail, then the ID.
  const extra = translated(lang, 'statExtra', id);
  if (extra) return extra;
  return lookup(lang, 'detail', id) ?? id;
}

/** Damage type name (contract `main_skill.hit_damage[].damage_type`) -> localized label. */
export function damageTypeLabel(lang: Lang, damageType: string): string {
  return lookup(lang, 'damageType', damageType) ?? damageType;
}

/** `display_catalog` category name -> localized label (Calcs section heading). */
export function statCategoryLabel(lang: Lang, category: string): string {
  return lookup(lang, 'statCategory', category) ?? category;
}
