import type { ComponentProps } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { beforeEach, expect, it, vi } from 'vitest';
import type { AttributionResponse } from '../../api/types';
import type { BuildSession } from '../../hooks/useBuildSession';
import { contributionFields, ItemContribution } from './ItemContribution';

const ui = vi.hoisted(() => ({ values: [] as unknown[], cursor: 0, button: undefined as ComponentProps<'button'> | undefined,
  effects: [] as (() => void)[], cleanups: [] as (() => void)[] }));
vi.mock('react', async original => ({ ...await original<typeof import('react')>(),
  useState: (initial: unknown) => {
    const index = ui.cursor++;
    if (!(index in ui.values)) ui.values[index] = initial;
    return [ui.values[index], (value: unknown) => { ui.values[index] = value; }];
  },
  useRef: (initial: unknown) => {
    const index = ui.cursor++;
    return ui.values[index] ??= { current: initial };
  },
  useEffect: (callback: () => (() => void), deps: unknown[]) => {
    const index = ui.cursor++;
    const previous = ui.values[index] as unknown[] | undefined;
    if (!previous || deps.some((value, i) => value !== previous[i])) {
      ui.values[index] = deps;
      ui.effects.push(() => { ui.cleanups[index]?.(); ui.cleanups[index] = callback(); });
    }
  },
}));
vi.mock('../shared/controls', () => ({ AppButton: (props: ComponentProps<'button'>) => { ui.button = props; return <button {...props} />; } }));
beforeEach(() => { ui.values = []; ui.effects = []; ui.cleanups = []; ui.button = undefined; });
const report: AttributionResponse = { baseline: { CombinedDPS: 1000, TotalEHP: 500 }, entries: [
  { kind: 'item', id: 'weapon1', deltas: { CombinedDPS: 300, TotalEHP: -20 } },
  { kind: 'item', id: 'ring1', deltas: { CombinedDPS: 10, TotalEHP: 50 } },
] };
const session = () => ({ stateVersion: 1, calc: {}, busy: false, runAttribution: vi.fn().mockResolvedValue(report) }) as unknown as BuildSession;
function render(current: BuildSession, slot = 'weapon1') {
  ui.cursor = 0; ui.button = undefined;
  const html = renderToStaticMarkup(<ItemContribution session={current} slot={slot} lang="en-US" />);
  ui.effects.splice(0).forEach(effect => effect());
  return html;
}

it('reuses the existing slot contribution with signed removal losses and explains swap gains', () => {
  const current = { ...session(), sourceContributions: report };
  const html = render(current);
  expect(html).toContain('Combined DPS 300 · Effective HP -20');
  expect(html).toContain('not the gain from swapping');
  expect(ui.button).toBeUndefined();
  expect(current.runAttribution).not.toHaveBeenCalled();
  expect(render(current, 'ring1')).toContain('Combined DPS 10 · Effective HP 50');
});

it('offers the existing attribution action when the slot result is missing and keeps absent numbers unknown', async () => {
  const current = session();
  expect(render(current)).toContain('No contribution result');
  await ui.button!.onClick!({} as never);
  await vi.waitFor(() => expect(render(current)).toContain('Combined DPS 300 · Effective HP -20'));
  expect(current.runAttribution).toHaveBeenCalledWith(contributionFields);
  expect(render(current, 'helmet')).toContain('No contribution result');
  expect(ui.button?.disabled).toBe(false);
  current.busy = true;
  render(current, 'helmet');
  expect(ui.button?.disabled).toBe(true);
});

it('discards a contribution calculation after the build changes', async () => {
  let resolve!: (report: AttributionResponse) => void;
  const pending = new Promise<AttributionResponse>(done => { resolve = done; });
  const current = session();
  vi.mocked(current.runAttribution).mockReturnValue(pending);
  render(current); ui.button!.onClick!({} as never);
  render(current);
  expect(ui.button?.disabled).toBe(true);
  current.stateVersion++;
  render(current);
  resolve(report); await pending; await Promise.resolve();
  expect(render(current)).toContain('No contribution result');
});
