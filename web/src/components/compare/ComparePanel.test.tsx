import type { ComponentProps } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { beforeEach, expect, test, vi } from 'vitest';
import { ComparePanel } from './ComparePanel';
import type { BuildSession, SavedSession } from '../../hooks/useBuildSession';
import { addWorkspaceBuild, createWorkspace, duplicateWorkspaceStage } from '../../lib/buildWorkspace';

// Replay the real picker and apply handlers without requiring a browser environment.
const ui = vi.hoisted(() => ({ index: 0, values: [] as unknown[],
  buttons: [] as ComponentProps<'button'>[], selects: [] as ComponentProps<'select'>[] }));
vi.mock('react', async original => ({ ...await original<typeof import('react')>(),
  useState: (initial: unknown) => {
    const index = ui.index++;
    if (!(index in ui.values)) ui.values[index] = typeof initial === 'function' ? initial() : initial;
    return [ui.values[index], (value: unknown) => { ui.values[index] = value; }];
  },
}));
vi.mock('../../app/pages/navigate', () => ({ useNavigateToPage: () => vi.fn() }));
vi.mock('../../hooks/useSkillName', () => ({ useSkillCatalog: () => new Map() }));
vi.mock('./TreeComparison', () => ({ TreeComparison: () => null }));
vi.mock('../shared/controls', () => ({
  AppButton: (props: ComponentProps<'button'>) => { ui.buttons.push(props); return <button {...props} />; },
  AppInput: (props: ComponentProps<'input'>) => <input {...props} />,
  AppSelectControl: (props: ComponentProps<'select'>) => { ui.selects.push(props); return <select {...props} />; },
}));

const saved = (): SavedSession => ({ version: 1, notes: '', state: {
  pobCode: null, character: { class_name: 'Ranger', ascendancy_name: '', level: 20 },
  allocatedNodes: [], attributeChoices: {}, items: [{ slot: 'ring1', text: 'A ring' }],
  flasks: [], limbs: [], jewels: [], annotations: {}, params: { config_inputs: {} },
  socketGroups: [{ enabled: true, gems: [{ skill_id: 'SparkPlayer', level: 5, quality: 0 }] }],
} });
const render = (session: BuildSession) => {
  ui.index = 0; ui.buttons = []; ui.selects = [];
  return renderToStaticMarkup(<ComparePanel session={session} lang="en-US" />);
};
beforeEach(() => { ui.values = []; });

test('an unrelated build leaves B unselected until the player chooses it', () => {
  const workspace = addWorkspaceBuild(createWorkspace(saved()), 'Imported', saved());
  const session = { workspace, dataVersion: '0.5', applyComparedStage: vi.fn() } as unknown as BuildSession;
  const empty = render(session);
  expect(empty).toContain('Choose B to start comparing.');
  expect(empty).toMatch(/class="cmp-root"[^>]*><div class="cmp-bar"><div class="cmp-stages"/);
  const b = ui.selects.find(select => select['aria-label'] === 'Build / stage B')!;
  expect(b.value).toBe('');
  b.onChange!({ target: { value: workspace.builds[0].activeStage } } as never);
  const selected = render(session);
  expect(selected).not.toContain('Choose B to start comparing.');
  expect(selected).toMatch(/class="cmp-root"[^>]*><div class="cmp-bar"><div class="cmp-stages"/);
  expect(ui.selects.find(select => select['aria-label'] === 'Build / stage B')!.value).toBe(workspace.builds[0].activeStage);
});

test('empty B cannot remove an A item even if its handler is called directly', () => {
  const workspace = duplicateWorkspaceStage(createWorkspace(saved()), 'Target');
  workspace.builds[0].stages[1].saved = { ...saved(), state: { ...saved().state, items: [] } };
  const [a, b] = workspace.builds[0].stages;
  ui.values = [a.id, b.id, 'own', true, '', 'items', {}, {
    key: JSON.stringify([a.saved.state, b.saved.state, 'own']),
    value: { stats: [], before: { unsupported_modifiers: [], item_errors: [] }, after: { unsupported_modifiers: [], item_errors: [] } },
  }];
  const applyComparedStage = vi.fn();
  const session = { workspace, dataVersion: '0.5', applyComparedStage } as unknown as BuildSession;
  const html = render(session);
  expect(html).toContain('B is not set; nothing to apply');
  const button = ui.buttons.find(button => button.children === 'B is not set; nothing to apply')!;
  expect(button.disabled).toBe(true);
  button.onClick!({} as never);
  expect(applyComparedStage).not.toHaveBeenCalled();
  expect(a.saved.state.items).toEqual([{ slot: 'ring1', text: 'A ring' }]);
});

test('skill comparison preserves the imported name of an unresolved gem', () => {
  const workspace = duplicateWorkspaceStage(createWorkspace(saved()), 'Target');
  const [a, b] = workspace.builds[0].stages;
  b.saved = structuredClone(b.saved);
  b.saved.state.socketGroups[0].gems.push({ skill_id: '', name_spec: 'Imported original gem', level: 0, quality: 0 });
  ui.values = [a.id, b.id, 'own', true, '', 'skills', {}, {
    key: JSON.stringify([a.saved.state, b.saved.state, 'own']),
    value: { stats: [], before: { unsupported_modifiers: [], item_errors: [] }, after: { unsupported_modifiers: [], item_errors: [] } },
  }];
  const html = render({ workspace, dataVersion: '0.5' } as unknown as BuildSession);
  expect(html).toContain('Imported original gem · Unresolved gem');
});
