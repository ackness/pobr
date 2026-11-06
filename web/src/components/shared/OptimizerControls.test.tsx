import type { ComponentProps } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { beforeEach, expect, it, vi } from 'vitest';
import { OptimizerResults } from './OptimizerControls';

const buttons = vi.hoisted(() => [] as ComponentProps<'button'>[]);
vi.mock('./controls', () => ({
  AppButton: (props: ComponentProps<'button'>) => { buttons.push(props); return <button {...props} />; },
  AppInput: (props: ComponentProps<'input'>) => <input {...props} />,
}));
beforeEach(() => { buttons.length = 0; });

it('labels mixed scores and distinguishes gains, losses and unchanged results', () => {
  const html = renderToStaticMarkup(<OptimizerResults baseline={{ TotalDPS: 100.4, TotalEHP: 100.4 }}
    objective={{ stat: 'TotalDPS', secondaryStat: 'TotalEHP', constraints: [] }} lang="en-US" onApply={() => {}}
    ranked={[
      { index: 0, error: null, label: 'Worse', stats: { TotalDPS: 38.7544, TotalEHP: 38.7544 } },
      { index: 1, error: null, label: 'Same', stats: { TotalDPS: 100.4, TotalEHP: 100.4 } },
      { index: 2, error: null, label: 'Better', stats: { TotalDPS: 200.8, TotalEHP: 200.8 } },
    ]} />);
  expect(html).toContain('Balanced score');
  expect(html).toContain('Change from current');
  expect(html).toContain('Current equipment</td><td class="opt-score">100.4</td>');
  expect(html).toContain('class="opt-delta is-negative">-61.4%');
  expect(html).toContain('class="opt-delta">+0.0%');
  expect(html).toContain('class="opt-delta is-positive">+100.0%');
  expect(html).toContain('Equip anyway');
});

it('keeps zero-baseline comparisons neutral and uses a plain score for a single goal', () => {
  const html = renderToStaticMarkup(<OptimizerResults baseline={{ Life: 0 }}
    objective={{ stat: 'Life', constraints: [] }} lang="en-US" onApply={() => {}}
    ranked={[{ index: 0, error: null, label: 'First item', stats: { Life: 100 } }]} />);
  expect(html).toContain('>Score</th>');
  expect(html).not.toContain('Balanced score');
  expect(html).toContain('class="opt-delta">—');
  expect(html).not.toContain('is-positive');
});

it('applies the selected negative result and keeps apply buttons disabled while busy', () => {
  const variant = { index: 3, error: null, label: 'Worse', stats: { Life: 40 } };
  const onApply = vi.fn();
  const props = { baseline: { Life: 100 }, objective: { stat: 'Life', constraints: [] }, ranked: [variant], onApply, lang: 'en-US' as const };
  renderToStaticMarkup(<OptimizerResults {...props} />);
  expect(buttons[0].children).toBe('Equip anyway');
  buttons[0].onClick!({} as never);
  expect(onApply).toHaveBeenCalledWith(variant);
  buttons.length = 0;
  const busy = renderToStaticMarkup(<OptimizerResults {...props} disabled />);
  expect(buttons[0].disabled).toBe(true);
  expect(busy).toContain('disabled=""');
});
