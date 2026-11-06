import { act, type ComponentProps } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import type { BuildSession } from '../../hooks/useBuildSession';
import type { GemCatalogEntry } from '../../api/types';
import type { SupportMetadata } from '../../lib/supportOptimizer';
import { SkillsPanel } from './SkillsPanel';

const ui = vi.hoisted(() => ({ buttons: [] as ComponentProps<'button'>[], inputs: [] as ComponentProps<'input'>[],
  gems: [] as GemCatalogEntry[], trade: [] as SupportMetadata[], optimize: vi.fn() }));
vi.mock('../../api/backend', () => ({ getBackend: async () => ({ gemCatalog: async () => ui.gems, gemDescriptions: async () => null, supportGroupsCompatible: async (groups: unknown[]) => groups.map(() => true) }) }));
vi.mock('../../lib/tradeOptimizer', () => ({ loadTradeCatalog: async () => ({ gems: ui.trade }) }));
vi.mock('../../lib/supportOptimizer', async original => ({ ...await original<typeof import('../../lib/supportOptimizer')>(), optimizeSupports: ui.optimize }));
vi.mock('../../hooks/useUpgradeGoal', () => ({ useUpgradeGoal: () => ({ goal: { preset: 'dps' }, setGoal: () => {} }) }));
vi.mock('../../hooks/useLocalizedLines', async original => ({ ...await original<typeof import('../../hooks/useLocalizedLines')>(), useLocalizedLines: (lines: string[]) => lines }));
vi.mock('../../app/jobs/useJobBeacon', () => ({ useJobBeacon: () => {} }));
vi.mock('../../app/pages/skills/useSkillMetrics', () => ({ useSkillMetrics: () => null }));
vi.mock('../shared/PreviewContext', () => ({ usePreview: () => null }));
vi.mock('../shared/NamedSets', () => ({ NamedSets: () => null }));
vi.mock('../shared/WeaponSetControl', () => ({ WeaponSetControl: () => null }));
vi.mock('../shared/PageHeader', () => ({ PageHeader: () => null }));
vi.mock('../shared/AppSelect', () => ({ AppSelect: () => null }));
vi.mock('../shared/NoteEditor', () => ({ NoteEditor: () => null }));
vi.mock('../shared/AppDialogs', () => ({ useAppDialogs: () => ({ confirm: async () => false }) }));
vi.mock('../shared/useSessionAction', () => ({ useSessionAction: () => () => () => true }));
vi.mock('../shared/OptimizerControls', async original => ({ ...await original<typeof import('../shared/OptimizerControls')>(), ObjectiveEditor: () => null, OptimizerProgress: () => null }));
vi.mock('./MinionSelection', () => ({ MinionSelection: () => null }));
vi.mock('./SkillFormControls', () => ({ SkillFormControls: () => null }));
vi.mock('./GemNameCopy', () => ({ GemNameCopy: () => null }));
vi.mock('./SupportDescriptions', () => ({ SupportDescriptions: () => null }));
vi.mock('./GemPicker', () => ({ GemPicker: () => null }));
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
  defaultView = { HTMLIFrameElement: class {} };
  createElement(tag: string) { return new HostNode(tag.toUpperCase(), this); }
  createElementNS(_namespace: string, tag: string) { return this.createElement(tag); }
  createTextNode(text: string) { const node = new HostNode('#text', this); node.nodeType = 3; node.textContent = text; return node; }
  addEventListener() {}
  removeEventListener() {}
}
let root: Root;
beforeEach(() => {
  const document = new HostDocument();
  vi.stubGlobal('document', document);
  vi.stubGlobal('window', document.defaultView);
  vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true);
  vi.stubGlobal('localStorage', { getItem: () => null });
  ui.buttons = []; ui.inputs = [];
  ui.optimize.mockReset().mockResolvedValue({ baseline: { CombinedDPS: 100 }, baselineUnsupported: [], plans: [], excluded: [], candidates: 0, evaluated: 0, unmodeled: 0 });
  root = createRoot(document.createElement('div') as unknown as HTMLElement);
});
afterEach(async () => { await act(async () => root.unmount()); vi.unstubAllGlobals(); });

it('remounts same-skill editors when switching groups and on revision changes, resetting capacity before evaluation', async () => {
  const fire: GemCatalogEntry = { skill_id: 'Fireball', name: 'Fireball', name_zh_cn: '火球', name_zh_tw: '火球', is_support: false, is_lineage: false,
    active_effect_ids: ['Fireball'], colour: 'int', tags: ['Spell'] };
  const supports = Array.from({ length: 5 }, (_, index) => ({ ...fire, skill_id: `Support${index}`, name: `Support ${index}`, is_support: true, active_effect_ids: [] }));
  ui.gems = [fire, ...supports];
  ui.trade = ui.gems.map(gem => ({ ...gem, family: gem.skill_id, max_level: 20, level_requirements: [0], compatibility_known: true, skill_types: ['Spell'] }));
  const groups = [5, 2].map(count => ({ enabled: true, gems: [{ skill_id: fire.skill_id, level: 20, quality: 0 },
    ...supports.slice(0, count).map(gem => ({ skill_id: gem.skill_id, level: 1, quality: 0 }))] }));
  const session = { socketGroups: groups, calcParams: { main_socket_group: 0 }, character: { level: 100 }, busy: false,
    calc: { main_skill: {}, stats: [] }, annotations: {}, items: [], library: { skillSets: [] }, socketGroupRevision: 1, stateVersion: 1,
    currentRequest: () => ({ socket_groups: groups }),
    updateParams: (patch: object) => { Object.assign(session.calcParams, patch); root.render(<SkillsPanel session={session} lang="en-US" active />); },
  } as unknown as BuildSession;
  const click = async (button: ComponentProps<'button'>) => { await act(async () => { button.onClick!({} as never); }); };
  const lastButton = (className: string) => ui.buttons.filter(button => button.className?.split(' ').includes(className)).at(-1)!;
  const capacity = () => ui.inputs.filter(input => input.type === 'number' && input.min === 2 && input.max === 5).at(-1)!;
  await act(async () => root.render(<SkillsPanel session={session} lang="en-US" active />));
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
