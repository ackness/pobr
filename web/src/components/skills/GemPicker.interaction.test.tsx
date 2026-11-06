import type { ComponentProps } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { beforeEach, expect, it, vi } from 'vitest';
import type { GemCatalogEntry } from '../../api/types';
import { buildGemDescriptionIndex, type GemDescriptionIndex } from '../../lib/gemDescriptions';
import { GemPicker } from './GemPicker';

const ui = vi.hoisted(() => ({ states: [] as unknown[], cursor: 0,
  descriptions: undefined as GemDescriptionIndex | null | undefined,
  input: {} as ComponentProps<'input'>, buttons: [] as ComponentProps<'button'>[],
  options: [] as ComponentProps<'li'>[], panel: {} as ComponentProps<'div'>,
}));
vi.mock('react', async original => ({ ...await original<typeof import('react')>(),
  useState: (initial: unknown) => {
    const index = ui.cursor++;
    if (!(index in ui.states)) ui.states[index] = typeof initial === 'function' ? initial() : initial;
    return [ui.states[index], (next: unknown) => { ui.states[index] = typeof next === 'function' ? next(ui.states[index]) : next; }];
  },
}));
vi.mock('../../hooks/useGemDescriptions', () => ({ useGemDescriptions: () => ui.descriptions }));
vi.mock('../shared/controls', () => ({
  AppButton: (props: ComponentProps<'button'>) => { ui.buttons.push(props); return <button {...props} />; },
  AppInput: (props: ComponentProps<'input'>) => { ui.input = props; return <input {...props} />; },
}));
vi.mock('../ui/popover', () => ({
  Popover: ({ open, children }: { open: boolean; children: React.ReactNode }) => open ? children : null,
  PopoverAnchor: () => null,
  PopoverContent: (props: ComponentProps<'div'>) => { ui.panel = props; return <div {...props} />; },
}));
vi.mock('react/jsx-dev-runtime', async original => {
  const runtime = await original<typeof import('react/jsx-dev-runtime')>();
  return { ...runtime, jsxDEV: (...args: Parameters<typeof runtime.jsxDEV>) => {
    if (args[0] === 'li') ui.options.push(args[1] as ComponentProps<'li'>);
    return runtime.jsxDEV(...args);
  } };
});

const gems: GemCatalogEntry[] = [
  { skill_id: 'Area', name: 'Area Support', name_zh_cn: '范围辅助', name_zh_tw: '範圍輔助',
    is_support: true, is_lineage: false, active_effect_ids: [], colour: 'int', tags: ['Area'] },
  { skill_id: 'Bolt', name: 'Bolt', name_zh_cn: '闪电箭', name_zh_tw: '閃電箭',
    is_support: false, is_lineage: false, active_effect_ids: ['Bolt'], colour: 'dex', tags: ['Projectile'] },
];
const data = buildGemDescriptionIndex({ scopes: {
  'GemEffects.SupportText': { AreaVariant: 'Supports [Area] Skills.\r\nSecond line.' },
  'ActiveSkills.Description': { bolt: 'Fires a [Projectile|bolt].' },
} }, [{ gem_id: 'gem', granted_effect_id: 'Area', variant_id: 'AreaVariant' }], { Bolt: 'bolt' }, [],
[{ id: 'gem', granted_effect_id: 'ignored', str_pct: 50, int_pct: 50 }]);
const pick = vi.fn();
function render(lang: 'en-US' | 'zh-CN' | 'zh-TW' = 'en-US') {
  ui.cursor = 0; ui.buttons = []; ui.options = [];
  return renderToStaticMarkup(<GemPicker entries={gems} placeholder="Pick a gem" disabled={false} lang={lang} onPick={pick} />);
}
function open(lang: 'en-US' | 'zh-CN' | 'zh-TW' = 'en-US') {
  render(lang);
  ui.input.onFocus!({} as never);
  return render(lang);
}
function key(key: string, extra = {}) {
  const event = { key, nativeEvent: {}, preventDefault: vi.fn(), ...extra };
  ui.input.onKeyDown!(event as never);
  return event;
}
beforeEach(() => { ui.states = []; ui.descriptions = data; pick.mockClear(); });

it('shows the highlighted gem, follows arrows/Home/End and pointer hover, and keeps aria-activedescendant', () => {
  let html = open();
  expect(html).toContain('Supports Area Skills.\nSecond line.');
  expect(html).toContain('Support · Area · Requires: Strength / Intelligence');
  expect(ui.input['aria-activedescendant']).toMatch(/-0$/);
  expect(key('ArrowDown').preventDefault).toHaveBeenCalledOnce();
  html = render();
  expect(html).toContain('Fires a bolt.');
  expect(html).not.toContain('Supports Area Skills.');
  expect(ui.input['aria-activedescendant']).toMatch(/-1$/);
  key('Home'); expect(render()).toContain('Supports Area Skills.');
  key('End'); expect(render()).toContain('Fires a bolt.');
  key('ArrowUp'); expect(render()).toContain('Supports Area Skills.');
  ui.options[1].onPointerEnter!({} as never);
  expect(render()).toContain('Fires a bolt.');
  key('Escape');
  expect(render()).not.toContain('gem-description-panel');
});

it('Add, Enter and the list row use the same pick path and close the picker', () => {
  for (const path of ['add', 'enter', 'row']) {
    ui.states = []; pick.mockClear();
    open(); key('ArrowDown'); render();
    if (path === 'add') ui.buttons.find(button => button.children === 'Add')!.onClick!({} as never);
    else if (path === 'enter') key('Enter');
    else ui.options[1].onClick!({} as never);
    expect(pick).toHaveBeenCalledExactlyOnceWith('Bolt');
    expect(render()).not.toContain('gem-description-panel');
    expect(ui.input.value).toBe('');
  }
});

it('marks English fallback as 原文 with lang=en in zh-CN and keeps bilingual gem names', () => {
  const html = open('zh-CN');
  expect(html).toContain('范围辅助 / <span lang="en">Area Support</span>');
  expect(html).toContain('>原文</small>');
  expect(html).toContain('<p class="gem-description-text" lang="en">Supports Area Skills.');
  expect(ui.buttons.some(button => button.children === '添加')).toBe(true);
});

it.each([null, undefined])('leaves the original picker and filter buttons when descriptions are %s', descriptions => {
  ui.descriptions = descriptions;
  const html = open();
  expect(html).not.toContain('gem-description-panel');
  expect(html).not.toContain('gem-picker-body');
  expect(html).not.toContain('has-descriptions');
  expect(ui.buttons).toHaveLength(4);
  key('ArrowDown'); render(); key('Enter');
  expect(pick).toHaveBeenCalledExactlyOnceWith('Bolt');
});

it('includes Add in the existing Tab loop, with the last button returning focus through the field', () => {
  open();
  const focus = vi.fn();
  const buttons = ui.buttons.map(() => ({ focus: vi.fn() }));
  (ui.input.ref as { current: unknown }).current = { focus };
  (ui.panel.ref as { current: unknown }).current = { querySelector: () => buttons[0] };
  key('Tab');
  expect(buttons[0].focus).toHaveBeenCalledOnce();
  expect(ui.buttons.at(-1)?.children).toBe('Add');
  const tab = (target: unknown, shiftKey = false) => {
    const event = { key: 'Tab', shiftKey, target, currentTarget: { querySelectorAll: () => buttons }, preventDefault: vi.fn() };
    ui.panel.onKeyDown!(event as never);
    return event;
  };
  tab(buttons.at(-2)); expect(focus).not.toHaveBeenCalled();
  tab(buttons.at(-1)); expect(focus).toHaveBeenCalledOnce();
  expect(tab(buttons[0], true).preventDefault).toHaveBeenCalledOnce();
  expect(focus).toHaveBeenCalledTimes(2);
});
