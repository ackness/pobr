import { CONFIG_LABEL_ZH, CONFIG_LABEL_ZH_TW } from './configLabels';
/** UI copy only (src/locales/<lang>.json). Game names and modifier text use the versioned localization data. */

import { STAT_SECTIONS } from './statDisplay';
import { lookup, messages, translated, type Lang, type Messages } from '../locales';

export { LANGS, isLang, lookup, messages, translated, type Lang } from '../locales';

export type UiKey = keyof Messages['ui'];

/** 取 UI 文案。 */
export function t(lang: Lang, key: UiKey): string {
  return messages(lang, 'ui')[key];
}

/** 组件便捷绑定：`const tt = bindT(lang)`。 */
export function bindT(lang: Lang): (key: UiKey) => string {
  return (key) => t(lang, key);
}

/** Localize existing progress messages at the UI boundary; unknown diagnostics
 * retain their original text. No change to the session or backend contract. */
export function bootMessageLabel(lang: Lang, message: string): string {
  const exact = lookup(lang, 'bootMessage', message);
  if (exact) return exact;
  const progress = /^加载游戏数据 (\d+)\/(\d+)…$/.exec(message);
  if (progress) {
    const prefix = lang === 'en-US' ? 'Loading game data' : lang === 'zh-TW' ? '載入遊戲資料' : '加载游戏数据';
    return `${prefix} ${progress[1]}/${progress[2]}…`;
  }
  return message;
}

// ---------------------------------------------------------------------------
// 动态键标签（槽位 / 配置分区 / 敌人档位 / 聚合属性名）——键来自数据，
// 不进 ui 命名空间；查不到回退原键。
// ---------------------------------------------------------------------------

/** 装备槽稳定 id → 本地化槽位名。 */
export function slotLabel(lang: Lang, slotId: string): string {
  const utility = /^(flask|charm)([1-3])$/i.exec(slotId);
  const canonical = utility ? `${utility[1].toLowerCase() === 'flask' ? 'Flask' : 'Charm'} ${utility[2]}` : slotId;
  return lookup(lang, 'slot', canonical) ?? slotId;
}

/**
 * 附赠技能组来源标注（PoB2 自动生成的组：`source="Item:14:Plague Edge, Akoyan Spear"`
 * 或 `"Tree:11641"`）→ 简短徽标文本；玩家手动组（无 source）返回 null。
 * 同一技能多次出现多半来自这类附赠组——标注让重复行可解释。
 */
export function grantedSourceLabel(
  lang: Lang,
  source: string | null | undefined,
): string | null {
  if (!source) return null;
  const [kind, , detail] = source.split(':');
  if (kind === 'Item') {
    const prefix =
      lang === 'en-US' ? 'from item' : lang === 'zh-TW' ? '裝備附贈' : '装备附赠';
    const name = detail?.split(',')[0]?.trim();
    return name ? `${prefix} · ${name}` : prefix;
  }
  if (kind === 'Tree') {
    return lang === 'en-US' ? 'from tree' : lang === 'zh-TW' ? '天賦附贈' : '天赋附赠';
  }
  return source;
}

/** Config 分区名（数据原名）→ 本地化。 */
export function configSectionLabel(lang: Lang, section: string): string {
  return lookup(lang, 'configSection', section) ?? section;
}

/** 敌人档位 → 本地化。 */
export function enemyTierLabel(lang: Lang, tier: string): string {
  return lookup(lang, 'enemyTier', tier) ?? tier;
}

/** 词条来源类别（SourceKind 名）→ 本地化。 */
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
    const label = lang === 'zh-CN' ? CONFIG_LABEL_ZH[id] : lang === 'zh-TW' ? CONFIG_LABEL_ZH_TW[id] : undefined;
    return label ?? source;
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

/** 聚合属性名（breakdown 键 / 归因字段 / diff 列表）→ 本地化。
 * 回退链：breakdown 名表（modName）→ 侧栏展示目录标签 → 侧栏目录之外的
 * 补充译名（statExtra：diff 列表 / Calcs 分节里的长尾字段）→ detail → 原 id。 */
export function statNameLabel(lang: Lang, id: string): string {
  const utility = /^(Flask|Charm)([1-3])(.+)$/.exec(id);
  if (utility) {
    const [, kind, slot, field] = utility;
    const label = lookup(lang, 'detail', `UtilityField${field}`);
    if (label) {
      const name = lang === 'en-US' ? kind : kind === 'Flask' ? (lang === 'zh-TW' ? '藥劑' : '药剂') : (lang === 'zh-TW' ? '護符' : '护符');
      return `${name} ${slot} · ${label}`;
    }
  }
  const skillContribution = /^SkillGroup(\d+)Effect(\d+)DPS$/.exec(id);
  if (skillContribution) {
    const [, group, effect] = skillContribution;
    if (lang === 'zh-TW') return `技能組 ${group} · 效果 ${effect} DPS`;
    if (lang === 'zh-CN') return `技能组 ${group} · 效果 ${effect} DPS`;
    return `Skill group ${group} · effect ${effect} DPS`;
  }
  const fromMods = lookup(lang, 'modName', id);
  if (fromMods) return fromMods;
  for (const section of STAT_SECTIONS) {
    const row = section.rows.find((r) => r.id === id);
    if (row) return row.label[lang];
  }
  // statExtra only translates: English keeps reading detail, then the ID.
  const extra = translated(lang, 'statExtra', id);
  if (extra) return extra;
  return lookup(lang, 'detail', id) ?? id;
}

/** 伤害类型名（契约 `main_skill.hit_damage[].damage_type`）→ 本地化。 */
export function damageTypeLabel(lang: Lang, damageType: string): string {
  return lookup(lang, 'damageType', damageType) ?? damageType;
}

/** display_catalog 分类名 → 本地化（Calcs 分节标题）。 */
export function statCategoryLabel(lang: Lang, category: string): string {
  return lookup(lang, 'statCategory', category) ?? category;
}
