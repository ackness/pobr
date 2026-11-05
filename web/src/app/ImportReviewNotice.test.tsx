import { renderToStaticMarkup } from 'react-dom/server';
import { afterEach, expect, it, vi } from 'vitest';
import { ImportReviewNotice } from './ImportReviewNotice';
import type { Route } from './routes';

const lifecycle = vi.hoisted(() => ({ effects: [] as (() => void)[] }));
vi.mock('react', async importOriginal => ({
  ...await importOriginal<typeof import('react')>(),
  useEffect: (effect: () => void) => { lifecycle.effects.push(effect); },
}));
afterEach(() => { lifecycle.effects = []; });

it('removes the notice after visiting Config through any navigation path', () => {
  let visible = true;
  const dismiss = vi.fn(() => { visible = false; });
  const review = vi.fn();
  const render = (route: Route) => visible ? renderToStaticMarkup(<ImportReviewNotice route={route} lang="en-US" onDismiss={dismiss} onReview={review} />) : '';
  expect(render('overview')).toContain('Build imported.');
  lifecycle.effects.splice(0).forEach(effect => effect());
  expect(dismiss).not.toHaveBeenCalled();
  expect(render('config')).toBe('');
  lifecycle.effects.splice(0).forEach(effect => effect());
  expect(dismiss).toHaveBeenCalledOnce();
  expect(render('overview')).toBe('');
  expect(review).not.toHaveBeenCalled();
});
