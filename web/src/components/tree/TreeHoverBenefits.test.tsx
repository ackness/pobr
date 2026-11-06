import { renderToStaticMarkup } from 'react-dom/server';
import { expect, test } from 'vitest';
import { treeHoverChanges, TreeHoverBenefits } from './TreeHoverBenefits';
import { refundDelta, treeStatDelta } from './treePresentation';
import type { CalculateBuildResponse } from '../../api/types';

const diff = (id: string, before: number, delta: number) => ({ id, before, after: before + delta, delta });

test('hover prioritizes combined damage and survival, then two relative changes; details name the remaining count', () => {
  const diffs = [diff('TotalDPS', 1e8, -1e6), diff('Armour', 100, -50), diff('Mana', 50, 50),
    diff('CombinedDPS', 1e9, -228958888), diff('Life', 5000, -400), diff('TotalEHP', 1e6, -12000),
    diff('CritChance', 50, 0.5), diff('ActionRate', 4, 0.004)];
  const ordered = treeHoverChanges(diffs);
  expect(ordered.primary.map(row => row.id)).toEqual(['CombinedDPS', 'TotalEHP', 'Life', 'Mana', 'Armour']);
  const html = renderToStaticMarkup(<TreeHoverBenefits diffs={diffs} lang="zh-CN" refundCount={12} />);
  expect(html).toContain('综合 DPS -228,958,888');
  expect(html).toContain('将一并退掉 11 个下游节点');
  expect(html).toContain('<summary>另有 2 项变化</summary>');
  expect(html).not.toContain('-228958888');
  expect(html).not.toContain('+100<');
});

test('percentage differences have units and a zero or missing downstream count has no warning', () => {
  const html = renderToStaticMarkup(<TreeHoverBenefits diffs={[diff('CritMultiplier', 2, -0.2)]} lang="en-US" refundCount={1} />);
  expect(html).toContain('-20 pp');
  expect(html).not.toContain('downstream');
});

const result = (stats: Record<string, number | null>) => ({ stats: Object.entries(stats).map(([id, value]) => ({ id, value })) }) as CalculateBuildResponse;
test('refund result uses CombinedDPS including damage over time, with a grouped delta and relative loss', () => {
  const a = result({ CombinedDPS: 1000000, TotalDPS: 100 });
  const b = result({ CombinedDPS: 771000, TotalDPS: 100 });
  expect(refundDelta(a, b, 'en-US')).toBe('-229,000 (-22.9%)');
  expect(refundDelta(a, b, 'zh-CN')).toBe('-229,000（-22.9%）');
  expect(treeStatDelta(a, b, 'TotalDPS')).toBe(0);
  expect(refundDelta(a, result({ TotalDPS: 100 }), 'zh-CN')).toContain('暂不可用');
  expect(treeStatDelta(a, result({ CombinedDPS: null }), 'CombinedDPS')).toBeNull();
});


test('redundant damage totals and raw unknown output keys stay in collapsed details', () => {
  const unknown = diff('UntranslatedFutureOutput', 1, 10000);
  const rows = treeHoverChanges([diff('CombinedDPS', 1e6, -200000), diff('TotalDPS', 1e5, -1e5),
    diff('WithIgniteDPS', 1e5, -1e5), unknown, diff('EnergyShield', 100, -5)]);
  expect(rows.primary.map(row => row.id)).toEqual(['CombinedDPS', 'EnergyShield']);
  expect(rows.more.map(row => row.id)).toContain('UntranslatedFutureOutput');
});
