import { expect, it } from 'vitest';
import type { CalculateBuildResponse, MinionOutput } from '../api/types';
import { MAIN_MINION_DPS, headlineDpsStat, mainMinion, minionDpsNote } from './mainMinion';

const wolf = { index: 3, name: 'Wolf Companion', dps: 5307 } as MinionOutput;
const calc = (main: number | null) => ({ minions: [{ ...wolf, index: 1 }, wolf], main_minion_index: main }) as Pick<CalculateBuildResponse, 'minions' | 'main_minion_index'>;

it('finds the calculated skill\'s minion by its index, not its position', () => {
  expect(mainMinion(calc(3))).toBe(wolf);
  expect(mainMinion(calc(null))).toBeUndefined();
  expect(mainMinion(calc(9))).toBeUndefined();
  expect(headlineDpsStat(calc(3))).toBe(MAIN_MINION_DPS);
  expect(headlineDpsStat(calc(null))).toBe('CombinedDPS');
  expect(headlineDpsStat(null)).toBe('CombinedDPS');
});

it('says the figure is one minion\'s and shows the limit only as a limit', () => {
  expect(minionDpsNote('en-US', 'Wolf', 7)).toBe('Wolf · per minion · limit 7');
  expect(minionDpsNote('zh-CN', '狼', 7)).toBe('狼 · 单只 · 上限 7 只');
  expect(minionDpsNote('zh-TW', '狼', 7)).toBe('狼 · 單隻 · 上限 7 隻');
  expect(minionDpsNote('zh-CN', '狼', 0)).toBe('狼 · 单只');
  expect(minionDpsNote('zh-CN', '狼', null)).toBe('狼 · 单只');
});
