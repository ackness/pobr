import type { ComponentProps } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { afterEach, beforeEach, expect, test, vi } from 'vitest';
import { TradePanel } from './TradePanel';
import type { BuildSession } from '../../hooks/useBuildSession';
import type { TradeOptimization, TradeCatalog } from '../../lib/tradeOptimizer';
import type { ObjectiveState } from '../shared/OptimizerControls';
import { bindT } from '../../lib/i18n';
import type { GemPlan } from '../../lib/tradeMarket';

const ui = vi.hoisted(() => ({ buttons: [] as (ComponentProps<'button'> & { 'data-slot-id'?: string })[], inputs: [] as ComponentProps<'input'>[], scroll: vi.fn(), focus: vi.fn(), optimize: vi.fn(), replacement: vi.fn(), gems: vi.fn(),
  selects: [] as { value: string; ariaLabel: string; onChange: (value: string) => void }[],
  frames: [] as (() => void)[], goal: { preset: 'life', cStat: '', cMin: '', cMax: '' } as ObjectiveState }));
const hooks = await vi.hoisted(async () => {
  const { createHookHarness } = await import('../../testing/hookHarness');
  return createHookHarness({ refs: { initialize: (initial, index) => [0, 1, 3].includes(index)
    ? { scrollIntoView: ui.scroll, focus: ui.focus } : initial }, memo: 'deps', callback: 'deps',
    effects: { comparison: 'objectIs', cleanup: 'previous' }, layoutEffect: 'effect' });
});
vi.mock('react', async original => hooks.mockReact(await original<typeof import('react')>()));

vi.mock('../../hooks/useUpgradeGoal', () => ({ useUpgradeGoal: () => ({ goal: ui.goal, setGoal: (goal: ObjectiveState) => { ui.goal = goal; } }) }));
vi.mock('../../hooks/useSkillName', () => ({ useSkillCatalog: () => new Map() }));
vi.mock('../tree/passiveNodeNames', () => ({ usePassiveNodeNames: () => new Map() }));
vi.mock('../../hooks/useLocalizedLines', () => ({
  useItemDisplayNames: (texts: (string | undefined)[]) => texts.map(text => text?.split('\n')[1] ?? ''),
  useLocalizedLines: (lines: string[]) => lines,
  useLocalizedNames: (names: readonly { name: string }[]) => names.map(entry => entry.name),
}));
vi.mock('../../app/jobs/useJobBeacon', () => ({ useJobBeacon: () => {} }));
vi.mock('../shared/PreviewContext', () => ({ usePreview: () => null }));
vi.mock('./ItemReplacementCheck', () => ({ ItemReplacementCheck: (props: unknown) => { ui.replacement(props); return null; } }));
vi.mock('../shared/AppSelect', () => ({ AppSelect: (props: { value: string; ariaLabel: string; onChange: (value: string) => void }) => { ui.selects.push(props); return null; } }));
vi.mock('../../api/backend', () => ({ getBackend: async () => ({ loadPassiveTree: async () => [] }) }));
vi.mock('../../lib/trade', async original => ({ ...await original<typeof import('../../lib/trade')>(), loadTradeLeagues: async () => ['Standard'] }));
vi.mock('../../lib/tradeMarket', () => ({ planGemUpgrades: (...args: unknown[]) => ui.gems(...args) }));
vi.mock('../../lib/tradeOptimizer', async original => ({ ...await original<typeof import('../../lib/tradeOptimizer')>(),
  loadTradeCatalog: async () => catalog, optimizeTradeAffixes: (...args: unknown[]) => ui.optimize(...args),
}));
vi.mock('../shared/controls', () => ({
  AppButton: (props: ComponentProps<'button'>) => { ui.buttons.push(props); return <button {...props} />; },
  AppInput: (props: ComponentProps<'input'>) => { ui.inputs.push(props); return <input {...props} />; },
}));
const catalog: TradeCatalog = { bases: [{ name: 'Sapphire Ring', category: 'accessory.ring', tags: [], implicits: [], level: 1 }], mods: [] };
const item = 'Rarity: RARE\nCurrent Ring\nSapphire Ring';
const request = { items: [{ slot: 'ring1', text: item }, { slot: 'ring2', text: item.replace('Current', 'Second') }] };
let session: BuildSession;
let weights: TradeOptimization;
let focus: ComponentProps<typeof TradePanel>['focus'];
let active: boolean;
function render(lang: ComponentProps<typeof TradePanel>['lang'] = 'en-US') {
  let html = '';
  for (let count = 0; count < 10; count++) {
    hooks.beginRender(); ui.buttons = []; ui.selects = []; ui.inputs = [];
    html = renderToStaticMarkup(<TradePanel active={active} focus={focus} lang={lang} session={session} />);
    hooks.runEffects();
    if (!hooks.dirty) return html;
  }
  throw new Error('Render did not settle');
}
function button(label: string) {
  const found = ui.buttons.find(props => props.children === label);
  expect(found, label).toBeDefined();
  return found!;
}
async function ready() {
  render();
  await Promise.resolve(); await Promise.resolve(); await Promise.resolve();
  ui.frames.splice(0).forEach(frame => frame());
  return render();
}
function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>(done => { resolve = done; });
  return { promise, resolve };
}
beforeEach(() => {
  hooks.reset(); ui.scroll.mockClear(); ui.focus.mockClear(); ui.optimize.mockReset(); ui.replacement.mockClear(); ui.gems.mockReset();
  ui.goal = { preset: 'life', cStat: '', cMin: '', cMax: '' }; focus = undefined; active = true;
  vi.stubGlobal('localStorage', { getItem: () => null, setItem: vi.fn() });
  ui.frames = [];
  vi.stubGlobal('requestAnimationFrame', (callback: () => void) => ui.frames.push(callback));
  vi.stubGlobal('cancelAnimationFrame', vi.fn());
  session = { socketGroups: [], calcParams: {}, calc: { stats: [] }, items: request.items, flasks: [], jewels: [], allocatedNodes: [],
    currentRequest: () => request, character: { level: 90 }, activeWeaponSet: 1,
  } as unknown as BuildSession;
  weights = { weighted: [], baseline: { Life: 100, CombinedDPS: 100, TotalDPS: 10, TotalEHP: 100 }, combinations: [], evaluated: 1, limited: false,
    minimumWeight: 0, unsupported: [], currentItemScore: null, scoreWarnings: [] };
});
afterEach(() => { vi.unstubAllGlobals(); vi.restoreAllMocks(); });

test('blocked localStorage access keeps market defaults usable', async () => {
  vi.spyOn(globalThis, 'localStorage', 'get').mockImplementation(() => { throw new Error('Storage blocked'); });
  expect(await ready()).toContain('value="100"');
  expect(ui.selects.find(select => select.ariaLabel === 'League')).toBeDefined();
});

test('full storage keeps budget, currency, realm and league changes in memory', async () => {
  vi.stubGlobal('localStorage', { getItem: () => null, setItem: () => { throw new Error('Storage full'); } });
  await ready();
  ui.inputs.find(input => input['aria-label'] === bindT('en-US')('trade.budget'))!.onChange!({ target: { value: '250' } } as never);
  ui.selects.find(select => select.ariaLabel === bindT('en-US')('trade.currency'))!.onChange('divine');
  ui.selects.find(select => select.ariaLabel === bindT('en-US')('trade.realm'))!.onChange('cn');
  render();
  expect(ui.selects.find(select => select.ariaLabel === bindT('en-US')('trade.currency'))!.value).toBe('divine');
  expect(ui.selects.find(select => select.ariaLabel === bindT('en-US')('trade.realm'))!.value).toBe('cn');
  ui.selects.find(select => select.ariaLabel === 'League')!.onChange('__custom');
  render();
  ui.inputs.find(input => input['aria-label'] === bindT('en-US')('trade.leagueCustom'))!.onChange!({ target: { value: 'Custom League' } } as never);
  expect(render()).toContain('value="Custom League"');
  expect(render()).toContain('value="250"');
});

test('no useful scores disable market search and offer a focused target adjustment', async () => {
  await ready();
  ui.optimize.mockResolvedValue(weights);
  await button('Analyze all positions').onClick!({} as never);
  ui.buttons.find(props => props['data-slot-id'] === 'ring1')!.onClick!({} as never);
  const html = render();
  expect(html).toContain('a trade search cannot be generated');
  expect(html).not.toContain('class="trade-primary trade-market-link"');
  expect(button('Open trade search').disabled).toBe(true);
  button('Change target or goal').onClick!({} as never);
  expect(ui.scroll).toHaveBeenCalledWith({ block: 'start' });
  expect(ui.focus).toHaveBeenCalledWith({ preventScroll: true });
});

test('usable scores show integer ranking scores, one-decimal gains and one equipment search action', async () => {
  await ready();
  weights.weighted = [{ id: 'explicit.test', line: '+10 to maximum Life', value: 10, weight: 32.046, gain: 10, gainPercent: 1.234 }];
  weights.currentItemScore = { score: 797.094, complete: true, contributions: [], unscoredLines: [], matchedLines: 1, totalLines: 1 };
  ui.optimize.mockResolvedValue(weights);
  await button('Analyze all positions').onClick!({} as never);
  ui.buttons.find(props => props['data-slot-id'] === 'ring1')!.onClick!({} as never);
  const html = render();
  expect(html).toContain('<strong>797</strong>');
  expect(html).toContain('+1.2%');
  expect(html.match(/class="trade-primary trade-market-link"/g)).toHaveLength(1);
  expect(html).toMatch(/<details class="trade-method"><summary class="disclosure">Calculation details: trade weights<\/summary>.*32.046/);
  ui.selects.find(select => select.ariaLabel === 'League')!.onChange('__custom');
  expect(render()).toContain('Enter a league in Market settings to open this search.');
  expect(button('Open trade search').disabled).toBe(true);
});

test('measures completed positions for ETA and hides resume until Stop is clicked', async () => {
  await ready();
  const first = deferred<TradeOptimization>(), second = deferred<TradeOptimization>();
  ui.optimize.mockReturnValueOnce(first.promise).mockReturnValueOnce(second.promise);
  const now = vi.spyOn(performance, 'now').mockReturnValue(1000);
  const run = button('Analyze all positions').onClick!({} as never);
  const firstHtml = render();
  expect(firstHtml).toContain('Slot 1/2');
  expect(firstHtml).not.toContain('minutes remaining');
  expect(ui.buttons.some(props => props.className === 'trade-primary trade-analyze-all')).toBe(false);
  expect(button('Stop')).toBeDefined();
  now.mockReturnValue(91_000);
  first.resolve(weights);
  await Promise.resolve(); await Promise.resolve();
  const nextHtml = render();
  expect(nextHtml).toContain('Slot 2/2');
  expect(nextHtml).toContain('About 2 minutes remaining');
  expect(nextHtml).not.toContain('Resume remaining positions');
  button('Stop').onClick!({} as never);
  const stopped = render();
  expect(stopped).toContain('Analysis stopped.');
  expect(button('Resume remaining positions').disabled).toBe(false);
  second.resolve(weights); await run;
  expect(render()).not.toContain('Slot 2/2');
  const last = deferred<TradeOptimization>();
  ui.optimize.mockReturnValueOnce(last.promise);
  const resume = button('Resume remaining positions').onClick!({} as never);
  const resuming = render();
  expect(resuming).toContain('Slot 2/2');
  expect(resuming).not.toContain('minutes remaining');
  expect(resuming).not.toContain('Resume remaining positions');
  expect(ui.optimize).toHaveBeenLastCalledWith(expect.objectContaining({ slot: 'ring2' }));
  last.resolve(weights); await resume;
  expect(render()).toContain('Reanalyze all positions');
});

test('slot hand-off focuses step 4 and retains the current goal and baseline item', async () => {
  focus = { slot: 'ring2', nonce: 12 };
  ui.goal.preset = 'ehp';
  render();
  expect(ui.scroll).not.toHaveBeenCalled();
  const html = await ready();
  expect(html).toContain('id="up-market"');
  expect(html).toContain('data-slot-id="ring2" aria-pressed="true"');
  expect(html).toContain('Goal: Effective HP');
  expect(html).toContain('Compare with current item: Second Ring');
  expect(ui.goal.preset).toBe('ehp');
  expect(ui.scroll).toHaveBeenCalledWith({ block: 'start' });
  expect(ui.focus).toHaveBeenCalledWith({ preventScroll: true });
  const calls = ui.focus.mock.calls.length;
  render();
  expect(ui.focus).toHaveBeenCalledTimes(calls);
});

test('separates balanced score from Combined DPS, keeps the four steps in order and sends a result to trade', async () => {
  await ready();
  weights.combinations = [{ mods: [], text: 'Reference', score: 150, stats: { Life: 200, CombinedDPS: 180.6, TotalDPS: 11, TotalEHP: 100 } }];
  ui.optimize.mockResolvedValue(weights);
  await button('Analyze all positions').onClick!({} as never);
  const html = render();
  expect(html.indexOf('id="up-goal"')).toBeLessThan(html.indexOf('id="up-method"'));
  expect(html.indexOf('id="up-method"')).toBeLessThan(html.indexOf('id="up-gain"'));
  expect(html.indexOf('id="up-gain"')).toBeLessThan(html.indexOf('id="up-market"'));
  expect(html).toContain('<th>Balanced score</th><th>Combined DPS</th><th>Effective HP</th>');
  expect(html).toContain('+34.4%'); expect(html).toContain('+80.6%');
  const result = ui.buttons.find(props => props.className === 'trade-priority-position')!;
  result.onClick!({} as never);
  expect(render()).toContain('Compare with current item: Current Ring');
  expect(ui.focus).toHaveBeenCalledWith({ preventScroll: true });
});

test('damage goal uses Combined DPS and the main minion hit DPS when applicable', async () => {
  ui.goal.preset = 'dps';
  await ready();
  expect(ui.replacement).toHaveBeenLastCalledWith(expect.objectContaining({ objective: expect.objectContaining({ stat: 'CombinedDPS' }) }));
  expect(ui.replacement).toHaveBeenLastCalledWith(expect.objectContaining({ actionClassName: 'trade-secondary' }));
  session.calc = { ...session.calc!, main_minion_index: 2, minions: [{ index: 2, name: 'Wolf', dps: 500 }] } as BuildSession['calc'];
  const html = render();
  expect(html).toContain('Minion hit DPS');
  expect(html).toContain('Wolf · per minion');
  expect(ui.replacement).toHaveBeenLastCalledWith(expect.objectContaining({ objective: expect.objectContaining({ stat: 'MainMinionDPS' }) }));
});

test('keeps the saved per-mana goal on hit DPS even for a summoner', async () => {
  ui.goal.preset = 'dpsPerMana';
  session.calc = { ...session.calc!, main_minion_index: 2, minions: [{ index: 2, name: 'Wolf', dps: 500 }] } as BuildSession['calc'];
  await ready();
  expect(ui.replacement).toHaveBeenLastCalledWith(expect.objectContaining({ objective: expect.objectContaining({ stat: 'TotalDPS', per: 'ManaCost' }) }));
});

test.each([false, true])('gem recommendations display the headline damage gain and localized Effective HP (summoner: %s)', async summoner => {
  ui.goal.preset = 'dps';
  session.socketGroups = [{ enabled: true, gems: [], main_active_skill: 1 }];
  session.calcParams = { ...session.calcParams, main_socket_group: 0 };
  session.calc = { ...session.calc!, main_skill: { group_index: 0, active_skill_index: 1 },
    ...(summoner ? { main_minion_index: 2, minions: [{ index: 2, name: 'Wolf' }], stats: [{ id: 'ActiveMinionLimit', value: 7 }] } : {}),
  } as unknown as BuildSession['calc'];
  const plan: GemPlan = { acquisition: 'skill-adjustment', gem: { skill_id: 'Support', name: 'Support', family: 'Support', is_support: true, max_level: 1 },
    group: 0, position: 0, level: 1, quality: 0, variant: {},
    baseline: { TotalDPS: 10, CombinedDPS: 100, MainMinionDPS: 100, TotalEHP: 200 },
    stats: { TotalDPS: 10, CombinedDPS: 150, MainMinionDPS: 180, TotalEHP: 210 } };
  ui.gems.mockResolvedValue([plan]);
  await ready();
  ui.buttons.find(props => props['data-slot-id'] === 'gems')!.onClick!({} as never);
  render(); await button('Calculate modifier scores').onClick!({} as never);
  const html = render();
  const card = html.slice(html.indexOf('class="trade-gem-card"'));
  expect(card).toContain(`data-stat="${summoner ? 'MainMinionDPS' : 'CombinedDPS'}">${summoner ? 'Minion hit DPS' : 'Combined DPS'} <b class="delta-pos">100 → ${summoner ? 180 : 150}</b>`);
  expect(card).toContain('Effective HP <b class="delta-pos">200 → 210</b>');
  expect(card).not.toContain('data-stat="TotalDPS"');
  if (summoner) expect(card).toContain('Wolf · per minion · limit 7');
  const localized = render('zh-CN');
  const chinese = localized.slice(localized.indexOf('class="trade-gem-card"'));
  expect(chinese).toContain('有效生命 <b class="delta-pos">200 → 210</b>');
  if (summoner) expect(chinese).toContain('Wolf · 单只 · 上限 7 只');
});
