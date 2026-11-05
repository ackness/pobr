import type { ComponentProps } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { afterEach, beforeEach, expect, test, vi } from 'vitest';
import { TradePanel } from './TradePanel';
import type { BuildSession } from '../../hooks/useBuildSession';
import type { TradeOptimization } from '../../lib/tradeOptimizer';

const ui = vi.hoisted(() => ({ index: 0, refIndex: 0, weights: undefined as TradeOptimization | undefined,
  buttons: [] as ComponentProps<'button'>[], scroll: vi.fn(), focus: vi.fn() }));
vi.mock('react', async original => {
  const react = await original<typeof import('react')>();
  return { ...react,
    useState: (initial: unknown) => {
      const index = ui.index++;
      const value = index === 0 ? { bases: [], mods: [] }
        : index === 14 ? { weapon1: { category: 'weapon.wand', weights: ui.weights } }
        : typeof initial === 'function' ? initial() : initial;
      return [value, vi.fn()];
    },
    useRef: (initial: unknown) => ui.refIndex++ === 1
      ? { current: { scrollIntoView: ui.scroll, focus: ui.focus } } : { current: initial },
  };
});
vi.mock('../../hooks/useUpgradeGoal', () => ({ useUpgradeGoal: () => ({ goal: { preset: 'life', cStat: '', cMin: '', cMax: '' }, setGoal: vi.fn() }) }));
vi.mock('../../hooks/useSkillName', () => ({ useSkillCatalog: () => new Map() }));
vi.mock('../../hooks/useLocalizedLines', () => ({ useItemDisplayNames: () => [], useLocalizedLines: (lines: string[]) => lines }));
vi.mock('../../app/jobs/useJobBeacon', () => ({ useJobBeacon: () => {} }));
vi.mock('../shared/PreviewContext', () => ({ usePreview: () => null }));
vi.mock('./ItemReplacementCheck', () => ({ ItemReplacementCheck: () => null }));
vi.mock('../shared/AppSelect', () => ({ AppSelect: () => null }));
vi.mock('../shared/controls', () => ({
  AppButton: (props: ComponentProps<'button'>) => { ui.buttons.push(props); return <button {...props} />; },
  AppInput: (props: ComponentProps<'input'>) => <input {...props} />,
}));

beforeEach(() => {
  ui.index = 0; ui.refIndex = 0; ui.buttons = []; ui.scroll.mockClear(); ui.focus.mockClear();
  vi.stubGlobal('localStorage', { getItem: () => null });
  ui.weights = { weighted: [], baseline: { Life: 100 }, combinations: [], evaluated: 1, limited: false,
    minimumWeight: 0, unsupported: [], currentItemScore: null, scoreWarnings: [] };
});
const render = () => renderToStaticMarkup(<TradePanel active lang="en-US" session={{
  socketGroups: [], calcParams: {}, calc: { stats: [] }, items: [], flasks: [], jewels: [], allocatedNodes: [],
} as unknown as BuildSession} />);

test('no useful scores disable market search and offer a focused target adjustment', () => {
  const html = render();
  expect(html).toContain('a trade search cannot be generated');
  expect(html).not.toContain('class="trade-primary trade-market-link"');
  expect(html).not.toContain('trade-search-card');
  expect(ui.buttons.find(button => button.children === 'Browse matching items')?.disabled).toBe(true);
  ui.buttons.find(button => button.children === 'Change target or goal')!.onClick!({} as never);
  expect(ui.scroll).toHaveBeenCalledWith({ block: 'start' });
  expect(ui.focus).toHaveBeenCalledWith({ preventScroll: true });
});

test('usable scores show integer ranking scores, one-decimal gains and weights in details', () => {
  ui.weights!.weighted = [{ id: 'explicit.test', line: '+10 to maximum Life', value: 10, weight: 32.046, gain: 10, gainPercent: 1.234 }];
  ui.weights!.currentItemScore = { score: 797.094, complete: true, contributions: [], unscoredLines: [], matchedLines: 1, totalLines: 1 };
  const html = render();
  expect(html).toContain('<strong>797</strong>');
  expect(html).toContain('+1.2%');
  expect(html).toContain('trade-search-card');
  const table = html.slice(html.indexOf('class="trade-score-table"'), html.indexOf('class="trade-method"'));
  expect(table).not.toContain('32.046');
  expect(html).toMatch(/<details class="trade-method"><summary>Calculation details: trade weights<\/summary>.*32.046/);
});

afterEach(() => vi.unstubAllGlobals());
