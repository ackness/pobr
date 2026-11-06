import { renderToStaticMarkup } from 'react-dom/server';
import { expect, it } from 'vitest';
import { SwapGainCard } from './SwapGainCard';
import { minionDpsNote } from '../../lib/mainMinion';

it('prioritizes combined DPS, effective HP and life with before/after and percentages, folding all other changes once', () => {
  const before = [{ id: 'TotalDPS', value: 80 }, { id: 'CombinedDPS', value: 100 }, { id: 'Life', value: 200 }, { id: 'TotalEHP', value: 1000 }, { id: 'Armour', value: 500 }];
  const after = [{ id: 'TotalDPS', value: 100 }, { id: 'CombinedDPS', value: 150 }, { id: 'Life', value: 250 }, { id: 'TotalEHP', value: 800 }, { id: 'Armour', value: 500 }];
  const html = renderToStaticMarkup(<SwapGainCard before={before} after={after} lang="zh-CN" />);
  expect(html.indexOf('data-stat="CombinedDPS"')).toBeLessThan(html.indexOf('data-stat="TotalEHP"'));
  expect(html.indexOf('data-stat="TotalEHP"')).toBeLessThan(html.indexOf('data-stat="Life"'));
  expect(html).toContain('+50.0%');
  expect(html).toContain('-20.0%');
  expect(html).toContain('100</span> → <b class="swap-after">150');
  expect(html).toContain('<details><summary class="disclosure" data-disclosure-level="inline">全部变化（1）</summary>');
  expect(html).not.toContain('<details open');
  expect(html).not.toContain('Armour');
  expect(html.match(/data-stat="CombinedDPS"/g)).toHaveLength(1);
});

it('keeps unavailable stats unavailable and uses an absolute change for a zero baseline', () => {
  const html = renderToStaticMarkup(<SwapGainCard before={[{ id: 'CombinedDPS', value: null }, { id: 'Life', value: 0 }]}
    after={[{ id: 'CombinedDPS', value: 100 }, { id: 'Life', value: 50 }]} lang="en-US" />);
  expect(html).not.toContain('Infinity');
  expect(html).not.toContain('NaN');
  expect(html).not.toContain('%');
  expect(html).toContain('>+50<');
  expect(html).toContain('>—<');
});

it('leads with the main minion gain when player hit and combined DPS do not change', () => {
  const before = [{ id: 'TotalDPS', value: 0 }, { id: 'CombinedDPS', value: 0 }, { id: 'MainMinionDPS', value: 100.4 }];
  const after = before.map(row => ({ ...row, value: row.id === 'MainMinionDPS' ? 150.6 : row.value }));
  const html = renderToStaticMarkup(<SwapGainCard before={before} after={after} lang="zh-CN"
    damage={{ stat: 'MainMinionDPS', label: '召唤物击中 DPS', note: minionDpsNote('zh-CN', '狼', 7) }} />);
  const headline = html.match(/data-stat="MainMinionDPS"[\s\S]*?<\/div>/)![0];
  expect(headline).toContain('召唤物击中 DPS');
  expect(headline).toContain('狼 · 单只 · 上限 7 只');
  expect(headline).toContain('+50.0%');
  expect(headline).toContain('100.4</span> → <b class="swap-after">150.6');
  expect(html).not.toContain('data-stat="CombinedDPS"');
  expect(html).not.toContain('全部变化');
});

it.each(['en-US', 'zh-CN', 'zh-TW'] as const)('uses metric precision for absolute gains and groups negative DPS in %s', lang => {
  const html = renderToStaticMarkup(<SwapGainCard
    before={[{ id: 'CombinedDPS', value: 0 }, { id: 'TotalEHP', value: 0 }, { id: 'Life', value: 0 }, { id: 'TotalDPS', value: 12345.67 }]}
    after={[{ id: 'CombinedDPS', value: 12345.67 }, { id: 'TotalEHP', value: 1001.83 }, { id: 'Life', value: 1.83 }, { id: 'TotalDPS', value: 0 }]} lang={lang} />);
  const metric = (id: string) => html.match(new RegExp(`data-stat="${id}"[\\s\\S]*?</div>`))![0];
  expect(metric('CombinedDPS')).toContain('<strong class="delta-pos">+12,346</strong>');
  expect(metric('TotalEHP')).toContain('<strong class="delta-pos">+1,002</strong>');
  expect(metric('Life')).toContain('<strong class="delta-pos">+2</strong>');
  expect(metric('Life')).toContain('<span><span>0</span> → <b class="swap-after">2</b></span>');
  expect(html).toContain('<td>12,346</td><td>0</td><td>-12,346</td>');
});

it('uses shared grouping and precision for relative gains too', () => {
  const html = renderToStaticMarkup(<SwapGainCard before={[{ id: 'Life', value: 100 }]}
    after={[{ id: 'Life', value: 12445.67 }]} lang="en-US" />);
  expect(html).toContain('<strong class="delta-pos">+12,345.7%</strong>');
});


it.each([
  [null, 100, '—', '100'],
  [100, null, '100', '—'],
  [undefined, 0, '—', '0'],
  [0, undefined, '0', '—'],
  [undefined, undefined, '—', '—'],
] as const)('renders unavailable before and after values as dashes (%s → %s)', (before, after, shownBefore, shownAfter) => {
  const html = renderToStaticMarkup(<SwapGainCard
    before={before === undefined ? [] : [{ id: 'CombinedDPS', value: before }]}
    after={after === undefined ? [] : [{ id: 'CombinedDPS', value: after }]} lang="en-US" />);
  const combined = html.match(/data-stat="CombinedDPS"[\s\S]*?<\/div>/)![0];
  expect(combined).toContain(`<span><span>${shownBefore}</span> → <b class="swap-after">${shownAfter}</b></span>`);
  expect(html).not.toContain('∞');
});
