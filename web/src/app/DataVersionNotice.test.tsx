import { renderToStaticMarkup } from 'react-dom/server';
import { expect, test } from 'vitest';
import { DataVersionNotice } from './DataVersionNotice';

test.each(['en-US', 'zh-CN', 'zh-TW'] as const)('a stage from another data version shows the notice in %s', lang => {
  const html = renderToStaticMarkup(<DataVersionNotice savedVersion="0.5.5.5" currentVersion="1.0.0.1" lang={lang} />);
  expect(html).toContain('role="status"');
  expect(html).toContain('0.5.5.5');
  expect(html).toContain('1.0.0.1');
  expect(html).toContain(lang === 'en-US' ? 'now calculated' : lang === 'zh-CN' ? '当前按' : '目前按');
});

test('same-version and legacy stages have no mismatch notice', () => {
  for (const savedVersion of [undefined, '0.5.5.5']) {
    expect(renderToStaticMarkup(<DataVersionNotice savedVersion={savedVersion} currentVersion="0.5.5.5" lang="en-US" />)).toBe('');
  }
});
