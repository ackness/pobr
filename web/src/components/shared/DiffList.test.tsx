import { renderToStaticMarkup } from 'react-dom/server';
import { expect, test } from 'vitest';
import { DiffList } from './DiffList';

test('item and passive gains use the same metric precision as their results', () => {
  const html = renderToStaticMarkup(<DiffList lang="zh-CN" diffs={[
    { id: 'Life', before: 200, after: 201.83, delta: 1.83 },
    { id: 'TotalDPS', before: 20000, after: 7654.322, delta: -12345.678 },
    { id: 'CritChance', before: 20, after: 22.35, delta: 2.35 },
    { id: 'CritMultiplier', before: 1.5, after: 1.8, delta: 0.3 },
  ]} />);
  expect(html).toContain('生命 +2');
  expect(html).toContain('击中 DPS -12,346');
  expect(html).toContain('暴击率 +2.4 个百分点');
  expect(html).toContain('暴击伤害加成 +30 个百分点');
});
