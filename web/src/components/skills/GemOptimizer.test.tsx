import type { ComponentProps } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { beforeEach, expect, it, vi } from 'vitest';
import type { BuildSession } from '../../hooks/useBuildSession';
import { GemOptimizer } from './GemOptimizer';
import { ChangeBar } from '../../app/preview/ChangeBar';

const harness = vi.hoisted(() => ({ states: [] as unknown[], cursor: 0, buttons: [] as ComponentProps<'button'>[], optimize: vi.fn(), articles: [] as ComponentProps<'article'>[], show: vi.fn(), showChanges: vi.fn(), settings: null as { current: HTMLDetailsElement | null } | null, previewState: null as import('../shared/PreviewContext').PreviewState | null }));
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
vi.mock('../../app/jobs/useJobBeacon', () => ({ useJobBeacon: () => {} }));
vi.mock('../shared/PreviewContext', () => ({ usePreview: () => ({ state: harness.previewState,
  show: harness.show, showChanges: harness.showChanges, clear: () => {} }) }));
vi.mock('react/jsx-dev-runtime', async original => {
  const runtime = await original<typeof import('react/jsx-dev-runtime')>();
  return { ...runtime, jsxDEV: (...args: Parameters<typeof runtime.jsxDEV>) => {
    if (args[0] === 'article') harness.articles.push(args[1] as ComponentProps<'article'>);
    if (args[0] === 'details' && (args[1] as ComponentProps<'details'>).className === 'gem-optimizer-settings') harness.settings = (args[1] as { ref: { current: HTMLDetailsElement | null } }).ref;
    return runtime.jsxDEV(...args);
  } };
});
vi.mock('../../hooks/useUpgradeGoal', () => ({ useUpgradeGoal: () => ({ goal: { preset: 'dps' }, setGoal: () => {} }) }));
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
    harness.cursor = 0; harness.buttons.length = 0; harness.articles.length = 0;
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
  harness.previewState = null;
  harness.show.mockClear();
  harness.showChanges.mockReset().mockImplementation((label, changes) => { harness.previewState = { label, changes, status: 'ready' }; });
  harness.optimize.mockReset().mockResolvedValue({ baseline: { CombinedDPS: 120, TotalDPS: 100, TotalEHP: 200 }, evaluated: 12, unmodeled: 2,
    baselineUnsupported: ['Internal diagnostic retained'], excluded: [], plans: [] });
});

it('selects a calculation skill without starting optimization', async () => {
  const { session, render } = setup(1);
  render();
  expand(); render();
  await button('Use for calculations').onClick!({} as never);
  expect(session.updateParams).toHaveBeenCalledWith({ main_socket_group: 0 });
  expect(harness.optimize).not.toHaveBeenCalled();
  session.calcParams.main_socket_group = 0;
  render();
  expect(button('Calculate support combinations')).toBeDefined();
});

it('shows reason-labelled exclusion totals and keeps calculation diagnostics folded', async () => {
  const { render } = setup();
  render(); expand();
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


function expand() {
  harness.buttons.find(props => props.className === 'gem-optimizer-toggle')!.onClick!({} as never);
}

it('keeps recommendations collapsed until requested', () => {
  const { render } = setup();
  expect(render()).not.toContain('gem-optimizer-body');
  expand();
  expect(render()).toContain('gem-optimizer-body');
});

it('zero evaluations explain the blocker without claiming no improvement', async () => {
  harness.optimize.mockResolvedValueOnce({ baseline: { CombinedDPS: 100, TotalEHP: 200 }, evaluated: 0, unmodeled: 0,
    baselineUnsupported: [], excluded: [], plans: [] });
  const { render } = setup();
  render(); expand(); render();
  await button('Calculate support combinations').onClick!({} as never);
  const html = render();
  expect(html).toContain('No alternative support combinations were evaluated.');
  expect(html).toContain('No new compatible combinations were available.');
  expect(html).toContain('Adjust capacity and candidates');
  expect(html).not.toContain('No fully recalculated improvement');
  const focus = vi.fn();
  const settings = { open: false, querySelector: () => ({ focus }) } as unknown as HTMLDetailsElement;
  harness.settings!.current = settings;
  button('Adjust capacity and candidates').onClick!({} as never);
  expect(settings.open).toBe(true);
  expect(focus).toHaveBeenCalledOnce();
});

it('card, baseline and hover reuse the evaluated Combined DPS with identical integer formatting', async () => {
  harness.optimize.mockResolvedValueOnce({ baseline: { CombinedDPS: 1400000.4, TotalDPS: 123.2, TotalEHP: 200 },
    evaluated: 1, unmodeled: 0, baselineUnsupported: [], excluded: [],
    plans: [{ supports: [{ skill_id: 'New', level: 1, quality: 0 }], variant: {},
      stats: { CombinedDPS: 4461800.6, TotalDPS: 777.7, TotalEHP: 200 } }] });
  const { render } = setup();
  render(); expand(); render();
  await button('Calculate support combinations').onClick!({} as never);
  const html = render();
  expect(html).toContain('Combined DPS 1,400,000');
  expect(html).toContain('Combined DPS 4,461,801');
  expect(html).toContain('+218.7%');
  expect(html).not.toContain('777.7');
  const target = {} as HTMLElement;
  harness.articles[0].onMouseEnter!({ currentTarget: target } as never);
  expect(harness.showChanges.mock.calls[0][2]).toBe(target);
  expect(harness.show).not.toHaveBeenCalled();
  expect(harness.showChanges.mock.calls[0][1][0]).toMatchObject({ id: 'CombinedDPS', before: 1400000.4, after: 4461800.6 });
  const hover = renderToStaticMarkup(<ChangeBar lang="en-US" />);
  expect(hover).toContain('1,400,000 → 4,461,801');
  expect(hover).toContain('+218.7%');
  expect(html).toContain('Effective HP 200 <strong>+0.0%</strong>');
  expect(html).not.toContain('is-positive">Effective HP');
  expect(html).toContain('Equip');
  expect(html).toContain('New');
});
