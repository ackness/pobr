import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import type { CalculateBuildRequest, CalculateBuildResponse, PassiveTreeMeta, SkillMetricsResponse } from '../api/types';
import { advanceImportReview } from '../app/importReview';
import { activeWorkspaceStage, addWorkspaceBuild, createWorkspace, defaultStarterSession, duplicateWorkspaceStage, workspaceEnvelope } from '../lib/buildWorkspace';
import { useBuildSession } from './useBuildSession';
import { STORAGE } from '../lib/sessionStorage';

// Replay the real hook's state and callbacks; boot effects are unnecessary for importing a saved session.
const harness = vi.hoisted(() => ({ calculate: vi.fn<(request: CalculateBuildRequest) => Promise<CalculateBuildResponse>>(),
  metrics: vi.fn<() => Promise<SkillMetricsResponse>>(),
}));
const hooks = await vi.hoisted(async () => {
  const { createHookHarness } = await import('../testing/hookHarness');
  return createHookHarness({ separateCursors: true, refs: { reuse: 'nullish' }, callback: 'identity', effects: 'collect' });
});
vi.mock('react', () => hooks.mockReact());
vi.mock('../api/backend', () => ({ getBackend: async () => ({
  calculateBuild: harness.calculate, skillMetrics: harness.metrics,
  init: async () => {}, gameLocales: async () => [], loadClassNames: async () => ({ classes: {}, ascendancies: {} }),
  loadTreeMeta: async (): Promise<PassiveTreeMeta> => ({ tree: 'Default', classes: [
    { name: 'Wayfarer', base_str: 30, base_dex: 7, base_int: 7 },
  ] }),
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
  harness.calculate.mockReset(); harness.metrics.mockReset();
  const storage = new Map<string, string>();
  vi.stubGlobal('localStorage', { getItem: (key: string) => storage.get(key) ?? null,
    setItem: (key: string, value: string) => storage.set(key, value) });
});
afterEach(() => { vi.restoreAllMocks(); vi.unstubAllGlobals(); });

it('initializes notes when access to localStorage itself throws', () => {
  vi.spyOn(globalThis, 'localStorage', 'get').mockImplementation(() => { throw new Error('Storage blocked'); });
  expect(renderSession().notes).toBe('');
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
