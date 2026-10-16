/**
 * 侧边栏展示目录：display_catalog 字段 id → 分组 / 双语标签 / 格式 / 着色。
 * 纯展示映射，不含任何计算。
 */

import type { DisplayStatValue } from '../api/types';
import type { Lang } from './i18n';

export type { Lang };

export type StatFormat = 'int' | 'float2' | 'percent' | 'rate' | 'modifier' | 'pierce' | 'chanceMultiplier';

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
    title: { 'en-US': 'Included skills', 'zh-TW': '納入的技能', 'zh-CN': '纳入的技能' },
    rows: [
      { id: 'FullDPS', label: { 'en-US': 'Full DPS', 'zh-TW': '完整 DPS', 'zh-CN': '完整 DPS' }, format: 'float2', colorVar: 'accent-gold-bright' },
      { id: 'FullDotDPS', label: { 'en-US': 'Full DoT DPS', 'zh-TW': '完整持續傷害 DPS', 'zh-CN': '完整持续伤害 DPS' }, format: 'float2' },
    ],
  },
  {
    title: { 'en-US': 'Offence', 'zh-TW': '攻擊', 'zh-CN': '攻击' },
    requiresSkill: true,
    rows: [
      { id: 'DecayDPS', label: { 'en-US': 'Profane Decay DPS', 'zh-TW': '褻瀆腐朽每秒傷害', 'zh-CN': '亵渎腐朽每秒伤害' }, format: 'float2', hideZero: true },
      { id: 'MirageBurningGroundDPS', label: { 'en-US': 'Mirage Ignited Ground DPS', 'zh-TW': '幻影燃燒地面每秒傷害', 'zh-CN': '幻影燃烧地面每秒伤害' }, format: 'float2', hideZero: true },
      { id: 'ReservationDPS', label: { 'en-US': 'Reservation DPS', 'zh-TW': '保留等效傷害', 'zh-CN': '保留等效伤害' }, format: 'float2', hideZero: true },
      { id: 'MirageDPS', label: { 'en-US': 'Mirage DPS', 'zh-TW': '幻影每秒傷害', 'zh-CN': '幻影每秒伤害' }, format: 'float2', hideZero: true },
      { id: 'BurningGroundDPS', label: { 'en-US': 'Ignited / Burning Ground DPS', 'zh-TW': '燃燒地面每秒傷害', 'zh-CN': '燃烧地面每秒伤害' }, format: 'float2', hideZero: true },
      { id: 'ExplodeChance', label: { 'en-US': 'Chance to Explode Enemies', 'zh-TW': '敵人爆炸機率', 'zh-CN': '敌人爆炸几率' }, format: 'percent', hideZero: true },
      { id: 'CombinedAvgToMonsterLife', label: { 'en-US': 'Enemy Life Equivalent', 'zh-TW': '敵人生命等值', 'zh-CN': '敌人生命等值' }, format: 'percent', hideZero: true },
      { id: 'CritBifurcates', label: { 'en-US': 'Double Critical Roll Chance', 'zh-TW': '雙重暴擊機率', 'zh-CN': '双重暴击几率' }, format: 'chanceMultiplier', hideZero: true },
      { id: 'AverageDamage', label: { 'en-US': 'Average Damage', 'zh-TW': '平均傷害', 'zh-CN': '平均伤害' }, format: 'float2', hideZero: true },
      { id: 'AverageBurstDamage', label: { 'en-US': 'Average Burst Damage', 'zh-TW': '平均爆發傷害', 'zh-CN': '平均爆发伤害' }, format: 'float2', hideZero: true },
      { id: 'CombinedAvg', label: { 'en-US': 'Combined Average', 'zh-TW': '合計平均傷害', 'zh-CN': '合计平均伤害' }, format: 'float2', hideZero: true },
      { id: 'HitSpeed', label: { 'en-US': 'Hit Rate', 'zh-TW': '命中速率', 'zh-CN': '命中速率' }, format: 'rate', hideZero: true },
      { id: 'IgniteDamage', label: { 'en-US': 'Ignite Damage', 'zh-TW': '點燃傷害', 'zh-CN': '点燃伤害' }, format: 'float2', hideZero: true },
      { id: 'BleedDamage', label: { 'en-US': 'Bleed Damage', 'zh-TW': '流血傷害', 'zh-CN': '流血伤害' }, format: 'float2', hideZero: true },
      { id: 'PoisonDamage', label: { 'en-US': 'Poison Damage', 'zh-TW': '中毒傷害', 'zh-CN': '中毒伤害' }, format: 'float2', hideZero: true },
      { id: 'WithIgniteDPS', label: { 'en-US': 'DPS including Ignite', 'zh-TW': '包含點燃 DPS', 'zh-CN': '包含点燃 DPS' }, format: 'float2', hideZero: true },
      { id: 'WithBleedDPS', label: { 'en-US': 'DPS including Bleed', 'zh-TW': '包含流血 DPS', 'zh-CN': '包含流血 DPS' }, format: 'float2', hideZero: true },
      { id: 'WithPoisonDPS', label: { 'en-US': 'DPS including Poison', 'zh-TW': '包含中毒 DPS', 'zh-CN': '包含中毒 DPS' }, format: 'float2', hideZero: true },
      { id: 'MainHandAccuracy', label: { 'en-US': 'Main Hand Accuracy', 'zh-TW': '主手命中值', 'zh-CN': '主手命中值' }, format: 'int', hideZero: true },
      { id: 'OffHandAccuracy', label: { 'en-US': 'Off Hand Accuracy', 'zh-TW': '副手命中值', 'zh-CN': '副手命中值' }, format: 'int', hideZero: true },
      { id: 'AccuracyHitChanceUncapped', label: { 'en-US': 'Uncapped Chance to Hit', 'zh-TW': '無上限命中率', 'zh-CN': '无上限命中率' }, format: 'percent', hideZero: true },
      { id: 'QuantityMultiplier', label: { 'en-US': 'Quantity Multiplier', 'zh-TW': '數量倍率', 'zh-CN': '数量倍率' }, format: 'float2', hideZero: true },
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
      { id: 'EffectiveMovementSpeedMod', label: { 'en-US': 'Movement Speed Modifier', 'zh-TW': '移動速度倍率', 'zh-CN': '移动速度倍率' }, format: 'modifier', hideZero: true },
      { id: 'PresenceRadiusMetres', label: { 'en-US': 'Presence Radius (m)', 'zh-TW': '存在範圍半徑（公尺）', 'zh-CN': '在场半径（米）' }, format: 'float2', hideZero: true },
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
    title: { 'en-US': 'Recovery', 'zh-TW': '恢復', 'zh-CN': '恢复' },
    rows: [
      { id: 'LifeRegenRecovery', label: { 'en-US': 'Life Regeneration and Recovery per Second', 'zh-TW': '每秒生命再生與恢復', 'zh-CN': '每秒生命再生与恢复' }, format: 'float2', hideZero: true },
      { id: 'ManaRegenRecovery', label: { 'en-US': 'Mana Regeneration and Recovery per Second', 'zh-TW': '每秒魔力再生與恢復', 'zh-CN': '每秒魔力再生与恢复' }, format: 'float2', hideZero: true },
      { id: 'EnergyShieldRegenRecovery', label: { 'en-US': 'Energy Shield Regeneration and Recovery per Second', 'zh-TW': '每秒能量護盾再生與恢復', 'zh-CN': '每秒能量护盾再生与恢复' }, format: 'float2', hideZero: true },
      { id: 'WardRegenRecovery', label: { 'en-US': 'Runic Ward Regeneration and Recovery per Second', 'zh-TW': '每秒符文結界再生與恢復', 'zh-CN': '每秒符文结界再生与恢复' }, format: 'float2', hideZero: true },
      { id: 'RageRegenRecovery', label: { 'en-US': 'Rage Regeneration and Recovery per Second', 'zh-TW': '每秒盛怒再生與恢復', 'zh-CN': '每秒怒气再生与恢复' }, format: 'float2', hideZero: true },
      { id: 'NetLifeRegen', label: { 'en-US': 'Net Life Recovery per Second', 'zh-TW': '每秒生命淨恢復', 'zh-CN': '每秒生命净恢复' }, format: 'float2', hideZero: true },
      { id: 'NetManaRegen', label: { 'en-US': 'Net Mana Recovery per Second', 'zh-TW': '每秒魔力淨恢復', 'zh-CN': '每秒魔力净恢复' }, format: 'float2', hideZero: true },
      { id: 'NetWardRegen', label: { 'en-US': 'Net Runic Ward Regeneration', 'zh-TW': '符文守護淨再生', 'zh-CN': '符文守护净再生' }, format: 'float2', hideZero: true },
      { id: 'NetEnergyShieldRegen', label: { 'en-US': 'Net Energy Shield Recovery per Second', 'zh-TW': '每秒能量護盾淨恢復', 'zh-CN': '每秒能量护盾净恢复' }, format: 'float2', hideZero: true },
      { id: 'TotalNetRegen', label: { 'en-US': 'Total Net Recovery per Second', 'zh-TW': '每秒總淨恢復', 'zh-CN': '每秒总净恢复' }, format: 'float2', hideZero: true },
      { id: 'TotalBuildDegen', label: { 'en-US': 'Build Resource Loss per Second', 'zh-TW': '每秒構築資源損失', 'zh-CN': '每秒构筑资源损失' }, format: 'float2', hideZero: true },
      { id: 'LifeLeechGainRate', label: { 'en-US': 'Life Leech and Gain on Hit per Second', 'zh-TW': '每秒生命偷取與擊中獲得', 'zh-CN': '每秒生命偷取与击中获得' }, format: 'float2', hideZero: true },
      { id: 'ManaLeechGainRate', label: { 'en-US': 'Mana Leech and Gain on Hit per Second', 'zh-TW': '每秒魔力偷取與擊中獲得', 'zh-CN': '每秒魔力偷取与击中获得' }, format: 'float2', hideZero: true },
      { id: 'EnergyShieldLeechGainRate', label: { 'en-US': 'Energy Shield Leech and Gain on Hit per Second', 'zh-TW': '每秒能量護盾偷取與擊中獲得', 'zh-CN': '每秒能量护盾偷取与击中获得' }, format: 'float2', hideZero: true },
      { id: 'LifeLeechGainPerHit', label: { 'en-US': 'Life Leeched and Gained per Hit', 'zh-TW': '每次擊中偷取與獲得生命', 'zh-CN': '每次击中偷取与获得生命' }, format: 'float2', hideZero: true },
      { id: 'ManaLeechGainPerHit', label: { 'en-US': 'Mana Leeched and Gained per Hit', 'zh-TW': '每次擊中偷取與獲得魔力', 'zh-CN': '每次击中偷取与获得魔力' }, format: 'float2', hideZero: true },
      { id: 'EnergyShieldLeechGainPerHit', label: { 'en-US': 'Energy Shield Leeched and Gained per Hit', 'zh-TW': '每次擊中偷取與獲得能量護盾', 'zh-CN': '每次击中偷取与获得能量护盾' }, format: 'float2', hideZero: true },
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
      { id: 'MovementSpeedWhileUsingSkill', label: { 'en-US': 'Skill Movement Speed', 'zh-TW': '技能移動速度', 'zh-CN': '技能移动速度' }, format: 'modifier', hideZero: true },
      { id: 'AuraEffectMod', label: { 'en-US': 'Aura Magnitude Multiplier', 'zh-TW': '光環幅度倍率', 'zh-CN': '光环幅度倍率' }, format: 'float2', hideZero: true },
      { id: 'CurseEffectMod', label: { 'en-US': 'Curse Magnitude Multiplier', 'zh-TW': '詛咒幅度倍率', 'zh-CN': '诅咒幅度倍率' }, format: 'float2', hideZero: true },
      { id: 'ActiveMinionLimit', label: { 'en-US': 'Active Minion Limit', 'zh-TW': '召喚物上限', 'zh-CN': '召唤物上限' }, format: 'int', hideZero: true },
      { id: 'Duration', label: { 'en-US': 'Skill Duration (s)', 'zh-TW': '技能持續時間（秒）', 'zh-CN': '技能持续时间（秒）' }, format: 'float2', hideZero: true },
      { id: 'DurationSecondary', label: { 'en-US': 'Secondary Duration (s)', 'zh-TW': '次要持續時間（秒）', 'zh-CN': '次要持续时间（秒）' }, format: 'float2', hideZero: true },
      { id: 'AuraDuration', label: { 'en-US': 'Aura Duration (s)', 'zh-TW': '光環持續時間（秒）', 'zh-CN': '光环持续时间（秒）' }, format: 'float2', hideZero: true },
      { id: 'ReserveDuration', label: { 'en-US': 'Reservation Duration (s)', 'zh-TW': '保留持續時間（秒）', 'zh-CN': '保留持续时间（秒）' }, format: 'float2', hideZero: true },
      { id: 'AreaOfEffectRadiusMetres', label: { 'en-US': 'Area Radius (m)', 'zh-TW': '範圍半徑（公尺）', 'zh-CN': '范围半径（米）' }, format: 'float2', hideZero: true },
      { id: 'PierceCount', label: { 'en-US': 'Pierce Count', 'zh-TW': '穿透次數', 'zh-CN': '穿透次数' }, format: 'pierce', hideZero: true },
      { id: 'ForkCount', label: { 'en-US': 'Fork Count', 'zh-TW': '分岔次數', 'zh-CN': '分岔次数' }, format: 'float2', hideZero: true },
      { id: 'ChainMax', label: { 'en-US': 'Maximum Chains', 'zh-TW': '最大連鎖次數', 'zh-CN': '最大连锁次数' }, format: 'float2', hideZero: true },
      { id: 'SplitCount', label: { 'en-US': 'Split Count', 'zh-TW': '分裂數量', 'zh-CN': '分裂数量' }, format: 'float2', hideZero: true },
      { id: 'BounceCount', label: { 'en-US': 'Bounce Count', 'zh-TW': '彈跳次數', 'zh-CN': '弹跳次数' }, format: 'float2', hideZero: true },
      { id: 'ReqStr', label: { 'en-US': 'Strength Required', 'zh-TW': '力量需求', 'zh-CN': '力量需求' }, format: 'int', hideZero: true },
      { id: 'ReqDex', label: { 'en-US': 'Dexterity Required', 'zh-TW': '敏捷需求', 'zh-CN': '敏捷需求' }, format: 'int', hideZero: true },
      { id: 'ReqInt', label: { 'en-US': 'Intelligence Required', 'zh-TW': '智慧需求', 'zh-CN': '智慧需求' }, format: 'int', hideZero: true },
      { id: 'ManaPercentCost', label: { 'en-US': 'Mana Cost (%)', 'zh-TW': '魔力消耗（%）', 'zh-CN': '魔力消耗（%）' }, format: 'percent', hideZero: true },
      { id: 'ManaPercentPerSecondCost', label: { 'en-US': 'Mana Cost (%/s)', 'zh-TW': '魔力每秒消耗（%）', 'zh-CN': '魔力每秒消耗（%）' }, format: 'percent', hideZero: true },
      { id: 'ManaPerSecondCost', label: { 'en-US': 'Mana Cost / s', 'zh-TW': '魔力每秒消耗', 'zh-CN': '魔力每秒消耗' }, format: 'float2', hideZero: true },
      { id: 'LifePercentCost', label: { 'en-US': 'Life Cost (%)', 'zh-TW': '生命消耗（%）', 'zh-CN': '生命消耗（%）' }, format: 'percent', hideZero: true },
      { id: 'LifePercentPerSecondCost', label: { 'en-US': 'Life Cost (%/s)', 'zh-TW': '生命每秒消耗（%）', 'zh-CN': '生命每秒消耗（%）' }, format: 'percent', hideZero: true },
      { id: 'LifePerSecondCost', label: { 'en-US': 'Life Cost / s', 'zh-TW': '生命每秒消耗', 'zh-CN': '生命每秒消耗' }, format: 'float2', hideZero: true },
      { id: 'ESCost', label: { 'en-US': 'Energy Shield Cost', 'zh-TW': '能量護盾消耗', 'zh-CN': '能量护盾消耗' }, format: 'float2', hideZero: true },
      { id: 'ESPercentCost', label: { 'en-US': 'Energy Shield Cost (%)', 'zh-TW': '能量護盾消耗（%）', 'zh-CN': '能量护盾消耗（%）' }, format: 'percent', hideZero: true },
      { id: 'ESPercentPerSecondCost', label: { 'en-US': 'Energy Shield Cost (%/s)', 'zh-TW': '能量護盾每秒消耗（%）', 'zh-CN': '能量护盾每秒消耗（%）' }, format: 'percent', hideZero: true },
      { id: 'ESPerSecondCost', label: { 'en-US': 'Energy Shield Cost / s', 'zh-TW': '能量護盾每秒消耗', 'zh-CN': '能量护盾每秒消耗' }, format: 'float2', hideZero: true },
      { id: 'WardCost', label: { 'en-US': 'Runic Ward Cost', 'zh-TW': '符文守護消耗', 'zh-CN': '符文守护消耗' }, format: 'float2', hideZero: true },
      { id: 'WardPercentCost', label: { 'en-US': 'Runic Ward Cost (%)', 'zh-TW': '符文守護消耗（%）', 'zh-CN': '符文守护消耗（%）' }, format: 'percent', hideZero: true },
      { id: 'WardPercentPerSecondCost', label: { 'en-US': 'Runic Ward Cost (%/s)', 'zh-TW': '符文守護每秒消耗（%）', 'zh-CN': '符文守护每秒消耗（%）' }, format: 'percent', hideZero: true },
      { id: 'WardPerSecondCost', label: { 'en-US': 'Runic Ward Cost / s', 'zh-TW': '符文守護每秒消耗', 'zh-CN': '符文守护每秒消耗' }, format: 'float2', hideZero: true },
      { id: 'RageCost', label: { 'en-US': 'Rage Cost', 'zh-TW': '盛怒消耗', 'zh-CN': '怒气消耗' }, format: 'float2', hideZero: true },
      { id: 'RagePerSecondCost', label: { 'en-US': 'Rage Cost / s', 'zh-TW': '盛怒每秒消耗', 'zh-CN': '怒气每秒消耗' }, format: 'float2', hideZero: true },
      { id: 'TotalTotemPlacementTime', label: { 'en-US': 'Totem Placement Time (s)', 'zh-TW': '圖騰放置時間（秒）', 'zh-CN': '图腾放置时间（秒）' }, format: 'float2', hideZero: true },
      { id: 'TrapThrowingTime', label: { 'en-US': 'Trap Throwing Time (s)', 'zh-TW': '陷阱投擲時間（秒）', 'zh-CN': '陷阱投掷时间（秒）' }, format: 'float2', hideZero: true },
      { id: 'TrapThrowCount', label: { 'en-US': 'Traps per Throw', 'zh-TW': '每次投擲陷阱數', 'zh-CN': '每次投掷陷阱数' }, format: 'float2', hideZero: true },
      { id: 'MineLayingTime', label: { 'en-US': 'Mine Throwing Time (s)', 'zh-TW': '地雷投擲時間（秒）', 'zh-CN': '地雷投掷时间（秒）' }, format: 'float2', hideZero: true },
      { id: 'MineThrowCount', label: { 'en-US': 'Mines per Throw', 'zh-TW': '每次投擲地雷數', 'zh-CN': '每次投掷地雷数' }, format: 'float2', hideZero: true },
      { id: 'WarcryCastTime', label: { 'en-US': 'Warcry Time (s)', 'zh-TW': '戰吼施放時間（秒）', 'zh-CN': '战吼施放时间（秒）' }, format: 'float2', hideZero: true },
      { id: 'ChannelTimeToTrigger', label: { 'en-US': 'Channel Time to Trigger (s)', 'zh-TW': '觸發前引導時間（秒）', 'zh-CN': '触发前引导时间（秒）' }, format: 'float2', hideZero: true },
      { id: 'ChannelTime', label: { 'en-US': 'Channel Time (s)', 'zh-TW': '引導時間（秒）', 'zh-CN': '引导时间（秒）' }, format: 'float2', hideZero: true },
      { id: 'StoredUses', label: { 'en-US': 'Stored Uses', 'zh-TW': '儲存使用次數', 'zh-CN': '储存使用次数' }, format: 'int', hideZero: true },
      { id: 'FiringRate', label: { 'en-US': 'Firing Rate', 'zh-TW': '射擊速率', 'zh-CN': '射击速率' }, format: 'rate', hideZero: true },
      { id: 'ReloadTime', label: { 'en-US': 'Reload Time (s)', 'zh-TW': '裝填時間（秒）', 'zh-CN': '装填时间（秒）' }, format: 'float2', hideZero: true },
      { id: 'ProjectileSpeedMod', label: { 'en-US': 'Projectile Speed Multiplier', 'zh-TW': '投射物速度倍率', 'zh-CN': '投射物速度倍率' }, format: 'float2', hideZero: true },
    ],
  },
];

/** 数值格式化（null = 计算侧的 ∞ / 不适用）。 */
export function formatStatValue(value: number | null, format: StatFormat, lang: Lang = 'en-US'): string {
  if (value === null || !Number.isFinite(value)) return '∞';
  switch (format) {
    case 'chanceMultiplier':
      return `${roundTo(Math.max(0, value - 1) * 100, 1)}%`;
    case 'modifier':
      return `${value >= 1 ? '+' : ''}${roundTo((value - 1) * 100, 1)}%`;
    case 'pierce':
      return value >= 100 ? ({ 'en-US': 'All targets', 'zh-TW': '所有目標', 'zh-CN': '所有目标' }[lang]) : Math.round(value).toLocaleString('en-US');
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
