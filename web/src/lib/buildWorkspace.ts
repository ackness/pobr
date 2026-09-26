import type { SavedSession } from '../hooks/useBuildSession';

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
  const stage: BuildStage = { id: crypto.randomUUID(), name: '', saved, drafts: {} };
  const build: LocalBuild = { id: crypto.randomUUID(), name: '', activeStage: stage.id, stages: [stage] };
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
  const stage = { ...activeWorkspaceStage(workspace), id: crypto.randomUUID(), name: name.trim() };
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
