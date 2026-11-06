import type { ComponentProps } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { beforeEach, expect, it, vi } from 'vitest';
import type { BuildSession } from '../../hooks/useBuildSession';
import { bindT, type Lang } from '../../lib/i18n';
import type { GemCatalogEntry } from '../../api/types';
import { SkillsPanel, SkillSets } from './SkillsPanel';
import { NamedSets } from '../shared/NamedSets';

const harness = vi.hoisted(() => ({ states: [] as unknown[], cursor: 0, buttons: [] as ComponentProps<'button'>[],
  inputs: [] as ComponentProps<'input'>[], localized: {} as Record<string, string>, confirm: vi.fn(), valid: true, pickers: [] as { placeholder: string; disabled: boolean }[] }));
vi.mock('react', async original => ({ ...await original<typeof import('react')>(),
  useState: (initial: unknown) => {
    const index = harness.cursor++;
    if (!(index in harness.states)) harness.states[index] = typeof initial === 'function' ? initial() : initial;
    return [harness.states[index], (next: unknown) => { harness.states[index] = typeof next === 'function' ? next(harness.states[index]) : next; }];
  },
}));
vi.mock('../shared/controls', () => ({
  AppButton: (props: ComponentProps<'button'>) => { harness.buttons.push(props); return <button {...props} />; },
  AppInput: (props: ComponentProps<'input'>) => { harness.inputs.push(props); return <input {...props} />; },
  AppTooltip: ({ children }: ComponentProps<'span'>) => children,
  AppSelectControl: (props: ComponentProps<'select'>) => <select {...props} />,
}));
vi.mock('../shared/AppDialogs', () => ({ useAppDialogs: () => ({ confirm: harness.confirm }) }));
vi.mock('../shared/useSessionAction', () => ({ useSessionAction: () => () => () => harness.valid }));
vi.mock('../shared/AppSelect', () => ({ AppSelect: () => null }));
vi.mock('../shared/WeaponSetControl', () => ({ WeaponSetControl: () => null }));
vi.mock('../shared/NoteEditor', () => ({ NoteEditor: () => null }));
vi.mock('./GemNameCopy', () => ({ GemNameCopy: () => null }));
vi.mock('./GemOptimizer', () => ({ GemOptimizer: () => <div className="gem-optimizer">Recommendations</div> }));
vi.mock('./MinionSelection', () => ({ MinionSelection: () => null }));
vi.mock('./GemPicker', () => ({ GemPicker: (props: { placeholder: string; disabled: boolean }) => { harness.pickers.push(props); return <input placeholder={props.placeholder} disabled={props.disabled} />; } }));
vi.mock('../../app/pages/skills/useSkillMetrics', () => ({ useSkillMetrics: () => null }));
vi.mock('../../hooks/useLocalizedLines', async original => ({ ...await original<typeof import('../../hooks/useLocalizedLines')>(), useLocalizedLines: (lines: string[]) => lines.map(line => harness.localized[line] ?? line) }));

const fire: GemCatalogEntry = { skill_id: 'Fireball', name: 'Fireball', name_zh_cn: '火球', name_zh_tw: '火球',
  colour: 'int', is_support: false, is_lineage: false, active_effect_ids: ['Fireball'], tags: [] };
const support: GemCatalogEntry = { ...fire, skill_id: 'Support', name: 'Support', active_effect_ids: [], is_support: true };
const catalog = new Map([[fire.skill_id, fire], [support.skill_id, support]]);
function session() {
  return { socketGroups: [{ enabled: true, gems: [{ skill_id: 'Fireball', level: 20, quality: 20 }, { skill_id: 'Support', level: 1, quality: 0 }] }],
    character: { level: 80 }, calcParams: { main_socket_group: 0 }, socketGroupRevision: 1, stateVersion: 1,
    annotations: {}, items: [], library: { skillSets: [{ id: 'template', name: 'Caster', groups: [] }] },
    classNames: { classes: {}, ascendancies: {} }, setSocketGroups: vi.fn(), applySkillSet: vi.fn(),
  } as unknown as BuildSession;
}
function render(current: BuildSession, templates = false, lang: Lang = 'en-US') {
  harness.cursor = 0; harness.buttons = []; harness.inputs = []; harness.pickers = [];
  return renderToStaticMarkup(templates ? <SkillSets session={current} lang="en-US" catalog={catalog} /> : <SkillsPanel session={current} lang={lang} active />);
}
beforeEach(() => { harness.states = []; harness.localized = {}; harness.valid = true; harness.confirm.mockReset().mockResolvedValue(false); });

it('lists every replaced group and gems, cancelling or approving the template without bypassing stale-state guards', async () => {
  const current = session();
  render(current, true);
  const apply = () => harness.buttons.find(button => button.children === 'Apply (replace current skill groups)')!.onClick!({} as never);
  await apply();
  expect(harness.confirm).toHaveBeenCalledWith(expect.stringContaining('1. Fireball（Fireball · Support）'));
  expect(current.applySkillSet).not.toHaveBeenCalled();
  harness.confirm.mockResolvedValueOnce(true);
  await apply();
  expect(current.applySkillSet).toHaveBeenCalledWith('template');
  vi.mocked(current.applySkillSet).mockClear();
  harness.confirm.mockResolvedValueOnce(true);
  harness.valid = false;
  await apply();
  expect(current.applySkillSet).not.toHaveBeenCalled();
});

it('shows support level and quality as inapplicable and keeps count/corruption in gem options', () => {
  const current = session();
  harness.states = [[fire, support], [], false, false];
  const html = render(current);
  // The active gem has editable level/quality; the support does not.
  expect(harness.inputs.filter(input => input['aria-label'] === 'Level')).toHaveLength(1);
  expect(harness.inputs.filter(input => input['aria-label'] === 'Quality')).toHaveLength(1);
  expect(html).toContain('aria-label="Not applicable">—');
  expect(html).toContain('<details class="gem-advanced"><summary>Gem options</summary>');
  expect(html.indexOf('placeholder="Add a support gem…"')).toBeLessThan(html.indexOf('class="gem-optimizer"'));
  const count = harness.inputs.find(input => input['aria-label'] === 'Gem count')!;
  count.onChange!({ target: { value: '1.5' } } as never);
  expect(current.setSocketGroups).toHaveBeenLastCalledWith([{ ...current.socketGroups[0], gems: [{ ...current.socketGroups[0].gems[0], count: 1.5 }, current.socketGroups[0].gems[1]] }]);
  harness.inputs.find(input => input['aria-label'] === 'Corrupted')!.onChange!({ target: { checked: true } } as never);
  expect(vi.mocked(current.setSocketGroups).mock.calls.at(-1)![0][0].gems[0].corrupted).toBe(true);
});

it('labels a nonempty unrecognised group and preserves the raw gem name', () => {
  const current = session();
  current.socketGroups[0].gems = [{ skill_id: '', name_spec: 'Imported Gem', level: 1, quality: 0 }];
  harness.states = [[fire, support], [], false, false];
  const html = render(current);
  expect(html).toContain('(Unrecognised active skill)');
  expect(html).toContain('The active gem in this group is unrecognised and is not calculated.');
  expect(html).toContain('Imported Gem');
});

it('localises the default skill-set title as well as the configuration-set title', () => {
  const current = session();
  const html = renderToStaticMarkup(<NamedSets session={current} lang="zh-CN" kind="skill" />);
  expect(html).toContain('技能配置');
  expect(html).toContain('默认');
  expect(html).not.toContain('Default');
});


it('labels triggered skills and resolves unique item source names through the existing translation path', () => {
  const current = session();
  const trigger = { ...fire, skill_id: 'Trigger', name: 'Trigger', tags: ['Meta'], active_effect_ids: ['Trigger'] };
  current.socketGroups[0] = { ...current.socketGroups[0], main_active_skill: 2, source: 'Item:7:Known Unique, Sapphire Ring',
    gems: [{ skill_id: 'Trigger', level: 20, quality: 0 }, ...current.socketGroups[0].gems] };
  current.items = [{ slot: 'ring1', text: 'Rarity: UNIQUE\nKnown Unique\nSapphire Ring' }];
  harness.localized['Known Unique'] = '语言包中的暗金名';
  harness.states = [[fire, support, trigger], [], false, false];
  const html = render(current);
  expect(html).toContain('Triggered by Trigger');
  expect(html).toContain('from item · 语言包中的暗金名');
});

it('keeps supported synthetic skills named even though they have no gem-catalog entry', () => {
  const current = session();
  current.socketGroups[0].gems = [{ skill_id: 'ThornsPlayer', level: 1, quality: 0 }];
  harness.states = [[fire, support], [], false, false];
  const html = render(current);
  expect(html).toContain('Thorns');
  expect(html).not.toContain('Unrecognised active skill');
});


for (const lang of ['en-US', 'zh-CN', 'zh-TW'] as const) {
  for (const state of ['disabledActive', 'missingActive', 'unrecognisedActive'] as const) {
    it(`distinguishes ${state} without a misleading replacement hint in ${lang}`, () => {
      const current = session();
      current.socketGroups[0].gems = state === 'disabledActive'
        ? [{ skill_id: 'Fireball', level: 20, quality: 0, enabled: false }]
        : state === 'missingActive' ? [{ skill_id: 'Support', level: 1, quality: 0 }]
        : [{ skill_id: '', name_spec: 'Imported Gem', level: 1, quality: 0 }];
      harness.states = [[fire, support], [], false, false];
      const html = render(current, false, lang);
      const tt = bindT(lang);
      expect(html).toContain(tt(`skills.${state}`));
      expect(html).toContain(tt(`skills.${state}Hint`));
      if (state !== 'unrecognisedActive') {
        expect(html).not.toContain(tt('skills.unrecognisedActiveHint'));
        expect(html).not.toContain(tt('skills.unrecognisedActive'));
      }
    });
  }
}
