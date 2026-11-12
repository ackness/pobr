import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import type { BuildJson, CalculateBuildRequest, CalculateBuildResponse, PassiveTreeMeta, SkillMetricsResponse } from '../api/types';
import { advanceImportReview } from '../app/importReview';
import { activeWorkspaceStage, addWorkspaceBuild, createWorkspace, defaultStarterSession, duplicateWorkspaceStage, workspaceEnvelope } from '../lib/buildWorkspace';
import { useBuildSession } from './useBuildSession';
import { STORAGE } from '../lib/sessionStorage';

// Replay the real hook's state and callbacks; boot effects are unnecessary for importing a saved session.
const harness = vi.hoisted(() => ({ calculate: vi.fn<(request: CalculateBuildRequest) => Promise<CalculateBuildResponse>>(),
  metrics: vi.fn<() => Promise<SkillMetricsResponse>>(),
  decode: vi.fn<(code: string) => Promise<BuildJson>>(),
  meta: vi.fn<() => Promise<PassiveTreeMeta>>(),
}));
const hooks = await vi.hoisted(async () => {
  const { createHookHarness } = await import('../testing/hookHarness');
  return createHookHarness({ separateCursors: true, refs: { reuse: 'nullish' }, callback: 'identity', effects: 'collect' });
});
vi.mock('react', () => hooks.mockReact());
vi.mock('../api/backend', () => ({ getBackend: async () => ({
  calculateBuild: harness.calculate, skillMetrics: harness.metrics,
  decodeBuild: harness.decode, decodeBuildFile: harness.decode,
  init: async () => {}, gameLocales: async () => [], loadClassNames: async () => ({ classes: {}, ascendancies: {} }),
  loadTreeMeta: harness.meta,
}) }));

function deferred<T>() {
  let resolve!: (value: T) => void, reject!: (reason: Error) => void;
  const promise = new Promise<T>((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
}
function renderSession() {
  hooks.beginRender();
  return useBuildSession();
}
const result = (skill_id: string, value: number) => ({ main_skill: { skill_id }, stats: [{ id: 'CombinedDPS', value }] }) as CalculateBuildResponse;
const initial = result('FireballPlayer', 100), selected = result('SparkPlayer', 900);
function importWithoutSelection() {
  const saved = defaultStarterSession('Witch');
  saved.state.socketGroups = ['FireballPlayer', 'SparkPlayer'].map(skill_id => ({ enabled: true,
    gems: [{ skill_id, level: 20, quality: 0 }] }));
  renderSession().importSession(JSON.stringify(saved));
}

beforeEach(() => {
  hooks.reset();
  harness.calculate.mockReset(); harness.metrics.mockReset(); harness.decode.mockReset();
  harness.meta.mockResolvedValue({ tree: 'Default', classes: [{ name: 'Wayfarer', base_str: 30, base_dex: 7, base_int: 7 }] });
  const storage = new Map<string, string>();
  vi.stubGlobal('localStorage', { getItem: (key: string) => storage.get(key) ?? null,
    setItem: (key: string, value: string) => storage.set(key, value) });
});
afterEach(() => { vi.restoreAllMocks(); vi.unstubAllGlobals(); });

it('initializes notes when access to localStorage itself throws', () => {
  vi.spyOn(globalThis, 'localStorage', 'get').mockImplementation(() => { throw new Error('Storage blocked'); });
  expect(renderSession().notes).toBe('');
});

it('refuses missing class metadata instead of creating a class not supplied by the data pack', async () => {
  harness.meta.mockResolvedValue({ tree: 'Default', classes: [] });
  vi.stubGlobal('window', { addEventListener: vi.fn(), removeEventListener: vi.fn() });
  renderSession();
  const cleanups = hooks.runEffects();
  await vi.waitFor(() => expect(renderSession().bootError).toContain('No character classes available'));
  expect(renderSession().character).toBeNull();
  expect(localStorage.getItem(STORAGE.workspace)).toBeNull();
  cleanups.forEach(cleanup => cleanup?.());
});

it('ignores storage events when access to localStorage becomes blocked', () => {
  const addEventListener = vi.fn(), removeEventListener = vi.fn();
  vi.stubGlobal('window', { addEventListener, removeEventListener });
  renderSession();
  const cleanup = hooks.effects.at(-1)!();
  const onStorage = addEventListener.mock.calls.find(([type]) => type === 'storage')![1];
  vi.spyOn(globalThis, 'localStorage', 'get').mockImplementation(() => { throw new Error('Storage blocked'); });
  expect(() => onStorage({ storageArea: null })).not.toThrow();
  if (cleanup) cleanup();
  expect(removeEventListener).toHaveBeenCalledWith('storage', onStorage);
});

it('delayed highest-DPS metrics and their recalculation finish before the import summary freezes', async () => {
  const metrics = deferred<SkillMetricsResponse>(), recalculation = deferred<CalculateBuildResponse>();
  harness.metrics.mockReturnValue(metrics.promise);
  harness.calculate.mockImplementation(async request => request.main_socket_group === 1 ? recalculation.promise : initial);
  importWithoutSelection();
  await vi.waitFor(() => expect(renderSession().calc?.main_skill?.skill_id).toBe('FireballPlayer'));
  // The first calculation is already done, but automatic selection is still waiting for metrics.
  const waiting = renderSession();
  expect(waiting.busy).toBe(true);
  const pending = advanceImportReview({ scope: null }, 'import:stage', waiting)!;
  expect(pending.summary).toBeUndefined();
  metrics.resolve({ item_errors: [], skills: [100, 900].map((combined_dps, group_index) => ({
    group_index, active_skill_index: 1, skill_id: ['FireballPlayer', 'SparkPlayer'][group_index], status: 'calculated',
    unsupported_modifiers: [], error: null,
    metrics: { combined_dps, hit_dps: combined_dps, dot_dps: 0, average_hit: 0, action_rate: 1, mana_cost: 0, life_cost: 0, cooldown: 0 },
  })) });
  await vi.waitFor(() => expect(harness.calculate).toHaveBeenCalledTimes(2));
  expect(renderSession().calcParams.main_socket_group).toBe(1);
  expect(renderSession().busy).toBe(true);
  expect(advanceImportReview(pending, 'import:stage', renderSession())?.summary).toBeUndefined();
  recalculation.resolve(selected);
  await vi.waitFor(() => expect(renderSession().busy).toBe(false));
  const summary = advanceImportReview(pending, 'import:stage', renderSession())!.summary!;
  expect(summary.calc?.main_skill?.skill_id).toBe('SparkPlayer');
  expect(summary.calc?.stats[0].value).toBe(900);
});

it('a rejected metrics request releases busy state so the completed calculation can still be summarised', async () => {
  const metrics = deferred<SkillMetricsResponse>();
  harness.metrics.mockReturnValue(metrics.promise);
  harness.calculate.mockResolvedValue(initial);
  importWithoutSelection();
  await vi.waitFor(() => expect(renderSession().calc).toBe(initial));
  expect(renderSession().busy).toBe(true);
  metrics.reject(new Error('Metrics unavailable'));
  await vi.waitFor(() => expect(renderSession().busy).toBe(false));
  expect(advanceImportReview({ scope: null }, 'import:stage', renderSession())?.summary?.calc).toBe(initial);
});

it.each(['workspace', 'build'] as const)('refuses a %s file with an invalid inactive stage without changing the current workspace or library', async format => {
  const saved = defaultStarterSession('Wayfarer');
  saved.notes = 'Keep these notes';
  harness.calculate.mockResolvedValue(initial);
  renderSession().importSession(JSON.stringify(saved));
  await vi.waitFor(() => expect(renderSession().calc).toBe(initial));
  const current = renderSession();
  const before = current.exportWorkspace();
  const library = JSON.stringify(current.library);
  localStorage.setItem(STORAGE.library, library);
  const malformed = duplicateWorkspaceStage(structuredClone(current.workspace!), 'Valid active copy');
  malformed.builds[0].stages[0].saved = structuredClone(malformed.builds[0].stages[0].saved);
  malformed.builds[0].stages[0].saved.state.socketGroups = [
    { enabled: true, gems: [{ skill_id: 'CometPlayer', level: 20, quality: -1 }] },
  ];
  const raw = format === 'workspace' ? JSON.stringify({ ...saved, workspace: malformed })
    : JSON.stringify({ format: 'pobr-build', version: 1, build: malformed.builds[0] });
  expect(() => renderSession().importSession(raw)).toThrow(format === 'workspace' ? 'invalid workspace file' : 'invalid multi-stage build file');
  expect(renderSession().exportWorkspace()).toBe(before);
  expect(renderSession().workspace).toBe(current.workspace);
  expect(renderSession().notes).toBe(saved.notes);
  expect(renderSession().workspaceRecovery).toBeNull();
  expect(JSON.stringify(renderSession().library)).toBe(library);
  expect(localStorage.getItem(STORAGE.workspace)).toBe(before);
  expect(localStorage.getItem(STORAGE.library)).toBe(library);
});

it('boots into a valid stage when the active build and legacy current-session copy are invalid', async () => {
  const saved = defaultStarterSession('Wayfarer');
  saved.notes = 'Recovered on startup';
  const valid = createWorkspace(saved);
  const workspace = addWorkspaceBuild(valid, 'Broken', saved);
  workspace.builds[1].stages[0].saved = null as never;
  const raw = JSON.stringify({ version: 1, state: null, notes: '', workspace });
  localStorage.setItem(STORAGE.workspace, raw);
  vi.stubGlobal('window', { addEventListener: vi.fn(), removeEventListener: vi.fn() });
  harness.calculate.mockResolvedValue(initial);
  renderSession();
  const cleanups = hooks.runEffects();
  await vi.waitFor(() => expect(renderSession().calc).toBe(initial));
  const restored = renderSession();
  expect(restored.bootError).toBeNull();
  expect(restored.character?.class_name).toBe('Wayfarer');
  expect(restored.notes).toBe(saved.notes);
  expect(restored.workspace?.activeBuild).toBe(valid.activeBuild);
  expect(restored.workspace?.builds).toEqual(valid.builds);
  expect(restored.workspaceRecovery).toBe(raw);
  expect(localStorage.getItem(STORAGE.workspace)).toBe(raw);
  restored.setNotes('Recovered edits');
  expect(renderSession().notes).toBe('Recovered edits');
  expect(localStorage.getItem(STORAGE.workspace)).toBe(raw);
  expect(JSON.parse(renderSession().exportWorkspace()).notes).toBe('Recovered edits');
  cleanups.forEach(cleanup => cleanup?.());
  hooks.reset();
  renderSession();
  const reloadedCleanups = hooks.runEffects();
  await vi.waitFor(() => expect(renderSession().calc).toBe(initial));
  expect(renderSession().workspaceRecovery).toBe(raw);
  expect(renderSession().notes).toBe(saved.notes);
  expect(localStorage.getItem(STORAGE.workspace)).toBe(raw);
  reloadedCleanups.forEach(cleanup => cleanup?.());
});

it('follows a merged active stage while preserving an unrelated unreadable entry', async () => {
  vi.stubGlobal('window', { setTimeout, clearTimeout });
  const saved = defaultStarterSession('Wayfarer');
  const workspace = createWorkspace(saved);
  harness.calculate.mockResolvedValue(initial);
  renderSession().importSession(workspaceEnvelope(workspace));
  await vi.waitFor(() => expect(renderSession().calc).toBe(initial));
  const changed = structuredClone(renderSession().workspace!);
  const stage = activeWorkspaceStage(changed);
  stage.saved.state.character.level = 80;
  const theirs = addWorkspaceBuild(changed, 'Unreadable', saved);
  theirs.builds[1].stages[0].saved = null as never;
  const raw = JSON.stringify({ version: 1, state: null, workspace: theirs });
  localStorage.setItem(STORAGE.workspace, raw);
  // This write discovers the other tab's change before its storage event arrives.
  renderSession().setNotes(saved.notes);
  await vi.waitFor(() => expect(renderSession().character?.level).toBe(80));
  expect(renderSession().workspaceRecovery).toBe(raw);
  expect(localStorage.getItem(STORAGE.workspace)).toBe(raw);
  renderSession().setNotes('After following the other tab');
  expect(activeWorkspaceStage(renderSession().workspace!).saved.state.character.level).toBe(80);
  renderSession().dismissSyncNotice();
});

it.each((['write', 'event'] as const).flatMap(discovery => [
  [discovery, '  {"version": 2, "workspace": {"version": 2}, "notes": "原始存档"}\n'],
  [discovery, '  {"workspace": ["原始存档"'],
] as const))('preserves completely unreadable storage discovered by a %s: %s', async (discovery, raw) => {
  const addEventListener = vi.fn();
  vi.stubGlobal('window', { addEventListener, removeEventListener: vi.fn(), setTimeout, clearTimeout });
  harness.calculate.mockResolvedValue(initial);
  renderSession().importSession(JSON.stringify(defaultStarterSession('Wayfarer')));
  await vi.waitFor(() => expect(renderSession().calc).toBe(initial));
  localStorage.setItem(STORAGE.workspace, raw);
  if (discovery === 'event') {
    hooks.effects.at(-1)!();
    addEventListener.mock.calls.find(([type]) => type === 'storage')![1]({ storageArea: localStorage, key: STORAGE.workspace });
    expect(renderSession().workspaceRecovery).toBe(raw);
  }
  renderSession().setNotes('Edits retained in memory');
  expect(localStorage.getItem(STORAGE.workspace)).toBe(raw);
  expect(renderSession().workspaceRecovery).toBe(raw);
  expect(JSON.parse(renderSession().exportWorkspace()).notes).toBe('Edits retained in memory');
});

it('keeps recovered deletions when the same stored change is read again before its storage event', async () => {
  vi.stubGlobal('window', { setTimeout, clearTimeout });
  harness.calculate.mockResolvedValue(initial);
  renderSession().importSession(JSON.stringify(defaultStarterSession('Wayfarer')));
  await vi.waitFor(() => expect(renderSession().calc).toBe(initial));
  const saved = defaultStarterSession('Wayfarer');
  const added = addWorkspaceBuild(structuredClone(renderSession().workspace!), 'Remote build', saved);
  const theirs = addWorkspaceBuild(added, 'Unreadable', saved);
  theirs.builds[2].stages[0].saved = null as never;
  const raw = workspaceEnvelope(theirs);
  localStorage.setItem(STORAGE.workspace, raw);
  renderSession().setNotes(saved.notes);
  expect(renderSession().workspace?.builds).toHaveLength(2);
  renderSession().removeBuild(added.activeBuild);
  expect(renderSession().workspace?.builds).toHaveLength(1);
  renderSession().setNotes('Recovered local notes');
  expect(JSON.parse(renderSession().exportWorkspace()).workspace.builds).toHaveLength(1);
  expect(renderSession().workspaceRecovery).toBe(raw);
  expect(localStorage.getItem(STORAGE.workspace)).toBe(raw);
  renderSession().dismissSyncNotice();
});

it('keeps in-memory recovered edits across later storage events that update another stage', async () => {
  const addEventListener = vi.fn();
  vi.stubGlobal('window', { addEventListener, removeEventListener: vi.fn(), setTimeout, clearTimeout });
  harness.calculate.mockResolvedValue(initial);
  const saved = defaultStarterSession('Wayfarer');
  const workspace = duplicateWorkspaceStage(createWorkspace(saved), 'Inactive');
  renderSession().importSession(workspaceEnvelope(workspace));
  await vi.waitFor(() => expect(renderSession().calc).toBe(initial));
  hooks.effects.at(-1)!();
  const onStorage = addEventListener.mock.calls.find(([type]) => type === 'storage')![1];
  const theirs = addWorkspaceBuild(structuredClone(workspace), 'Unreadable', saved);
  theirs.builds[1].stages[0].saved = null as never;
  const saveRemote = () => {
    const raw = workspaceEnvelope(theirs);
    localStorage.setItem(STORAGE.workspace, raw);
    onStorage({ storageArea: localStorage, key: STORAGE.workspace });
  };
  saveRemote();
  renderSession().setNotes('Recovered local notes');
  for (const notes of ['Remote first edit', 'Remote second edit']) {
    theirs.builds[0].stages[0].saved = { ...saved, notes };
    saveRemote();
    expect(renderSession().notes).toBe('Recovered local notes');
    expect(activeWorkspaceStage(renderSession().workspace!).saved.notes).toBe('Recovered local notes');
  }
  renderSession().dismissSyncNotice();
});

it.each((['code', 'file'] as const).flatMap(format => [
  [format, 'state'], [format, 'library'],
] as const))('a failed %s import during %s preparation leaves display metadata, workspace, notes and library unchanged', async (format, failure) => {
  harness.calculate.mockResolvedValue(initial);
  renderSession().importSession(JSON.stringify(defaultStarterSession('Wayfarer')));
  await vi.waitFor(() => expect(renderSession().calc).toBe(initial));
  const before = renderSession();
  const raw = before.exportWorkspace(), library = JSON.stringify(before.library);
  // Refuse the import even when a later preparation step discovers malformed content.
  harness.decode.mockResolvedValue({ character: { class_name: 'Oracle', level: 90, ascendancy_name: '' },
    tree: { allocated_nodes: [] }, socket_groups: [], notes: 'Imported notes', config_inputs: {},
    items: failure === 'state' ? null : { equipped: [{ slot: 'ring1', text: null }] } } as unknown as BuildJson);
  expect(await before.importCode(format === 'code' ? 'build-code' : '{}')).toBe(false);
  const after = renderSession();
  expect(after.build).toBe(before.build);
  expect(after.exportWorkspace()).toBe(raw);
  expect(after.currentRequest()).toEqual(before.currentRequest());
  expect(after.notes).toBe(before.notes);
  expect(JSON.stringify(after.library)).toBe(library);
  expect(localStorage.getItem(STORAGE.workspace)).toBe(raw);
});

it('applies a complete code import and assigns collected equipment to the newly imported build', async () => {
  harness.calculate.mockResolvedValue(initial);
  renderSession().importSession(JSON.stringify(defaultStarterSession('Wayfarer')));
  await vi.waitFor(() => expect(renderSession().calc).toBe(initial));
  harness.decode.mockResolvedValue({ character: { class_name: 'Oracle', level: 90, ascendancy_name: '' },
    tree: { allocated_nodes: [], attribute_choices: {} }, socket_groups: [], notes: 'Imported notes', config_inputs: {},
    items: { equipped: [{ slot: 'relic', text: 'Future relic' }] } } as unknown as BuildJson);
  expect(await renderSession().importCode('complete-build-code')).toBe(true);
  const imported = renderSession();
  expect(imported.character?.class_name).toBe('Oracle');
  expect(imported.notes).toBe('Imported notes');
  expect(imported.build?.character).toEqual(imported.character);
  expect(imported.workspace?.builds).toHaveLength(2);
  expect(imported.library.items[0].sourceBuild?.id).toBe(imported.workspace?.activeBuild);
  expect(imported.library.items[0].text).toBe('Future relic');
  expect(localStorage.getItem(STORAGE.workspace)).toBe(imported.exportWorkspace());
});
