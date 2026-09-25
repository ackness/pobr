import { describe, expect, it } from 'vitest';
import { parseSaved, type SavedSession } from '../hooks/useBuildSession';
import { activeWorkspaceStage, addWorkspaceBuild, createWorkspace, duplicateWorkspaceStage, parseWorkspace, renameWorkspaceEntry, selectWorkspaceStage, updateWorkspaceStage, workspaceEnvelope } from './buildWorkspace';

const saved: SavedSession = {
  version: 1, notes: 'Leveling route', state: {
    pobCode: null, character: { level: 30, class_name: 'Ranger', ascendancy_name: 'Deadeye' },
    allocatedNodes: [12, 34], attributeChoices: { '12': 'dex' }, socketGroups: [],
    items: [{ slot: 'ring1', text: 'Rarity: NORMAL\nSapphire Ring' }], flasks: [], jewels: [],
    annotations: { 'item:ring1': 'Keep until maps' }, params: { config_inputs: {} },
  },
};

describe('local build workspace', () => {
  it('migrates a standalone session without changing its payload', () => {
    const workspace = createWorkspace(saved);
    expect(activeWorkspaceStage(workspace).saved).toEqual(saved);
    expect(parseSaved(workspaceEnvelope(workspace))).toEqual(saved);
  });

  it('keeps stage progress and editor drafts independent after copying', () => {
    const initial = updateWorkspaceStage(createWorkspace(saved), saved, { 'item:ring1': 'Unapplied item' });
    const firstId = activeWorkspaceStage(initial).id;
    const copied = duplicateWorkspaceStage(initial, ' Endgame ');
    const edited = updateWorkspaceStage(copied, { ...saved, notes: 'Bossing route', state: { ...saved.state, allocatedNodes: [56], items: [], character: { ...saved.state.character, level: 95 } } }, {});
    expect(activeWorkspaceStage(edited).name).toBe('Endgame');
    const original = activeWorkspaceStage(selectWorkspaceStage(edited, edited.activeBuild, firstId));
    expect(original.saved).toEqual(saved);
    expect(original.drafts).toEqual({ 'item:ring1': 'Unapplied item' });
    expect(initial.builds[0].stages).toHaveLength(1);
  });

  it('remembers the last selected stage separately for each build', () => {
    const initial = duplicateWorkspaceStage(createWorkspace(saved), 'Maps');
    const stage = activeWorkspaceStage(initial).id;
    const added = addWorkspaceBuild(initial, 'Whirlwind', { ...saved, notes: 'Another skill' });
    const returned = selectWorkspaceStage(added, initial.activeBuild);
    expect(activeWorkspaceStage(returned).id).toBe(stage);
    expect(added.builds).toHaveLength(2);
    expect(selectWorkspaceStage(added, 'missing')).toBe(added);
  });

  it('round trips all stages including notes, annotations and drafts', () => {
    const workspace = renameWorkspaceEntry(duplicateWorkspaceStage(createWorkspace(saved), 'Maps'), 'build', 'Ice Shot');
    expect(parseWorkspace(JSON.parse(workspaceEnvelope(workspace)).workspace, parseSaved)).toEqual(workspace);
  });

  it('rejects duplicate IDs, missing active stages and malformed saves', () => {
    const workspace = duplicateWorkspaceStage(createWorkspace(saved), 'Maps');
    const invalid = structuredClone(workspace);
    invalid.builds[0].stages[1].id = invalid.builds[0].stages[0].id;
    expect(parseWorkspace(invalid, parseSaved)).toBeNull();
    expect(parseWorkspace({ ...workspace, activeBuild: 'missing' }, parseSaved)).toBeNull();
    const broken = structuredClone(workspace);
    broken.builds[0].stages[0].saved = null as unknown as SavedSession;
    expect(parseWorkspace(broken, parseSaved)).toBeNull();
  });
});
