import type { Lang } from './i18n';

/**
 * 玩家可读的 SkillType 白名单（按展示优先级排序）：[引擎词, en, zh-TW, zh-CN]。
 * 引擎共 ~177 个 SkillType，其余是内部机制词（CrossbowAmmoSkill 等），不展示。
 */
const TAG_LABELS: ReadonlyArray<[string, string, string, string]> = [
  ['Attack', 'Attack', '攻擊', '攻击'],
  ['Spell', 'Spell', '法術', '法术'],
  ['Melee', 'Melee', '近戰', '近战'],
  ['RangedAttack', 'Ranged', '遠程', '远程'],
  ['Projectile', 'Projectile', '投射物', '投射物'],
  ['Area', 'Area', '範圍', '范围'],
  ['Duration', 'Duration', '持續時間', '持续时间'],
  ['DamageOverTime', 'Damage over Time', '持續傷害', '持续伤害'],
  ['Physical', 'Physical', '物理', '物理'],
  ['Fire', 'Fire', '火焰', '火焰'],
  ['Cold', 'Cold', '冰冷', '冰冷'],
  ['Lightning', 'Lightning', '閃電', '闪电'],
  ['Chaos', 'Chaos', '混沌', '混沌'],
  ['Minion', 'Minion', '召喚物', '召唤物'],
  ['SummonsTotem', 'Totem', '圖騰', '图腾'],
  ['Buff', 'Buff', '增益', '增益'],
  ['Channel', 'Channelling', '引導', '引导'],
  ['Warcry', 'Warcry', '戰吼', '战吼'],
  ['AppliesCurse', 'Curse', '咒詛', '诅咒'],
  ['Bow', 'Bow', '弓', '弓'],
  ['Spear', 'Spear', '長矛', '长矛'],
  ['CrossbowSkill', 'Crossbow', '弩', '弩'],
  ['Slam', 'Slam', '猛擊', '猛击'],
  ['Nova', 'Nova', '新星', '新星'],
  ['Shapeshift', 'Shapeshift', '變形', '变形'],
  ['Persistent', 'Persistent', '常駐', '常驻'],
  ['Meta', 'Meta', '元技能', '元技能'],
];

const LANG_COLUMN: Record<Lang, 1 | 2 | 3> = { 'en-US': 1, 'zh-TW': 2, 'zh-CN': 3 };

/** 按白名单优先级返回本地化 tag 标签，最多 limit 个。 */
export function gemTagLabels(tags: string[], lang: Lang, limit = 4): string[] {
  const column = LANG_COLUMN[lang];
  const owned = new Set(tags);
  return TAG_LABELS.filter(([type]) => owned.has(type))
    .slice(0, limit)
    .map((row) => row[column]);
}

/** 搜索匹配：英文原词与所有语言的译名都可命中（中文用户看英文 BD 也能搜）。 */
export function gemTagMatches(tags: string[], q: string): boolean {
  if (tags.some((tag) => tag.toLowerCase().includes(q))) return true;
  const owned = new Set(tags);
  return TAG_LABELS.some(
    ([type, en, tw, cn]) =>
      owned.has(type) && (en.toLowerCase().includes(q) || tw.includes(q) || cn.includes(q)),
  );
}
