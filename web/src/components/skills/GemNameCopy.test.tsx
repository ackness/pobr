import { renderToStaticMarkup } from 'react-dom/server';
import { expect, it } from 'vitest';
import { GemNameCopy } from './GemNameCopy';

const render = (skillId: string, name: string, lang: 'en-US' | 'zh-CN' | 'zh-TW') =>
  renderToStaticMarkup(<GemNameCopy skillId={skillId} name={name} lang={lang} />);

it('renders a labelled copy button for an active gem in each UI language', () => {
  expect(render('Fireball', 'Fireball', 'en-US')).toContain('aria-label="Copy name"');
  expect(render('Fireball', '火球术', 'zh-CN')).toContain('aria-label="复制名称"');
  expect(render('Fireball', '火球術', 'zh-TW')).toContain('aria-label="複製名稱"');
});

it('renders a copy button for a support gem', () => {
  expect(render('SupportRakiatasFlowPlayer', '拉琪塔之流', 'zh-CN')).toContain('copy-control');
});

it('renders no button for an unresolved gem', () => {
  expect(render('', 'Unresolved', 'en-US')).toBe('');
});
