/**
 * 侧边栏展示目录：display_catalog 字段 id → 分组 / 双语标签 / 格式 / 着色。
 * 纯展示映射，不含任何计算。
 */

import type { DisplayStatValue } from '../api/types';
import type { Lang } from './i18n';

export type { Lang };

export type StatFormat = 'int' | 'float2' | 'percent' | 'rate';

export interface StatRowDef {
  id: string;
  label: Record<Lang, string>;
  format: StatFormat;
  /** CSS 变量名（不带 var()），用于数值着色。 */
  colorVar?: string;
  /** 值为 0 时隐藏（DoT 等可选输出）。 */
  hideZero?: boolean;
}

export interface StatSectionDef {
  title: Record<Lang, string>;
  rows: StatRowDef[];
  requiresSkill?: boolean;
}

export const STAT_SECTIONS: StatSectionDef[] = [
  {
    title: { 'en-US': 'Offence', 'zh-TW': '攻擊', 'zh-CN': '攻击' },
    requiresSkill: true,
    rows: [
      { id: 'TotalDPS', label: { 'en-US': 'Total DPS', 'zh-TW': '總 DPS', 'zh-CN': '总 DPS' }, format: 'float2', colorVar: 'accent-gold-bright' },
      { id: 'TotalHitAvg', label: { 'en-US': 'Average Hit', 'zh-TW': '平均擊中', 'zh-CN': '平均击中' }, format: 'float2' },
      { id: 'ActionRate', label: { 'en-US': 'Attack/Cast Rate', 'zh-TW': '攻擊/施放速率', 'zh-CN': '攻击/施放速率' }, format: 'rate' },
      { id: 'HitChance', label: { 'en-US': 'Hit Chance', 'zh-TW': '命中率', 'zh-CN': '命中率' }, format: 'percent' },
      { id: 'CritChance', label: { 'en-US': 'Crit Chance', 'zh-TW': '暴擊率', 'zh-CN': '暴击率' }, format: 'percent' },
      { id: 'CritMultiplier', label: { 'en-US': 'Crit Multiplier', 'zh-TW': '暴擊加成', 'zh-CN': '暴击加成' }, format: 'float2' },
      { id: 'IgniteDPS', label: { 'en-US': 'Ignite DPS', 'zh-TW': '點燃 DPS', 'zh-CN': '点燃 DPS' }, format: 'float2', colorVar: 'color-fire', hideZero: true },
      { id: 'BleedDPS', label: { 'en-US': 'Bleed DPS', 'zh-TW': '流血 DPS', 'zh-CN': '流血 DPS' }, format: 'float2', colorVar: 'color-life', hideZero: true },
      { id: 'PoisonDPS', label: { 'en-US': 'Poison DPS', 'zh-TW': '中毒 DPS', 'zh-CN': '中毒 DPS' }, format: 'float2', colorVar: 'color-chaos', hideZero: true },
      { id: 'ManaCost', label: { 'en-US': 'Mana Cost', 'zh-TW': '魔力消耗', 'zh-CN': '魔力消耗' }, format: 'int', colorVar: 'color-mana' },
      { id: 'PreEffectiveCritChance', label: { 'en-US': 'Crit Chance before Accuracy', 'zh-TW': '命中確認前暴擊率', 'zh-CN': '命中确认前暴击率' }, format: 'percent', hideZero: true },
      { id: 'TotalDot', label: { 'en-US': 'Skill DoT DPS', 'zh-TW': '技能持續傷害 DPS', 'zh-CN': '技能持续伤害 DPS' }, format: 'float2', hideZero: true },
      { id: 'WithDotDPS', label: { 'en-US': 'Hit and Skill DoT DPS', 'zh-TW': '擊中與技能持續傷害 DPS', 'zh-CN': '击中与技能持续伤害 DPS' }, format: 'float2', hideZero: true },
      { id: 'CullingDPS', label: { 'en-US': 'Culling DPS', 'zh-TW': '撲殺 DPS', 'zh-CN': '终结 DPS' }, format: 'float2', hideZero: true },
    ],
  },
  {
    title: { 'en-US': 'Resources', 'zh-TW': '資源', 'zh-CN': '资源' },
    rows: [
      { id: 'Life', label: { 'en-US': 'Life', 'zh-TW': '生命', 'zh-CN': '生命' }, format: 'int', colorVar: 'color-life' },
      { id: 'Mana', label: { 'en-US': 'Mana', 'zh-TW': '魔力', 'zh-CN': '魔力' }, format: 'int', colorVar: 'color-mana' },
      { id: 'EnergyShield', label: { 'en-US': 'Energy Shield', 'zh-TW': '能量護盾', 'zh-CN': '能量护盾' }, format: 'int', colorVar: 'color-es' },
      { id: 'Spirit', label: { 'en-US': 'Spirit', 'zh-TW': '精魂', 'zh-CN': '精魂' }, format: 'int', colorVar: 'color-chaos' },
      { id: 'SpiritUnreserved', label: { 'en-US': 'Spirit Unreserved', 'zh-TW': '未保留精魂', 'zh-CN': '未保留精魂' }, format: 'int' },
      { id: 'Ward', label: { 'en-US': 'Ward', 'zh-TW': '結界', 'zh-CN': '结界' }, format: 'int', hideZero: true },
      { id: 'Rage', label: { 'en-US': 'Rage', 'zh-TW': '盛怒', 'zh-CN': '怒气' }, format: 'int', hideZero: true },
      { id: 'LifeUnreservedPercent', label: { 'en-US': 'Unreserved Life', 'zh-TW': '未保留生命', 'zh-CN': '未保留生命' }, format: 'percent' },
      { id: 'ManaUnreservedPercent', label: { 'en-US': 'Unreserved Mana', 'zh-TW': '未保留魔力', 'zh-CN': '未保留魔力' }, format: 'percent' },
      { id: 'SpiritUnreservedPercent', label: { 'en-US': 'Unreserved Spirit', 'zh-TW': '未保留精魂', 'zh-CN': '未保留精魂' }, format: 'percent' },
    ],
  },
  {
    title: { 'en-US': 'Defence', 'zh-TW': '防禦', 'zh-CN': '防御' },
    rows: [
      { id: 'Armour', label: { 'en-US': 'Armour', 'zh-TW': '護甲', 'zh-CN': '护甲' }, format: 'int', colorVar: 'color-defence' },
      { id: 'Evasion', label: { 'en-US': 'Evasion', 'zh-TW': '閃避', 'zh-CN': '闪避' }, format: 'int', colorVar: 'color-defence' },
      { id: 'PhysicalDamageReduction', label: { 'en-US': 'Phys. DR', 'zh-TW': '物理減傷', 'zh-CN': '物理减伤' }, format: 'percent' },
      { id: 'BlockChance', label: { 'en-US': 'Block Chance', 'zh-TW': '格擋率', 'zh-CN': '格挡率' }, format: 'percent', hideZero: true },
      { id: 'StunThreshold', label: { 'en-US': 'Stun Threshold', 'zh-TW': '暈眩門檻', 'zh-CN': '晕眩门槛' }, format: 'int' },
    ],
  },
  {
    title: { 'en-US': 'Resistances', 'zh-TW': '抗性', 'zh-CN': '抗性' },
    rows: [
      { id: 'FireResist', label: { 'en-US': 'Fire Res.', 'zh-TW': '火焰抗性', 'zh-CN': '火焰抗性' }, format: 'percent', colorVar: 'color-fire' },
      { id: 'ColdResist', label: { 'en-US': 'Cold Res.', 'zh-TW': '冰冷抗性', 'zh-CN': '冰冷抗性' }, format: 'percent', colorVar: 'color-cold' },
      { id: 'LightningResist', label: { 'en-US': 'Lightning Res.', 'zh-TW': '閃電抗性', 'zh-CN': '闪电抗性' }, format: 'percent', colorVar: 'color-lightning' },
      { id: 'ChaosResist', label: { 'en-US': 'Chaos Res.', 'zh-TW': '混沌抗性', 'zh-CN': '混沌抗性' }, format: 'percent', colorVar: 'color-chaos' },
      { id: 'FireResistOverCap', label: { 'en-US': 'Fire Res. Over Max', 'zh-TW': '超過上限火焰抗性', 'zh-CN': '超过上限火焰抗性' }, format: 'percent', hideZero: true },
      { id: 'ColdResistOverCap', label: { 'en-US': 'Cold Res. Over Max', 'zh-TW': '超過上限冰冷抗性', 'zh-CN': '超过上限冰冷抗性' }, format: 'percent', hideZero: true },
      { id: 'LightningResistOverCap', label: { 'en-US': 'Lightning Res. Over Max', 'zh-TW': '超過上限閃電抗性', 'zh-CN': '超过上限闪电抗性' }, format: 'percent', hideZero: true },
      { id: 'ChaosResistOverCap', label: { 'en-US': 'Chaos Res. Over Max', 'zh-TW': '超過上限混沌抗性', 'zh-CN': '超过上限混沌抗性' }, format: 'percent', hideZero: true },
    ],
  },
  {
    title: { 'en-US': 'Survivability', 'zh-TW': '生存', 'zh-CN': '生存' },
    rows: [
      { id: 'TotalEHP', label: { 'en-US': 'Effective HP', 'zh-TW': '有效生命', 'zh-CN': '有效生命' }, format: 'int' },
      { id: 'PhysicalMaxHit', label: { 'en-US': 'Phys. Max Hit', 'zh-TW': '物理最大承受', 'zh-CN': '物理最大承受' }, format: 'int' },
      { id: 'FireMaxHit', label: { 'en-US': 'Fire Max Hit', 'zh-TW': '火焰最大承受', 'zh-CN': '火焰最大承受' }, format: 'int', colorVar: 'color-fire' },
      { id: 'ColdMaxHit', label: { 'en-US': 'Cold Max Hit', 'zh-TW': '冰冷最大承受', 'zh-CN': '冰冷最大承受' }, format: 'int', colorVar: 'color-cold' },
      { id: 'LightningMaxHit', label: { 'en-US': 'Lightning Max Hit', 'zh-TW': '閃電最大承受', 'zh-CN': '闪电最大承受' }, format: 'int', colorVar: 'color-lightning' },
      { id: 'ChaosMaxHit', label: { 'en-US': 'Chaos Max Hit', 'zh-TW': '混沌最大承受', 'zh-CN': '混沌最大承受' }, format: 'int', colorVar: 'color-chaos' },
    ],
  },
  {
    title: { 'en-US': 'Attributes', 'zh-TW': '屬性', 'zh-CN': '属性' },
    rows: [
      { id: 'Str', label: { 'en-US': 'Strength', 'zh-TW': '力量', 'zh-CN': '力量' }, format: 'int', hideZero: true },
      { id: 'Dex', label: { 'en-US': 'Dexterity', 'zh-TW': '敏捷', 'zh-CN': '敏捷' }, format: 'int', hideZero: true },
      { id: 'Int', label: { 'en-US': 'Intelligence', 'zh-TW': '智慧', 'zh-CN': '智慧' }, format: 'int', hideZero: true },
    ],
  },
  {
    title: { 'en-US': 'Skill mechanics', 'zh-TW': '技能機制', 'zh-CN': '技能机制' },
    requiresSkill: true,
    rows: [
      { id: 'StoredUses', label: { 'en-US': 'Stored Uses', 'zh-TW': '儲存使用次數', 'zh-CN': '储存使用次数' }, format: 'int', hideZero: true },
      { id: 'FiringRate', label: { 'en-US': 'Firing Rate', 'zh-TW': '射擊速率', 'zh-CN': '射击速率' }, format: 'rate', hideZero: true },
      { id: 'ReloadTime', label: { 'en-US': 'Reload Time (s)', 'zh-TW': '裝填時間（秒）', 'zh-CN': '装填时间（秒）' }, format: 'float2', hideZero: true },
      { id: 'ProjectileSpeedMod', label: { 'en-US': 'Projectile Speed Multiplier', 'zh-TW': '投射物速度倍率', 'zh-CN': '投射物速度倍率' }, format: 'float2', hideZero: true },
    ],
  },
];

/** 数值格式化（null = 计算侧的 ∞ / 不适用）。 */
export function formatStatValue(value: number | null, format: StatFormat): string {
  if (value === null || !Number.isFinite(value)) return '∞';
  switch (format) {
    case 'int':
      return Math.round(value).toLocaleString('en-US');
    case 'percent':
      return `${roundTo(value, 1)}%`;
    case 'rate':
      return roundTo(value, 2).toFixed(2);
    case 'float2':
      return value >= 1000
        ? Math.round(value).toLocaleString('en-US')
        : roundTo(value, 2).toString();
  }
}

function roundTo(value: number, digits: number): number {
  const scale = 10 ** digits;
  return Math.round(value * scale) / scale;
}

/** stats 数组 → id 索引。 */
export function statMap(stats: DisplayStatValue[]): Map<string, number | null> {
  return new Map(stats.map((s) => [s.id, s.value]));
}
