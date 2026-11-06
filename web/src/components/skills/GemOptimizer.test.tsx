import type { ComponentProps } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { beforeEach, expect, it, vi } from 'vitest';
import type { BuildSession } from '../../hooks/useBuildSession';
import { GemOptimizer } from './GemOptimizer';

const harness = vi.hoisted(() => ({ states: [] as unknown[], cursor: 0, buttons: [] as ComponentProps<'button'>[], optimize: vi.fn(),
  preset: 'dps' }));
vi.mock('react', async original => ({ ...await original<typeof import('react')>(),
  useState: (initial: unknown) => {
    const index = harness.cursor++;
    if (!(index in harness.states)) harness.states[index] = typeof initial === 'function' ? initial() : initial;
    return [harness.states[index], (next: unknown) => { harness.states[index] = typeof next === 'function' ? next(harness.states[index]) : next; }];
  },
}));
vi.mock('../shared/controls', () => ({
  AppButton: (props: ComponentProps<'button'>) => { harness.buttons.push(props); return <button {...props} />; },
  AppInput: (props: ComponentProps<'input'>) => <input {...props} />,
}));
vi.mock('../../hooks/useLocalizedLines', () => ({ useLocalizedNames: (names: { name: string }[]) => names.map(entry => entry.name) }));
vi.mock('../../app/jobs/useJobBeacon', () => ({ useJobBeacon: () => {} }));
vi.mock('../shared/PreviewContext', () => ({ usePreview: () => null }));
vi.mock('../../hooks/useUpgradeGoal', () => ({ useUpgradeGoal: () => ({ goal: { preset: harness.preset, cStat: '', cMin: '', cMax: '' }, setGoal: () => {} }) }));
vi.mock('../shared/OptimizerControls', async original => ({ ...await original<typeof import('../shared/OptimizerControls')>(),
  ObjectiveEditor: () => null, OptimizerProgress: () => null,
}));
vi.mock('../../lib/supportOptimizer', async original => ({ ...await original<typeof import('../../lib/supportOptimizer')>(),
  eligibleSupports: () => ({ gems: [{ skill_id: 'support', name: 'Support' }], unknown: 28, levelBlocked: 0, incompatible: 280 }),
  lineageAvailable: () => true, optimizeSupports: harness.optimize,
}));

const catalog: never[] = [];
function setup(main = 0) {
  const request = { socket_groups: [] };
  const session = { socketGroups: [{ enabled: true, gems: [] }], character: { level: 100 }, calcParams: { main_socket_group: main },
    currentRequest: () => request, calc: { main_skill: {}, stats: [] }, dataVersion: 'data-123', treeVersion: 'tree-456',
    updateParams: vi.fn(), stateVersion: 1, socketGroupRevision: 1, activeWeaponSet: 1,
  } as unknown as BuildSession;
  const render = () => {
    harness.cursor = 0; harness.buttons.length = 0;
    return renderToStaticMarkup(<GemOptimizer session={session} lang="en-US" active groupIndex={0} catalog={catalog} gemName={id => id} skillKey="skill" />);
  };
  return { session, render };
}
function button(label: string) {
  const found = harness.buttons.find(props => props.children === label);
  expect(found).toBeDefined();
  return found!;
}
beforeEach(() => {
  harness.states = [];
  harness.preset = 'dps';
  harness.optimize.mockReset().mockResolvedValue({ baseline: { TotalDPS: 100, TotalEHP: 200 }, evaluated: 12, unmodeled: 2,
    baselineUnsupported: ['Internal diagnostic retained'], excluded: [], plans: [] });
});

it.each(['dps', 'balanced', 'dpsPerMana'])('passes the resolved %s objective into support optimization for normal and summoner builds', async preset => {
  harness.preset = preset;
  const { session, render } = setup();
  render(); await button('Calculate support combinations').onClick!({} as never);
  expect(harness.optimize).toHaveBeenLastCalledWith(expect.objectContaining({ objective: expect.objectContaining({
    stat: preset === 'dpsPerMana' ? 'TotalDPS' : 'CombinedDPS',
  }) }));
  session.calc = { ...session.calc!, main_minion_index: 2, minions: [{ index: 2, name: 'Wolf' }],
    stats: [{ id: 'ActiveMinionLimit', value: 7 }] } as BuildSession['calc'];
  render(); await button('Calculate support combinations').onClick!({} as never);
  expect(harness.optimize).toHaveBeenLastCalledWith(expect.objectContaining({ objective: expect.objectContaining({
    stat: preset === 'dpsPerMana' ? 'TotalDPS' : 'MainMinionDPS',
    ...(preset === 'dpsPerMana' ? { per: 'ManaCost' } : {}),
  }) }));
});

it('shows a minion-only support improvement in the headline and explains that it is per minion', async () => {
  const { session, render } = setup();
  session.calc = { ...session.calc!, main_minion_index: 2, minions: [{ index: 2, name: 'Wolf' }],
    stats: [{ id: 'ActiveMinionLimit', value: 7 }] } as BuildSession['calc'];
  harness.optimize.mockResolvedValue({ baseline: { TotalDPS: 0, CombinedDPS: 0, MainMinionDPS: 100, TotalEHP: 200 },
    plans: [{ supports: [], variant: {}, stats: { TotalDPS: 0, CombinedDPS: 0, MainMinionDPS: 150, TotalEHP: 200 } }],
    baselineUnsupported: [], excluded: [], evaluated: 1, unmodeled: 0 });
  render(); await button('Calculate support combinations').onClick!({} as never);
  const html = render();
  expect(html).toContain('Minion hit DPS 150');
  expect(html).toContain('+50.0%');
  expect(html).toContain('Wolf · per minion · limit 7');
});

it('selects a calculation skill without starting optimization', async () => {
  const { session, render } = setup(1);
  render();
  await button('Use for calculations').onClick!({} as never);
  expect(session.updateParams).toHaveBeenCalledWith({ main_socket_group: 0 });
  expect(harness.optimize).not.toHaveBeenCalled();
  session.calcParams.main_socket_group = 0;
  render();
  expect(button('Calculate support combinations')).toBeDefined();
});

it('shows reason-labelled exclusion totals and keeps calculation diagnostics folded', async () => {
  const { render } = setup();
  expect(render()).toContain('308 excluded: compatibility unknown 28 · level too low 0 · incompatible 280');
  await button('Calculate support combinations').onClick!({} as never);
  const html = render();
  expect(harness.optimize).toHaveBeenCalledOnce();
  expect(html).toContain('<details class="support-calculation-details"><summary>Calculation details</summary>');
  expect(html).not.toMatch(/class="support-calculation-details"[^>]*\bopen=/);
  expect(html).toContain('data-123');
  expect(html).toContain('Internal diagnostic retained');
  expect(html).toContain('Complete recalculations 12');
  expect(html).toContain('Ordinary support gems do not need a trade search');
});
