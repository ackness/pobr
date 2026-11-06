import type { ComponentProps } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { afterEach, beforeEach, expect, test, vi } from 'vitest';
import type { BuildSession } from '../../hooks/useBuildSession';
import type { CalculateBuildRequest } from '../../api/types';
import type { ReplacementReport } from '../../lib/itemReplacement';
import { ItemReplacementCheck } from './ItemReplacementCheck';
import type { Objective } from '../../lib/optimize';
import type { Lang } from '../../lib/i18n';
import { upgradeT } from '../../lib/upgradeText';

// Replay state and committed effects without a browser; retain real dependency comparisons.
const ui = vi.hoisted(() => ({ cursor: 0, slots: [] as unknown[], effects: [] as (() => void)[],
  buttons: [] as ComponentProps<'button'>[], input: undefined as ComponentProps<'textarea'> | undefined,
  compare: vi.fn(), equipment: vi.fn() }));
vi.mock('react', async original => {
  const memo = (factory: () => unknown, deps?: unknown[]) => {
    const index = ui.cursor++;
    const previous = ui.slots[index] as { deps?: unknown[]; value: unknown } | undefined;
    if (!previous || !deps || deps.some((value, i) => !Object.is(value, previous.deps?.[i]))) {
      ui.slots[index] = { deps, value: factory() };
    }
    return (ui.slots[index] as { value: unknown }).value;
  };
  const effect = (callback: () => void | (() => void), deps?: unknown[]) => {
    const index = ui.cursor++;
    const previous = ui.slots[index] as { deps?: unknown[]; cleanup?: () => void } | undefined;
    if (!previous || !deps || deps.some((value, i) => !Object.is(value, previous.deps?.[i]))) {
      const entry = { deps, cleanup: undefined as (() => void) | undefined };
      ui.slots[index] = entry;
      ui.effects.push(() => { previous?.cleanup?.(); entry.cleanup = callback() || undefined; });
    }
  };
  return { ...await original<typeof import('react')>(),
    useState: (initial: unknown) => {
      const index = ui.cursor++;
      if (!(index in ui.slots)) ui.slots[index] = typeof initial === 'function' ? initial() : initial;
      return [ui.slots[index], (value: unknown) => { ui.slots[index] = typeof value === 'function' ? value(ui.slots[index]) : value; }];
    },
    useRef: (initial: unknown) => {
      const index = ui.cursor++;
      if (!(index in ui.slots)) ui.slots[index] = { current: initial };
      return ui.slots[index];
    },
    useMemo: memo, useCallback: (callback: unknown, deps?: unknown[]) => memo(() => callback, deps),
    useEffect: effect, useLayoutEffect: effect,
  };
});
vi.mock('../../app/jobs/useJobBeacon', () => ({ useJobBeacon: () => {} }));
vi.mock('../shared/PreviewContext', () => ({ usePreview: () => null }));
vi.mock('../../hooks/useLocalizedLines', () => ({ useLocalizedLines: (lines: string[]) => lines, useLocalizedNames: (names: { name: string }[]) => names.map(name => name.name) }));
vi.mock('../../lib/itemReplacement', async original => ({ ...await original<typeof import('../../lib/itemReplacement')>(),
  compareReplacementPositions: (...args: unknown[]) => ui.compare(...args),
}));
vi.mock('../shared/controls', () => ({
  AppButton: (props: ComponentProps<'button'>) => { ui.buttons.push(props); return <button {...props} />; },
  AppTextarea: (props: ComponentProps<'textarea'>) => { ui.input = props; return <textarea {...props} />; },
}));
vi.mock('./ReplacementAffixEditor', () => ({ ReplacementAffixEditor: () => null }));
vi.mock('./ReplacementAugmentPicker', () => ({ ReplacementAugmentPicker: () => null }));
vi.mock('./EquipmentPlanner', () => ({ EquipmentPlanner: (props: unknown) => { ui.equipment(props); return null; } }));

const text = 'Rarity: RARE\nLocal Defence Fixture\nChain Mail\n+80 to maximum Life';
const catalog = { bases: [], mods: [] };
const objective = { stat: 'Life', constraints: [] };
const report = { text, base: { name: 'Chain Mail' }, requiredLevel: 1, lines: [], baseline: { Life: 100 }, rejected: [],
  positions: [{ slot: 'bodyarmour', stats: { Life: 180 }, unsupported: [], lines: [],
    variant: { set_items: [{ slot: 'bodyarmour', text }] }, augments: { runes: [], info: { options: [] } } }],
} as unknown as ReplacementReport;

beforeEach(() => {
  ui.cursor = 0; ui.slots = []; ui.effects = []; vi.clearAllMocks();
  vi.stubGlobal('window', { clearTimeout });
});
afterEach(() => { vi.unstubAllGlobals(); });

function setup(allocatedNodes: number[] = [], settings: { calc?: BuildSession['calc']; objective?: Objective; report?: ReplacementReport } = {}, lang: Lang = 'en-US') {
  const request: CalculateBuildRequest = { allocated_nodes: allocatedNodes };
  const session = { currentRequest: () => request, allocatedNodes, activeWeaponSet: 1, items: [], flasks: [], jewels: [],
    setItems: vi.fn(), busy: false, calc: settings.calc } as unknown as BuildSession;
  let resolve!: (report: ReplacementReport) => void;
  ui.compare.mockImplementation(() => new Promise<ReplacementReport>(done => { resolve = done; }));
  const render = (sockets: number[], socketLabel = () => 'Jewel socket') => {
    ui.cursor = 0; ui.buttons = [];
    const html = renderToStaticMarkup(<ItemReplacementCheck session={session} lang={lang} active objective={settings.objective ?? objective}
      catalog={catalog} jewelSockets={sockets} socketLabel={socketLabel} goalLabel="Life" onEditGoal={() => {}} />);
    ui.effects.splice(0).forEach(effect => effect());
    return html;
  };
  render([]);
  ui.input!.onChange?.({ target: { value: text } } as never);
  render([]);
  ui.buttons.find(button => button.children === upgradeT(lang, 'compare'))!.onClick?.({} as never);
  render([]);
  return { render, resolve: (value = settings.report ?? report) => resolve(value), signal: ui.compare.mock.calls[0][2] as AbortSignal };
}

test('late tree socket metadata and display-name rerenders preserve an in-flight equipment comparison and its visible result', async () => {
  const comparison = setup();
  comparison.render([7960]);
  expect(comparison.signal.aborted).toBe(false);
  comparison.resolve();
  await Promise.resolve();
  const html = comparison.render([7960]);
  expect(html).toContain('class="replacement-content" aria-busy="false"');
  expect(html).toContain('Apply to build');
  expect(comparison.render([7960, 9000], () => 'Jewel socket · Near Power')).toContain('class="replacement-content" aria-busy="false"');
  expect(ui.compare).toHaveBeenCalledTimes(1);
});

test('discovering an allocated jewel destination still invalidates the previous comparison', () => {
  const comparison = setup([7960]);
  comparison.render([7960]);
  expect(comparison.signal.aborted).toBe(true);
});

test('recommends and shows a minion-only gain in the destination and shared before/after card', async () => {
  const comparison = setup([], {
    calc: { main_minion_index: 2, minions: [{ index: 2, name: 'Wolf' }], stats: [{ id: 'ActiveMinionLimit', value: 7 }] } as BuildSession['calc'],
    objective: { stat: 'MainMinionDPS', constraints: [] },
    report: { ...report, baseline: { TotalDPS: 0, CombinedDPS: 0, MainMinionDPS: 100 },
      positions: [{ ...report.positions[0], stats: { TotalDPS: 0, CombinedDPS: 0, MainMinionDPS: 150 } }] },
  });
  comparison.resolve(); await Promise.resolve();
  const html = comparison.render([]);
  expect(html).toContain('Minion hit DPS +50');
  expect(html).toContain('data-stat="MainMinionDPS"');
  expect(html).toContain('+50.0%');
  expect(html).toContain('Wolf · per minion · limit 7');
  expect(html).toContain('replacement-recommended');
  expect(html).not.toContain('data-stat="CombinedDPS"');
  expect(html).not.toContain('DPS decreases');
  expect(ui.equipment).toHaveBeenLastCalledWith(expect.objectContaining({ damage: {
    stat: 'MainMinionDPS', label: 'Minion hit DPS', note: 'Wolf · per minion · limit 7',
  } }));
});

test.each([
  ['en-US', 'Life', 'Effective HP', 'Combined DPS', 'Hit DPS', 'Fire Resistance', ' pp'],
  ['zh-CN', '生命', '有效生命', '综合 DPS', '击中 DPS', '火焰抗性', ' 个百分点'],
  ['zh-TW', '生命', '有效生命', '綜合 DPS', '擊中 DPS', '火焰抗性', ' 個百分點'],
] as const)('replacement changes preserve metric precision and resistance units in %s', async (lang, life, ehp, combined, hit, resistance, unit) => {
  const comparison = setup([], {}, lang);
  comparison.resolve({ ...report, baseline: { Life: 100, TotalEHP: 1999.2, CombinedDPS: 12345.2, TotalDPS: 100, FireResist: 70 },
    positions: report.positions.map(position => ({ ...position, stats: { Life: 101.83, TotalEHP: 2001.03, CombinedDPS: 12347.03, TotalDPS: 101.83, FireResist: 72.36 } })),
  });
  await Promise.resolve();
  const html = comparison.render([]);
  expect(html).toContain('class="swap-gain-card"');
  const metric = (id: string) => html.match(new RegExp(`data-stat="${id}"[\\s\\S]*?</div>`))![0];
  expect(metric('Life')).toContain(`<span>${life}</span><strong class="delta-pos">+1.8%</strong>`);
  expect(metric('Life')).toContain('<span><span>100</span> → <b class="swap-after">102</b></span>');
  expect(metric('TotalEHP')).toContain(`<span>${ehp}</span>`);
  expect(metric('TotalEHP')).toContain('<span><span>1,999</span> → <b class="swap-after">2,001</b></span>');
  expect(metric('CombinedDPS')).toContain(`<span>${combined}</span>`);
  expect(metric('CombinedDPS')).toContain('<span><span>12,345</span> → <b class="swap-after">12,347</b></span>');
  expect(html).toContain(`<tr><th>${hit}</th><td>100</td><td>101.83</td><td>+1.83</td></tr>`);
  expect(html).toContain(`<tr><th>${resistance}</th><td>70%</td><td>72.4%</td><td>+2.4${unit}</td></tr>`);
});
