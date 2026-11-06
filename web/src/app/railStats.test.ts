import { expect, it } from 'vitest';
import { headlineRows, hideDuplicateRow } from './railStats';
it('leads with the shield for a shield-dominant character without losing the other essentials', () => {
  const values = new Map(Object.entries({ TotalEHP: 9000, Life: 1, EnergyShield: 5000, FireResist: 75,
    ColdResist: 75, LightningResist: 75, ChaosResist: 75, SpiritUnreserved: 10 }));
  expect(headlineRows(values)).toEqual(['TotalEHP', 'EnergyShield', 'FireResist', 'ColdResist', 'LightningResist', 'ChaosResist', 'SpiritUnreserved']);
});
it('hides equivalent derived totals but keeps distinct totals and unrelated equal values', () => {
  const values = new Map(Object.entries({ CombinedDPS: 1000, TotalDPS: 1000, FullDPS: 1000, WithIgniteDPS: 1100, Life: 1000, EnergyShield: 1000 }));
  expect(hideDuplicateRow('TotalDPS', values)).toBe(true);
  expect(hideDuplicateRow('FullDPS', values)).toBe(true);
  expect(hideDuplicateRow('WithIgniteDPS', values)).toBe(false);
  expect(hideDuplicateRow('Life', values)).toBe(false);
  expect(hideDuplicateRow('EnergyShield', values)).toBe(false);
});
