import { expect, it } from 'vitest';
import { isContinuation, lineText, shownLines } from './continuedLines';

const sources = ['Gain 4% of Damage as Extra Fire Damage for', 'every different Grenade fired in the past 8 seconds', '', '+20% to Fire Resistance'];
const translated = ['过去 8 秒内每发射一种不同的榴弹，获得相当于伤害 4% 的额外火焰伤害', '', '', undefined];

it('a covered line is the "" of a non-blank source; a blank source stays a blank line', () => {
  expect(sources.map((source, index) => isContinuation(source, translated[index]))).toEqual([false, true, false, false]);
});

it('a list drops covered lines and shows a missing translation as its source', () => {
  expect(shownLines(sources, translated)).toEqual([translated[0], '', '+20% to Fire Resistance']);
});

it('a row that cannot be dropped keeps its source text', () => {
  expect(sources.map((source, index) => lineText(source, translated[index])))
    .toEqual([translated[0], sources[1], '', '+20% to Fire Resistance']);
});
