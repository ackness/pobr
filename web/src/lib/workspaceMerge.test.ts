import { describe, expect, it } from 'vitest';
import { parseSaved, type SavedSession } from '../hooks/useBuildSession';
import {
  activeWorkspaceStage, addWorkspaceBuild, createWorkspace, duplicateWorkspaceStage, parseWorkspace, removeWorkspaceBuild,
  removeWorkspaceStage, renameWorkspaceEntry, selectWorkspaceStage, updateWorkspaceStage, workspaceEnvelope, type BuildWorkspace,
} from './buildWorkspace';
import { mergeWorkspaces } from './workspaceMerge';

const session = (level: number, notes = ''): SavedSession => ({
  version: 1, notes, state: {
    pobCode: null, character: { level, class_name: 'Ranger', ascendancy_name: 'Deadeye' },
    allocatedNodes: [12], attributeChoices: {}, socketGroups: [], items: [], flasks: [], limbs: [], jewels: [],
    annotations: {}, params: { config_inputs: {} },
  },
});

/** Builds A (stages A1), B (stages B1, B2) and C (stage C1); the active one is `active`. */
function library(active: 'A' | 'B' | 'C' = 'B'): BuildWorkspace {
  let workspace = renameWorkspaceEntry(createWorkspace(session(10)), 'build', 'A');
  workspace = renameWorkspaceEntry(workspace, 'stage', 'A1');
  workspace = renameWorkspaceEntry(addWorkspaceBuild(workspace, 'B', session(20)), 'stage', 'B1');
  workspace = renameWorkspaceEntry(duplicateWorkspaceStage(workspace, 'B2'), 'stage', 'B2');
  workspace = renameWorkspaceEntry(addWorkspaceBuild(workspace, 'C', session(30)), 'stage', 'C1');
  return selectWorkspaceStage(workspace, workspace.builds.find(build => build.name === active)!.id);
}
const build = (workspace: BuildWorkspace, name: string) => workspace.builds.find(entry => entry.name === name)!;
const names = (workspace: BuildWorkspace) => workspace.builds.map(entry => entry.name);
const stageNames = (workspace: BuildWorkspace, name: string) => build(workspace, name).stages.map(stage => stage.name);
/** Edits the stage `stage` of build `name` the way a tab saves an edit. */
function edit(workspace: BuildWorkspace, name: string, stage: string, level: number): BuildWorkspace {
  const target = build(workspace, name);
  return { ...workspace, builds: workspace.builds.map(entry => entry !== target ? entry : {
    ...entry, stages: entry.stages.map(candidate => candidate.name !== stage ? candidate : { ...candidate, saved: session(level, `edited ${stage}`) }),
  }) };
}
const level = (workspace: BuildWorkspace, name: string, stage: string) =>
  build(workspace, name).stages.find(candidate => candidate.name === stage)!.saved.state.character.level;

describe('merging what another tab wrote', () => {
  it('has nothing to merge when neither tab changed anything', () => {
    const base = library();
    expect(mergeWorkspaces(base, base, base)).toEqual({ workspace: base, conflicts: [] });
  });

  it('adopts the other tab when this tab changed nothing since it last saw the library', () => {
    const base = library('B');
    let theirs = addWorkspaceBuild(base, 'D', session(40));
    theirs = edit(theirs, 'A', 'A1', 11);
    const { workspace, conflicts } = mergeWorkspaces(base, base, theirs);
    expect(names(workspace)).toEqual(['A', 'B', 'C', 'D']);
    expect(level(workspace, 'A', 'A1')).toBe(11);
    expect(conflicts).toEqual([]);
    // This tab keeps the build it is looking at; the other tab's choice of D is not imposed on it.
    expect(build(workspace, 'B').id).toBe(base.activeBuild);
    expect(workspace.activeBuild).toBe(base.activeBuild);
  });

  it('keeps both a build this tab created and an edit the other tab made elsewhere', () => {
    const base = library('B');
    const mine = addWorkspaceBuild(base, 'Mine', session(50));
    const theirs = edit(base, 'C', 'C1', 31);
    const { workspace, conflicts } = mergeWorkspaces(base, mine, theirs);
    expect(names(workspace)).toEqual(['A', 'B', 'C', 'Mine']);
    expect(level(workspace, 'C', 'C1')).toBe(31);
    expect(conflicts).toEqual([]);
    expect(workspace.activeBuild).toBe(mine.activeBuild);
  });

  it('takes each stage from the only tab that changed it, and keeps this tab\'s when both did', () => {
    const base = library('B');
    const mine = edit(edit(base, 'B', 'B1', 21), 'A', 'A1', 12);
    const theirs = edit(edit(base, 'B', 'B2', 22), 'A', 'A1', 13);
    const { workspace, conflicts } = mergeWorkspaces(base, mine, theirs);
    expect(level(workspace, 'B', 'B1')).toBe(21);
    expect(level(workspace, 'B', 'B2')).toBe(22);
    // Both tabs changed A1: this tab's edit is the latest one and stays, and the clash is reported.
    expect(level(workspace, 'A', 'A1')).toBe(12);
    expect(conflicts).toEqual([build(base, 'A').stages[0].id]);
  });

  it('keeps a stage the other tab added and a rename this tab made in the same build', () => {
    const base = library('B');
    const theirs = renameWorkspaceEntry(selectWorkspaceStage(duplicateWorkspaceStage(base, 'B3'), base.activeBuild), 'stage', 'B3');
    const mine = renameWorkspaceEntry(base, 'build', 'Renamed');
    const { workspace } = mergeWorkspaces(base, mine, theirs);
    expect(build(workspace, 'Renamed').stages.map(stage => stage.name)).toEqual(['B1', 'B2', 'B3']);
  });

  describe('deleting', () => {
    it('honours the other tab deleting a build this tab did not touch', () => {
      const base = library('A');
      const theirs = removeWorkspaceBuild(selectWorkspaceStage(base, build(base, 'B').id));
      const { workspace } = mergeWorkspaces(base, base, theirs);
      expect(names(workspace)).toEqual(['A', 'C']);
    });

    it('keeps a build the other tab deleted but this tab edited: an edit beats a deletion', () => {
      const base = library('A');
      const theirs = removeWorkspaceBuild(selectWorkspaceStage(base, build(base, 'B').id));
      const mine = edit(base, 'B', 'B1', 25);
      const { workspace } = mergeWorkspaces(base, mine, theirs);
      expect(names(workspace)).toEqual(['A', 'C', 'B']);
      expect(level(workspace, 'B', 'B1')).toBe(25);
    });

    it('does not bring back a build this tab deleted, unless the other tab edited it', () => {
      const base = library('B');
      const mine = removeWorkspaceBuild(base);
      expect(names(mergeWorkspaces(base, mine, base).workspace)).toEqual(['A', 'C']);
      const edited = mergeWorkspaces(base, mine, edit(base, 'B', 'B2', 26)).workspace;
      expect(names(edited)).toEqual(['A', 'B', 'C']);
      expect(level(edited, 'B', 'B2')).toBe(26);
    });

    it('does the same for stages', () => {
      const base = library('B');
      const theirs = removeWorkspaceStage(base);
      const deletedId = base.builds.find(entry => entry.name === 'B')!.activeStage;
      const untouched = mergeWorkspaces(base, base, theirs).workspace;
      expect(stageNames(untouched, 'B')).toHaveLength(1);
      expect(build(untouched, 'B').stages.some(stage => stage.id === deletedId)).toBe(false);
      const mine = updateWorkspaceStage(base, session(99, 'my edit'), {});
      const kept = mergeWorkspaces(base, mine, theirs).workspace;
      expect(stageNames(kept, 'B')).toHaveLength(2);
      expect(activeWorkspaceStage(kept).saved.notes).toBe('my edit');
    });

    it('opens another build when the one this tab was on is deleted by the other tab', () => {
      const base = library('B');
      const theirs = removeWorkspaceBuild(base);
      const { workspace } = mergeWorkspaces(base, base, theirs);
      expect(names(workspace)).toEqual(['A', 'C']);
      expect(workspace.builds.some(entry => entry.id === workspace.activeBuild)).toBe(true);
      expect(workspace.activeBuild).toBe(theirs.activeBuild);
    });

    it('opens another stage when the stage this tab was on is deleted by the other tab', () => {
      const base = library('B');
      const theirs = removeWorkspaceStage(base);
      const { workspace } = mergeWorkspaces(base, base, theirs);
      const merged = build(workspace, 'B');
      expect(merged.stages.some(stage => stage.id === merged.activeStage)).toBe(true);
    });
  });

  it('never leaves the library without a build or a build without a stage', () => {
    const base = library('A');
    const onlyA = removeWorkspaceBuild(removeWorkspaceBuild(selectWorkspaceStage(base, build(base, 'C').id)));
    // This tab holds only A; the other tab deleted A and kept B: neither side has everything.
    const mine = removeWorkspaceBuild(removeWorkspaceBuild(base));
    const theirs = removeWorkspaceBuild(selectWorkspaceStage(base, build(base, 'A').id));
    for (const [a, b] of [[onlyA, onlyA], [mine, theirs]]) {
      const { workspace } = mergeWorkspaces(base, a, b);
      expect(workspace.builds.length).toBeGreaterThan(0);
      expect(workspace.builds.every(entry => entry.stages.length > 0 && entry.stages.some(stage => stage.id === entry.activeStage))).toBe(true);
      expect(workspace.builds.some(entry => entry.id === workspace.activeBuild)).toBe(true);
    }
  });

  it('keeps everything it can when the library as this tab last saw it is not known', () => {
    const base = library('B');
    const mine = addWorkspaceBuild(edit(base, 'A', 'A1', 12), 'Mine', session(50));
    const theirs = edit(addWorkspaceBuild(base, 'Theirs', session(60)), 'A', 'A1', 13);
    const { workspace, conflicts } = mergeWorkspaces(null, mine, theirs);
    expect(new Set(names(workspace))).toEqual(new Set(['A', 'B', 'C', 'Mine', 'Theirs']));
    expect(level(workspace, 'A', 'A1')).toBe(12);
    expect(conflicts).toHaveLength(1);
  });

  it('gives a library the app can store and read back', () => {
    const base = library('B');
    const { workspace } = mergeWorkspaces(base, addWorkspaceBuild(base, 'Mine', session(5)), edit(base, 'C', 'C1', 31));
    expect(parseWorkspace(JSON.parse(workspaceEnvelope(workspace)).workspace, parseSaved)).toEqual(workspace);
  });

  it('is stable: merging what was just merged changes nothing', () => {
    const base = library('B');
    const mine = edit(base, 'B', 'B1', 21);
    const theirs = addWorkspaceBuild(edit(base, 'A', 'A1', 12), 'D', session(40));
    const once = mergeWorkspaces(base, mine, theirs).workspace;
    expect(mergeWorkspaces(theirs, once, theirs).workspace).toEqual(once);
  });
});
