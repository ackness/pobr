import { describe, expect, test } from 'vitest';
import { formatNumber, formatStatValue, statFormat, statMap, STAT_SECTIONS } from './statDisplay';
import { statNameLabel } from './i18n';

describe('copied flask recovery labels', () => {
  test('both resources and their detail inputs have localized labels', () => {
    for (const resource of ['Life', 'Mana']) {
      for (const field of ['Recovery', 'InstantRecovery', 'RecoveryPerSecond']) {
        const id = `Flask1${resource}${field}`;
        for (const lang of ['en-US', 'zh-CN', 'zh-TW'] as const) {
          expect(statNameLabel(lang, id)).not.toContain(id);
          if (lang !== 'en-US') expect(statNameLabel(lang, id)).not.toMatch(/[A-Za-z]/);
        }
      }
    }
    expect(statNameLabel('zh-CN', 'Flask1ManaRecoveryPerSecond')).toBe('药剂 1 · 每秒额外恢复的魔力');
    for (const id of ['UtilityCopiedInstant', 'UtilityCopiedGradual', 'UtilityReceiverRecoveryMod']) {
      expect(statNameLabel('zh-CN', id)).not.toMatch(/[A-Za-z]/);
    }
    expect(statNameLabel('zh-CN', 'Flask1UnknownFutureRecovery')).toBe('Flask1UnknownFutureRecovery');
  });
});

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

  test.each(['en-US', 'zh-CN', 'zh-TW'] as const)('groups large negative values with the same precision in %s', lang => {
    expect(formatStatValue(-12345.678, 'float2', lang)).toBe('-12,346');
    expect(formatStatValue(-12345, 'int', lang)).toBe('-12,345');
    expect(formatStatValue(-999.125, 'float2', lang)).toBe('-999.12');
    expect(formatStatValue(-1000, 'float2', lang)).toBe('-1,000');
    expect(formatStatValue(1000, 'float2', lang)).toBe('1,000');
  });

  test('keeps total, maximum and internal resistance values in percent', () => {
    for (const id of ['FireResistTotal', 'ColdResistMax', 'LightningResistOverCap', 'TotemChaosResist']) {
      expect(statFormat(id), id).toBe('percent');
      expect(formatStatValue(75, statFormat(id)), id).toBe('75%');
    }
  });

  test('uses metric precision for both a headline and its result', () => {
    expect(formatStatValue(1693.83, statFormat('Life'))).toBe('1,694');
    expect(formatStatValue(5307.37, statFormat('TotalDPS'))).toBe('5,307');
    expect(formatStatValue(5307.37, statFormat('CombinedDPS'))).toBe('5,307');
    expect(formatStatValue(22.35, statFormat('CritChance'))).toBe('22.4%');
    expect(formatStatValue(2.345, statFormat('ActionRate'))).toBe('2.35');
    expect(formatStatValue(2, statFormat('ActionRate'))).toBe('2.00');
    expect(formatNumber(-12345.678)).toBe('-12,345.68');
    expect(formatNumber(12, 1, 1)).toBe('12.0');
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

describe('stat section labels', () => {
  test('rows in one section never share a label, so a value and its percentage stay distinguishable', () => {
    for (const section of STAT_SECTIONS) {
      for (const lang of ['en-US', 'zh-CN', 'zh-TW'] as const) {
        const labels = section.rows.map(row => row.label[lang]);
        expect(labels.filter((label, index) => labels.indexOf(label) !== index), `${section.title['en-US']} ${lang}`).toEqual([]);
      }
    }
  });
});

test('player and main-minion DPS share precision above and below 1000', () => {
  for (const [value, expected] of [[5307.37, '5,307'], [999.37, '999.37'], [0.37, '0.37']] as const) {
    for (const id of ['MainMinionDPS', 'TotalDPS', 'CombinedDPS']) {
      expect(formatStatValue(value, statFormat(id), 'zh-CN'), id).toBe(expected);
    }
  }
});

test('values that round to zero never show a minus sign', () => {
  expect(formatNumber(-0.001)).toBe('0');
  expect(formatNumber(-0)).toBe('0');
  expect(formatNumber(-0.004, 2, 2)).toBe('0.00');
  expect(formatNumber(-0.01)).toBe('-0.01');
  expect(formatStatValue(-0.001, 'float2')).toBe('0');
});
