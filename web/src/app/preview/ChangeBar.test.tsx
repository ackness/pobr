import { renderToStaticMarkup } from 'react-dom/server';
import { expect, test } from 'vitest';
import { PreviewContext, type PreviewApi } from '../../components/shared/PreviewContext';
import { computeStatChanges } from '../motion/statChanges';
import { ChangeBar } from './ChangeBar';

test('Combined DPS hover keeps fractional final values below 1000', () => {
  const row = (value: number) => ({ id: 'CombinedDPS', category: 'Offence' as const, value });
  const preview: PreviewApi = {
    state: { label: 'Support', status: 'ready', changes: [...computeStatChanges([row(998.37)], [row(999.37)]).values()] },
    show: () => {}, showChanges: () => {}, clear: () => {},
  };
  const html = renderToStaticMarkup(<PreviewContext value={preview}><ChangeBar lang="zh-CN" /></PreviewContext>);
  expect(html).toContain('<span class="cb-values">998.37 → 999.37</span>');
});
