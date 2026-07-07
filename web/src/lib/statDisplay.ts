/**
 * 侧边栏展示目录：display_catalog 字段 id → 分组 / 双语标签 / 格式 / 着色。
 * 纯展示映射，不含任何计算。
 */

import type { DisplayStatValue } from '../api/types';

export type Lang = 'en-US' | 'zh-TW';

export type StatFormat = 'int' | 'float2' | 'percent' | 'rate';

export interface StatRowDef {
  id: string;
  label: { 'en-US': string; 'zh-TW': string };
  format: StatFormat;
  /** CSS 变量名（不带 var()），用于数值着色。 */
  colorVar?: string;
  /** 值为 0 时隐藏（DoT 等可选输出）。 */
  hideZero?: boolean;
}

export interface StatSectionDef {
  title: { 'en-US': string; 'zh-TW': string };
  rows: StatRowDef[];
}

export const STAT_SECTIONS: StatSectionDef[] = [
  {
    title: { 'en-US': 'Offence', 'zh-TW': '攻擊' },
    rows: [
      { id: 'TotalDPS', label: { 'en-US': 'Total DPS', 'zh-TW': '總 DPS' }, format: 'float2', colorVar: 'accent-gold-bright' },
      { id: 'TotalHitAvg', label: { 'en-US': 'Average Hit', 'zh-TW': '平均擊中' }, format: 'float2' },
      { id: 'ActionRate', label: { 'en-US': 'Attack/Cast Rate', 'zh-TW': '攻擊/施放速率' }, format: 'rate' },
      { id: 'HitChance', label: { 'en-US': 'Hit Chance', 'zh-TW': '命中率' }, format: 'percent' },
      { id: 'CritChance', label: { 'en-US': 'Crit Chance', 'zh-TW': '暴擊率' }, format: 'percent' },
      { id: 'CritMultiplier', label: { 'en-US': 'Crit Multiplier', 'zh-TW': '暴擊加成' }, format: 'float2' },
      { id: 'IgniteDPS', label: { 'en-US': 'Ignite DPS', 'zh-TW': '點燃 DPS' }, format: 'float2', colorVar: 'color-fire', hideZero: true },
      { id: 'BleedDPS', label: { 'en-US': 'Bleed DPS', 'zh-TW': '流血 DPS' }, format: 'float2', colorVar: 'color-life', hideZero: true },
      { id: 'PoisonDPS', label: { 'en-US': 'Poison DPS', 'zh-TW': '中毒 DPS' }, format: 'float2', colorVar: 'color-chaos', hideZero: true },
      { id: 'ManaCost', label: { 'en-US': 'Mana Cost', 'zh-TW': '魔力消耗' }, format: 'int', colorVar: 'color-mana' },
    ],
  },
  {
    title: { 'en-US': 'Resources', 'zh-TW': '資源' },
    rows: [
      { id: 'Life', label: { 'en-US': 'Life', 'zh-TW': '生命' }, format: 'int', colorVar: 'color-life' },
      { id: 'Mana', label: { 'en-US': 'Mana', 'zh-TW': '魔力' }, format: 'int', colorVar: 'color-mana' },
      { id: 'EnergyShield', label: { 'en-US': 'Energy Shield', 'zh-TW': '能量護盾' }, format: 'int', colorVar: 'color-es' },
      { id: 'Spirit', label: { 'en-US': 'Spirit', 'zh-TW': '精魂' }, format: 'int', colorVar: 'color-chaos' },
      { id: 'SpiritUnreserved', label: { 'en-US': 'Spirit Unreserved', 'zh-TW': '未保留精魂' }, format: 'int' },
      { id: 'Ward', label: { 'en-US': 'Ward', 'zh-TW': '結界' }, format: 'int', hideZero: true },
    ],
  },
  {
    title: { 'en-US': 'Defence', 'zh-TW': '防禦' },
    rows: [
      { id: 'Armour', label: { 'en-US': 'Armour', 'zh-TW': '護甲' }, format: 'int', colorVar: 'color-defence' },
      { id: 'Evasion', label: { 'en-US': 'Evasion', 'zh-TW': '閃避' }, format: 'int', colorVar: 'color-defence' },
      { id: 'PhysicalDamageReduction', label: { 'en-US': 'Phys. DR', 'zh-TW': '物理減傷' }, format: 'percent' },
      { id: 'BlockChance', label: { 'en-US': 'Block Chance', 'zh-TW': '格擋率' }, format: 'percent', hideZero: true },
      { id: 'StunThreshold', label: { 'en-US': 'Stun Threshold', 'zh-TW': '暈眩門檻' }, format: 'int' },
    ],
  },
  {
    title: { 'en-US': 'Resistances', 'zh-TW': '抗性' },
    rows: [
      { id: 'FireResist', label: { 'en-US': 'Fire Res.', 'zh-TW': '火焰抗性' }, format: 'percent', colorVar: 'color-fire' },
      { id: 'ColdResist', label: { 'en-US': 'Cold Res.', 'zh-TW': '冰冷抗性' }, format: 'percent', colorVar: 'color-cold' },
      { id: 'LightningResist', label: { 'en-US': 'Lightning Res.', 'zh-TW': '閃電抗性' }, format: 'percent', colorVar: 'color-lightning' },
      { id: 'ChaosResist', label: { 'en-US': 'Chaos Res.', 'zh-TW': '混沌抗性' }, format: 'percent', colorVar: 'color-chaos' },
    ],
  },
  {
    title: { 'en-US': 'Survivability', 'zh-TW': '生存' },
    rows: [
      { id: 'TotalEHP', label: { 'en-US': 'Effective HP', 'zh-TW': '有效生命' }, format: 'int' },
      { id: 'PhysicalMaxHit', label: { 'en-US': 'Phys. Max Hit', 'zh-TW': '物理最大承受' }, format: 'int' },
      { id: 'FireMaxHit', label: { 'en-US': 'Fire Max Hit', 'zh-TW': '火焰最大承受' }, format: 'int', colorVar: 'color-fire' },
      { id: 'ColdMaxHit', label: { 'en-US': 'Cold Max Hit', 'zh-TW': '冰冷最大承受' }, format: 'int', colorVar: 'color-cold' },
      { id: 'LightningMaxHit', label: { 'en-US': 'Lightning Max Hit', 'zh-TW': '閃電最大承受' }, format: 'int', colorVar: 'color-lightning' },
      { id: 'ChaosMaxHit', label: { 'en-US': 'Chaos Max Hit', 'zh-TW': '混沌最大承受' }, format: 'int', colorVar: 'color-chaos' },
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
