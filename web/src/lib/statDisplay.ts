/** Sidebar presentation metadata keyed by engine stat ID; no calculation. */

import type { DisplayCatalogEntry, DisplayStatValue } from '../api/types';
import generatedCatalog from '../api/generated/display_catalog.json' with { type: 'json' };
import { lookup, messages, type Lang, type Messages } from '../locales';

export type { Lang };

export type StatFormat = 'int' | 'float2' | 'percent' | 'rate' | 'modifier' | 'pierce' | 'chanceMultiplier';

export interface StatRowDef {
  id: string;
  /** Numeric precision or a multiplier conversion; percentage units come from the catalog. */
  format?: Exclude<StatFormat, 'percent'>;
  /** CSS variable name (without `var()`) for numeric tinting. */
  colorVar?: string;
  /** Hide when value is 0 (for optional outputs like DoT). */
  hideZero?: boolean;
}

export interface StatSectionDef {
  /** Stable storage key for the sidebar's collapsed sections. */
  id: string;
  titleKey: keyof Messages['statSection'];
  rows: StatRowDef[];
  requiresSkill?: boolean;
  /** The player's own damage: hidden when a minion carries the build and the player deals none. */
  playerDamage?: boolean;
}

export function statRowLabel(lang: Lang, id: string): string {
  return lookup(lang, 'statRow', id) ?? lookup(lang, 'modName', id) ?? id;
}

export function statSectionLabel(lang: Lang, section: StatSectionDef): string {
  return messages(lang, 'statSection')[section.titleKey];
}

export const STAT_SECTIONS: StatSectionDef[] = [
  {
    id: 'Included skills',
    titleKey: 'includedSkills',
    rows: [
      { id: 'FullDPS', format: 'float2', colorVar: 'accent-gold-bright' },
      { id: 'FullDotDPS', format: 'float2' },
    ],
  },
  {
    id: 'Offence',
    titleKey: 'offence',
    requiresSkill: true,
    playerDamage: true,
    rows: [
      { id: 'DecayDPS', format: 'float2', hideZero: true },
      { id: 'MirageBurningGroundDPS', format: 'float2', hideZero: true },
      { id: 'ReservationDPS', format: 'float2', hideZero: true },
      { id: 'MirageDPS', format: 'float2', hideZero: true },
      { id: 'BurningGroundDPS', format: 'float2', hideZero: true },
      { id: 'ExplodeChance', hideZero: true },
      { id: 'CombinedAvgToMonsterLife', hideZero: true },
      { id: 'CritBifurcates', format: 'chanceMultiplier', hideZero: true },
      { id: 'AverageDamage', format: 'float2', hideZero: true },
      { id: 'AverageBurstDamage', format: 'float2', hideZero: true },
      { id: 'CombinedAvg', format: 'float2', hideZero: true },
      { id: 'HitSpeed', format: 'rate', hideZero: true },
      { id: 'IgniteDamage', format: 'float2', hideZero: true },
      { id: 'BleedDamage', format: 'float2', hideZero: true },
      { id: 'PoisonDamage', format: 'float2', hideZero: true },
      { id: 'WithIgniteDPS', format: 'float2', hideZero: true },
      { id: 'WithBleedDPS', format: 'float2', hideZero: true },
      { id: 'WithPoisonDPS', format: 'float2', hideZero: true },
      { id: 'MainHandAccuracy', format: 'int', hideZero: true },
      { id: 'OffHandAccuracy', format: 'int', hideZero: true },
      { id: 'AccuracyHitChanceUncapped', hideZero: true },
      { id: 'QuantityMultiplier', format: 'float2', hideZero: true },
      { id: 'TotalDotDPS', format: 'float2', hideZero: true },
      { id: 'TotalDPS', format: 'float2', colorVar: 'accent-gold-bright' },
      { id: 'TotalHitAvg', format: 'float2' },
      { id: 'ActionRate', format: 'rate' },
      { id: 'HitChance' },
      { id: 'CritChance' },
      { id: 'CritMultiplier', format: 'modifier' },
      { id: 'IgniteDPS', format: 'float2', colorVar: 'color-fire', hideZero: true },
      { id: 'BleedDPS', format: 'float2', colorVar: 'color-life', hideZero: true },
      { id: 'PoisonDPS', format: 'float2', colorVar: 'color-chaos', hideZero: true },
      { id: 'ManaCost', format: 'int', colorVar: 'color-mana' },
      { id: 'PreEffectiveCritChance', hideZero: true },
      { id: 'TotalDot', format: 'float2', hideZero: true },
      { id: 'WithDotDPS', format: 'float2', hideZero: true },
      { id: 'CullingDPS', format: 'float2', hideZero: true },
    ],
  },
  {
    id: 'Resources',
    titleKey: 'resources',
    rows: [
      { id: 'Life', format: 'int', colorVar: 'color-life' },
      { id: 'Mana', format: 'int', colorVar: 'color-mana' },
      { id: 'EnergyShield', format: 'int', colorVar: 'color-es' },
      { id: 'Spirit', format: 'int', colorVar: 'color-chaos' },
      { id: 'SpiritUnreserved', format: 'int' },
      { id: 'Ward', format: 'int', hideZero: true },
      { id: 'Rage', format: 'int', hideZero: true },
      { id: 'LifeUnreservedPercent' },
      { id: 'ManaUnreservedPercent' },
      { id: 'SpiritUnreservedPercent' },
    ],
  },
  {
    id: 'Defence',
    titleKey: 'defence',
    rows: [
      { id: 'EffectiveMovementSpeedMod', format: 'modifier', hideZero: true },
      { id: 'PresenceRadiusMetres', format: 'float2', hideZero: true },
      { id: 'Armour', format: 'int', colorVar: 'color-defence' },
      { id: 'Evasion', format: 'int', colorVar: 'color-defence' },
      { id: 'PhysicalDamageReduction' },
      { id: 'BlockChance', hideZero: true },
      { id: 'StunThreshold', format: 'int' },
    ],
  },
  {
    id: 'Resistances',
    titleKey: 'resistances',
    rows: [
      { id: 'FireResist', colorVar: 'color-fire' },
      { id: 'ColdResist', colorVar: 'color-cold' },
      { id: 'LightningResist', colorVar: 'color-lightning' },
      { id: 'ChaosResist', colorVar: 'color-chaos' },
      { id: 'FireResistOverCap', hideZero: true },
      { id: 'ColdResistOverCap', hideZero: true },
      { id: 'LightningResistOverCap', hideZero: true },
      { id: 'ChaosResistOverCap', hideZero: true },
    ],
  },
  {
    id: 'Survivability',
    titleKey: 'survivability',
    rows: [
      { id: 'TotalEHP', format: 'int' },
      { id: 'PhysicalMaxHit', format: 'int' },
      { id: 'FireMaxHit', format: 'int', colorVar: 'color-fire' },
      { id: 'ColdMaxHit', format: 'int', colorVar: 'color-cold' },
      { id: 'LightningMaxHit', format: 'int', colorVar: 'color-lightning' },
      { id: 'ChaosMaxHit', format: 'int', colorVar: 'color-chaos' },
    ],
  },
  {
    id: 'Recovery',
    titleKey: 'recovery',
    rows: [
      { id: 'LifeRegenRecovery', format: 'float2', hideZero: true },
      { id: 'ManaRegenRecovery', format: 'float2', hideZero: true },
      { id: 'EnergyShieldRegenRecovery', format: 'float2', hideZero: true },
      { id: 'WardRegenRecovery', format: 'float2', hideZero: true },
      { id: 'RageRegenRecovery', format: 'float2', hideZero: true },
      { id: 'NetLifeRegen', format: 'float2', hideZero: true },
      { id: 'NetManaRegen', format: 'float2', hideZero: true },
      { id: 'NetWardRegen', format: 'float2', hideZero: true },
      { id: 'NetEnergyShieldRegen', format: 'float2', hideZero: true },
      { id: 'TotalNetRegen', format: 'float2', hideZero: true },
      { id: 'TotalBuildDegen', format: 'float2', hideZero: true },
      { id: 'LifeLeechGainRate', format: 'float2', hideZero: true },
      { id: 'ManaLeechGainRate', format: 'float2', hideZero: true },
      { id: 'EnergyShieldLeechGainRate', format: 'float2', hideZero: true },
      { id: 'LifeLeechGainPerHit', format: 'float2', hideZero: true },
      { id: 'ManaLeechGainPerHit', format: 'float2', hideZero: true },
      { id: 'EnergyShieldLeechGainPerHit', format: 'float2', hideZero: true },
    ],
  },
  {
    id: 'Attributes',
    titleKey: 'attributes',
    rows: [
      { id: 'Str', format: 'int', hideZero: true },
      { id: 'Dex', format: 'int', hideZero: true },
      { id: 'Int', format: 'int', hideZero: true },
    ],
  },
  {
    id: 'Skill mechanics',
    titleKey: 'skillMechanics',
    requiresSkill: true,
    rows: [
      { id: 'MovementSpeedWhileUsingSkill', format: 'modifier', hideZero: true },
      { id: 'AuraEffectMod', format: 'float2', hideZero: true },
      { id: 'CurseEffectMod', format: 'float2', hideZero: true },
      { id: 'ActiveMinionLimit', format: 'int', hideZero: true },
      { id: 'Duration', format: 'float2', hideZero: true },
      { id: 'DurationSecondary', format: 'float2', hideZero: true },
      { id: 'AuraDuration', format: 'float2', hideZero: true },
      { id: 'ReserveDuration', format: 'float2', hideZero: true },
      { id: 'AreaOfEffectRadiusMetres', format: 'float2', hideZero: true },
      { id: 'PierceCount', format: 'pierce', hideZero: true },
      { id: 'ForkCount', format: 'float2', hideZero: true },
      { id: 'ChainMax', format: 'float2', hideZero: true },
      { id: 'SplitCount', format: 'float2', hideZero: true },
      { id: 'BounceCount', format: 'float2', hideZero: true },
      { id: 'ReqStr', format: 'int', hideZero: true },
      { id: 'ReqDex', format: 'int', hideZero: true },
      { id: 'ReqInt', format: 'int', hideZero: true },
      { id: 'ManaPercentCost', hideZero: true },
      { id: 'ManaPercentPerSecondCost', hideZero: true },
      { id: 'ManaPerSecondCost', format: 'float2', hideZero: true },
      { id: 'LifePercentCost', hideZero: true },
      { id: 'LifePercentPerSecondCost', hideZero: true },
      { id: 'LifePerSecondCost', format: 'float2', hideZero: true },
      { id: 'ESCost', format: 'float2', hideZero: true },
      { id: 'ESPercentCost', hideZero: true },
      { id: 'ESPercentPerSecondCost', hideZero: true },
      { id: 'ESPerSecondCost', format: 'float2', hideZero: true },
      { id: 'WardCost', format: 'float2', hideZero: true },
      { id: 'WardPercentCost', hideZero: true },
      { id: 'WardPercentPerSecondCost', hideZero: true },
      { id: 'WardPerSecondCost', format: 'float2', hideZero: true },
      { id: 'RageCost', format: 'float2', hideZero: true },
      { id: 'RagePerSecondCost', format: 'float2', hideZero: true },
      { id: 'TotalTotemPlacementTime', format: 'float2', hideZero: true },
      { id: 'TrapThrowingTime', format: 'float2', hideZero: true },
      { id: 'TrapThrowCount', format: 'float2', hideZero: true },
      { id: 'MineLayingTime', format: 'float2', hideZero: true },
      { id: 'MineThrowCount', format: 'float2', hideZero: true },
      { id: 'WarcryCastTime', format: 'float2', hideZero: true },
      { id: 'ChannelTimeToTrigger', format: 'float2', hideZero: true },
      { id: 'ChannelTime', format: 'float2', hideZero: true },
      { id: 'StoredUses', format: 'int', hideZero: true },
      { id: 'FiringRate', format: 'rate', hideZero: true },
      { id: 'ReloadTime', format: 'float2', hideZero: true },
      { id: 'ProjectileSpeedMod', format: 'float2', hideZero: true },
    ],
  },
];

/** Shared metric meanings beside the display labels; UI text only. */
export function metricMeaning(id: string, lang: Lang): string | null {
  if (['FireResist', 'ColdResist', 'LightningResist', 'ChaosResist'].includes(id)) {
    return messages(lang, 'ui')['stat.resistanceMeaning'];
  }
  return lookup(lang, 'metricMeaning', id) ?? null;
}

const STAT_FORMATS = new Map(STAT_SECTIONS.flatMap(section => section.rows.map(row => [row.id, row.format] as const)));

// Keep pre-boot displays consistent; backend initialization installs its engine's catalog.
const DISPLAY_CATALOG = new Map<string, DisplayCatalogEntry>(
  (generatedCatalog as DisplayCatalogEntry[]).map(entry => [entry.id, entry]),
);

/** Installed once by the shared backend initialization, before any calculation. */
export function setDisplayCatalog(entries: readonly DisplayCatalogEntry[]): void {
  DISPLAY_CATALOG.clear();
  for (const entry of entries) DISPLAY_CATALOG.set(entry.id, entry);
}

export function displayStatDefinition(id: string): DisplayCatalogEntry | undefined {
  const stat = id === 'MainMinionDPS' || id === 'MainSkillHitDPS' ? 'TotalDPS'
    : id === 'MainSkillDotDPS' ? 'TotalDotDPS'
    : id === 'MainSkillCombinedDPS' ? 'CombinedDPS' : id;
  return DISPLAY_CATALOG.get(stat);
}

export function statFormat(stat: string): StatFormat {
  const type = displayStatDefinition(stat)?.value_type;
  if (type === 'Percent') return 'percent';
  if (type === 'TimeSeconds') return 'float2';
  return STAT_FORMATS.get(stat) ?? 'float2';
}

const NUMBER_FORMATTERS = new Map<string, Intl.NumberFormat>();

/** Shared display rounding and grouping for numbers that are not tied to a stat. */
export function formatNumber(value: number, digits = 2, minimumDigits = 0): string {
  // A tiny negative that rounds to zero would print as "-0", which reads like a loss.
  const shown = Math.round(value * 10 ** digits) === 0 ? 0 : value;
  const key = `${digits}:${minimumDigits}`;
  let formatter = NUMBER_FORMATTERS.get(key);
  if (!formatter) {
    formatter = new Intl.NumberFormat('en-US', { maximumFractionDigits: digits, minimumFractionDigits: minimumDigits });
    NUMBER_FORMATTERS.set(key, formatter);
  }
  return formatter.format(shown);
}

/** Signed number with explicit `+` on positive values (and optionally on zero). */
export function formatSignedNumber(value: number, digits = 2, minimumDigits = 0, plusZero = false): string {
  const shown = Math.round(value * 10 ** digits) === 0 ? 0 : value;
  const sign = shown > 0 || (plusZero && shown === 0) ? '+' : '';
  return `${sign}${formatNumber(shown, digits, minimumDigits)}`;
}

export function signedPercent(percent: number, digits = 1): string {
  const text = formatNumber(Math.abs(percent), digits, digits);
  return `${percent > 0 ? '+' : percent < 0 ? '−' : ''}${text}%`;
}

export function formatStat(id: string, value: number | null, lang: Lang = 'en-US'): string {
  return formatStatValue(value, statFormat(id), lang);
}

/** Format a result consistently in headlines, detail results and comparisons. */
export function formatStatValue(value: number | null, format: StatFormat, lang: Lang = 'en-US'): string {
  if (value === null || !Number.isFinite(value)) return '∞';
  switch (format) {
    case 'chanceMultiplier':
      return `${roundTo(Math.max(0, value - 1) * 100, 1)}%`;
    case 'modifier':
      return `${value >= 1 ? '+' : ''}${roundTo((value - 1) * 100, 1)}%`;
    case 'pierce':
      return value >= 100 ? messages(lang, 'ui')['stat.allTargets'] : formatNumber(Math.round(value), 0);
    case 'int':
      return formatNumber(Math.round(value), 0);
    case 'percent':
      return `${roundTo(value, 1)}%`;
    case 'rate':
      return formatNumber(roundTo(value, 2), 2, 2);
    case 'float2':
      return Math.abs(value) >= 1000
        ? formatNumber(Math.round(value), 0)
        : formatNumber(roundTo(value, 2));
  }
}

function roundTo(value: number, digits: number): number {
  const scale = 10 ** digits;
  return Math.round(value * scale) / scale;
}

/** Index `stats` entries by `id`. */
export function statMap(stats: DisplayStatValue[]): Map<string, number | null> {
  return new Map(stats.map((s) => [s.id, s.value]));
}
