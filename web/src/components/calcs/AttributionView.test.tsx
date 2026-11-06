import type { ComponentProps } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { beforeEach, expect, it, vi } from 'vitest';
import type { AttributionResponse } from '../../api/types';
import type { BuildSession } from '../../hooks/useBuildSession';
import { AttributionView } from './CalcsPanel';

// Replay the component's async action and render its completed report in the SSR environment.
const state = vi.hoisted(() => ({ values: [] as unknown[], cursor: 0, click: undefined as ComponentProps<'button'>['onClick'] }));
vi.mock('react', async importOriginal => ({
  ...await importOriginal<typeof import('react')>(),
  useState: (initial: unknown) => {
    const index = state.cursor++;
    if (!(index in state.values)) state.values[index] = initial;
    return [state.values[index], (value: unknown) => { state.values[index] = value; }];
  },
}));
vi.mock('../../hooks/useSkillName', () => ({ useSkillName: () => (id: string) => id }));
vi.mock('../shared/controls', async importOriginal => ({
  ...await importOriginal<typeof import('../shared/controls')>(),
  AppButton: ({ onClick, ...props }: ComponentProps<'button'>) => { state.click = onClick; return <button {...props} />; },
}));
beforeEach(() => { state.values = []; state.cursor = 0; state.click = undefined; });

it('requests combined DPS, then shows removal losses, gains and unique source labels', async () => {
  const report: AttributionResponse = { baseline: { CombinedDPS: 2382936, Life: 100 }, entries: [
    { kind: 'item', id: 'weapon1', deltas: { CombinedDPS: 2290002, Life: -20 } },
    { kind: 'socket_group', id: '1', deltas: { CombinedDPS: 200 } },
  ] };
  const runAttribution = vi.fn().mockResolvedValue(report);
  const session = { stateVersion: 1, calcParams: {}, calc: {}, runAttribution, socketGroups: [{ gems: [] }, { gems: [{ skill_id: 'Fireball' }] }] } as unknown as BuildSession;
  const render = () => { state.cursor = 0; return renderToStaticMarkup(<AttributionView session={session} lang="en-US" />); };
  expect(render()).not.toContain('attribution-table');
  await state.click!({} as never);
  expect(runAttribution).toHaveBeenCalledWith(['CombinedDPS', 'Life', 'EnergyShield', 'TotalEHP']);
  const html = render();
  expect(html).toContain('Baseline (current build)');
  expect(html).toContain('Equipment · Main Hand');
  expect(html).toContain('Skill group 2 · Fireball');
  expect(html).not.toContain('Skill group Skill group');
  expect(html).toContain('>-2,290,002<span class="delta-pct"> (-96.1%)');
  expect(html).toContain('>+20<span class="delta-pct"> (20.0%)');
  expect(html).toContain('totals can exceed 100%');
});

it('distinguishes a changed calculated skill from a build edit', async () => {
  const runAttribution = vi.fn().mockResolvedValue({ baseline: {}, entries: [] });
  const session = { stateVersion: 1, calcParams: { main_socket_group: 0 }, calc: {}, runAttribution, socketGroups: [{ gems: [] }, { gems: [] }] } as unknown as BuildSession;
  const render = () => { state.cursor = 0; return renderToStaticMarkup(<AttributionView session={session} lang="en-US" />); };
  render(); await state.click!({} as never);
  session.stateVersion = 2; session.calcParams.main_socket_group = 1;
  expect(render()).toContain('Calculated skill changed; run the analysis again');
  session.calcParams.main_socket_group = 0;
  expect(render()).toContain('The build changed. Measure source contributions again');
});
