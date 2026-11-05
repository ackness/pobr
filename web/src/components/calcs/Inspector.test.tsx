import { renderToStaticMarkup } from 'react-dom/server';
import { expect, test, vi } from 'vitest';
import Inspector from '../../app/inspector/Inspector';
import { useStatBreakdown } from '../../app/inspector/useStatBreakdown';
import type { SettledCalc } from '../../app/motion/useSettledCalc';
import type { Breakdown, CalculateBuildResponse, DetailKind } from '../../api/types';
import type { BuildSession } from '../../hooks/useBuildSession';
import { bindT, type Lang } from '../../lib/i18n';
import { TooltipProvider } from '../ui/tooltip';

vi.mock('../../app/inspector/useStatBreakdown', () => ({ useStatBreakdown: vi.fn() }));

const empty: Breakdown = { base_total: 0, inc_total: 0, more_total: 1, mods: [], formula: null };

function inspect(calc: Partial<CalculateBuildResponse>, id: string, lang: Lang) {
  const session = { calc } as BuildSession;
  return renderToStaticMarkup(<TooltipProvider><Inspector session={session} view={{ calc } as SettledCalc}
    id={id} lang={lang} pinned={false} onClose={() => {}} onOpenCalcs={() => {}} onTogglePin={() => {}} /></TooltipProvider>);
}

test.each(['en-US', 'zh-CN', 'zh-TW'] as const)('Inspector distinguishes zero rows from no breakdown in %s', lang => {
  const tt = bindT(lang);
  for (const kind of ['steps', 'derived', 'none'] as DetailKind[]) {
    for (const detail of [null, empty]) {
      vi.mocked(useStatBreakdown).mockReturnValue({ detail, loading: false, retry: vi.fn() });
      const html = inspect({ stats: [{ id: 'Life', category: 'Resource', value: 0 }], breakdowns: {}, breakdown_descriptors: { Life: kind } }, 'Life', lang);
      expect(html).toContain(tt(kind === 'none' ? 'calcs.noBreakdown' : 'calcs.zeroNoRows'));
      expect(html).not.toContain('class="breakdown-heading"');
    }
  }
  vi.mocked(useStatBreakdown).mockReturnValue({ detail: null, loading: true, retry: vi.fn() });
  const loading = inspect({ stats: [{ id: 'Life', category: 'Resource', value: 0 }], breakdowns: {}, breakdown_descriptors: { Life: 'steps' } }, 'Life', lang);
  expect(loading).toContain(tt('calcs.loadingDetail'));
  expect(loading).not.toContain(tt('calcs.zeroNoRows'));
});

test.each([['CritChance', 22.35, '22.4%'], ['FireResist', 20, '20%']] as const)('Inspector minion %s keeps its unit', (id, value, expected) => {
  vi.mocked(useStatBreakdown).mockReturnValue({ detail: null, loading: false, retry: vi.fn() });
  const html = inspect({ minions: [{ index: 7, name: 'Wolf Companion', stats: { [id]: value }, breakdowns: {}, breakdown_descriptors: { [id]: 'none' } }] } as unknown as CalculateBuildResponse, `Minion:7:${id}`, 'en-US');
  expect(html).toContain(`<p class="insp-value">${expected}</p>`);
});
