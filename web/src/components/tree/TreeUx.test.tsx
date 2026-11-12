import { useState, type ComponentProps, type ReactElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { beforeEach, expect, test, vi } from 'vitest';
import type { BuildSession } from '../../hooks/useBuildSession';
import { messages } from '../../lib/i18n';
import type { PassiveJewelState, PassiveNode } from '../../api/types';
import type { PowerRow } from '../../lib/nodePower';
import { TreeOptimizer, useTreeOptimizer } from './TreeOptimizer';
import { NodePowerTable } from './NodePowerTable';
import { TreeRefundNotice } from './TreeRefundNotice';
import { PlannerResultsNotice } from './PlannerResultsNotice';
import { TreeConversionControls, useTreeConversion } from './TreeConversionControls';
import { JewelPassivePlanner, type JewelPassivePlannerState } from './JewelPassivePlanner';
import { usePassiveNodeNames } from './passiveNodeNames';

// Replay the real event handlers in the repository's node-only test environment.
const ui = vi.hoisted(() => ({ buttons: [] as ComponentProps<'button'>[],
  inputs: [] as ComponentProps<'input'>[], refs: [] as { current: unknown }[],
  captureEffects: false,
  evaluate: vi.fn(), convert: vi.fn(), decode: vi.fn(), translate: vi.fn() }));
const hooks = await vi.hoisted(async () => {
  const { createHookHarness } = await import('../../testing/hookHarness');
  return createHookHarness({ refs: { onRead: ref => { ui.refs.push(ref); } }, memo: 'eager',
    effects: 'collect', captureEffects: () => ui.captureEffects, layoutEffect: 'ignore' });
});
vi.mock('react', async original => hooks.mockReact(await original<typeof import('react')>()));
vi.mock('../../hooks/useUpgradeGoal', () => ({ useUpgradeGoal: () => ({ goal: { preset: 'life', cStat: '', cMin: '', cMax: '' }, setGoal: vi.fn() }) }));
vi.mock('../../hooks/useLocalizedLines', async original => ({ ...await original<typeof import('../../hooks/useLocalizedLines')>(),
  useLocalizedNames: (names: { name: string }[]) => names.map(entry => entry.name),
}));
vi.mock('../../app/jobs/useJobBeacon', () => ({ useJobBeacon: () => {} }));
vi.mock('../../lib/nodePower', async original => ({ ...await original<typeof import('../../lib/nodePower')>(),
  evaluateNodePower: (...args: unknown[]) => ui.evaluate(...args),
}));
vi.mock('../../api/backend', () => ({ getBackend: async () => ({ previewTreeConversion: ui.convert, decodeBuild: ui.decode, loadPassiveTree: async () => [], translateLines: ui.translate }) }));
vi.mock('../shared/controls', () => ({
  AppButton: (props: ComponentProps<'button'>) => { ui.buttons.push(props); return <button {...props} />; },
  AppInput: (props: ComponentProps<'input'>) => { ui.inputs.push(props); return <input {...props} />; },
  AppSelectControl: (props: ComponentProps<'select'>) => <select {...props} />,
  AppTextarea: (props: ComponentProps<'textarea'>) => <textarea {...props} />,
}));
vi.mock('../shared/AppSelect', () => ({ AppSelect: ({ ariaLabel }: { ariaLabel: string }) => <select aria-label={ariaLabel} /> }));

const render = (make: () => ReactElement) => {
  hooks.beginRender(); ui.buttons = []; ui.inputs = []; ui.refs = [];
  const element = make();
  return { element, html: renderToStaticMarkup(element) };
};
const button = (text: string) => ui.buttons.find(props => props.children === text)!;
const click = (text: string) => button(text).onClick?.({} as never);
const searchTable = (value: string) => ui.inputs.find(input => input.type === 'search')!.onChange?.({ target: { value } } as never);
const flushEffects = () => hooks.runEffects();
const node = (skill: number, name: string, connections: number[], kind: PassiveNode['kind']): PassiveNode => ({ skill, id: String(skill), name, connections, kind, x: skill * 100, y: 0 });
const nodes = [node(1, 'WITCH', [2], 'normal'), node(2, 'Power', [1], 'notable')];
const effects: PassiveJewelState = { class_starts: { witch: 1 }, allocation_grants: [],
  nodes: {}, conquered: [], unresolved: [], rings: [], warnings: [] };
const session = { currentRequest: () => ({ allocated_nodes: [1] }), allocatedNodes: [1], stateVersion: 0,
  character: { class_name: 'Witch' }, jewels: [], calc: { tree_effects: effects }, busy: false } as unknown as BuildSession;

beforeEach(() => { hooks.reset(); ui.captureEffects = false; vi.clearAllMocks(); });

test('report opens with an entry empty state, evaluates, locates a result and resets back to the entry', async () => {
  const focus = vi.fn();
  const props = { session, nodes, nodeNames: new Map(), lang: 'en-US' as const, stat: 'Life', stats: ['Life'], onStat: vi.fn(), onFocus: focus, onSearch: vi.fn() };
  const make = () => NodePowerTable(props);
  const initial = render(make);
  expect(initial.element.props).toMatchObject({ open: true });
  expect(initial.html).toContain('Run an evaluation to see candidate rankings');
  expect(initial.html).not.toContain('<tbody>');
  expect(button('Evaluate all candidate nodes').disabled).toBe(false);
  ui.evaluate.mockImplementation(async ({ rows, onBatch }) => {
    onBatch(rows.map((row: { node: PassiveNode }) => ({ ...row, status: 'ready', gain: 5, perPoint: 5, single: 5, diagnostics: [] })), 1, 1, []);
  });
  await click('Evaluate all candidate nodes');
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

test.each(['zh-CN', 'zh-TW'] as const)('a table-only modifier search loads matching passive stats in %s', async lang => {
  ui.captureEffects = true;
  const searchable = [{ ...nodes[0], connections: [2, 3] },
    { ...nodes[1], stats: ['30% increased [Spirit|Spirit]'] },
    { ...node(3, 'Health', [1], 'notable'), stats: ['10% increased maximum Life'] }];
  ui.translate.mockImplementation(async (lines: string[]) => lines.map(line => line.includes('Spirit') ? '精魂提高 30%' : line));
  ui.evaluate.mockImplementation(async ({ rows, onBatch }) => {
    onBatch(rows.map((row: PowerRow) => ({ ...row, status: 'ready', gain: 5, perPoint: 5, single: 5 })), 1, 1, []);
  });
  const text = messages(lang, 'nodePower');
  const make = () => {
    const [tableSearch, onSearch] = useState('');
    // The canvas query stays empty: only the table can open this shared translation gate.
    const nodeNames = usePassiveNodeNames(searchable, lang, true, tableSearch);
    return NodePowerTable({ session, nodes: searchable, nodeNames, lang, stat: 'Life', stats: ['Life'],
      onStat: vi.fn(), onFocus: vi.fn(), onSearch });
  };
  render(make); flushEffects();
  expect(ui.translate).not.toHaveBeenCalled();
  searchTable('精魂');
  render(make); flushEffects();
  render(make); flushEffects();
  await vi.waitFor(() => expect(ui.translate).toHaveBeenCalledTimes(1));
  const filtered = render(make).html;
  expect(filtered).toContain(`${text.results}：1`);
  await click(text.run);
  expect(ui.evaluate.mock.calls[0][0].rows.map((row: PowerRow) => row.node.skill)).toEqual([2]);
  const evaluated = render(make).html;
  expect(evaluated).toContain('data-node-id="2"');
  expect(evaluated).not.toContain('data-node-id="3"');
});

test('a table query stops requesting stats while its disclosure or sidebar is hidden and clears on unmount', () => {
  ui.captureEffects = true;
  let visible = true;
  const onSearch = vi.fn();
  const make = () => NodePowerTable({ session, nodes, nodeNames: new Map(), lang: 'zh-CN', stat: 'Life', stats: ['Life'],
    onStat: vi.fn(), onFocus: vi.fn(), onSearch, visible });
  const initial = render(make);
  const cleanups = flushEffects();
  const toggle = (open: boolean) => (initial.element.props as ComponentProps<'details'>).onToggle?.({ currentTarget: { open } } as never);
  const notify = () => { render(make); flushEffects(); };
  searchTable('精魂');
  notify(); expect(onSearch).toHaveBeenLastCalledWith('精魂');
  toggle(false); notify(); expect(onSearch).toHaveBeenLastCalledWith('');
  toggle(true); notify(); expect(onSearch).toHaveBeenLastCalledWith('精魂');
  visible = false; notify(); expect(onSearch).toHaveBeenLastCalledWith('');
  visible = true; notify(); expect(onSearch).toHaveBeenLastCalledWith('精魂');
  cleanups.forEach(cleanup => cleanup?.());
  expect(onSearch).toHaveBeenLastCalledWith('');
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
  expect(render(make).html).toContain('<summary class="disclosure">Conversion options</summary>');
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

test('joint jewel and passive results show a minion-only gain with the same damage headline', () => {
  const planner = { session: { ...session, calc: { stats: [], main_minion_index: 2, minions: [{ index: 2, name: 'Wolf' }] } },
    lang: 'en-US', nodeLabel: () => 'Jewel socket', raw: '', selectedSocket: '2', sockets: [], candidates: [], exclusive: [], preview: null,
    completed: { objective: { stat: 'MainMinionDPS', constraints: [] } },
    result: { baseline: { TotalDPS: 0, CombinedDPS: 0, MainMinionDPS: 100 }, evaluated: 1, diagnostics: [],
      plans: [{ candidateId: null, allocate: [], deallocate: [], stats: { TotalDPS: 0, CombinedDPS: 0, MainMinionDPS: 150 } }] },
  } as unknown as JewelPassivePlannerState;
  const html = render(() => JewelPassivePlanner({ planner })).html;
  expect(html).toContain('Minion hit DPS <b>+50%');
  expect(html).toContain('data-stat="MainMinionDPS">150');
  expect(html).toContain('Wolf · per minion');
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
  expect(hooks.slots[0]).toBe(true);
});


test.each(['en-US', 'zh-CN', 'zh-TW'] as const)('report and open route planner have distinct attribute selectors in %s', lang => {
  const renderBoth = () => {
    const planner = useTreeOptimizer({ session: { ...session, attributeChoices: {} }, lang, active: true,
      nodes, nodeLabel: () => 'Power', onPreview: vi.fn(), focusPlanner: { nonce: 1 } });
    return <>{NodePowerTable({ session, nodes, nodeNames: new Map(), lang, stat: 'Life', stats: ['Life'],
      onStat: vi.fn(), onFocus: vi.fn(), onSearch: vi.fn() })}{TreeOptimizer({ planner })}</>;
  };
  const html = render(renderBoth).html;
  const reportLabel = messages(lang, 'nodePower').attributeLabel;
  const routeLabel = messages(lang, 'treeOptimizer').attributeLabel;
  expect(reportLabel).not.toBe(routeLabel);
  expect(html.match(new RegExp(`aria-label="${reportLabel}"`, 'g'))).toHaveLength(1);
  expect(html.match(new RegExp(`aria-label="${routeLabel}"`, 'g'))).toHaveLength(1);
});


test('cancelling a report retains completed rows and counts skipped and unprocessed candidates', async () => {
  const extra = node(3, 'Island', [], 'notable');
  const fourth = node(4, 'Later', [1], 'notable');
  const props = { session, nodes: [...nodes, extra, fourth], nodeNames: new Map(), lang: 'en-US' as const,
    stat: 'Life', stats: ['Life'], onStat: vi.fn(), onFocus: vi.fn(), onSearch: vi.fn() };
  let release!: () => void;
  let signal!: AbortSignal;
  ui.evaluate.mockImplementation(async options => {
    signal = options.signal;
    options.onBatch([{ ...options.rows[0], status: 'ready', gain: 5, perPoint: 5, single: 5 }], 1, 2, []);
    await new Promise<void>(resolve => { release = resolve; });
    options.onBatch(options.rows.map((row: unknown) => ({ ...row as object, status: 'ready' })), 2, 2, []);
  });
  const make = () => NodePowerTable(props);
  render(make);
  const running = click('Evaluate all candidate nodes');
  const active = render(make).html;
  expect(active).toContain('<progress'); expect(active).toContain('max="2"');
  expect(active).toContain('Skipped: 1'); expect(active).toContain('Not yet evaluated: 1');
  expect(active).toContain('Rankings are provisional');
  click('Cancel report'); expect(signal.aborted).toBe(true);
  release(); await running;
  const cancelled = render(make).html;
  expect(cancelled).toContain('Report cancelled; rankings are provisional');
  expect(cancelled).toContain('Not yet evaluated: 1');
  expect(cancelled.match(/data-status="ready"/g)).toHaveLength(1);
});

test('refund notice waits for recalculation, keeps total refunded points and invokes undo beside the tree', () => {
  const baseline = { stats: [{ id: 'CombinedDPS', value: 1000 }] } as BuildSession['calc'];
  const current = { stats: [{ id: 'CombinedDPS', value: 771 }] } as BuildSession['calc'];
  const receipt = { version: 1, points: 12, baseline };
  const undo = vi.fn();
  let updated = { ...session, stateVersion: 1, calc: baseline, busy: true, canUndoTree: true };
  const make = () => <TreeRefundNotice receipt={receipt} session={updated} lang="en-US" onUndo={undo} />;
  expect(render(make).html).toContain('Refunded 12 points · Combined DPS calculating');
  expect(button('Undo refund').disabled).toBe(true);
  updated = { ...updated, calc: current, busy: false };
  expect(render(make).html).toContain('Refunded 12 points · Combined DPS -229 (-22.9%)');
  click('Undo refund'); expect(undo).toHaveBeenCalledOnce();
  updated = { ...updated, stateVersion: 2 };
  expect(render(make).html).toBe('');
});

test('conversion preview cleans raw modifier markup, hides versions in details and confirms the reviewed snapshot', async () => {
  const apply = vi.fn(() => true);
  const state = { ...session, exportCode: async () => 'original', applyTreeConversion: apply };
  let conversion: ReturnType<typeof useTreeConversion>;
  const make = () => {
    conversion = useTreeConversion(state, 'en-US', true);
    return <TreeConversionControls session={state} conversion={conversion} />;
  };
  const lost = { id: 9, name: 'Spirit', stats: ['{crafted}+10 to [Spirit|Spirit]'], reason: 'missing', jewel_item_id: null };
  const result = { code: 'converted', specs: [{ index: 0, title: 'Spec 1', from_version: '0_4', to_version: '0_5',
    kept: [1], dropped: [lost], disconnected: [], disconnected_by_set: [[], [], []], allocation: { shared: [1], exclusive: [[], []] } }] };
  ui.convert.mockResolvedValue(result); ui.decode.mockResolvedValue({ character: {} });
  render(make); await conversion!.createPreview();
  const html = render(make).html;
  expect(html).toContain('<h3>Passive plan 1</h3>');
  expect(html).toContain('<summary class="disclosure">Details</summary><p>Spec 1 · 0_4 → 0_5</p>');
  expect(html).toContain('+10 to Spirit');
  expect(html).not.toContain('{crafted}'); expect(html).not.toContain('[Spirit|');
  expect(html).toContain('class="tree-conversion-footer"');
  click('Confirm conversion');
  expect(apply).toHaveBeenCalledWith({ character: {} }, 'converted', 0);
  expect(render(make).html).not.toContain('tree-conversion-footer');
});

test('passive plans keep unchanged metrics out of the compact row and reveal exact values in route details', () => {
  const make = () => {
    const planner = useTreeOptimizer({ session: { ...session, attributeChoices: {} }, lang: 'en-US', active: true,
      nodes, nodeLabel: () => 'Power', onPreview: vi.fn(), focusPlanner: { nonce: 1 } });
    const result = { baseline: { TotalDPS: 100, TotalEHP: 100, Life: 100 }, baselineUnsupported: [], excluded: [],
      plans: [{ target: 2, allocate: [2], deallocate: [], stats: { TotalDPS: 100, TotalEHP: 100, Life: 120 } }],
      evaluated: 2, limited: false, unmodeled: 0 };
    return TreeOptimizer({ planner: { ...planner, evaluated: result,
      completed: { key: '', request: {}, preparedRequest: {}, result, objective: { stat: 'Life', constraints: [] }, dataVersion: null, treeVersion: null } } });
  };
  const html = render(make).html;
  const metrics = html.split('class="tree-plan-metrics"')[1].split('</div>')[0];
  expect(metrics).toContain('+20.0%');
  expect(metrics).not.toContain('+0.0%');
  expect(metrics).not.toContain('Effective HP');
  expect(html).toContain('output data-stat="TotalEHP">100');
  expect(html).toContain('output data-stat="Life">120');
});


test.each(['zh-CN', 'zh-TW', 'en-US'] as const)('report signals row readiness and completion with the current all-candidates control in %s', async lang => {
  const text = messages(lang, 'nodePower');
  const make = () => NodePowerTable({ session, nodes, nodeNames: new Map(), lang,
    stat: 'Life', stats: ['Life'], onStat: vi.fn(), onFocus: vi.fn(), onSearch: vi.fn() });
  render(make);
  expect(button(text.run).disabled).toBe(false);
  ui.evaluate.mockImplementation(async ({ rows, onBatch }) => {
    onBatch(rows.map((row: object) => ({ ...row, status: 'ready', gain: 5, perPoint: 5, single: 5, diagnostics: [] })), 1, 1, []);
  });
  await click(text.run);
  const html = render(make).html;
  expect(html).toContain('data-node-id="2" data-status="ready"');
  expect(html).toContain(`<p role="status">${text.complete}</p>`);
  expect(button(text.run).disabled).toBe(true);
});
