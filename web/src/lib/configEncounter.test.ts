import { describe, expect, it } from 'vitest';
import type { CalculateBuildResponse, ConfigOption, SocketGroupInput } from '../api/types';
import type { EnemyPresets } from '../api/enemyPresets';
import { encounterInheritedValues, encounterHitTypes, encounterTierLabel, encounterValue, encounterValueLabel, hasEnergyTrigger } from './configEncounter';

const response = (fields: Partial<CalculateBuildResponse>) => fields as CalculateBuildResponse;
const minion = () => ({ index: 3, minion_id: 'wolf', name: 'Wolf', summoned_by: 'WolfPackPlayer', skill_id: 'Bite', level: 85, dps: 10, life: 100, armour: 10, evasion: 10, energy_shield: 0, stats: { TotalDPS: 10 }, breakdowns: {} }) as NonNullable<CalculateBuildResponse['minions']>[number];

describe('encounter checks', () => {
  it('does not pretend production minion responses contain hit damage types', () => {
    const calc = response({
      main_skill: { hit_damage: [{ damage_type: 'Fire', avg: 100 }] } as CalculateBuildResponse['main_skill'],
      main_minion_index: 3, minions: [minion()],
    });
    expect([...encounterHitTypes(calc)]).toEqual([]);
    calc.minions = [];
    expect([...encounterHitTypes(calc)]).toEqual([]);
    calc.main_minion_index = null;
    expect([...encounterHitTypes(calc)]).toEqual(['Fire']);
  });

  it('uses shipped base resistances and clamps inherited level without inventing scaled ratings', () => {
    const table: EnemyPresets = { max_enemy_level: 85, tiers: [{ id: 'Pinnacle', min_level: 82,
      elemental_resist_bonus: 50, chaos_resist_bonus: 0, armour_mult_pct: { base: 100, num: 1100, den: 22 },
      evasion_mult_pct: { base: 100, num: 548, den: 22 }, enemy_mods: [] }] };
    const values = encounterInheritedValues(table, 'pinnacle', 20, {});
    expect(values).toEqual({ enemyFireResist: 50, enemyColdResist: 50, enemyLightningResist: 50, enemyChaosResist: 0, enemyLevel: 82 });
    expect(encounterInheritedValues(table, 'pinnacle', 100, {}).enemyLevel).toBe(85);
    expect(encounterInheritedValues(table, 'pinnacle', 84, { enemyLevel: 0 }).enemyLevel).toBe(84);
    expect(encounterInheritedValues(table, 'pinnacle', 84, { enemyLevel: 83 }).enemyLevel).toBe(83);
    expect(encounterInheritedValues(table, 'custom', 84, {})).toEqual({});
    expect(encounterInheritedValues(null, 'pinnacle', 84, {})).toEqual({});
    const option: ConfigOption = { var: 'enemyFireResist', input_type: 'count', default: { placeholder_number: 75 } };
    expect(encounterValue(option, undefined, values)).toBe(50);
    expect(encounterValue(option, 0, values)).toBe(0);
    expect(encounterValue(option, 25, values)).toBe(25);
    expect(encounterValue({ var: 'enemyArmour', input_type: 'count' }, undefined, values)).toBeUndefined();
  });

  it('never replaces missing enemy preset values with catalog examples', () => {
    const option: ConfigOption = { var: 'enemyFireResist', input_type: 'count', default: { placeholder_number: 50 } };
    expect(encounterValue(option, undefined)).toBeUndefined();
    for (const value of [0, 25, false, '23', ''] as const) expect(encounterValue(option, value)).toBe(value);
    expect(encounterValue({ var: 'conditionEnemyShocked', input_type: 'check' }, undefined)).toBe(false);
    expect(encounterValue({ var: 'enemyMaxResist', input_type: 'check', default: true }, undefined)).toBe(true);
    expect(encounterValue({ var: 'enemyDamageType', input_type: 'list', list_options: [{ value: 'Physical', label: 'Physical' }] }, undefined)).toBe('Physical');
  });

  it.each([
    ['en-US', 'Off', 'On', 'Pinnacle Boss', 'Mapping'],
    ['zh-CN', '关闭', '开启', '巅峰 Boss', '刷图'],
    ['zh-TW', '關閉', '開啟', '巔峰頭目', '刷圖'],
  ] as const)('formats values and inherited tiers in %s without raw booleans or ids', (lang, off, on, pinnacle, mapping) => {
    const option: ConfigOption = { var: 'conditionEnemyShocked', input_type: 'check' };
    expect(encounterValueLabel(lang, option, false)).toBe(off);
    expect(encounterValueLabel(lang, option, 0)).toBe(off);
    expect(encounterValueLabel(lang, option, 1)).toBe(on);
    expect(encounterTierLabel(lang, undefined, {})).toBe(pinnacle);
    expect(encounterTierLabel(lang, 'none', {})).toBe(mapping);
    expect(encounterTierLabel(lang, undefined, { enemyIsBoss: 'futureRawTier' })).not.toContain('futureRawTier');
    expect(encounterValueLabel(lang, { var: 'enemyColdResist', input_type: 'count' }, 0)).toBe('0%');
    expect(encounterValueLabel(lang, option, undefined)).not.toMatch(/false|undefined|unspecified/i);
    expect(encounterValueLabel(lang, option, 'raw-string')).not.toContain('raw-string');
  });
});

describe('energy-trigger note eligibility', () => {
  const energy = { maximum_energy: 100, source: 'SparkPlayer', at_own_rate: false, missing_source: null } as NonNullable<CalculateBuildResponse['energy_trigger']>;
  const main_skill = { group_index: 0, active_skill_index: 2, skill_id: 'CometPlayer', hit_damage: [], hit_dps: 10, dot_dps: 0, combined_dps: 10 };
  const gem = (skill_id: string) => ({ skill_id, level: 16, quality: 0 });
  const groups = (meta: string): SocketGroupInput[] => [
    { enabled: true, gems: [gem(meta), gem('CometPlayer')] },
    { enabled: true, gems: [gem('SparkPlayer')] },
  ];
  it.each(['MetaCastOnCritPlayer', 'MetaCastOnFreezePlayer', 'MetaCastOnIgnitePlayer', 'MetaCastOnShockPlayer'])('confirms power-dependent generation for %s', meta => {
    expect(hasEnergyTrigger(response({ main_skill, energy_trigger: energy }), groups(meta))).toBe(true);
  });
  it('requires a confirmed trigger, a known active meta gem, and a matching source skill', () => {
    const calc = response({ main_skill, energy_trigger: energy });
    const active = groups('MetaCastOnCritPlayer');
    expect(hasEnergyTrigger(calc)).toBe(false);
    expect(hasEnergyTrigger(null, active)).toBe(false);
    expect(hasEnergyTrigger(response({}), active)).toBe(false);
    for (const patch of [{ maximum_energy: 0 }, { source: null }, { source: 'UnknownSkill' }, { missing_source: 'EnergyNeedsCriticalHits' }, { at_own_rate: true }])
      expect(hasEnergyTrigger(response({ main_skill, energy_trigger: { ...energy, ...patch } }), active)).toBe(false);
    active[0].gems[0].enabled = false;
    expect(hasEnergyTrigger(calc, active)).toBe(false);
    active[0].gems[0].enabled = true;
    active[0].enabled = false;
    expect(hasEnergyTrigger(calc, active)).toBe(false);
    active[0].enabled = true;
    active[1].gems[0].enabled = false;
    expect(hasEnergyTrigger(calc, active)).toBe(false);
    expect(hasEnergyTrigger(response({ main_skill, energy_trigger: energy, main_minion_index: 3, minions: [minion()] }), groups('MetaCastOnCritPlayer'))).toBe(false);
  });
  it.each(['Block', 'MinionDeath'])('suppresses the pack explanation for %s even with a known meta gem', source => {
    expect(hasEnergyTrigger(response({ main_skill, energy_trigger: { ...energy, source } }), groups('MetaCastOnCritPlayer'))).toBe(false);
  });
  it.each(['SpellslingerPlayer', 'MetaSpellslingerPlayer', 'MetaBarrierInvocationPlayer', 'MetaReapersInvocationPlayer', 'MetaCastOnBlockPlayer', 'MetaCastOnMinionDeathPlayer', 'MetaUnknownPlayer'])('suppresses unrelated or uncertain generation for %s', meta => {
    expect(hasEnergyTrigger(response({ main_skill, energy_trigger: energy }), groups(meta))).toBe(false);
  });
});
