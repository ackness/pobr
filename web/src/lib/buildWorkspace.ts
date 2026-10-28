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

export function addWorkspaceBuild(workspace: BuildWorkspace, name: string, saved: SavedSession): BuildWorkspace {
  const added = createWorkspace(saved);
  return { ...workspace, activeBuild: added.activeBuild, builds: [...workspace.builds, { ...added.builds[0], name: name.trim() }] };
}

export function duplicateWorkspaceStage(workspace: BuildWorkspace, name: string): BuildWorkspace {
  const stage = { ...activeWorkspaceStage(workspace), id: createId(), name: name.trim() };
  return { ...workspace, builds: workspace.builds.map(build => build.id !== workspace.activeBuild ? build : {
    ...build, activeStage: stage.id, stages: [...build.stages, stage],
  }) };
}

export function renameWorkspaceEntry(workspace: BuildWorkspace, kind: 'build' | 'stage', name: string): BuildWorkspace {
  if (!name.trim()) return workspace;
  return { ...workspace, builds: workspace.builds.map(build => build.id !== workspace.activeBuild ? build : {
    ...build,
    ...(kind === 'build' ? { name: name.trim() } : { stages: build.stages.map(stage => stage.id !== build.activeStage ? stage : { ...stage, name: name.trim() }) }),
  }) };
}

/**
 * Removes the active build, the build that follows it (else the one before) becoming the active
 * one. The library always keeps a build, so the only one is never removed.
 */
export function removeWorkspaceBuild(workspace: BuildWorkspace): BuildWorkspace {
  const index = workspace.builds.findIndex(build => build.id === workspace.activeBuild);
  if (index < 0 || workspace.builds.length < 2) return workspace;
  const builds = workspace.builds.filter((_, at) => at !== index);
  return { ...workspace, activeBuild: builds[Math.min(index, builds.length - 1)].id, builds };
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

/** Reject malformed collections rather than silently dropping a player's stages. */
export function parseWorkspace(raw: unknown, parseSession: (json: string) => SavedSession | null): BuildWorkspace | null {
  try {
    const workspace = raw as BuildWorkspace;
    if (workspace.version !== 1 || !Array.isArray(workspace.builds) || !workspace.builds.length) return null;
    const ids = new Set<string>();
    const validId = (id: unknown) => typeof id === 'string' && !!id && !ids.has(id) && !!ids.add(id);
    const builds: LocalBuild[] = [];
    for (const build of workspace.builds) {
      if (!validId(build.id) || typeof build.name !== 'string' || !Array.isArray(build.stages) || !build.stages.length) return null;
      const stages: BuildStage[] = [];
      for (const stage of build.stages) {
        const saved = parseSession(JSON.stringify(stage.saved));
        if (!validId(stage.id) || typeof stage.name !== 'string' || !saved || !stage.drafts || typeof stage.drafts !== 'object'
          || Array.isArray(stage.drafts) || Object.values(stage.drafts).some(text => typeof text !== 'string')) return null;
        stages.push({ id: stage.id, name: stage.name, saved, drafts: stage.drafts });
      }
      if (!stages.some(stage => stage.id === build.activeStage)) return null;
      builds.push({ id: build.id, name: build.name, activeStage: build.activeStage, stages });
    }
    if (!builds.some(build => build.id === workspace.activeBuild)) return null;
    return { version: 1, activeBuild: workspace.activeBuild, builds };
  } catch { return null; }
}

export interface StageSnapshot {
  buildId: string;
  stageId: string;
  saved: SavedSession;
}

/** Apply only to the exact snapshots inspected, retaining selection and every other stage. */
export function replaceComparedStage(workspace: BuildWorkspace, a: StageSnapshot, b: StageSnapshot,
  state: SavedSession['state']): BuildWorkspace | null {
  const find = (ref: StageSnapshot) => workspace.builds.find(build => build.id === ref.buildId)?.stages.find(stage => stage.id === ref.stageId);
  const target = find(a), source = find(b);
  if (!target || !source || target === source || target.saved !== a.saved || source.saved !== b.saved
    || Object.keys(target.drafts).length) return null;
  return { ...workspace, builds: workspace.builds.map(build => build.id !== a.buildId ? build : {
    ...build, stages: build.stages.map(stage => stage !== target ? stage : { ...stage, saved: { ...stage.saved, state } }),
  }) };
}
