import { isValidElement, type KeyboardEvent, type ReactElement, type ReactNode } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { beforeEach, expect, it, vi } from 'vitest';
import type { BuildSession } from '../hooks/useBuildSession';
import { CommandPalette, type PaletteProps } from './CommandPalette';

const hooks = await vi.hoisted(async () => {
  const { createHookHarness } = await import('../testing/hookHarness');
  return createHookHarness({ refs: true, memo: 'deps', effects: 'ignore' });
});
vi.mock('react', async original => ({
  ...hooks.mockReact(await original<typeof import('react')>()),
  useId: () => 'palette-test',
}));
vi.mock('../hooks/useSkillName', () => ({ useSkillCatalog: () => new Map() }));
vi.mock('../components/ui/dialog', () => {
  const Wrap = ({ children }: { children?: ReactNode }) => <div>{children}</div>;
  return { Dialog: Wrap, DialogContent: Wrap, DialogDescription: Wrap, DialogTitle: Wrap };
});
beforeEach(() => hooks.reset());

type AnyElement = ReactElement<Record<string, unknown>>;
function walk(node: ReactNode, visit: (el: AnyElement) => void) {
  if (Array.isArray(node)) { node.forEach(item => walk(item, visit)); return; }
  if (!isValidElement(node)) return;
  const el = node as AnyElement;
  visit(el);
  if (typeof el.type === 'function') walk((el.type as (p: unknown) => ReactNode)(el.props), visit);
  else walk(el.props.children as ReactNode, visit);
}

function render(overrides: Partial<PaletteProps> = {}) {
  hooks.beginRender();
  const props: PaletteProps = {
    open: true,
    onOpenChange: vi.fn(),
    session: { socketGroups: [], updateParams: vi.fn(), canUndo: false, canRedo: false } as unknown as BuildSession,
    view: { calc: null, previous: null, stale: false },
    lang: 'en-US',
    pageLabel: route => route,
    onNavigate: vi.fn(),
    onInspect: vi.fn(),
    onOpenImport: vi.fn(),
    onSettings: vi.fn(),
    onToggleTheme: vi.fn(),
    themeLabel: 'Theme',
    ...overrides,
  };
  const tree = CommandPalette(props);
  let rootKeyDown: ((event: KeyboardEvent) => void) | undefined;
  let inputChange: ((event: { target: { value: string } }) => void) | undefined;
  let inputProps: Record<string, unknown> = {};
  const options = new Map<string, Record<string, unknown>>();
  walk(tree, el => {
    if (el.props.className === 'palette-root') rootKeyDown = el.props.onKeyDown as typeof rootKeyDown;
    if (el.props.className === 'palette-input') {
      inputProps = el.props;
      inputChange = el.props.onChange as typeof inputChange;
    }
    if (typeof el.props['data-command'] === 'string') options.set(el.props['data-command'], el.props);
  });
  hooks.beginRender();
  const html = renderToStaticMarkup(<CommandPalette {...props} />);
  return { html, props, rootKeyDown: rootKeyDown!, inputChange: inputChange!, inputProps, options };
}

function keyEvent(key: string, isComposing = false) {
  return { key, preventDefault: vi.fn(), nativeEvent: { isComposing } } as unknown as KeyboardEvent;
}

it('shows one calculated-skill command per skill, without duplicating identical groups', () => {
  const session = { socketGroups: ['Spark', 'Spark', 'Fireball'].map(skill_id => ({ enabled: true, gems: [{ skill_id }] })) } as BuildSession;
  const { html } = render({ session });
  expect(html.match(/data-command="skill:/g)).toHaveLength(2);
  expect(html).toContain('1. Spark'); expect(html).not.toContain('2. Spark'); expect(html).toContain('3. Fireball');
});

it('prefers an enabled duplicate and selects its original group index', () => {
  const updateParams = vi.fn();
  const onOpenChange = vi.fn();
  const session = { socketGroups: [false, true].map(enabled => ({ enabled, gems: [{ skill_id: 'Spark' }] })), updateParams } as unknown as BuildSession;
  const { html, options } = render({ session, onOpenChange });
  expect(html.match(/data-command="skill:/g)).toHaveLength(1);
  expect(html).toContain('2. Spark'); expect(html).not.toContain('1. Spark');
  (options.get('skill:1')!.onClick as () => void)();
  expect(onOpenChange).toHaveBeenCalledWith(false);
  expect(updateParams).toHaveBeenCalledWith({ main_socket_group: 1 });
});

it('supports accessible keyboard navigation (ArrowDown, ArrowUp, Home, End, Enter, Escape)', () => {
  const onNavigate = vi.fn();
  const onSettings = vi.fn();
  const onOpenChange = vi.fn();
  const overrides = { onNavigate, onSettings, onOpenChange };

  let view = render(overrides);
  expect(view.inputProps.role).toBe('combobox');
  expect(view.inputProps['aria-activedescendant']).toBe('palette-test-option-0');
  expect(view.options.get('page:overview')?.['aria-selected']).toBe(true);

  // ArrowDown moves to second option (skills)
  view.rootKeyDown(keyEvent('ArrowDown'));
  view = render(overrides);
  expect(view.inputProps['aria-activedescendant']).toBe('palette-test-option-1');
  expect(view.options.get('page:skills')?.['aria-selected']).toBe(true);

  // End jumps to the last option (action:settings)
  view.rootKeyDown(keyEvent('End'));
  view = render(overrides);
  expect(view.options.get('action:settings')?.['aria-selected']).toBe(true);

  // ArrowDown wraps back to 0
  view.rootKeyDown(keyEvent('ArrowDown'));
  view = render(overrides);
  expect(view.inputProps['aria-activedescendant']).toBe('palette-test-option-0');

  // ArrowUp wraps to the last option
  view.rootKeyDown(keyEvent('ArrowUp'));
  view = render(overrides);
  expect(view.options.get('action:settings')?.['aria-selected']).toBe(true);

  // IME composing Enter does not run the command
  view.rootKeyDown(keyEvent('Enter', true));
  expect(onSettings).not.toHaveBeenCalled();

  // Real Enter runs the active command and closes the palette
  view.rootKeyDown(keyEvent('Enter'));
  expect(onSettings).toHaveBeenCalledOnce();
  expect(onOpenChange).toHaveBeenCalledWith(false);

  // Home returns to index 0, Escape closes
  view.rootKeyDown(keyEvent('Home'));
  view = render(overrides);
  expect(view.inputProps['aria-activedescendant']).toBe('palette-test-option-0');
  view.rootKeyDown(keyEvent('Escape'));
  expect(onOpenChange).toHaveBeenCalledTimes(2);
});
