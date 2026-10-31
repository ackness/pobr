import { renderToStaticMarkup } from 'react-dom/server';
import { expect, it } from 'vitest';
import descriptions from '../../lib/supportDescriptions.zh-CN.json';
import { SupportDescriptions } from './SupportDescriptions';

const gem = { skill_id: 'SupportRakiatasFlowPlayer', name: '拉琪塔之流', description: descriptions.entries.SupportRakiatasFlowPlayer.en };

it('renders translated prose with its language, and labels English fallback separately', () => {
  const html = renderToStaticMarkup(<SupportDescriptions lang="zh-CN" gems={[
    gem, { skill_id: 'unknown', name: 'Unknown support', description: 'Untranslated source.' },
    { skill_id: 'missing', name: 'Missing support' },
  ]} />);
  expect(html).toContain('<summary>查看辅助效果说明</summary>');
  expect(html).toContain(`<p lang="zh-CN">${descriptions.entries.SupportRakiatasFlowPlayer.zh_cn}</p>`);
  expect(html).toContain('英文来源');
  expect(html).toContain('<p lang="en">Untranslated source.</p>');
  expect(html).toContain('此数据快照没有提供来源说明。');
  expect(html).not.toContain(gem.description);
});

it('preserves English and explains Traditional Chinese script fallback', () => {
  expect(renderToStaticMarkup(<SupportDescriptions lang="en-US" gems={[gem]} />)).toContain(`<p lang="en">${gem.description}</p>`);
  const tw = renderToStaticMarkup(<SupportDescriptions lang="zh-TW" gems={[gem]} />);
  expect(tw).toContain('轉為繁體字形');
  expect(tw).toContain('元素抗性視為反轉');
  expect(tw).toContain('lang="zh-TW"');
  expect(renderToStaticMarkup(<SupportDescriptions lang="zh-CN" gems={[]} />)).toBe('');
});
