import type { ComponentProps, ReactNode } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { afterEach, expect, test, vi } from 'vitest';
import type { EnemyPresets } from '../../api/enemyPresets';
import type { CalculateBuildResponse, ConfigOption } from '../../api/types';
import type { BuildSession } from '../../hooks/useBuildSession';
import { ConfigPanel } from './ConfigPanel';

// Replay the panel's state and actual control handlers in the SSR test environment.
// Child rows can render their own draft state, but only panel state is persisted.
const harness = vi.hoisted(() => ({ presets: null as EnemyPresets | null, index: 0, values: [] as unknown[],
  category: undefined as ComponentProps<'select'> | undefined,
  search: undefined as ComponentProps<'input'> | undefined,
  inputs: new Map<string, ComponentProps<'input'>>(),
  changeScope: (_scope: string) => {},
  restore: undefined as ComponentProps<'button'>['onClick'],
}));
vi.mock('react', async original => ({
  ...await original<typeof import('react')>(),
  useState: (initial: unknown) => {
    const index = harness.index++;
    if (index >= 14) return [initial, () => {}];
    if (!(index in harness.values)) harness.values[index] = initial;
    return [harness.values[index], (value: unknown) => {
      harness.values[index] = typeof value === 'function' ? value(harness.values[index]) : value;
    }];
  },
}));
vi.mock('../../hooks/useEnemyPresets', () => ({ useEnemyPresets: () => harness.presets }));
vi.mock('../../hooks/useSkillName', () => ({ useSkillName: () => (id: string) => id }));
vi.mock('../shared/NamedSets', () => ({ NamedSets: () => null }));
vi.mock('./PartyEditor', () => ({ PartyEditor: () => null }));
vi.mock('../../app/pages/config/ScenarioBar', () => ({ ScenarioBar: () => null }));
vi.mock('../ui/tabs', () => {
  const wrapper = ({ children }: { children?: ReactNode }) => <div>{children}</div>;
  return { Tabs: wrapper, TabsList: wrapper, TabsTrigger: wrapper, TabsContent: wrapper };
});
vi.mock('./ConfigScope', async original => ({
  ...await original<typeof import('./ConfigScope')>(),
  ConfigScopeControl: ({ scope, onChange }: { scope: string; onChange: (scope: string) => void }) => {
    harness.changeScope = onChange;
    return <div data-scope={scope} />;
  },
}));
vi.mock('../shared/controls', () => ({
  AppButton: (props: ComponentProps<'button'>) => {
    if (props.className === 'config-reset') harness.restore = props.onClick;
    return <button {...props} />;
  },
  AppInput: (props: ComponentProps<'input'>) => {
    if (props.type === 'search') harness.search = props;
    if (props.id) harness.inputs.set(props.id, props);
    return <input {...props} />;
  },
  AppSelectControl: (props: ComponentProps<'select'>) => {
    if (props['aria-label'] === 'Configuration category') harness.category = props;
    return <select {...props} />;
  },
  AppTextarea: (props: ComponentProps<'textarea'>) => <textarea {...props} />,
}));

afterEach(() => {
  harness.inputs.clear();
  harness.restore = undefined;
  harness.presets = null;
});

test('quick checks use current values and sources, and change damage checks to the calculated minion', () => {
  harness.presets = { max_enemy_level: 85, tiers: [{ id: 'Pinnacle', min_level: 82, elemental_resist_bonus: 50, chaos_resist_bonus: 0,
    armour_mult_pct: { base: 100, num: 0, den: 1 }, evasion_mult_pct: { base: 100, num: 0, den: 1 }, enemy_mods: [] }] };
  const options: ConfigOption[] = [
    { var: 'conditionEnemyShocked', label: 'Enemy shocked', input_type: 'check' },
    { var: 'enemyFireResist', label: 'Enemy fire resistance', input_type: 'count', default: { placeholder_number: 50 } },
    { var: 'enemyPhysicalReduction', label: 'Enemy physical reduction', input_type: 'count' },
  ];
  harness.values = ['options', null, options, 'ready', 0, '', 'all', null,
    new Set(['General']), {}, {}];
  const calc = { main_skill: { skill_id: 'Fireball', hit_damage: [{ damage_type: 'Fire', avg: 100 }] } } as CalculateBuildResponse;
  const session = { calcParams: { config_inputs: {}, enemy_tier: 'pinnacle' }, calc, busy: false,
    setConfigInput: vi.fn(), stateVersion: 1 } as unknown as BuildSession;
  const render = () => {
    harness.index = 0;
    return renderToStaticMarkup(<ConfigPanel session={session} lang="en-US" />);
  };
  const quick = (html: string) => html.match(/<ul class="config-quick-list">[\s\S]*?<\/ul>/)![0];
  expect(quick(render())).toContain('Inherited from Pinnacle Boss');
  expect(quick(render())).toContain('>50%</output>');
  expect(harness.inputs.get('config-enemyFireResist')!.placeholder).toBe('50 (Inherited from Pinnacle Boss)');
  expect(quick(render())).toContain('>Off</output>');
  expect(quick(render())).not.toMatch(/false|Boolean/);
  session.calcParams.config_inputs = { enemyFireResist: 0 };
  expect(quick(render())).toContain('Your setting');
  expect(quick(render())).toContain('>0%</output>');
  session.build = { config_inputs: { enemyFireResist: 0 } } as unknown as BuildSession['build'];
  expect(quick(render())).toContain('Imported setting');
  session.calcParams.enemy_tier = 'none';
  session.calcParams.config_inputs = {};
  expect(quick(render())).toContain('Inherited from Mapping');
  session.calcParams.config_inputs = { enemyFireResist: 50 };
  session.build = undefined as unknown as BuildSession['build'];
  harness.restore = undefined;
  render();
  // A catalog placeholder matching the explicit value must not remove Restore.
  expect(harness.restore).toBeTypeOf('function');
  session.calcParams.config_inputs = {};
  calc.main_minion_index = 7;
  calc.minions = [{ index: 7, minion_id: 'wolf', name: 'Wolf', summoned_by: 'WolfPackPlayer', skill_id: 'Bite', level: 85,
    dps: 10, life: 100, armour: 10, evasion: 10, energy_shield: 0, stats: { TotalDPS: 10 }, breakdowns: {} }];
  const minionChecks = quick(render());
  expect(minionChecks).toContain('data-config-var="enemyPhysicalReduction"');
  expect(minionChecks).toContain('data-config-var="enemyFireResist"');
  expect(minionChecks).not.toContain('Calculated skill hit damage');
  expect(render()).toContain('The result does not report the main minion’s hit damage types.');
  expect(session.setConfigInput).not.toHaveBeenCalled();
});

test('mobile category selection narrows Modified results and exits search without changing scope', () => {
  const options: ConfigOption[] = [
    { var: 'generalCount', label: 'General count', section: 'General', input_type: 'count', default: 0 },
    { var: 'enemyCount', label: 'Enemy count', section: 'Enemy Stats', input_type: 'count', default: 0 },
    { var: 'unchangedCount', label: 'Unchanged count', section: 'Enemy Stats', input_type: 'count', default: 0 },
  ];
  harness.values = ['options', null, options, 'ready', 0, '', 'modified', null,
    new Set(['General', 'Enemy Stats']), {}, {}];
  const session = { calcParams: { config_inputs: { generalCount: 1, enemyCount: 2, unchangedCount: 0 } },
    calc: null, busy: false, stateVersion: 1 } as unknown as BuildSession;
  const render = () => {
    harness.index = 0;
    return renderToStaticMarkup(<ConfigPanel session={session} lang="en-US" />);
  };
  const visibleSections = (html: string) => (html.match(/<section[^>]*class="config-section"[^>]*>/g) ?? [])
    .filter(section => !section.includes('hidden=""'));

  const initial = render();
  expect(harness.category!.value).toBe('General');
  expect(initial).not.toContain('Search results');
  expect(initial).not.toContain('data-config-var="unchangedCount"');
  expect(visibleSections(initial)).toHaveLength(1);

  harness.category!.onChange!({ target: { value: 'Enemy Stats' } } as never);
  const category = render();
  expect(harness.category!.value).toBe('Enemy Stats');
  expect(category).toContain('data-scope="modified"');
  expect(visibleSections(category)).toEqual([expect.stringContaining('id="config-section-Enemy-Stats"')]);

  harness.search!.onChange!({ target: { value: 'count' } } as never);
  const searching = render();
  expect(harness.category!.value).toBe('search');
  expect(visibleSections(searching)).toHaveLength(2);

  harness.category!.onChange!({ target: { value: 'General' } } as never);
  const selected = render();
  expect(harness.search!.value).toBe('');
  expect(harness.category!.value).toBe('General');
  expect(selected).not.toContain('Search results');
  expect(selected).toContain('data-scope="modified"');
  expect(selected).not.toContain('data-config-var="unchangedCount"');
  expect(visibleSections(selected)).toEqual([expect.stringContaining('id="config-section-General"')]);
});

test.each([true, false])('Modified retains rapid edits to an option initially set to %s until scope or search changes', initiallyChecked => {
  const variable = 'conditionEnemyShocked';
  const id = `config-${variable}`;
  const options: ConfigOption[] = [{ var: variable, label: 'Shock', section: 'For Effective DPS', input_type: 'check', default: false }];
  harness.values = ['options', null, options, 'ready', 0, variable, 'all', null,
    new Set(['For Effective DPS']), {}, {}];
  const setConfigInput = vi.fn((key: string, value: boolean) => {
    session.calcParams.config_inputs = { ...session.calcParams.config_inputs, [key]: value };
  });
  const session = { calcParams: { config_inputs: { [variable]: initiallyChecked } }, setConfigInput,
    build: { config_inputs: { [variable]: true } },
    calc: null, busy: false, stateVersion: 1 } as unknown as BuildSession;
  const render = () => {
    harness.index = 0;
    harness.inputs.clear();
    return renderToStaticMarkup(<ConfigPanel session={session} lang="en-US" />);
  };
  const change = (checked: boolean) => harness.inputs.get(id)!.onChange!({ target: { checked } } as never);

  render();
  if (!initiallyChecked) {
    change(true);
    render();
  }
  harness.changeScope('modified');
  render();
  expect(harness.inputs.get(id)!.checked).toBe(true);

  change(false);
  expect(setConfigInput).toHaveBeenLastCalledWith(variable, false);
  expect(session.calcParams.config_inputs[variable]).toBe(false);
  expect(render()).toContain(`id="${id}"`);
  expect(harness.inputs.get(id)!.checked).toBe(false);
  harness.restore!({} as never);
  expect(render()).toContain(`id="${id}"`);
  expect(harness.inputs.get(id)!.checked).toBe(true);
  change(false);
  render();
  // Rapid toggles and repeated renders keep the same row and its current value.
  change(true);
  render();
  expect(harness.inputs.get(id)!.checked).toBe(true);
  change(false);
  expect(render()).toContain(`id="${id}"`);
  expect(harness.inputs.get(id)!.checked).toBe(false);
  expect(render()).toContain(`id="${id}"`);

  // Leaving and returning to Modified recomputes membership from current values.
  harness.changeScope('all');
  render();
  expect(harness.inputs.get(id)!.checked).toBe(false);
  harness.changeScope('modified');
  expect(render()).not.toContain(`id="${id}"`);

  // A change in another scope is immediately available on entering Modified.
  harness.changeScope('all');
  render();
  change(true);
  render();
  harness.changeScope('modified');
  render();
  expect(harness.inputs.get(id)!.checked).toBe(true);
  change(false);
  expect(render()).toContain(`id="${id}"`);

  // A different search text also recomputes membership, even if it still matches.
  harness.search!.onChange!({ target: { value: 'Shocked' } } as never);
  expect(render()).not.toContain(`id="${id}"`);
  harness.search!.onChange!({ target: { value: variable } } as never);
  expect(render()).not.toContain(`id="${id}"`);
});
