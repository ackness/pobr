import { expect, it } from 'vitest';
import type { CalculateBuildResponse, DisplayStatValue } from '../api/types';
import { paletteStats } from './paletteStats';
const calc = (values: Record<string, number | null>) => ({ stats: Object.entries(values).map(([id, value]) => ({ id, value })) as DisplayStatValue[] }) as CalculateBuildResponse;
it('puts common resistances ahead of internal totem metrics, hides internal zeros, and formats maximums as percentages', () => {
  const result = paletteStats(calc({ TotemFireResist: 0, TotemColdResist: 0, TotemLightningResist: 12,
    FireResistMax: 78, FireResist: 75, ColdResist: 40, ChaosResist: -20, LightningResist: 0 }), 'resist', 'en-US');
  expect(result.map(row => row.id)).toEqual(['FireResist', 'ColdResist', 'LightningResist', 'ChaosResist', 'FireResistMax', 'TotemLightningResist']);
  expect(result.find(row => row.id === 'FireResistMax')?.value).toBe('78%');
  expect(result.find(row => row.id === 'LightningResist')?.value).toBe('0%');
});
it('supports one-character Chinese searches and prioritizes common metrics before truncating', () => {
  const values = Object.fromEntries(Array.from({ length: 50 }, (_, i) => [`Totem${i}Resist`, 12]));
  const result = paletteStats(calc({ ...values, FireResist: 75 }), 'Resist', 'zh-CN', 1);
  expect(result[0].id).toBe('FireResist');
  expect(paletteStats(calc({ Life: 1000 }), '生', 'zh-CN')[0].value).toBe('1,000');
});
