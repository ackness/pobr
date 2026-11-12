import type { SavedSession } from '../hooks/useBuildSession';
import { createId } from './createId';

export interface BuildStage {
  id: string;
  name: string;
  saved: SavedSession;
  drafts: Record<string, string>;
}

export interface LocalBuild {
  id: string;
  name: string;
  activeStage: string;
  stages: BuildStage[];
  baseline?: { saved: SavedSession; recordedAt: number };
}

/** A build owns one independent comparison snapshot; recording never adds a stage. */
export function recordWorkspaceBaseline(workspace: BuildWorkspace, recordedAt = Date.now()): BuildWorkspace {
  const saved = structuredClone(activeWorkspaceStage(workspace).saved);
  return { ...workspace, builds: workspace.builds.map(build => build.id !== workspace.activeBuild ? build : {
    ...build, baseline: { saved, recordedAt },
  }) };
}

export interface BuildWorkspace {
  version: 1;
  activeBuild: string;
  builds: LocalBuild[];
}

export function activeWorkspaceStage(workspace: BuildWorkspace): BuildStage {
  const build = workspace.builds.find(entry => entry.id === workspace.activeBuild)!;
  return build.stages.find(stage => stage.id === build.activeStage)!;
}

export function createWorkspace(saved: SavedSession): BuildWorkspace {
  const stage: BuildStage = { id: createId(), name: '', saved, drafts: {} };
  const build: LocalBuild = { id: createId(), name: '', activeStage: stage.id, stages: [stage] };
  return { version: 1, activeBuild: build.id, builds: [build] };
}

/** Keep the legacy current-session envelope and all stages in one atomic write. */
export function workspaceEnvelope(workspace: BuildWorkspace): string {
  return JSON.stringify({ ...activeWorkspaceStage(workspace).saved, workspace });
}

export function updateWorkspaceStage(workspace: BuildWorkspace, saved: SavedSession, drafts: Record<string, string>): BuildWorkspace {
  return { ...workspace, builds: workspace.builds.map(build => build.id !== workspace.activeBuild ? build : {
    ...build, stages: build.stages.map(stage => stage.id !== build.activeStage ? stage : { ...stage, saved, drafts }),
  }) };
}

export function selectWorkspaceStage(workspace: BuildWorkspace, buildId: string, stageId?: string): BuildWorkspace {
  const build = workspace.builds.find(entry => entry.id === buildId);
  if (!build || !build.stages.some(stage => stage.id === (stageId ?? build.activeStage))) return workspace;
  return { ...workspace, activeBuild: buildId, builds: workspace.builds.map(entry => entry !== build ? entry : {
    ...entry, activeStage: stageId ?? build.activeStage,
  }) };
}

/** The same initial content is used for a fresh session and first-import cleanup. */
export function defaultStarterSession(className: string): SavedSession {
  return { version: 1, state: {
    pobCode: null, character: { level: 1, class_name: className, ascendancy_name: '' },
    allocatedNodes: [], attributeChoices: {}, socketGroups: [], items: [], flasks: [], limbs: [], jewels: [],
    annotations: {}, params: { config_inputs: {} },
  }, notes: '' };
}

/** Ignore object key order and the empty optional fields materialized on reload. */
function starterContent(saved: SavedSession): string {
  const state = saved.state;
  return JSON.stringify({ ...saved, data_version: undefined, state: { ...state,
    treeVersion: state.treeVersion ?? null, weaponSwap: state.weaponSwap ?? null,
    equipmentJewels: state.equipmentJewels ?? [],
  } }, (_key, value) => value && typeof value === 'object' && !Array.isArray(value)
    ? Object.fromEntries(Object.entries(value).filter(([, entry]) => entry !== undefined).sort(([a], [b]) => a.localeCompare(b))) : value);
}

/** Identify the sole, unnamed default by content, including after a reload. */
export function appendImportedBuild(workspace: BuildWorkspace, build: LocalBuild, defaultStarter?: SavedSession | null): BuildWorkspace {
  const starter = workspace.builds[0], stage = starter.stages[0];
  const replaceStarter = !!defaultStarter && workspace.builds.length === 1 && starter.name === ''
    && !starter.baseline && starter.stages.length === 1 && stage.name === '' && Object.keys(stage.drafts).length === 0
    && starterContent(stage.saved) === starterContent(defaultStarter);
  return { ...workspace, activeBuild: build.id, builds: [...(replaceStarter ? [] : workspace.builds), build] };
}

export function addWorkspaceBuild(workspace: BuildWorkspace, name: string, saved: SavedSession, defaultStarter?: SavedSession | null): BuildWorkspace {
  const added = createWorkspace(saved);
  return appendImportedBuild(workspace, { ...added.builds[0], name: name.trim() }, defaultStarter);
}

export function duplicateWorkspaceStage(workspace: BuildWorkspace, name: string): BuildWorkspace {
  const stage = { ...activeWorkspaceStage(workspace), id: createId(), name: name.trim() };
  return { ...workspace, builds: workspace.builds.map(build => build.id !== workspace.activeBuild ? build : {
    ...build, activeStage: stage.id, stages: [...build.stages, stage],
  }) };
}

/** Copy every stage, retaining content and drafts while giving each entry a fresh ID. */
export function duplicateWorkspaceBuild(workspace: BuildWorkspace, buildId: string, name: string): BuildWorkspace {
  const source = workspace.builds.find(build => build.id === buildId);
  if (!source || !name.trim()) return workspace;
  let activeStage = '';
  const stages = source.stages.map(stage => {
    const id = createId();
    if (stage.id === source.activeStage) activeStage = id;
    return { ...stage, id };
  });
  const copy = { ...source, id: createId(), name: name.trim(), activeStage, stages };
  return { ...workspace, builds: [...workspace.builds, copy] };
}

export function renameWorkspaceEntry(workspace: BuildWorkspace, kind: 'build' | 'stage', name: string, buildId = workspace.activeBuild): BuildWorkspace {
  if (!name.trim()) return workspace;
  return { ...workspace, builds: workspace.builds.map(build => build.id !== buildId ? build : {
    ...build,
    ...(kind === 'build' ? { name: name.trim() } : { stages: build.stages.map(stage => stage.id !== build.activeStage ? stage : { ...stage, name: name.trim() }) }),
  }) };
}

/**
 * Removes the requested build. Removing the active one opens its next neighbor (else the previous).
 * The library always keeps a build, so the only one is never removed.
 */
export function removeWorkspaceBuild(workspace: BuildWorkspace, buildId = workspace.activeBuild): BuildWorkspace {
  const index = workspace.builds.findIndex(build => build.id === buildId);
  if (index < 0 || workspace.builds.length < 2) return workspace;
  const builds = workspace.builds.filter((_, at) => at !== index);
  return { ...workspace, activeBuild: buildId === workspace.activeBuild ? builds[Math.min(index, builds.length - 1)].id : workspace.activeBuild, builds };
}

/**
 * Removes the active stage of the active build, the stage that follows it (else the one before)
 * becoming the active one. A build always keeps a stage, so its only stage is never removed.
 */
export function removeWorkspaceStage(workspace: BuildWorkspace): BuildWorkspace {
  const build = workspace.builds.find(entry => entry.id === workspace.activeBuild);
  const index = build?.stages.findIndex(stage => stage.id === build.activeStage) ?? -1;
  if (!build || index < 0 || build.stages.length < 2) return workspace;
  const stages = build.stages.filter((_, at) => at !== index);
  return { ...workspace, builds: workspace.builds.map(entry => entry !== build ? entry : {
    ...entry, activeStage: stages[Math.min(index, stages.length - 1)].id, stages,
  }) };
}

/** Isolate unreadable stages and builds so the rest of the library remains usable. */
export function parseWorkspace(raw: unknown, parseSession: (json: string) => SavedSession | null,
  onIsolated?: () => void): BuildWorkspace | null {
  try {
    const workspace = raw as BuildWorkspace;
    if (workspace.version !== 1 || !Array.isArray(workspace.builds) || !workspace.builds.length) return null;
    const ids = new Set<string>();
    let isolated = false;
    const isolate = (...details: unknown[]) => {
      isolated = true;
      console.warn('Unreadable workspace entry isolated.', ...details);
    };
    const builds: LocalBuild[] = [];
    for (const build of workspace.builds) {
      if (!build || typeof build.id !== 'string' || !build.id || typeof build.name !== 'string' || !Array.isArray(build.stages) || !build.stages.length) {
        isolate(build);
        continue;
      }
      if (ids.has(build.id)) { isolate(build); continue; }
      const buildIds = new Set([build.id]);
      const stages: BuildStage[] = [];
      for (const stage of build.stages) {
        if (!stage || typeof stage.id !== 'string' || !stage.id) {
          isolate(build.id, stage);
          continue;
        }
        if (ids.has(stage.id) || buildIds.has(stage.id)) { isolate(build.id, stage); continue; }
        const saved = parseSession(JSON.stringify(stage.saved));
        if (typeof stage.name !== 'string' || !saved || !stage.drafts || typeof stage.drafts !== 'object'
          || Array.isArray(stage.drafts) || Object.values(stage.drafts).some(text => typeof text !== 'string')) {
          isolate(build.id, stage);
          continue;
        }
        buildIds.add(stage.id);
        stages.push({ id: stage.id, name: stage.name, saved, drafts: stage.drafts });
      }
      if (!stages.length) continue;
      const activeStage = stages.find(stage => stage.id === build.activeStage)?.id ?? stages[0].id;
      if (activeStage !== build.activeStage) isolate(build.id, build.activeStage);
      let baseline: LocalBuild['baseline'];
      if (build.baseline !== undefined) {
        const saved = build.baseline && parseSession(JSON.stringify(build.baseline.saved));
        if (!saved || !Number.isFinite(build.baseline.recordedAt) || build.baseline.recordedAt < 0 || Number.isNaN(new Date(build.baseline.recordedAt).getTime())) {
          isolate(build.id, build.baseline);
        } else baseline = { saved, recordedAt: build.baseline.recordedAt };
      }
      buildIds.forEach(id => ids.add(id));
      builds.push({ id: build.id, name: build.name, activeStage, stages, ...(baseline ? { baseline } : {}) });
    }
    if (!builds.length) return null;
    const activeBuild = builds.find(build => build.id === workspace.activeBuild)?.id ?? builds[0].id;
    if (activeBuild !== workspace.activeBuild) isolate(workspace.activeBuild);
    if (isolated) onIsolated?.();
    return { version: 1, activeBuild, builds };
  } catch { return null; }
}

export interface StageSnapshot {
  buildId: string;
  stageId: string;
  saved: SavedSession;
}

/** Apply only to the exact snapshots inspected, retaining selection and every other stage. */
export function replaceComparedStage(workspace: BuildWorkspace, a: StageSnapshot, b: StageSnapshot,
  state: SavedSession['state'], dataVersion?: string): BuildWorkspace | null {
  const find = (ref: StageSnapshot) => workspace.builds.find(build => build.id === ref.buildId)?.stages.find(stage => stage.id === ref.stageId);
  const target = find(a), source = find(b);
  if (!target || !source || target === source || target.saved !== a.saved || source.saved !== b.saved
    || Object.keys(target.drafts).length) return null;
  return { ...workspace, builds: workspace.builds.map(build => build.id !== a.buildId ? build : {
    ...build, stages: build.stages.map(stage => stage !== target ? stage : { ...stage, saved: {
      ...stage.saved, state, ...(dataVersion ? { data_version: dataVersion } : {}),
    } }),
  }) };
}
