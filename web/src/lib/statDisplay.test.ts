import { describe, expect, test } from 'vitest';
import { formatStatValue, statMap, STAT_SECTIONS } from './statDisplay';

describe('formatStatValue', () => {
  test('int 取整加千分位', () => {
    expect(formatStatValue(10005.12, 'int')).toBe('10,005');
  });

  test('percent 保留一位', () => {
    expect(formatStatValue(75, 'percent')).toBe('75%');
    expect(formatStatValue(46.832, 'percent')).toBe('46.8%');
  });

  test('float2 大数退化为整数展示', () => {
    expect(formatStatValue(4143.7479, 'float2')).toBe('4,144');
    expect(formatStatValue(4.59, 'float2')).toBe('4.59');
  });

  test('null / 非有限值显示 ∞（ChaosMaxHit 免疫语义）', () => {
    expect(formatStatValue(null, 'int')).toBe('∞');
    expect(formatStatValue(Number.POSITIVE_INFINITY, 'int')).toBe('∞');
  });
});

describe('statMap', () => {
  test('id 索引', () => {
    const map = statMap([{ id: 'Life', value: 1, category: 'Resource' }]);
    expect(map.get('Life')).toBe(1);
  });
});

describe('STAT_SECTIONS', () => {
  test('行 id 全局唯一（侧边栏不重复展示同一字段）', () => {
    const ids = STAT_SECTIONS.flatMap((s) => s.rows.map((r) => r.id));
    expect(new Set(ids).size).toBe(ids.length);
  });

  test('Full DPS remains visible without a selected main skill', () => {
    expect(STAT_SECTIONS.find(section => section.rows.some(row => row.id === 'FullDPS'))?.requiresSkill).not.toBe(true);
  });

  test('requested Full DPS includes a computed zero DoT total', () => {
    const rows = STAT_SECTIONS.flatMap(section => section.rows);
    expect(rows.find(row => row.id === 'FullDotDPS')?.hideZero).not.toBe(true);
  });

  test('双语标签齐全', () => {
    for (const section of STAT_SECTIONS) {
      for (const row of section.rows) {
        expect(row.label['en-US']).toBeTruthy();
        expect(row.label['zh-TW']).toBeTruthy();
      }
    }
  });

  test('sidebar extensions have rows with display units', () => {
    const rows = new Map(STAT_SECTIONS.flatMap((section) => section.rows.map((row) => [row.id, row] as const)));
    for (const id of [
      'Str', 'Dex', 'Int', 'Rage', 'FireResistOverCap', 'ColdResistOverCap',
      'LightningResistOverCap', 'ChaosResistOverCap', 'LifeUnreservedPercent',
      'ManaUnreservedPercent', 'SpiritUnreservedPercent', 'PreEffectiveCritChance',
      'AverageDamage',
      'AverageBurstDamage',
      'CombinedAvg',
      'HitSpeed',
      'IgniteDamage',
      'BleedDamage',
      'PoisonDamage',
      'WithIgniteDPS',
      'WithBleedDPS',
      'WithPoisonDPS',
      'MainHandAccuracy',
      'OffHandAccuracy',
      'AccuracyHitChanceUncapped',
      'QuantityMultiplier',
      'Duration',
      'DurationSecondary',
      'AuraDuration',
      'ReserveDuration',
      'AreaOfEffectRadiusMetres',
      'PierceCount',
      'ForkCount',
      'ChainMax',
      'SplitCount',
      'BounceCount',
      'ReqStr',
      'ReqDex',
      'ReqInt',
      'ManaPercentCost',
      'ManaPercentPerSecondCost',
      'ManaPerSecondCost',
      'LifePercentCost',
      'LifePercentPerSecondCost',
      'LifePerSecondCost',
      'ESCost',
      'ESPercentCost',
      'ESPercentPerSecondCost',
      'ESPerSecondCost',
      'WardCost',
      'WardPercentCost',
      'WardPercentPerSecondCost',
      'WardPerSecondCost',
      'RageCost',
      'RagePerSecondCost',
      'TotalTotemPlacementTime',
      'TrapThrowingTime',
      'TrapThrowCount',
      'MineLayingTime',
      'MineThrowCount',
      'WarcryCastTime',
      'ChannelTime',
      'StoredUses', 'TotalDot', 'WithDotDPS', 'FullDPS', 'FullDotDPS',
      'FiringRate', 'ReloadTime', 'CullingDPS', 'ProjectileSpeedMod',
      'LifeRegenRecovery', 'ManaRegenRecovery', 'EnergyShieldRegenRecovery',
      'WardRegenRecovery', 'RageRegenRecovery', 'NetLifeRegen', 'NetManaRegen',
      'NetEnergyShieldRegen', 'TotalNetRegen', 'TotalBuildDegen',
      'LifeLeechGainRate', 'ManaLeechGainRate', 'EnergyShieldLeechGainRate',
      'LifeLeechGainPerHit', 'ManaLeechGainPerHit', 'EnergyShieldLeechGainPerHit',
    ]) {
      expect(rows.has(id), id).toBe(true);
    }
    expect(rows.get('LifeUnreservedPercent')?.format).toBe('percent');
    expect(rows.get('PreEffectiveCritChance')?.format).toBe('percent');
    expect(rows.get('ReloadTime')?.label['en-US']).toContain('(s)');
    expect(rows.get('ProjectileSpeedMod')?.format).toBe('float2');
  });
});

test('movement multipliers and unlimited piercing use display conventions', () => {
  expect(formatStatValue(1.3, 'modifier')).toBe('+30%');
  expect(formatStatValue(0.39, 'modifier')).toBe('-61%');
  expect(formatStatValue(100, 'pierce')).toBe('All targets');
  expect(formatStatValue(100, 'pierce', 'zh-TW')).toBe('所有目標');
  expect(formatStatValue(3, 'pierce')).toBe('3');
  const rows = new Map(STAT_SECTIONS.flatMap((section) => section.rows.map((row) => [row.id, row] as const)));
  for (const id of ['ChainMax', 'ForkCount', 'SplitCount']) {
    expect(formatStatValue(2, rows.get(id)!.format)).toBe('2');
  }
});

test('double critical roll multiplier is displayed as a chance', () => {
  const row = STAT_SECTIONS.flatMap(section => section.rows).find(row => row.id === 'CritBifurcates')!;
  expect(formatStatValue(4 / 3, row.format)).toBe('33.3%');
  expect(formatStatValue(1, row.format)).toBe('0%');
  expect(formatStatValue(2, row.format)).toBe('100%');
  expect(formatStatValue(0, row.format)).toBe('0%');
});
