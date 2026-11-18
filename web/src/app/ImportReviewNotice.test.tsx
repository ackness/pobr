import { isValidElement, type ReactElement, type ReactNode } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { afterEach, expect, it, vi } from 'vitest';
import type { CalculateBuildResponse } from '../api/types';
import { ImportReviewNotice } from './ImportReviewNotice';
import type { ImportSummary } from './importReview';
import type { Route } from './routes';

const effects = vi.hoisted(() => [] as (() => void)[]);
vi.mock('react', async importOriginal => ({
  ...await importOriginal<typeof import('react')>(),
  useEffect: (effect: () => void) => { effects.push(effect); },
}));
afterEach(() => { effects.length = 0; vi.unstubAllGlobals(); });

const summary: ImportSummary = {
  character: { class_name: 'Mercenary', ascendancy_name: 'Tactician', level: 98 },
  classNames: { classes: {}, ascendancies: {} },
  calc: { unsupported_modifiers: ['10% increased Damage', 'Gain an unknown buff'] } as CalculateBuildResponse,
};
const props = (route: Route = 'overview', data = summary) => ({ route, summary: data, lang: 'en-US' as const,
  onDismiss: vi.fn(), onReview: vi.fn(), onReviewNotes: vi.fn() });

function elements(node: ReactNode): ReactElement<Record<string, any>>[] {
  if (Array.isArray(node)) return node.flatMap(elements);
  if (!isValidElement(node)) return [];
  const element = node as ReactElement<Record<string, any>>;
  return [element, ...elements(element.props.children)];
}

it('removes the notice after visiting Configuration through any path', () => {
  let visible = true;
  const dismiss = vi.fn(() => { visible = false; });
  const render = (route: Route) => visible ? renderToStaticMarkup(<ImportReviewNotice {...props(route)} onDismiss={dismiss} />) : '';
  expect(render('overview')).toContain('Build imported');
  effects.splice(0).forEach(effect => effect());
  expect(dismiss).not.toHaveBeenCalled();
  expect(render('config')).toBe('');
  effects.splice(0).forEach(effect => effect());
  expect(dismiss).toHaveBeenCalledOnce();
  expect(render('overview')).toBe('');
});

it('links to the permanent diagnostics rather than repeating results or raw modifier text', () => {
  for (const lang of ['en-US', 'zh-CN', 'zh-TW'] as const) {
    const html = renderToStaticMarkup(<ImportReviewNotice {...props()} lang={lang} />);
    expect(html).toContain('href="/config"');
    expect(html).toContain('href="#ov-unincluded"');
    expect(html).not.toContain('Tactician');
    expect(html).not.toContain('Gain an unknown buff');
    expect(html).not.toContain('Check import notes');
    expect(html).toContain(lang === 'en-US' ? 'View 2 modifiers not included in calculation'
      : lang === 'zh-CN' ? '查看 2 条未计入计算的词缀' : '查看 2 條未計入計算的詞綴');
  }
  expect(renderToStaticMarkup(<ImportReviewNotice {...props()} modifierCount={0} />)).not.toContain('#ov-unincluded');
  expect(renderToStaticMarkup(<ImportReviewNotice {...props('build')} />)).not.toContain('#ov-unincluded');
});

it('opens and focuses the existing diagnostic on request, and offers config navigation and dismissal', () => {
  const values = props();
  const nodes = elements(ImportReviewNotice(values));
  const details = { open: false };
  const target = { focus: vi.fn(), closest: vi.fn(() => details) };
  vi.stubGlobal('document', { getElementById: () => target });
  const view = nodes.find(node => node.type === 'a' && node.props.href === '#ov-unincluded')!;
  view.props.onClick({ preventDefault() {} });
  expect(details.open).toBe(true);
  expect(target.focus).toHaveBeenCalledOnce();
  nodes.find(node => node.props.href === '/config')!.props.onClick({ preventDefault() {} });
  expect(values.onReview).toHaveBeenCalledOnce();
  nodes.find(node => node.props.onClick === values.onDismiss)!.props.onClick();
  expect(values.onDismiss).toHaveBeenCalledOnce();
});

it('locates WeGame import notes separately from retained but uncalculated modifiers', () => {
  const values = props('overview', { ...summary, importSource: 'wegame' });
  const html = renderToStaticMarkup(<ImportReviewNotice {...values} />);
  expect(html).toContain('WeGame import can skip unrecognized content');
  expect(html).toContain('href="/build#bd-notes"');
  expect(html).toContain('Skills → the skill group → Skill weapon set');
  expect(html).toContain('href="#ov-unincluded"');
  elements(ImportReviewNotice(values)).find(node => node.props.href === '/build#bd-notes')!.props.onClick({ preventDefault() {} });
  expect(values.onReviewNotes).toHaveBeenCalledOnce();
});

it('distinguishes a pending calculation from a calculation error without showing stale diagnostics', () => {
  const waiting = { ...summary, calc: null };
  const pending = renderToStaticMarkup(<ImportReviewNotice {...props('overview', waiting)} modifierCount={2} />);
  expect(pending).toContain('Calculating…');
  expect(pending).not.toContain('#ov-unincluded');
  const failed = renderToStaticMarkup(<ImportReviewNotice {...props('overview', { ...waiting, failed: true })} />);
  expect(failed).toContain('Calculation unavailable.');
  expect(failed).not.toContain('Calculating…');
});
