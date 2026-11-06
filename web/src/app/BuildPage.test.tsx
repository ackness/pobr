import { isValidElement, type ReactElement, type ReactNode } from 'react';
import { beforeEach, expect, it, vi } from 'vitest';
import type { BuildSession } from '../hooks/useBuildSession';
import { createWorkspace, addWorkspaceBuild, defaultStarterSession } from '../lib/buildWorkspace';
import { AppInput, AppSelectControl, AppTextarea } from '../components/shared/controls';
import { DropdownMenuContent, DropdownMenuItem } from '../components/ui/dropdown-menu';
import { BuildPage } from './BuildPage';
import { DialogRequests, restoreDialogFocus } from '../components/shared/dialogRequests';

const harness = vi.hoisted(() => ({ focus: vi.fn(),
  confirm: vi.fn<(...args: unknown[]) => Promise<boolean>>(), valid: true }));
const hooks = await vi.hoisted(async () => {
  const { createHookHarness } = await import('../testing/hookHarness');
  return createHookHarness({ lazyState: false, functionalUpdates: false, separateCursors: true,
    refs: { reuse: 'nullish', initialize: (initial, index) => index === 0 ? { focus: harness.focus } : initial },
    effects: { comparison: 'strict' } });
});
vi.mock('react', async original => hooks.mockReact(await original<typeof import('react')>()));
vi.mock('../components/shared/AppDialogs', () => ({ useAppDialogs: () => ({ confirm: harness.confirm }) }));
vi.mock('../components/shared/useSessionAction', () => ({ useSessionAction: () => () => () => harness.valid }));
// The controls' behavior belongs to their own tests; this suite drives the page's actual handlers.
vi.mock('../components/shared/controls', () => ({ AppInput: () => null, AppSelectControl: () => null, AppTextarea: () => null }));
vi.mock('../components/ui/dropdown-menu', () => ({ DropdownMenu: () => null, DropdownMenuTrigger: () => null,
  DropdownMenuContent: () => null, DropdownMenuGroup: () => null, DropdownMenuItem: () => null }));

function elements(node: ReactNode): ReactElement<Record<string, any>>[] {
  if (Array.isArray(node)) return node.flatMap(elements);
  if (!isValidElement(node)) return [];
  const element = node as ReactElement<Record<string, any>>;
  return [element, ...elements(element.props.children)];
}
function fixture() {
  const saved = defaultStarterSession('Witch');
  saved.state.character = { class_name: 'Witch', ascendancy_name: 'Lich', level: 98 };
  saved.state.allocatedNodes = [11, 12, 13];
  saved.notes = 'Keep my notes';
  const workspace = addWorkspaceBuild(createWorkspace(saved), 'Second build', saved);
  workspace.builds[0].name = 'First build';
  const session = { workspace, character: saved.state.character, allocatedNodes: saved.state.allocatedNodes,
    socketGroups: [{ gems: [], enabled: true }], items: [{ slot: 'ring1', text: 'Ring' }], flasks: [], limbs: [], jewels: [],
    annotations: {}, editorDrafts: {}, notes: saved.notes, calcParams: { config_inputs: { quest: 1 } },
    treeMeta: { classes: [{ name: 'Witch', ascendancies: [{ id: 1, name: 'Lich' }] }, { name: 'Ranger', ascendancies: [] }] },
    classNames: { classes: {}, ascendancies: {} }, hasBuildContent: true, loadouts: [], build: null, busy: false,
    exportCode: vi.fn(), exportBuildFiles: vi.fn(), newBuild: vi.fn(), removeBuild: vi.fn(), renameWorkspace: vi.fn(), duplicateBuild: vi.fn(), selectStage: vi.fn(),
  } as unknown as BuildSession;
  let lastRequest: number | undefined, claimed = false;
  let claimImportFocus: () => boolean;
  const render = (focusRequest = 0) => {
    if (lastRequest !== focusRequest) {
      lastRequest = focusRequest; claimed = false;
      claimImportFocus = () => { if (claimed) return false; claimed = true; return true; };
    }
    hooks.beginRender();
    const tree = BuildPage({ session, lang: 'en-US', onImported: vi.fn(), claimImportFocus: claimImportFocus! });
    return elements(tree);
  };
  return { session, render };
}
const control = (nodes: ReturnType<typeof elements>, type: unknown, predicate = (_props: Record<string, any>) => true) =>
  nodes.find(node => node.type === type && predicate(node.props))!.props;
const clickMenu = (nodes: ReturnType<typeof elements>, label: string, row: number) =>
  nodes.filter(node => node.type === DropdownMenuItem && node.props.children === label)[row].props.onSelect();

beforeEach(() => {
  hooks.reset();
  harness.focus.mockClear(); harness.confirm.mockReset(); harness.valid = true;
});

it('focuses the always-visible import textarea on open and explicit reopen, without refocusing on renders', () => {
  const { render } = fixture();
  const nodes = render();
  expect(control(nodes, AppTextarea, props => props.className === 'import-code').ref).toBe(hooks.refs[0]);
  expect(nodes.some(node => node.type === 'details' && node.props.className.includes('bd-import'))).toBe(false);
  hooks.runEffects();
  expect(harness.focus).toHaveBeenCalledTimes(1);
  control(nodes, AppTextarea, props => props.className === 'import-code').onChange({ target: { value: 'Code' } });
  render(); hooks.runEffects();
  expect(harness.focus).toHaveBeenCalledTimes(1);
  render(1); hooks.runEffects();
  expect(harness.focus).toHaveBeenCalledTimes(2);
});

it('asks before resetting a class, lists the affected stage contents, and Cancel leaves all data unchanged', async () => {
  const { session, render } = fixture();
  const before = JSON.stringify(session.workspace);
  const change = control(render(), AppSelectControl, props => props.value === 'Witch').onChange;
  harness.confirm.mockResolvedValueOnce(false);
  await change({ target: { value: 'Ranger' } });
  expect(harness.confirm).toHaveBeenCalledWith(expect.stringContaining('Ranger'), 'Change and clear');
  const message = harness.confirm.mock.calls[0][0] as string;
  for (const text of ['Stage 1', 'level to 1', 'Ascendancy selection', 'passive points: 3', 'Skill groups: 1', 'jewels: 1', 'Notes', 'Configuration']) expect(message).toContain(text);
  expect(session.newBuild).not.toHaveBeenCalled();
  expect(JSON.stringify(session.workspace)).toBe(before);
  expect(session.character?.level).toBe(98);
  expect(session.notes).toBe('Keep my notes');
  harness.confirm.mockResolvedValueOnce(true);
  await change({ target: { value: 'Ranger' } });
  expect(session.newBuild).toHaveBeenCalledWith('Ranger', '');
});

it('does not reset after the owning stage changes while the confirmation is open', async () => {
  const { session, render } = fixture();
  harness.confirm.mockImplementationOnce(async () => { harness.valid = false; return true; });
  await control(render(), AppSelectControl, props => props.value === 'Witch').onChange({ target: { value: 'Ranger' } });
  expect(session.newBuild).not.toHaveBeenCalled();
});

it('each row menu renames, duplicates and deletes its own build, including an inactive row', async () => {
  const { session, render } = fixture();
  const id = session.workspace!.builds[0].id;
  const submit = (nodes: ReturnType<typeof elements>, name: string) => {
    control(nodes, AppInput, props => props['aria-label'] === 'Name').onChange({ target: { value: name } });
    control(render(), 'form').onSubmit({ preventDefault() {} });
  };
  clickMenu(render(), 'Rename build', 0);
  const naming = render();
  const focusName = vi.fn(), preventFocusReset = vi.fn();
  control(naming, AppInput, props => props['aria-label'] === 'Name').ref.current = { focus: focusName };
  control(naming, DropdownMenuContent).onCloseAutoFocus({ preventDefault: preventFocusReset });
  expect(preventFocusReset).toHaveBeenCalledOnce();
  expect(focusName).toHaveBeenCalledOnce();
  submit(naming, 'Renamed first');
  expect(session.renameWorkspace).toHaveBeenCalledWith('build', 'Renamed first', id);
  clickMenu(render(), 'Duplicate build', 0);
  submit(render(), 'Copied first');
  expect(session.duplicateBuild).toHaveBeenCalledWith(id, 'Copied first');
  harness.confirm.mockResolvedValueOnce(false);
  await clickMenu(render(), 'Delete build', 0);
  expect(harness.confirm).toHaveBeenLastCalledWith(expect.stringContaining('First build'), undefined, undefined);
  expect(session.removeBuild).not.toHaveBeenCalled();
  harness.confirm.mockResolvedValueOnce(true);
  vi.stubGlobal('requestAnimationFrame', vi.fn());
  await clickMenu(render(), 'Delete build', 0);
  expect(session.removeBuild).toHaveBeenCalledWith(id);
  expect(session.selectStage).not.toHaveBeenCalled();
  vi.unstubAllGlobals();
});

it('only lists the active stage in the class confirmation', async () => {
  const { session, render } = fixture();
  session.workspace!.builds[1].stages.push({ ...session.workspace!.builds[1].stages[0], id: 'other', name: 'Keep this stage' });
  harness.confirm.mockResolvedValueOnce(false);
  await control(render(), AppSelectControl, props => props.value === 'Witch').onChange({ target: { value: 'Ranger' } });
  expect(harness.confirm.mock.calls[0][0]).not.toContain('Keep this stage');
});


it('does not refocus import when a stage switch remounts the page', () => {
  const { session, render } = fixture();
  render(); hooks.runEffects();
  expect(harness.focus).toHaveBeenCalledOnce();
  const build = session.workspace!.builds[1];
  build.stages.push({ ...build.stages[0], id: 'another-stage' });
  build.activeStage = 'another-stage';
  // Discard only child hook state, as React does for the stage-keyed main element.
  hooks.reset();
  render(); hooks.runEffects();
  expect(harness.focus).toHaveBeenCalledOnce();
});

it.each(['dialog', 'body', 'reopened menu', 'another editor'])('cancelling deletion preserves focus ownership: %s', async destination => {
  const { session, render } = fixture();
  const nodes = render();
  const focus = vi.fn();
  const trigger = { isConnected: true, focus } as unknown as HTMLButtonElement;
  control(nodes, nodes.find(node => node.props['aria-label'] === 'Actions for First build')!.type,
    props => props['aria-label'] === 'Actions for First build').ref(trigger);
  harness.confirm.mockImplementationOnce(async (message, acceptLabel, returnFocus) => {
    const requests = new DialogRequests();
    const answer = requests.open('confirm', message as string, 'en-US', '', acceptLabel as string | undefined, returnFocus as HTMLElement);
    const request = requests.snapshot()!;
    expect(request.trigger).toBe(trigger);
    requests.finish(request, false);
    // The dialog uses this close handler after the keyboard-selected menu item has disappeared.
    const body = {}, dialogButton = {}, menuItem = {}, editor = {};
    const activeElement = { dialog: dialogButton, body, 'reopened menu': menuItem, 'another editor': editor }[destination];
    const content = { ownerDocument: { activeElement, body }, contains: (node: unknown) => node === dialogButton } as unknown as HTMLElement;
    restoreDialogFocus(request, { target: content, preventDefault() {} });
    return await answer === true;
  });
  await clickMenu(nodes, 'Delete build', 0);
  expect(session.removeBuild).not.toHaveBeenCalled();
  expect(focus).toHaveBeenCalledTimes(destination === 'dialog' || destination === 'body' ? 1 : 0);
});
