import type { ComponentProps } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { afterEach, expect, test, vi } from 'vitest';
import Inspector from '../../app/inspector/Inspector';
import { NavigateContext } from '../../app/pages/navigate';
import { CONFIG_FIELD_EVENT, takeConfigField } from '../../lib/configNavigation';
import type { BuildSession } from '../../hooks/useBuildSession';
import type { SettledCalc } from '../../app/motion/useSettledCalc';
import { TooltipProvider } from '../ui/tooltip';
import type { ConfigFieldLink } from '../config/ConfigFieldLink';

const harness = vi.hoisted(() => ({ click: undefined as ComponentProps<'a'>['onClick'] }));
vi.mock('../../app/inspector/useStatBreakdown', () => ({ useStatBreakdown: () => ({ loading: false, retry: () => {}, detail: {
  base_total: 0, inc_total: 0, more_total: 1, mods: [], formula: null,
  mitigation: [{ damage_type: 'Fire', average: 100, resist: 50, reduction: 50, taken: 1, multiplier: .5, mitigated: 50,
    resist_inverted: 0, defence_mods: [], taken_mods: [] }],
} }) }));
vi.mock('../config/ConfigFieldLink', async original => {
  const actual = await original<{ ConfigFieldLink: typeof ConfigFieldLink }>();
  return { ConfigFieldLink: (props: ComponentProps<typeof ConfigFieldLink>) => {
    const link = actual.ConfigFieldLink(props);
    harness.click = link.props.onClick;
    return link;
  } };
});
afterEach(() => { vi.unstubAllGlobals(); takeConfigField(); });

test.each([false, true])('an inspector condition link dismisses its modal before navigation and destination field focus (shell callback: %s)', shellCallback => {
  const events: string[] = [];
  let sheetOpen = true;
  vi.stubGlobal('window', new EventTarget());
  const focus = vi.fn(() => { expect(sheetOpen).toBe(false); events.push('focus'); });
  window.addEventListener(CONFIG_FIELD_EVENT, () => {
    expect(takeConfigField()).toBe('enemyFireResist');
    focus();
  });
  const calc = { stats: [{ id: 'TotalDPS', value: 50 }], breakdowns: {}, breakdown_descriptors: { TotalDPS: 'steps' } };
  const close = vi.fn(() => { sheetOpen = false; events.push('close'); });
  const navigate = vi.fn(() => { expect(sheetOpen).toBe(false); events.push('navigate'); });
  renderToStaticMarkup(<NavigateContext.Provider value={navigate}><TooltipProvider>
    <Inspector session={{ calc } as unknown as BuildSession} view={{ calc } as unknown as SettledCalc} lang="en-US" id="TotalDPS"
      onClose={close} onNavigate={shellCallback ? close : undefined} onOpenCalcs={() => {}} pinned={false} onTogglePin={() => {}} />
  </TooltipProvider></NavigateContext.Provider>);
  harness.click!({ preventDefault: vi.fn() } as never);
  expect(close).toHaveBeenCalledOnce();
  expect(navigate).toHaveBeenCalledWith('config');
  expect(focus).toHaveBeenCalledOnce();
  expect(events).toEqual(['close', 'navigate', 'focus']);
});
