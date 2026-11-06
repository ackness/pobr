import { act, isValidElement, useLayoutEffect, type ComponentProps, type ReactNode } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { useBuildSession, type BuildSession } from '../../hooks/useBuildSession';
import type { CalculateBuildResponse, GemCatalogEntry, SkillMetricsEntry } from '../../api/types';
import type { SupportMetadata } from '../../lib/supportOptimizer';
import { SkillsPanel } from './SkillsPanel';
import { useConsumedRequest } from '../../hooks/useConsumedRequest';
import { useSkillMetrics as useSharedSkillMetrics } from '../../app/pages/skills/useSkillMetrics';
import { Overview } from '../../app/Overview';
import { ResultsRail } from '../../app/ResultsRail';
import { bindT } from '../../lib/i18n';
import { AppSelect, type AppSelectOption } from '../shared/AppSelect';
import { defaultStarterSession } from '../../lib/buildWorkspace';

const ui = vi.hoisted(() => ({ buttons: [] as ComponentProps<'button'>[], inputs: [] as ComponentProps<'input'>[],
  gems: [] as GemCatalogEntry[], trade: [] as SupportMetadata[], optimize: vi.fn(), metrics: vi.fn(), calculate: vi.fn(),
  pickers: [] as { placeholder: string; entries: GemCatalogEntry[] }[], ranks: [] as ComponentProps<'button'>[],
  selects: [] as { value?: string; open?: boolean; onOpenChange?: (open: boolean) => void }[],
  mainOptions: [] as AppSelectOption[] }));
vi.mock('../../api/backend', () => ({ getBackend: async () => ({ gemCatalog: async () => ui.gems, gemDescriptions: async () => null,
  skillMetrics: ui.metrics, calculateBuild: ui.calculate, init: async () => {}, gameLocales: async () => [],
  loadTreeMeta: async () => ({ classes: [{ name: 'Witch' }] }), loadClassNames: async () => ({ classes: {}, ascendancies: {} }),
  supportGroupsCompatible: async (groups: unknown[]) => groups.map(() => true) }) }));
vi.mock('../../lib/tradeOptimizer', () => ({ loadTradeCatalog: async () => ({ gems: ui.trade }) }));
vi.mock('../../lib/supportOptimizer', async original => ({ ...await original<typeof import('../../lib/supportOptimizer')>(), optimizeSupports: ui.optimize }));
vi.mock('../../hooks/useUpgradeGoal', () => ({ useUpgradeGoal: () => ({ goal: { preset: 'dps' }, setGoal: () => {} }) }));
vi.mock('../../hooks/useLocalizedLines', async original => ({ ...await original<typeof import('../../hooks/useLocalizedLines')>(), useLocalizedLines: (lines: string[]) => lines }));
vi.mock('../../app/jobs/useJobBeacon', () => ({ useJobBeacon: () => {} }));
vi.mock('../../hooks/useSkillName', () => ({ useSkillCatalog: () => new Map(ui.gems.map(gem => [gem.skill_id, gem])) }));
vi.mock('../../app/motion/AnimatedNumber', () => ({ AnimatedNumber: ({ value, format }: {
  value?: number | null; format: (value: number | null) => string;
}) => format(value ?? null) }));
vi.mock('../shared/PreviewContext', () => ({ usePreview: () => null }));
vi.mock('../shared/NamedSets', () => ({ NamedSets: () => null }));
vi.mock('../shared/WeaponSetControl', () => ({ WeaponSetControl: () => null }));
vi.mock('../shared/PageHeader', () => ({ PageHeader: () => null }));
vi.mock('../ui/select', () => {
  const childrenOnly = ({ children }: { children?: ReactNode }) => children;
  return {
    Select: (props: { children: ReactNode; value?: string; open?: boolean; onOpenChange?: (open: boolean) => void }) => {
      ui.selects.push(props); return props.children;
    },
    SelectTrigger: (props: ComponentProps<'button'>) => <button {...props} />,
    SelectValue: ({ children, placeholder }: { children?: ReactNode; placeholder?: string }) => children ?? placeholder,
    SelectContent: () => null, SelectGroup: childrenOnly, SelectItem: childrenOnly, SelectLabel: childrenOnly,
  };
});
vi.mock('react/jsx-dev-runtime', async original => {
  const runtime = await original<typeof import('react/jsx-dev-runtime')>();
  return { ...runtime, jsxDEV: (...args: Parameters<typeof runtime.jsxDEV>) => {
    if (args[0] === AppSelect && (args[1] as ComponentProps<typeof AppSelect>).ariaLabel === bindT('en-US')('sidebar.mainSkill')) {
      ui.mainOptions = (args[1] as ComponentProps<typeof AppSelect>).options;
    }
    const props = args[1] as ComponentProps<'button'>;
    const first = Array.isArray(props?.children) ? props.children[0] : undefined;
    if (args[0] === 'button' && isValidElement<{ className?: string }>(first) && first.props.className === 'ov-rank-name') ui.ranks.push(props);
    return runtime.jsxDEV(...args);
  } };
});
vi.mock('../shared/NoteEditor', () => ({ NoteEditor: () => null }));
vi.mock('../shared/AppDialogs', () => ({ useAppDialogs: () => ({ confirm: async () => false }) }));
vi.mock('../shared/useSessionAction', () => ({ useSessionAction: () => () => () => true }));
vi.mock('../shared/OptimizerControls', async original => ({ ...await original<typeof import('../shared/OptimizerControls')>(), ObjectiveEditor: () => null, OptimizerProgress: () => null }));
vi.mock('./MinionSelection', () => ({ MinionSelection: () => null }));
vi.mock('./SkillFormControls', () => ({ SkillFormControls: () => null }));
vi.mock('./GemNameCopy', () => ({ GemNameCopy: () => null }));
vi.mock('./SupportDescriptions', () => ({ SupportDescriptions: () => null }));
vi.mock('./GemPicker', () => ({ GemPicker: (props: { placeholder: string; entries: GemCatalogEntry[] }) => { ui.pickers.push(props); return null; } }));
vi.mock('../shared/controls', () => ({
  AppButton: (props: ComponentProps<'button'>) => { ui.buttons.push(props); return <button {...props} />; },
  AppInput: (props: ComponentProps<'input'>) => { ui.inputs.push(props); return <input {...props} />; },
  AppTooltip: ({ children }: ComponentProps<'span'>) => children,
}));

// Minimal host nodes for ReactDOM's real reconciliation and effect lifecycle in
// this Node-only suite. Controls above are native; layout and browser events are
// outside this regression. React itself and its hooks are not mocked.
class HostNode {
  nodeType = 1;
  namespaceURI = 'http://www.w3.org/1999/xhtml';
  parentNode: HostNode | null = null;
  childNodes: HostNode[] = [];
  attributes = new Map<string, string>();
  style = { setProperty() {}, removeProperty() {} };
  value = '';
  checked = false;
  textContent = '';
  constructor(public tagName: string, public ownerDocument: HostDocument) {}
  get nodeName() { return this.tagName; }
  get nodeValue() { return this.textContent; }
  set nodeValue(value: string) { this.textContent = value; }
  appendChild(node: HostNode) { return this.insertBefore(node, null); }
  insertBefore(node: HostNode, before: HostNode | null) {
    node.parentNode?.removeChild(node);
    const index = before ? this.childNodes.indexOf(before) : this.childNodes.length;
    this.childNodes.splice(index, 0, node);
    node.parentNode = this;
    return node;
  }
  removeChild(node: HostNode) { this.childNodes.splice(this.childNodes.indexOf(node), 1); node.parentNode = null; return node; }
  setAttribute(name: string, value: string) { this.attributes.set(name, String(value)); }
  getAttribute(name: string) { return this.attributes.get(name) ?? null; }
  removeAttribute(name: string) { this.attributes.delete(name); }
  addEventListener() {}
  removeEventListener() {}
}
class HostDocument {
  nodeType = 9;
  documentElement = new HostNode('HTML', this);
  body = new HostNode('BODY', this);
  activeElement = this.body;
  defaultView = { HTMLIFrameElement: class {}, addEventListener() {}, removeEventListener() {} };
  createElement(tag: string) { return new HostNode(tag.toUpperCase(), this); }
  createElementNS(_namespace: string, tag: string) { return this.createElement(tag); }
  createTextNode(text: string) { const node = new HostNode('#text', this); node.nodeType = 3; node.textContent = text; return node; }
  addEventListener() {}
  removeEventListener() {}
}
let root: Root, host: HostNode;
beforeEach(() => {
  const document = new HostDocument();
  vi.stubGlobal('document', document);
  vi.stubGlobal('window', document.defaultView);
  vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true);
  vi.stubGlobal('localStorage', { getItem: () => null, setItem() {} });
  ui.buttons = []; ui.inputs = []; ui.pickers = []; ui.selects = []; ui.ranks = []; ui.gems = []; ui.trade = []; ui.mainOptions = [];
  ui.metrics.mockReset().mockResolvedValue({ skills: [] });
  ui.calculate.mockReset().mockResolvedValue({ stats: [], main_skill: null, unsupported_modifiers: [], item_errors: [],
    minions: [], breakdowns: {}, breakdown_descriptors: {} });
  ui.optimize.mockReset().mockResolvedValue({ baseline: { CombinedDPS: 100 }, baselineUnsupported: [], plans: [], excluded: [], candidates: 0, evaluated: 0, unmodeled: 0 });
  host = document.createElement('div');
  root = createRoot(host as unknown as HTMLElement);
});
afterEach(async () => { await act(async () => root.unmount()); vi.useRealTimers(); vi.unstubAllGlobals(); });

it('remounts same-skill editors when switching groups and on revision changes, resetting capacity before evaluation', async () => {
  const fire: GemCatalogEntry = { skill_id: 'Fireball', name: 'Fireball', name_zh_cn: '火球', name_zh_tw: '火球', is_support: false, is_lineage: false, is_unique_only: false,
    active_effect_ids: ['Fireball'], colour: 'int', tags: ['Spell'] };
  const supports = Array.from({ length: 5 }, (_, index) => ({ ...fire, skill_id: `Support${index}`, name: `Support ${index}`, is_support: true, active_effect_ids: [] }));
  ui.gems = [fire, ...supports];
  ui.trade = ui.gems.map(gem => ({ ...gem, family: gem.skill_id, max_level: 20, level_requirements: [0], compatibility_known: true, skill_types: ['Spell'] }));
  const groups = [5, 2].map(count => ({ enabled: true, gems: [{ skill_id: fire.skill_id, level: 20, quality: 0 },
    ...supports.slice(0, count).map(gem => ({ skill_id: gem.skill_id, level: 1, quality: 0 }))] }));
  const session = { socketGroups: groups, calcParams: { main_socket_group: 0 }, character: { level: 100 }, busy: false,
    calc: { main_skill: {}, stats: [] }, annotations: {}, items: [], library: { skillSets: [] }, socketGroupRevision: 1, stateVersion: 1, calculationScope: {},
    currentRequest: () => ({ socket_groups: groups }),
    updateParams: (patch: object) => { Object.assign(session.calcParams, patch); root.render(<SkillsPanel session={session} lang="en-US" active />); },
  } as unknown as BuildSession;
  const click = async (button: ComponentProps<'button'>) => { await act(async () => { button.onClick!({} as never); }); };
  const lastButton = (className: string) => ui.buttons.filter(button => button.className?.split(' ').includes(className)).at(-1)!;
  const capacity = () => ui.inputs.filter(input => input.type === 'number' && input.min === 2 && input.max === 5).at(-1)!;
  await act(async () => root.render(<SkillsPanel session={session} lang="en-US" active />));
  const supportEntries = () => ui.pickers.filter(picker => picker.placeholder === 'Add a support gem…').at(-1)!.entries;
  const entries = supportEntries();
  await click(lastButton('skills-list-toggle'));
  expect(supportEntries()).toBe(entries);
  await click(lastButton('gem-optimizer-toggle'));
  expect(capacity().value).toBe(5);

  await click(ui.buttons.filter(button => button.className?.split(' ').includes('skill-group-title')).at(-1)!);
  const expandedOnSwitch = lastButton('gem-optimizer-toggle')['aria-expanded'];
  if (!expandedOnSwitch) await click(lastButton('gem-optimizer-toggle'));
  expect(capacity().value).toBe(2);
  expect(expandedOnSwitch).toBe(false);
  await click(lastButton('opt-run'));
  expect(ui.optimize).toHaveBeenLastCalledWith(expect.objectContaining({ groupIndex: 1, capacity: 2 }));

  await act(async () => { capacity().onChange!({ target: { value: '4' } } as never); });
  expect(capacity().value).toBe(4);
  session.socketGroupRevision++;
  await act(async () => root.render(<SkillsPanel session={session} lang="en-US" active />));
  expect(lastButton('gem-optimizer-toggle')['aria-expanded']).toBe(false);
  await click(lastButton('gem-optimizer-toggle'));
  expect(capacity().value).toBe(2);
});

type Snapshot = ReturnType<typeof useSharedSkillMetrics>;
function metricsSession(): BuildSession {
  return { calculationScope: {}, stateVersion: 1, socketGroupRevision: 1, busy: false,
    socketGroups: [{ enabled: true, gems: [] }], currentRequest: () => ({ socket_groups: [] }),
  } as unknown as BuildSession;
}
function MetricsConsumer({ session, enabled = true, onSnapshot }: {
  session: BuildSession; enabled?: boolean; onSnapshot: (snapshot: Snapshot) => void;
}) {
  onSnapshot(useSharedSkillMetrics(session, enabled));
  return null;
}
async function debounceMetrics(ms = 250) {
  await act(async () => { await vi.advanceTimersByTimeAsync(ms); });
}

it('shares in-flight and resolved skill metrics, retaining an inactive page snapshot on return', async () => {
  vi.useFakeTimers();
  const session = metricsSession();
  let resolve!: (result: { skills: [] }) => void;
  ui.metrics.mockReturnValue(new Promise(done => { resolve = done; }));
  let editor: Snapshot = null, overview: Snapshot = null;
  const render = async (active: boolean, page = true) => {
    await act(async () => root.render(<>
      <MetricsConsumer session={session} enabled={active} onSnapshot={snapshot => { editor = snapshot; }} />
      {page && <MetricsConsumer session={session} onSnapshot={snapshot => { overview = snapshot; }} />}
    </>));
  };
  await render(true); await debounceMetrics();
  expect(ui.metrics).toHaveBeenCalledOnce();
  await act(async () => { resolve({ skills: [] }); });
  expect(editor).toEqual({ revision: 1, skills: [] });
  expect(editor).toBe(overview);
  const previous = editor;
  await render(false, false); await debounceMetrics();
  expect(editor).toBe(previous);
  await render(true, false);
  expect(editor).toBe(previous);
  await debounceMetrics();
  expect(ui.metrics).toHaveBeenCalledOnce();
});

it('keeps only the latest metric request key and separates independent sessions with equal versions', async () => {
  vi.useFakeTimers(); ui.metrics.mockResolvedValue({ skills: [] });
  const first = metricsSession(), second = metricsSession();
  const render = async (session: BuildSession) => {
    await act(async () => root.render(<MetricsConsumer session={session} onSnapshot={() => {}} />));
    await debounceMetrics();
  };
  await render(first);
  await render({ ...first, stateVersion: 2 });
  expect(ui.metrics).toHaveBeenCalledTimes(2);
  await render(first);
  expect(ui.metrics).toHaveBeenCalledTimes(3);
  await act(async () => root.render(null));
  await render(second);
  expect(ui.metrics).toHaveBeenCalledTimes(4);
});

it('retries failed metric batches and discards late results for an inactive consumer', async () => {
  vi.useFakeTimers();
  const session = metricsSession();
  let snapshot: Snapshot = null;
  const render = async (enabled: boolean) => {
    await act(async () => root.render(<MetricsConsumer session={session} enabled={enabled} onSnapshot={value => { snapshot = value; }} />));
  };
  ui.metrics.mockRejectedValueOnce(new Error('unavailable'));
  await render(true); await debounceMetrics();
  expect(snapshot).toBeNull();
  await render(false);
  let resolve!: (result: { skills: [] }) => void;
  ui.metrics.mockReturnValueOnce(new Promise(done => { resolve = done; }));
  await render(true); await debounceMetrics();
  expect(ui.metrics).toHaveBeenCalledTimes(2);
  await render(false);
  await act(async () => { resolve({ skills: [] }); });
  expect(snapshot).toBeNull();
  await render(true); await debounceMetrics();
  expect(snapshot).toEqual({ revision: 1, skills: [] });
  expect(ui.metrics).toHaveBeenCalledTimes(2);
});

it('does not request metric batches while inactive, busy, failed or groupless and cancels pending debounce', async () => {
  vi.useFakeTimers();
  const session = metricsSession();
  const render = async (current: BuildSession, enabled = true) => {
    await act(async () => root.render(<MetricsConsumer session={current} enabled={enabled} onSnapshot={() => {}} />));
  };
  await render(session, false); await debounceMetrics();
  await render({ ...session, busy: true }); await debounceMetrics();
  await render({ ...session, calcError: 'failed' }); await debounceMetrics();
  await render({ ...session, socketGroups: [] }); await debounceMetrics();
  await render(session); await render(session, false); await debounceMetrics();
  expect(ui.metrics).not.toHaveBeenCalled();
});

it('consumes focus nonces in the chosen effect phase only while active and before running the callback', async () => {
  const run = vi.fn();
  let consume!: () => void;
  function FocusRequest({ active, nonce }: { active: boolean; nonce?: number }) {
    consume = useConsumedRequest(active, nonce, () => { run(); consume(); });
    useLayoutEffect(consume, [consume]);
    return null;
  }
  const render = async (active: boolean, nonce?: number) => {
    await act(async () => root.render(<FocusRequest active={active} nonce={nonce} />));
  };
  await render(true);
  await render(false, 1);
  expect(run).not.toHaveBeenCalled();
  await render(true, 1);
  expect(run).toHaveBeenCalledOnce();
  await render(true, 1);
  await render(false, 1);
  await render(true, 1);
  expect(run).toHaveBeenCalledOnce();
  await render(true, 2);
  expect(run).toHaveBeenCalledTimes(2);
});

function consumerSession(): BuildSession {
  ui.gems = ['Fireball', 'IceNova'].map(skill_id => ({ skill_id, name: skill_id, name_zh_cn: null, name_zh_tw: null,
    active_effect_ids: [skill_id], is_support: false, is_lineage: false, colour: 'int', tags: ['Spell', 'Damage'] }));
  const groups = ui.gems.map(gem => ({ enabled: true, gems: [{ skill_id: gem.skill_id, level: 20, quality: 0 }] }));
  return { ...metricsSession(), socketGroups: groups, currentRequest: () => ({ socket_groups: groups }),
    character: { level: 100 }, classNames: { classes: {}, ascendancies: {} }, calcParams: { main_socket_group: 0, config_inputs: {} },
    annotations: {}, items: [], library: { skillSets: [] }, hasBuildContent: true, inspectSkill: vi.fn(),
    calc: { stats: [], main_skill: { group_index: 0, active_skill_index: 1, skill_id: 'Fireball',
      combined_dps: 100, hit_dps: 100, dot_dps: 0, hit_damage: [] },
    unsupported_modifiers: [], item_errors: [], minions: [], breakdowns: {}, breakdown_descriptors: {} },
  } as unknown as BuildSession;
}
function skillRows(damage: (number | null)[], offset = 0): SkillMetricsEntry[] {
  return damage.map((combined_dps, index) => ({ group_index: index + offset, active_skill_index: 1,
    skill_id: ui.gems[index].skill_id, status: combined_dps === null ? 'unavailable' : 'calculated',
    metrics: combined_dps === null ? null : { combined_dps, hit_dps: combined_dps, dot_dps: 0, average_hit: 0,
      action_rate: 0, mana_cost: 0, life_cost: 0, cooldown: 0 }, error: null, unsupported_modifiers: [],
  }));
}
function descendants(node: HostNode): HostNode[] {
  return [node, ...node.childNodes.flatMap(descendants)];
}
function nodeWith(name: string, value: string): HostNode {
  const node = descendants(host).find(node => node.getAttribute(name) === value);
  expect(node).toBeDefined();
  return node!;
}
function textOf(node: HostNode): string {
  return node.textContent + node.childNodes.map(textOf).join('');
}
const idle = () => {};
function rail(session: BuildSession) {
  return <ResultsRail session={session} view={{ calc: session.calc, previous: null, stale: false }} lang="en-US"
    onNavigate={idle} onStatClick={idle} pinned={[]} onTogglePin={idle} />;
}

it('keeps the unavailable marking on the actual closed rail trigger and refreshes it after edits', async () => {
  vi.useFakeTimers();
  const session = consumerSession();
  session.calc = { ...session.calc!, main_skill: null } as CalculateBuildResponse;
  const trigger = () => nodeWith('aria-label', bindT('en-US')('sidebar.mainSkill'));
  ui.metrics.mockResolvedValueOnce({ skills: skillRows([null, 200]) });
  await act(async () => root.render(rail(session)));
  await debounceMetrics();
  expect(ui.metrics).toHaveBeenCalledOnce();
  expect(textOf(trigger())).toContain('Not supported yet: Fireball · Group 1');
  expect(ui.selects.every(select => select.open === false)).toBe(true);

  session.stateVersion++;
  ui.metrics.mockResolvedValueOnce({ skills: skillRows([100, 200]) });
  await act(async () => root.render(rail(session)));
  await debounceMetrics();
  expect(textOf(trigger())).toBe('Fireball · Group 1');

  session.stateVersion++;
  ui.metrics.mockResolvedValueOnce({ skills: skillRows([null, 200]) });
  await act(async () => root.render(rail(session)));
  await debounceMetrics();
  expect(textOf(trigger())).toContain('Not supported yet: Fireball · Group 1');
  expect(ui.metrics).toHaveBeenCalledTimes(3);
  expect(ui.selects.every(select => select.open === false)).toBe(true);
});

it.each([undefined, 0])('prepares base dropdown metadata and hints before opening with main group %s', async selected => {
  vi.useFakeTimers();
  const session = consumerSession();
  const fire = ui.gems[0];
  ui.gems = [{ ...fire, skill_id: 'Buff', name: 'Buff', active_effect_ids: ['Buff'], tags: ['Buff'] }, fire];
  session.socketGroups = [{ enabled: true, gems: [] }, ...ui.gems.map(gem => ({ enabled: true,
    gems: [{ skill_id: gem.skill_id, level: 20, quality: 0 }] }))];
  session.currentRequest = () => ({ socket_groups: session.socketGroups });
  session.calcParams = { config_inputs: {}, main_socket_group: selected };
  session.calc = { ...session.calc!, main_skill: null } as CalculateBuildResponse;
  const tt = bindT('en-US');
  const expected: AppSelectOption[] = [
    { value: '0', label: `${tt('sidebar.skillUnsupported').replace('{name}', tt('skills.emptyGroup'))} · Group 1`,
      group: tt('sidebar.calculationSkills'), hint: tt('sidebar.skillDpsPending') },
    { value: '2', label: 'Fireball · Group 3', group: tt('sidebar.calculationSkills'), hint: '100 Combined DPS' },
    { value: '1', label: 'Buff · Group 2', group: tt('sidebar.nonDamagingSkills'), hint: '0 Combined DPS' },
  ];
  ui.metrics.mockResolvedValueOnce({ skills: skillRows([0, 100], 1) });
  await act(async () => root.render(rail(session)));
  await debounceMetrics(249);
  expect(ui.metrics).not.toHaveBeenCalled();
  await debounceMetrics(1);
  expect(ui.metrics).toHaveBeenCalledOnce();
  expect(ui.selects.every(select => select.open === false)).toBe(true);
  expect(ui.mainOptions).toEqual(expected);
  const mainSelect = ui.selects.filter(select => select.value === (selected === undefined ? '' : 'value:0')).at(-1)!;
  await act(async () => mainSelect.onOpenChange!(true));
  expect(ui.mainOptions).toEqual(expected);
  await debounceMetrics();
  expect(ui.mainOptions).toEqual(expected);
  expect(ui.metrics).toHaveBeenCalledOnce();

  await act(async () => mainSelect.onOpenChange!(false));
  session.stateVersion++;
  ui.metrics.mockResolvedValueOnce({ skills: skillRows([50, 200], 1) });
  await act(async () => root.render(rail(session)));
  await debounceMetrics(249);
  expect(ui.mainOptions).toEqual(expected);
  expect(ui.metrics).toHaveBeenCalledOnce();
  await debounceMetrics(1);
  const updated = [expected[0], { ...expected[2], group: tt('sidebar.calculationSkills'), hint: '50 Combined DPS' },
    { ...expected[1], hint: '200 Combined DPS' }];
  expect(ui.mainOptions).toEqual(updated);
  expect(ui.metrics).toHaveBeenCalledTimes(2);
  await act(async () => mainSelect.onOpenChange!(true));
  expect(ui.mainOptions).toEqual(updated);
  await debounceMetrics();
  expect(ui.mainOptions).toEqual(updated);
  expect(ui.metrics).toHaveBeenCalledTimes(2);
});

it('shares pending and completed batches across the real rail, Overview and Skills consumers with versioned selection', async () => {
  vi.useFakeTimers();
  const session = consumerSession();
  let resolve!: (result: { skills: SkillMetricsEntry[] }) => void;
  ui.metrics.mockReturnValueOnce(new Promise(done => { resolve = done; }));
  const render = async (page: 'overview' | 'skills') => {
    ui.ranks = []; ui.selects = [];
    await act(async () => root.render(<>{rail(session)}
      {page === 'overview'
        ? <Overview session={session} view={{ calc: session.calc, previous: null, stale: false }} lang="en-US" navigate={idle} focusStat={idle} />
        : <SkillsPanel session={session} lang="en-US" active />}
    </>));
  };
  const pending = () => nodeWith('aria-labelledby', 'ov-skills').getAttribute('aria-busy');
  const ranks = () => ui.ranks.slice(-2);
  await render('overview');
  expect(pending()).toBe('true');
  await debounceMetrics();
  expect(ui.metrics).toHaveBeenCalledOnce();
  const mainSelect = ui.selects.find(select => select.value === 'value:0')!;
  await act(async () => mainSelect.onOpenChange!(true));
  await debounceMetrics();
  expect(ui.metrics).toHaveBeenCalledOnce();
  await act(async () => { resolve({ skills: skillRows([100, 200]) }); });
  expect(pending()).toBe('false');
  expect(ranks()).toHaveLength(2);
  ranks().find(button => button['aria-pressed'])!.onClick!({} as never);
  expect(session.inspectSkill).not.toHaveBeenCalled();
  ranks().find(button => !button['aria-pressed'])!.onClick!({} as never);
  expect(session.inspectSkill).toHaveBeenLastCalledWith(1, 1, 1);

  await render('skills'); await debounceMetrics();
  expect(ui.metrics).toHaveBeenCalledOnce();
  expect(nodeWith('class', 'skill-dps-value').getAttribute('data-kind')).toBe('damage');
  await render('overview');
  expect(pending()).toBe('true');
  await debounceMetrics();
  expect(pending()).toBe('false');
  expect(ui.metrics).toHaveBeenCalledOnce();

  session.stateVersion = 2;
  let resolveStale!: typeof resolve;
  ui.metrics.mockReturnValueOnce(new Promise(done => { resolveStale = done; }));
  await render('overview');
  expect(pending()).toBe('true');
  expect(ranks().every(button => button.disabled)).toBe(true);
  await debounceMetrics();
  expect(ui.metrics).toHaveBeenCalledTimes(2);

  session.stateVersion = 3;
  ui.metrics.mockReturnValueOnce(new Promise(done => { resolve = done; }));
  await render('overview'); await debounceMetrics();
  expect(ui.metrics).toHaveBeenCalledTimes(3);
  await act(async () => { resolveStale({ skills: skillRows([1000, 2000]) }); });
  expect(pending()).toBe('true');
  expect(ranks().every(button => button.disabled)).toBe(true);
  await act(async () => { resolve({ skills: skillRows([100, 300]) }); });
  expect(pending()).toBe('false');
  expect(ranks().every(button => !button.disabled)).toBe(true);
  ranks().find(button => !button['aria-pressed'])!.onClick!({} as never);
  expect(session.inspectSkill).toHaveBeenLastCalledWith(1, 1, 3);
});

it.each([false, true])('shares metrics across annotation edits and navigation with the batch completed=%s', async completed => {
  vi.useFakeTimers();
  const saved = defaultStarterSession('Witch');
  saved.state.socketGroups = consumerSession().socketGroups;
  saved.state.params.main_socket_group = 0;
  let session!: BuildSession;
  function Workspace({ page }: { page: 'overview' | 'skills' }) {
    session = useBuildSession();
    return <>{rail(session)}{page === 'overview'
      ? <Overview session={session} view={{ calc: session.calc, previous: null, stale: false }} lang="en-US" navigate={idle} focusStat={idle} />
      : <SkillsPanel session={session} lang="en-US" active />}</>;
  }
  const render = async (page: 'overview' | 'skills') => {
    await act(async () => root.render(<Workspace page={page} />));
  };
  await render('skills');
  await act(async () => { session.importSession(JSON.stringify(saved)); });
  expect(session.bootError).toBeNull();
  expect(session.busy).toBe(false);
  let resolve!: (report: { skills: SkillMetricsEntry[] }) => void;
  ui.metrics.mockReturnValueOnce(new Promise(done => { resolve = done; }));
  await debounceMetrics();
  expect(ui.metrics).toHaveBeenCalledOnce();
  if (completed) await act(async () => { resolve({ skills: skillRows([100, 200]) }); });

  const { currentRequest, calculationScope, stateVersion } = session;
  const request = currentRequest();
  await act(async () => { session.setAnnotation('skill:0', 'Keep this skill for bosses.'); });
  expect(session.annotations['skill:0']).toBe('Keep this skill for bosses.');
  expect(session.currentRequest).not.toBe(currentRequest);
  expect(session.currentRequest()).toEqual(request);
  expect(session.calculationScope).toBe(calculationScope);
  expect(session.stateVersion).toBe(stateVersion);
  await render('overview'); await debounceMetrics();
  expect(ui.metrics).toHaveBeenCalledOnce();
  if (!completed) await act(async () => { resolve({ skills: skillRows([100, 200]) }); });
  expect(nodeWith('aria-labelledby', 'ov-skills').getAttribute('aria-busy')).toBe('false');
  await render('skills'); await debounceMetrics();
  expect(ui.metrics).toHaveBeenCalledOnce();

  await act(async () => { session.setCharacter({ level: 99 }); });
  expect(session.calculationScope).toBe(calculationScope);
  expect(session.stateVersion).toBe(stateVersion + 1);
  await debounceMetrics();
  expect(ui.metrics).toHaveBeenCalledTimes(2);
  expect(ui.metrics.mock.calls[1][0]).toMatchObject({ character: { level: 99 } });
});
