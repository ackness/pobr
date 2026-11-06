import { isValidElement, type ReactElement, type ReactNode } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { afterEach, expect, it, vi } from 'vitest';
import type { CalculateBuildResponse } from '../api/types';
import { mainMinion } from '../lib/mainMinion';
import { ImportReviewNotice } from './ImportReviewNotice';
import type { ImportSummary } from './importReview';
import type { Route } from './routes';

const lifecycle = vi.hoisted(() => ({ effects: [] as (() => void)[], expanded: false }));
vi.mock('react', async importOriginal => ({
  ...await importOriginal<typeof import('react')>(),
  useEffect: (effect: () => void) => { lifecycle.effects.push(effect); },
  useState: () => [lifecycle.expanded, (value: (previous: boolean) => boolean) => { lifecycle.expanded = value(lifecycle.expanded); }],
}));
vi.mock('../hooks/useSkillName', () => ({ useSkillName: () => () => 'Wolf Pack' }));
vi.mock('../hooks/useMainMinion', () => ({ useMainMinion: (calc: CalculateBuildResponse | null) => {
  const minion = mainMinion(calc);
  return minion ? { minion, name: minion.name } : null;
} }));
afterEach(() => { lifecycle.effects = []; lifecycle.expanded = false; });

const summary: ImportSummary = {
  character: { class_name: 'Mercenary', ascendancy_name: 'Tactician', level: 98 },
  classNames: { classes: {}, ascendancies: {} },
  calc: { main_skill: { skill_id: 'WolfPack' }, stats: [{ id: 'CombinedDPS', value: 900000 }, { id: 'ActiveMinionLimit', value: 7 }],
    main_minion_index: 1, minions: [{ index: 1, name: 'Wolf', dps: 5307 }],
    unsupported_modifiers: ['10% increased Damage', 'Gain an unknown buff'],
  } as CalculateBuildResponse,
};
const props = (route: Route = 'overview', data = summary) => ({ route, summary: data, lang: 'en-US' as const, onDismiss: vi.fn(), onReview: vi.fn() });

function elements(node: ReactNode): ReactElement<Record<string, any>>[] {
  if (Array.isArray(node)) return node.flatMap(elements);
  if (!isValidElement(node)) return [];
  const element = node as ReactElement<Record<string, any>>;
  return [element, ...elements(element.props.children)];
}

it('removes the notice after visiting Config through any navigation path', () => {
  let visible = true;
  const dismiss = vi.fn(() => { visible = false; });
  const review = vi.fn();
  const render = (route: Route) => visible ? renderToStaticMarkup(<ImportReviewNotice {...props(route)} onDismiss={dismiss} onReview={review} />) : '';
  expect(render('overview')).toContain('Imported: Tactician · Level 98');
  lifecycle.effects.splice(0).forEach(effect => effect());
  expect(dismiss).not.toHaveBeenCalled();
  expect(render('config')).toBe('');
  lifecycle.effects.splice(0).forEach(effect => effect());
  expect(dismiss).toHaveBeenCalledOnce();
  expect(render('overview')).toBe('');
  expect(review).not.toHaveBeenCalled();
});

it('summarises the calculated skill, per-minion hit DPS, modifier count and configuration hint in all languages', () => {
  for (const lang of ['en-US', 'zh-CN', 'zh-TW'] as const) {
    const html = renderToStaticMarkup(<ImportReviewNotice {...props()} lang={lang} />);
    expect(html).toContain('98');
    expect(html).toContain('Wolf Pack');
    expect(html).toContain('5,307');
    expect(html).not.toContain('900,000');
    expect(html).toContain('href="/config"');
    expect(html).toContain('href="#import-summary-modifiers"');
    if (lang === 'en-US') {
      expect(html).toContain('Calculated skill: Wolf Pack');
      expect(html).toContain('Minion hit DPS');
      expect(html).toContain('per minion');
      expect(html).toContain('Modifiers not included: 2');
      expect(html).toContain('Check quest rewards and enemy type.');
    } else {
      expect(html).toContain(lang === 'zh-CN' ? '计算技能：Wolf Pack' : '計算技能：Wolf Pack');
      expect(html).toContain(lang === 'zh-CN' ? '未计入计算的词缀：2 条' : '未計入計算的詞綴：2 條');
      expect(html).toContain(lang === 'zh-CN' ? '单只' : '單隻');
    }
  }
});

it('uses Combined DPS for a player build and replaces the previous import content', () => {
  const next = { ...summary, character: { class_name: 'Witch', ascendancy_name: '', level: 40 },
    calc: { ...summary.calc!, minions: [], main_minion_index: null, unsupported_modifiers: [] } };
  const html = renderToStaticMarkup(<ImportReviewNotice {...props('build', next)} />);
  expect(html).toContain('Imported: Witch · Level 40');
  expect(html).toContain('Combined DPS');
  expect(html).toContain('900,000');
  expect(html).toContain('Modifiers not included: 0');
  expect(html).not.toContain('Tactician');
  expect(html).not.toContain('Minion hit DPS');
});

it('offers dismissal, configuration navigation and access to the actual missing modifier text', () => {
  const values = props();
  const nodes = elements(ImportReviewNotice(values));
  const view = nodes.find(node => node.type === 'a' && node.props.href === '#import-summary-modifiers')!;
  view.props.onClick({ preventDefault() {} });
  const expanded = renderToStaticMarkup(<ImportReviewNotice {...values} />);
  expect(expanded).toContain('10% increased Damage');
  expect(expanded).toContain('Gain an unknown buff');
  nodes.find(node => node.type === 'a' && node.props.href === '/config')!.props.onClick({ preventDefault() {} });
  expect(values.onReview).toHaveBeenCalledOnce();
  nodes.find(node => node.props.onClick === values.onDismiss)!.props.onClick();
  expect(values.onDismiss).toHaveBeenCalledOnce();
});

it('does not show stale damage or invent a zero while waiting for calculation', () => {
  const html = renderToStaticMarkup(<ImportReviewNotice {...props('overview', { ...summary, calc: null })} />);
  expect(html).toContain('Calculating…');
  expect(html).not.toContain('Modifiers not included: 0');
  expect(html).not.toContain('900,000');
});


it('explains a failed calculation instead of leaving the summary calculating forever', () => {
  const html = renderToStaticMarkup(<ImportReviewNotice {...props('overview', { ...summary, calc: null, failed: true })} />);
  expect(html).toContain('Calculation unavailable.');
  expect(html).not.toContain('Calculating…');
  expect(html).not.toContain('900,000');
});
