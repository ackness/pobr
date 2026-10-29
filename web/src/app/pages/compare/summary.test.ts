import { describe, expect, it } from 'vitest';
import type { DisplayStatValue } from '../../../api/types';
import { compareStats } from '../../../lib/compare';
import { collapseContext, countPhrase, defaultCompareTab, deltaTone, headlineDeltas, signedPercent, summaryLine, totalChanges, treeDifferenceCount, type DiffCounts } from './summary';

const stat = (id: string, value: number): DisplayStatValue => ({ id, value, category: 'Offence' });
const counts = (over: Partial<DiffCounts> = {}): DiffCounts => ({ stats: 0, items: 0, skills: 0, config: 0, tree: 0, ...over });

describe('compare summary', () => {
  it('opens on the first part with a difference, otherwise on stats', () => {
    expect(defaultCompareTab(counts())).toBe('stats');
    expect(defaultCompareTab(counts({ skills: 2, tree: 9 }))).toBe('skills');
    expect(defaultCompareTab(counts({ stats: 1, items: 1 }))).toBe('stats');
  });

  it('sums every part', () => {
    expect(totalChanges(counts({ stats: 2, items: 1, tree: 4 }))).toBe(7);
  });

  it('reads one line in each language and skips empty parts', () => {
    expect(summaryLine('en-US', counts({ stats: 7, items: 1, skills: 0, config: 3 }))).toBe('B changes 7 stats, 1 item and 3 settings.');
    expect(summaryLine('en-US', counts({ skills: 1 }))).toBe('B changes 1 skill group.');
    expect(summaryLine('zh-CN', counts({ stats: 7, tree: 2 }))).toBe('B 相对 A 有 7 项属性、2 个天赋节点不同。');
    expect(summaryLine('zh-TW', counts({ items: 2 }))).toBe('B 相對 A 有 2 件裝備不同。');
    expect(summaryLine('en-US', counts())).toBe('A and B are identical.');
    expect(countPhrase('en-US', 'tree', 1)).toBe('1 passive node');
  });

  it('judges a delta by the stat, not by its sign', () => {
    expect(deltaTone('Life', 120)).toBe('better');
    expect(deltaTone('Life', -1)).toBe('worse');
    expect(deltaTone('ManaCost', 5)).toBe('worse');
    expect(deltaTone('ManaCost', -5)).toBe('better');
    expect(deltaTone('ManaCost', null)).toBe('neutral');
    expect(deltaTone('Life', 0)).toBe('neutral');
    expect(deltaTone('AreaOfEffectMod', 4)).toBe('neutral');
  });

  it('picks the damage and survivability movements for the headline', () => {
    const rows = compareStats(
      [stat('TotalDPS', 1000), stat('TotalEHP', 20000), stat('Life', 100), stat('Mana', 10)],
      [stat('TotalDPS', 1250), stat('TotalEHP', 19000), stat('Life', 100), stat('Mana', 50)],
    );
    const lines = headlineDeltas(rows);
    expect(lines.map(line => line.id)).toEqual(['TotalDPS', 'TotalEHP']);
    expect(lines[0]).toMatchObject({ percent: 25, tone: 'better' });
    expect(lines[1]).toMatchObject({ percent: -5, tone: 'worse' });
  });

  it('omits headline stats that did not move or cannot be compared', () => {
    const rows = compareStats([stat('TotalDPS', 0), stat('Life', 100)], [stat('TotalDPS', 5), stat('Life', 100)]);
    expect(headlineDeltas(rows)).toEqual([]);
  });

  it('counts passive differences only when ids mean the same node', () => {
    const a = { allocatedNodes: [1, 2, 3], jewels: [{ socket_node: 9, text: 'x' }] };
    const b = { allocatedNodes: [2, 3, 4, 5], jewels: [{ socket_node: 9, text: 'y' }] };
    expect(treeDifferenceCount(a, b, true)).toBe(4);
    expect(treeDifferenceCount(a, b, false)).toBeNull();
    expect(treeDifferenceCount(a, a, true)).toBe(0);
  });

  it('formats percentages with a real minus sign', () => {
    expect(signedPercent(12.34)).toBe('+12.3%');
    expect(signedPercent(-4.06)).toBe('−4.1%');
    expect(signedPercent(0)).toBe('0.0%');
  });
});

describe('collapseContext', () => {
  const line = (text: string, changed = false) => ({ text, changed });
  it('keeps changed lines with one line of context and folds the rest', () => {
    const rows = collapseContext([line('a'), line('b'), line('c'), line('d', true), line('e'), line('f'), line('g')]);
    expect(rows).toEqual([
      { kind: 'gap', count: 2 }, { kind: 'line', text: 'c', changed: false }, { kind: 'line', text: 'd', changed: true },
      { kind: 'line', text: 'e', changed: false }, { kind: 'gap', count: 2 },
    ]);
  });
  it('drops blank lines, trims indentation, and leaves a text with no change as one gap', () => {
    expect(collapseContext([line('\t\tx'), line('   '), line('y')])).toEqual([{ kind: 'gap', count: 2 }]);
    expect(collapseContext([line('\t\tx', true)])).toEqual([{ kind: 'line', text: 'x', changed: true }]);
    expect(collapseContext([])).toEqual([]);
  });
});
