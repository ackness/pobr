import { beforeAll, expect, test } from 'vitest';
import type { Breakdown, BreakdownMod, DisplayCatalogEntry, LineUsageLine } from '../api/types';
import { damageChain, groupModifiers, resistanceIds } from './calcsPresentation';
import { metricMeaning, setDisplayCatalog, statFormat } from './statDisplay';
import catalog from '../api/generated/display_catalog.json';
import { modifierText } from '../components/calcs/ModifierText';
import { displayedLedgerReason, filterLedger, NO_FILTER, reasonConfigField } from '../app/pages/calcs/ledger';

const mod = (value: number, type: BreakdownMod['mod_type'], kind = 'PassiveNode', active = true): BreakdownMod =>
  ({ value, mod_type: type, origin_kind: kind, active, origin_id: null, slot: null, source_text: null });
const empty: Breakdown = { base_total: 0, inc_total: 0, more_total: 1, mods: [], formula: null };

beforeAll(() => setDisplayCatalog(catalog as DisplayCatalogEntry[]));

test('real source families keep BASE, INC and multiplicative MORE subtotals separate', () => {
  const groups = groupModifiers([mod(20, 'BASE'), mod(15, 'INC'), mod(50, 'MORE'), mod(-20, 'MORE'),
    mod(900, 'INC', 'PassiveNode', false), mod(8, 'INC', 'ItemAffix'), mod(9, 'INC', 'Jewel'), mod(4, 'BASE', 'SupportGem')], { stat: 'Life', unit: 'points' });
  expect(groups.map(group => group.source)).toEqual(['tree', 'item', 'jewel', 'skill']);
  expect(groups[0].mods).toHaveLength(5);
  expect(groups[0].subtotals.slice(0, 2)).toEqual([{ type: 'BASE', value: 20 }, { type: 'INC', value: 15 }]);
  expect(groups[0].subtotals[2].value).toBeCloseTo(1.2);
});

test('damage chain uses returned rates and mitigation without inventing a trigger multiplier', () => {
  const breakdown: Breakdown = { ...empty, formula: { value: 160, steps: [
    { op: 'average_hit', operand: 100, result: 100 }, { op: 'enemy_mitigation', operand: .8, result: 80 },
    { op: 'uses_per_second', operand: 2, result: 160 }, { op: 'chance_to_hit', operand: 100, result: 160 },
  ] } };
  expect(damageChain(breakdown).map(link => link.value)).toEqual([100, 80, 2, 100, undefined, 160]);
  // Only links that equal a stat's final value name it; enemy mitigation and the trigger factor are intermediate.
  expect(damageChain(breakdown).map(link => link.stat)).toEqual(['TotalHitAvg', undefined, 'ActionRate', 'HitChance', undefined, 'TotalDPS']);
  breakdown.formula!.steps.push({ op: 'reload', operand: .5, result: 80 });
  expect(damageChain(breakdown).find(link => link.key === 'calcs.step.reload')?.value).toBe(.5);
  // Reload scales the rate after this link, so the link is no longer the final ActionRate.
  expect(damageChain(breakdown).find(link => link.key === 'calcs.chainRate')?.stat).toBeUndefined();
  breakdown.formula!.steps.push({ op: 'energy_trigger', operand: .25, result: 40 });
  expect(damageChain(breakdown).find(link => link.key === 'calcs.chainTrigger')?.value).toBe(.25);
  expect(damageChain(empty).every(link => link.value === undefined)).toBe(true);
});

test('resistance details request total, maximum and the engine over-cap value', () => {
  expect(resistanceIds('FireResist')).toEqual(['FireResistTotal', 'FireResistMax', 'FireResistOverCap']);
  expect(resistanceIds('Life')).toEqual([]);
});

test.each(['en-US', 'zh-CN', 'zh-TW'] as const)('shared meanings cover all requested headline metrics in %s', lang => {
  for (const stat of ['CombinedDPS', 'TotalDPS', 'AverageDamage', 'TotalHitAvg', 'CritChance', 'Life', 'EnergyShield', 'TotalEHP', 'FireResist', 'ColdResist', 'LightningResist', 'ChaosResist']) {
    expect(metricMeaning(stat, lang), stat).toBeTruthy();
  }
});

test('modifier display maps complete known IDs and templates and marks unknown original prose', () => {
  expect(modifierText('FireResist', undefined, 'zh-CN')).toMatchObject({ missing: false, internal: true });
  expect(modifierText('CharmSpirit', undefined, 'zh-CN')).toEqual({ text: '缺少词缀文本', missing: true, internal: true });
  expect(modifierText('-60% to FireResist', undefined, 'zh-CN').text).toContain('-60%');
  expect(modifierText('A future FireResist effect', undefined, 'zh-CN')).toEqual({ text: 'A future FireResist effect', missing: true, internal: false });
});

const line = (text: string | null, reason: string | null): LineUsageLine => ({ text, reason, source_kind: 'Config', source_id: 'config.lowLife',
  holder: null, slot: null, status: 'inactive', used_in: [], modifiers: [] });

test('ledger hides internal entries by default while preserving an explicit advanced filter', () => {
  const rows = [line('CharmSpirit', null), line('15% increased Damage', null), line(null, null)];
  expect(filterLedger(rows, { ...NO_FILTER, showInternal: false })).toEqual([rows[1]]);
  expect(filterLedger(rows, { ...NO_FILTER, showInternal: true })).toHaveLength(3);
});

test('ledger reasons use whole templates, with actionable config fields and preserved unknown reasons', () => {
  const row = line('15% increased Damage', 'Condition LowLife not met');
  expect(displayedLedgerReason(row, 'zh-CN')).toEqual({ text: '未满足条件：低血', untranslated: false });
  expect(displayedLedgerReason(row, 'en-US').text).toBe('Condition not met: Low Life');
  expect(reasonConfigField(row, { LowLife: 'conditionLowLife' })).toBe('conditionLowLife');
  expect(displayedLedgerReason(line('x', 'the build creates no minion to give it to'), 'zh-TW').text).toBe('此 BD 沒有召喚物');
  const unknown = line('x', 'new raw EngineReason');
  expect(displayedLedgerReason(unknown, 'zh-CN').untranslated).toBe(true);
  expect(displayedLedgerReason(unknown, 'zh-CN').text).not.toContain('EngineReason');
  expect(unknown.reason).toBe('new raw EngineReason');
});

test('resistance source details keep percent units on total and cap values', () => {
  expect(statFormat('FireResistTotal')).toBe('percent');
  expect(statFormat('ChaosResistMax')).toBe('percent');
});


test('unidentified mixed-unit enemy defence modifiers never produce a subtotal', () => {
  const armour = { ...mod(2000, 'BASE', 'EnemyConfig'), source_text: '2000 Armour' };
  const reduction = { ...mod(10, 'BASE', 'EnemyConfig'), source_text: '10% Physical Damage Reduction' };
  const [group] = groupModifiers([armour, reduction]);
  expect(group.mods).toEqual([armour, reduction]);
  expect(group.subtotals).toEqual([]);
});
