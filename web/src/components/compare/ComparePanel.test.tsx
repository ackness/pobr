import type { ComponentProps } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { beforeEach, expect, test, vi } from 'vitest';
import { ComparePanel } from './ComparePanel';
import type { BuildSession, SavedSession } from '../../hooks/useBuildSession';
import type { Lang } from '../../lib/i18n';
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
vi.mock('../ui/button', () => ({ Button: (props: ComponentProps<'button'>) => { ui.buttons.push(props); return <button {...props} />; } }));
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
const render = (session: BuildSession, lang: Lang = 'en-US') => {
  ui.index = 0; ui.buttons = []; ui.selects = [];
  return renderToStaticMarkup(<ComparePanel session={session} lang={lang} />);
};
beforeEach(() => { ui.values = []; });

test('an unrelated build leaves B unselected until the player chooses it', () => {
  const workspace = addWorkspaceBuild(createWorkspace(saved()), 'Imported', saved());
  const session = { workspace, dataVersion: '0.5', applyComparedStage: vi.fn() } as unknown as BuildSession;
  const empty = render(session);
  expect(empty).toContain('There is no baseline or second stage yet.');
  expect(empty).toMatch(/class="cmp-root"[^>]*><div class="cmp-bar"><div class="cmp-stages"/);
  const b = ui.selects.find(select => select['aria-label'] === 'B: After')!;
  expect(b.value).toBe('');
  expect(b.title).toBeUndefined();
  b.onChange!({ target: { value: workspace.builds[0].activeStage } } as never);
  const selected = render(session);
  expect(selected).not.toContain('There is no baseline or second stage yet.');
  expect(selected).toMatch(/class="cmp-root"[^>]*><div class="cmp-bar"><div class="cmp-stages"/);
  expect(ui.selects.find(select => select['aria-label'] === 'B: After')!.value).toBe(workspace.builds[0].activeStage);
  expect(ui.selects.find(select => select['aria-label'] === 'B: After')!.title).toBeUndefined();
});

test('empty B cannot remove an A item even if its handler is called directly', () => {
  const workspace = duplicateWorkspaceStage(createWorkspace(saved()), 'Target');
  workspace.builds[0].stages[1].saved = { ...saved(), state: { ...saved().state, items: [] } };
  const [a, b] = workspace.builds[0].stages;
  ui.values = [a.id, b.id, 'own', true, '', 'items', {
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

test.each([
  ['en-US', 'Enemy tier', 'Pinnacle Boss', 'Mapping'],
  ['zh-CN', '敌人档位', '巅峰 Boss', '刷图'],
  ['zh-TW', '敵人檔位', '巔峰頭目', '刷圖'],
] as const)('enemy differences use shared names while preserving the tier keys in %s', (lang, label, pinnacle, mapping) => {
  const workspace = duplicateWorkspaceStage(createWorkspace(saved()), 'Target');
  const [a, b] = workspace.builds[0].stages;
  a.saved = { ...a.saved, state: { ...a.saved.state, params: { ...a.saved.state.params, enemy_tier: 'pinnacle' } } };
  b.saved = { ...b.saved, state: { ...b.saved.state, params: { ...b.saved.state.params, enemy_tier: 'none' } } };
  ui.values = [a.id, b.id, 'own', true, '', 'config', {
    key: JSON.stringify([a.saved.state, b.saved.state, 'own']),
    value: { stats: [], before: { unsupported_modifiers: [], item_errors: [] }, after: { unsupported_modifiers: [], item_errors: [] } },
  }];
  const session = { workspace, dataVersion: '0.5', applyComparedStage: vi.fn() } as unknown as BuildSession;
  const html = render(session, lang);
  expect(html).toContain(label);
  expect(html).toContain(pinnacle);
  expect(html).toContain(mapping);
  expect(html).not.toContain('>pinnacle<');
  expect(html).not.toContain('>none<');
  expect(a.saved.state.params.enemy_tier).toBe('pinnacle');
  expect(b.saved.state.params.enemy_tier).toBe('none');
});

test('a baseline opens as A with current B, stays immutable, and can be swapped or replaced by any stage', () => {
  const workspace = duplicateWorkspaceStage(createWorkspace(saved()), 'Later');
  const build = workspace.builds[0];
  build.baseline = { saved: saved(), recordedAt: 1000 };
  const session = { workspace, dataVersion: '0.5', applyComparedStage: vi.fn() } as unknown as BuildSession;
  const html = render(session);
  expect(html).toContain('Baseline (');
  expect(html).toContain('Change: after − before (B − A).');
  expect(ui.selects.find(select => select['aria-label'] === 'A: Before')!.value).toBe(`baseline:${build.id}`);
  expect(ui.selects.find(select => select['aria-label'] === 'B: After')!.value).toBe(build.activeStage);
  ui.buttons.find(button => button['aria-label'] === 'Swap sides')!.onClick!({} as never);
  render(session);
  expect(ui.selects.find(select => select['aria-label'] === 'A: Before')!.value).toBe(build.activeStage);
  expect(ui.selects.find(select => select['aria-label'] === 'B: After')!.value).toBe(`baseline:${build.id}`);
  ui.selects.find(select => select['aria-label'] === 'A: Before')!.onChange!({ target: { value: build.stages[0].id } } as never);
  render(session);
  expect(ui.selects.find(select => select['aria-label'] === 'A: Before')!.value).toBe(build.stages[0].id);
  ui.selects.find(select => select['aria-label'] === 'B: After')!.onChange!({ target: { value: '' } } as never);
  const cleared = render(session);
  expect(ui.selects.find(select => select['aria-label'] === 'A: Before')!.value).toBe(build.stages[0].id);
  expect(ui.selects.find(select => select['aria-label'] === 'B: After')!.value).toBe('');
  expect(cleared).toContain('Choose a build / stage for B');
  expect(build.stages).toHaveLength(2);
  expect(build.baseline.saved).toEqual(saved());
});

test('table changes are after minus before and lower costs are coloured as improvements', () => {
  const workspace = duplicateWorkspaceStage(createWorkspace(saved()), 'Later');
  const [a, b] = workspace.builds[0].stages;
  const stats = [
    { id: 'Life', before: 100, after: 120, delta: 20, percent: 20, changed: true },
    { id: 'ManaCost', before: 20, after: 10, delta: -10, percent: -50, changed: true },
    { id: 'ReqStr', before: 50, after: 80, delta: 30, percent: 60, changed: true },
  ];
  ui.values = [a.id, b.id, 'own', true, '', 'stats', {
    key: JSON.stringify([a.saved.state, b.saved.state, 'own']),
    value: { stats, before: { unsupported_modifiers: [], item_errors: [] }, after: { unsupported_modifiers: [], item_errors: [] } },
  }];
  const html = render({ workspace, dataVersion: '0.5', applyComparedStage: vi.fn() } as unknown as BuildSession);
  expect(html).toMatch(/data-stat="Life"[^]*?data-tone="better"[^>]*>\+20/);
  expect(html).toMatch(/data-stat="ManaCost"[^]*?data-tone="better"[^>]*>-10/);
  expect(html).toMatch(/data-stat="ReqStr"[^]*?data-tone="worse"[^>]*>\+30/);
  expect(html).toContain('A: Before');
  expect(html).toContain('B: After');
  expect(html.match(/<thead>/g)).toHaveLength(1);
  expect(html).toMatch(/class="cmp-bar"><div class="cmp-stages"[^]*?<\/div><\/div><div class="cmp-context"/);
});

test.each(['zh-CN', 'zh-TW'] as const)('items and character identities reuse localized displays in %s', lang => {
  const workspace = duplicateWorkspaceStage(createWorkspace(saved()), 'Later');
  const [a, b] = workspace.builds[0].stages;
  b.saved = { ...b.saved, state: { ...b.saved.state, items: [{ slot: 'ring1', text: 'Rarity: RARE\nNew Ring\nCoral Ring\nUnique ID: kept-id\n+10 to maximum Life' }] } };
  ui.values = [a.id, b.id, 'own', true, '', 'items', {
    key: JSON.stringify([a.saved.state, b.saved.state, 'own']),
    value: { stats: [], before: { unsupported_modifiers: [], item_errors: [] }, after: { unsupported_modifiers: [], item_errors: [] } },
  }];
  const session = { workspace, dataVersion: '0.5', classNames: { classes: { Ranger: '游侠' }, ascendancies: {},
    locales: { [lang]: { classes: { Ranger: lang === 'zh-CN' ? '游侠' : '遊俠' }, ascendancies: {} } } }, applyComparedStage: vi.fn() } as unknown as BuildSession;
  const html = render(session, lang);
  expect(html).toContain('<h4>戒指 1</h4>');
  expect(html).toContain(lang === 'zh-CN' ? '游侠' : '遊俠');
  expect(html).toContain('class="item-display rarity-rare"');
  expect(html).toContain('kept-id');
  const visible = html.replace(/<details>[^]*?<\/details>/g, '');
  expect(visible).not.toContain('kept-id');
  expect(visible).not.toContain('Unique ID');
  expect(html).not.toContain('<details open');
});

test('an inherited enemy tier is named in the comparison instead of showing an absent setting', () => {
  const workspace = duplicateWorkspaceStage(createWorkspace(saved()), 'Target');
  const [a, b] = workspace.builds[0].stages;
  a.saved = { ...a.saved, state: { ...a.saved.state, params: { ...a.saved.state.params, enemy_tier: undefined } } };
  b.saved = { ...b.saved, state: { ...b.saved.state, params: { ...b.saved.state.params, enemy_tier: 'none' } } };
  ui.values = [a.id, b.id, 'own', true, '', 'config', {
    key: JSON.stringify([a.saved.state, b.saved.state, 'own']),
    value: { stats: [], before: { unsupported_modifiers: [], item_errors: [] }, after: { unsupported_modifiers: [], item_errors: [] } },
  }];
  const html = render({ workspace, dataVersion: '0.5', applyComparedStage: vi.fn() } as unknown as BuildSession);
  expect(html).toContain('Pinnacle Boss');
  expect(html).toContain('Mapping');
  expect(a.saved.state.params.enemy_tier).toBeUndefined();
});

test('skill comparison preserves the imported name of an unresolved gem', () => {
  const workspace = duplicateWorkspaceStage(createWorkspace(saved()), 'Target');
  const [a, b] = workspace.builds[0].stages;
  b.saved = structuredClone(b.saved);
  b.saved.state.socketGroups[0].gems.push({ skill_id: '', name_spec: 'Imported original gem', level: 0, quality: 0 });
  ui.values = [a.id, b.id, 'own', true, '', 'skills', {
    key: JSON.stringify([a.saved.state, b.saved.state, 'own']),
    value: { stats: [], before: { unsupported_modifiers: [], item_errors: [] }, after: { unsupported_modifiers: [], item_errors: [] } },
  }];
  const html = render({ workspace, dataVersion: '0.5' } as unknown as BuildSession);
  expect(html).toContain('Imported original gem · Unresolved gem');
});

test('comparison diagnostics use the shared excluded-modifier list and localize item error slots', () => {
  const workspace = duplicateWorkspaceStage(createWorkspace(saved()), 'Later');
  const [a, b] = workspace.builds[0].stages;
  ui.values = [a.id, b.id, 'own', true, '', 'stats', {
    key: JSON.stringify([a.saved.state, b.saved.state, 'own']),
    value: { stats: [], before: { unsupported_modifiers: ['Config: First unknown effect', 'Config: Second unknown effect'], item_errors: [{ slot: 'ring1', message: 'Invalid item' }] },
      after: { unsupported_modifiers: ['Config: Another unknown effect'], item_errors: [] } },
  }];
  const html = render({ workspace, dataVersion: '0.5' } as unknown as BuildSession, 'zh-CN');
  expect(html.match(/class="unincluded-modifiers"/g)).toHaveLength(2);
  expect(html.match(/<summary>词缀原文<\/summary>/g)).toHaveLength(2);
  expect(html).toContain('Config: First unknown effect\nConfig: Second unknown effect');
  expect(html).toContain('戒指 1：Invalid item');
  expect(html).not.toContain('class="modifier-raw"');
});

test('comparison controls and unavailable or percentage values keep explanations without native tooltips', () => {
  const workspace = duplicateWorkspaceStage(createWorkspace(saved()), 'Later');
  const [a, b] = workspace.builds[0].stages;
  ui.values = [a.id, b.id, 'own', true, '', 'stats', {
    key: JSON.stringify([a.saved.state, b.saved.state, 'own']),
    value: { stats: [
      { id: 'Life', before: null, after: 100, delta: null, percent: null, changed: true },
      { id: 'FireResist', before: 20, after: 30, delta: 10, percent: null, changed: true },
    ], before: { unsupported_modifiers: [], item_errors: [] }, after: { unsupported_modifiers: [], item_errors: [] } },
  }];
  const html = render({ workspace, dataVersion: '0.5' } as unknown as BuildSession);
  expect(html).not.toMatch(/\stitle=/);
  expect(html).toContain('aria-label="Swap sides"');
  expect(html).toContain('aria-label="Unavailable or unbounded"');
  expect(html).toContain('aria-label="Difference (percentage points): +10 pp"');
});
