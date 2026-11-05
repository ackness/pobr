import type { ComponentProps, ReactElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { beforeEach, expect, test, vi } from 'vitest';
import type { BuildSession } from '../../hooks/useBuildSession';
import { messages } from '../../lib/i18n';
import type { PassiveNode } from '../../api/types';
import { TreeOptimizer, useTreeOptimizer } from './TreeOptimizer';
import { NodePowerTable } from './NodePowerTable';
import { PlannerResultsNotice } from './PlannerResultsNotice';
import { TreeConversionControls, useTreeConversion } from './TreeConversionControls';
import { JewelPassivePlanner, type JewelPassivePlannerState } from './JewelPassivePlanner';

// Replay the real event handlers in the repository's node-only test environment.
const ui = vi.hoisted(() => ({ cursor: 0, slots: [] as unknown[], buttons: [] as ComponentProps<'button'>[],
  inputs: [] as ComponentProps<'input'>[], refs: [] as { current: unknown }[],
  evaluate: vi.fn(), convert: vi.fn(), decode: vi.fn() }));
vi.mock('react', async original => ({ ...await original<typeof import('react')>(),
  useState: (initial: unknown) => {
    const index = ui.cursor++;
    if (!(index in ui.slots)) ui.slots[index] = typeof initial === 'function' ? initial() : initial;
    return [ui.slots[index], (value: unknown) => { ui.slots[index] = typeof value === 'function' ? value(ui.slots[index]) : value; }];
  },
  useRef: (initial: unknown) => {
    const index = ui.cursor++;
    if (!(index in ui.slots)) ui.slots[index] = { current: initial };
    const ref = ui.slots[index] as { current: unknown }; ui.refs.push(ref); return ref;
  },
  useMemo: (fn: () => unknown) => fn(), useLayoutEffect: () => {}, useEffect: () => {},
}));
vi.mock('../../hooks/useUpgradeGoal', () => ({ useUpgradeGoal: () => ({ goal: { preset: 'life', cStat: '', cMin: '', cMax: '' }, setGoal: vi.fn() }) }));
vi.mock('../../app/jobs/useJobBeacon', () => ({ useJobBeacon: () => {} }));
vi.mock('../../lib/nodePower', async original => ({ ...await original<typeof import('../../lib/nodePower')>(),
  evaluateNodePower: (...args: unknown[]) => ui.evaluate(...args),
}));
vi.mock('../../api/backend', () => ({ getBackend: async () => ({ previewTreeConversion: ui.convert, decodeBuild: ui.decode }) }));
vi.mock('../shared/controls', () => ({
  AppButton: (props: ComponentProps<'button'>) => { ui.buttons.push(props); return <button {...props} />; },
  AppInput: (props: ComponentProps<'input'>) => { ui.inputs.push(props); return <input {...props} />; },
  AppSelectControl: (props: ComponentProps<'select'>) => <select {...props} />,
  AppTextarea: (props: ComponentProps<'textarea'>) => <textarea {...props} />,
}));
vi.mock('../shared/AppSelect', () => ({ AppSelect: ({ ariaLabel }: { ariaLabel: string }) => <select aria-label={ariaLabel} /> }));

const render = (make: () => ReactElement) => {
  ui.cursor = 0; ui.buttons = []; ui.inputs = []; ui.refs = [];
  const element = make();
  return { element, html: renderToStaticMarkup(element) };
};
const button = (text: string) => ui.buttons.find(props => props.children === text)!;
const click = (text: string) => button(text).onClick?.({} as never);
const node = (skill: number, name: string, connections: number[], kind: PassiveNode['kind']): PassiveNode => ({ skill, id: String(skill), name, connections, kind, x: skill * 100, y: 0 });
const nodes = [node(1, 'WITCH', [2], 'normal'), node(2, 'Power', [1], 'notable')];
const session = { currentRequest: () => ({ allocated_nodes: [1] }), allocatedNodes: [1], stateVersion: 0,
  character: { class_name: 'Witch' }, jewels: [], calc: {}, busy: false } as unknown as BuildSession;

beforeEach(() => { ui.slots = []; vi.clearAllMocks(); });

test('report opens with an entry empty state, evaluates, locates a result and resets back to the entry', async () => {
  const focus = vi.fn();
  const props = { session, nodes, nodeNames: new Map(), lang: 'en-US' as const, stat: 'Life', stats: ['Life'], onStat: vi.fn(), onFocus: focus };
  const make = () => NodePowerTable(props);
  const initial = render(make);
  expect(initial.element.props).toMatchObject({ open: true });
  expect(initial.html).toContain('Run an evaluation to see candidate rankings');
  expect(initial.html).not.toContain('<tbody>');
  expect(button('Evaluate next 128 nodes').disabled).toBe(false);
  ui.evaluate.mockImplementation(async ({ rows, onBatch }) => {
    onBatch(rows.map((row: { node: PassiveNode }) => ({ ...row, status: 'ready', gain: 5, perPoint: 5, single: 5, diagnostics: [] })), 1, 1, []);
  });
  await click('Evaluate next 128 nodes');
  const evaluated = render(make).html;
  expect(evaluated).toContain('Life gain per point');
  expect(evaluated).toContain('+5 points');
  expect(evaluated).not.toContain('#2');
  click('Power'); expect(focus).toHaveBeenCalledWith(nodes[1]);
  click('Reset report');
  expect(render(make).html).not.toContain('<tbody>');
  (initial.element.props as { onToggle: (event: unknown) => void }).onToggle({ currentTarget: { open: false } });
  expect(render(make).html).not.toContain('Filter nodes');
});

test('the results link scrolls to the visible heading and moves keyboard focus', () => {
  const result = render(() => PlannerResultsNotice({ count: 3, lang: 'en-US', children: <ol><li>Result</li></ol> }));
  expect(result.html).toContain('3 plans found');
  expect(result.html).toContain('tabindex="-1"');
  const target = { scrollIntoView: vi.fn(), focus: vi.fn() };
  ui.refs[0].current = target;
  click('View results');
  expect(target.scrollIntoView).toHaveBeenCalledWith({ block: 'start' });
  expect(target.focus).toHaveBeenCalledWith({ preventScroll: true });
});

test('conversion scope is inside conversion options and the request and button follow its selection', async () => {
  const exportCode = vi.fn(async () => 'original');
  let conversion: ReturnType<typeof useTreeConversion>;
  const make = () => {
    conversion = useTreeConversion({ ...session, exportCode }, 'en-US', true);
    return <TreeConversionControls session={session} conversion={conversion} />;
  };
  expect(render(make).html).toContain('<summary>Conversion options</summary>');
  expect(button('Convert to current tree')).toBeDefined();
  ui.inputs.find(props => props.type === 'checkbox')!.onChange?.({ target: { checked: true } } as never);
  render(make);
  expect(button('Convert (all passive specs)')).toBeDefined();
  ui.convert.mockResolvedValue({ code: 'converted', specs: [] });
  ui.decode.mockResolvedValue({});
  await conversion!.createPreview();
  expect(ui.convert).toHaveBeenCalledWith('original', true);
  expect(exportCode).toHaveBeenCalledTimes(1);
});

test('jewel planning locates the selected socket and its child section can collapse', () => {
  const locate = vi.fn(), stop = vi.fn();
  const planner = { session, lang: 'en-US', nodeLabel: () => 'Jewel socket · Near Power', onLocate: locate,
    raw: '', selectedSocket: '2', sockets: [{ node: nodes[1], cost: 1 }], candidates: [], exclusive: [], stop,
  } as unknown as JewelPassivePlannerState;
  const first = render(() => JewelPassivePlanner({ planner }));
  expect(first.html).toContain('Candidate jewel text');
  click('Locate on passive tree');
  expect(locate).toHaveBeenCalledWith(2);
  const toggle = ui.buttons.find(props => props['aria-expanded'] === true)!;
  toggle.onClick?.({} as never);
  expect(stop).toHaveBeenCalledTimes(1);
  expect(render(() => JewelPassivePlanner({ planner })).html).not.toContain('Candidate jewel text');
});

test('the passive planner starts collapsed and its main disclosure updates the expansion state', () => {
  const make = () => {
    const planner = useTreeOptimizer({ session: { ...session, attributeChoices: {} }, lang: 'en-US', active: true,
      nodes, nodeLabel: () => 'Power', onPreview: vi.fn() });
    return TreeOptimizer({ planner });
  };
  expect(render(make).html).not.toContain('Point budget');
  const toggle = ui.buttons.find(props => props['aria-expanded'] === false)!;
  toggle.onClick?.({} as never);
  expect(ui.slots[0]).toBe(true);
});


test.each(['en-US', 'zh-CN', 'zh-TW'] as const)('report and open route planner have distinct attribute selectors in %s', lang => {
  const renderBoth = () => {
    const planner = useTreeOptimizer({ session: { ...session, attributeChoices: {} }, lang, active: true,
      nodes, nodeLabel: () => 'Power', onPreview: vi.fn(), focusPlanner: { nonce: 1 } });
    return <>{NodePowerTable({ session, nodes, nodeNames: new Map(), lang, stat: 'Life', stats: ['Life'],
      onStat: vi.fn(), onFocus: vi.fn() })}{TreeOptimizer({ planner })}</>;
  };
  const html = render(renderBoth).html;
  const reportLabel = messages(lang, 'nodePower').attributeLabel;
  const routeLabel = messages(lang, 'treeOptimizer').attributeLabel;
  expect(reportLabel).not.toBe(routeLabel);
  expect(html.match(new RegExp(`aria-label="${reportLabel}"`, 'g'))).toHaveLength(1);
  expect(html.match(new RegExp(`aria-label="${routeLabel}"`, 'g'))).toHaveLength(1);
});
