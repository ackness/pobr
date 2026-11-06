import { renderToStaticMarkup } from 'react-dom/server';
import { beforeEach, expect, test, vi } from 'vitest';
import type { PassiveNode } from '../../api/types';
import type { Lang } from '../../lib/i18n';
import type { GameNameRequest } from '../../lib/nativeDisplayNames';
import { passiveNameKey, passiveNodeMatches, passiveNodeName, translatePassiveCard, translatePassiveStats, usePassiveNodeNames, usePassiveNodesBySkill } from './passiveNodeNames';

const translate = vi.hoisted(() => vi.fn<(names: readonly GameNameRequest[], lang: Lang) => string[]>());
const ui = vi.hoisted(() => ({ effects: [] as (() => void | (() => void))[], backend: {
  translateLines: vi.fn<(lines: string[], lang: Lang) => Promise<string[]>>(),
  loadPassiveTree: vi.fn<(version?: string | null) => Promise<PassiveNode[]>>(),
} }));
vi.mock('react', async original => ({ ...await original<typeof import('react')>(),
  useEffect: (effect: () => void | (() => void)) => { ui.effects.push(effect); },
}));
vi.mock('../../api/backend', () => ({ getBackend: async () => ui.backend }));
vi.mock('../../hooks/useLocalizedLines', async original => ({
  ...await original<typeof import('../../hooks/useLocalizedLines')>(), useLocalizedNames: translate,
}));
const node: PassiveNode = Object.freeze({ skill: 6898, id: 'templar_druid_notable1', name: 'Relentless Vindicator',
  kind: 'notable', stats: ['10% increased [Critical|Critical Hit Chance]'] });
const names = (lang: Lang, entries: [Pick<PassiveNode, 'id' | 'name'>, string][]) =>
  new Map(lang === 'en-US' ? [] : entries.map(([key, name]) => [passiveNameKey(key), name]));
const translations = {
  'zh-CN': names('zh-CN', [[node, '无休仲裁者'], [{ ...node, name: 'Devoted Protector' }, '献身卫士']]),
  'zh-TW': names('zh-TW', [[node, '無休仲裁者'], [{ ...node, name: 'Devoted Protector' }, '獻身衛士']]),
  'en-US': names('en-US', []),
};

beforeEach(() => {
  ui.effects = [];
  ui.backend = { translateLines: vi.fn(async lines => lines), loadPassiveTree: vi.fn(async () => [node]) };
  translate.mockReset().mockImplementation((requests, lang) => requests.map(request =>
    request.kind === 'passive' ? translations[lang].get(passiveNameKey(request)) ?? request.name : request.name));
});

test.each(['zh-CN', 'zh-TW', 'en-US'] as const)('searches display names, original names, IDs and English modifiers in %s', lang => {
  const shown = translations[lang];
  expect(passiveNodeName(node, shown)).toBe(shown.get(passiveNameKey(node)) ?? node.name);
  for (const query of ['', '  ', passiveNodeName(node, shown), ' RELENTLESS ', 'templar_druid_notable1', '6898', 'Critical Hit Chance', '[Critical|']) {
    expect(passiveNodeMatches(node, query, shown), query).toBe(true);
  }
  expect(passiveNodeMatches(node, 'missing node', shown)).toBe(false);
  expect(node.name).toBe('Relentless Vindicator');
  expect(node.stats).toEqual(['10% increased [Critical|Critical Hit Chance]']);
});

test('uses the effective transformed name, preserves unknown names and falls back to stable ID', () => {
  expect(passiveNodeName({ ...node, name: 'Devoted Protector' }, translations['zh-CN'])).toBe('献身卫士');
  expect(passiveNodeName({ ...node, name: 'Unknown transformed power' }, translations['zh-CN'])).toBe('Unknown transformed power');
  expect(passiveNodeName({ ...node, name: undefined }, translations['zh-CN'])).toBe(node.id);
});

test('batches unique id/name pairs without stats, and gates inactive trees', () => {
  const nodes = [node, { ...node, skill: 42 }, { ...node, skill: 43, name: 'Devoted Protector' }, { ...node, name: undefined }];
  function Probe({ active }: { active: boolean }) {
    const shown = usePassiveNodeNames(nodes, 'zh-CN', active);
    return <span>{passiveNodeName(node, shown)}</span>;
  }
  expect(renderToStaticMarkup(<Probe active />)).toContain('无休仲裁者');
  expect(translate).toHaveBeenLastCalledWith([{ kind: 'passive', id: node.id, name: 'Relentless Vindicator' },
    { kind: 'passive', id: node.id, name: 'Devoted Protector' }], 'zh-CN');
  expect(renderToStaticMarkup(<Probe active={false} />)).toContain('Relentless Vindicator');
  expect(translate).toHaveBeenLastCalledWith([], 'zh-CN');
});

test('two nodes with the same English keep their own id-scoped translations', () => {
  const mana = { ...node, skill: 1, id: 'mana10', name: 'Mana Regeneration' };
  const recovery = { ...node, skill: 2, id: 'energy_shield_recovery10', name: 'Mana Regeneration' };
  translate.mockImplementation(requests => requests.map(request =>
    request.kind === 'passive' && request.id === 'mana10' ? '魔力再生' : '魔力回复'));
  function Probe() {
    const shown = usePassiveNodeNames([mana, recovery], 'zh-CN', true);
    return <span>{passiveNodeName(mana, shown)}|{passiveNodeName(recovery, shown)}</span>;
  }
  expect(renderToStaticMarkup(<Probe />)).toContain('魔力再生|魔力回复');
});

test('a tooltip card names the node by id and translates only its lines as text', async () => {
  const backend = {
    translateNames: vi.fn(async (requests: readonly GameNameRequest[]) => requests.map(() => '魔力再生')),
    translateLines: vi.fn(async (lines: string[]) => lines.map(line => `译:${line}`)),
  };
  expect(await translatePassiveCard(backend, { id: 'mana10', name: 'Mana Regeneration' }, ['a', 'b'], 'zh-CN'))
    .toEqual(['魔力再生', '译:a', '译:b']);
  expect(backend.translateNames).toHaveBeenCalledWith([{ kind: 'passive', id: 'mana10', name: 'Mana Regeneration' }], 'zh-CN');
  expect(backend.translateLines).toHaveBeenCalledWith(['a', 'b'], 'zh-CN');
  expect(await translatePassiveCard(backend, { id: 'unnamed' }, [], 'zh-CN')).toEqual(['unnamed']);
  expect(backend.translateNames).toHaveBeenCalledTimes(1);
});

test('a card drops a line a multi-line description translated into the line above', async () => {
  const backend = {
    translateNames: vi.fn(async (requests: readonly GameNameRequest[]) => requests.map(request => request.name)),
    translateLines: vi.fn(async () => ['过去 8 秒内每发射一种不同的榴弹，获得相当于伤害 4% 的额外火焰伤害', '', '火焰抗性 +20%']),
  };
  expect(await translatePassiveCard(backend, { id: 'n', name: 'N' }, ['Gain 4% of Damage as Extra Fire Damage for',
    'every different Grenade fired in the past 8 seconds', '+20% to Fire Resistance'], 'zh-CN'))
    .toEqual(['N', '过去 8 秒内每发射一种不同的榴弹，获得相当于伤害 4% 的额外火焰伤害', '火焰抗性 +20%']);
});


test.each(['zh-CN', 'zh-TW'] as const)('indexes the displayed Spirit modifier for a Chinese query in %s and retains English matching', async lang => {
  const spirit = { ...node, name: 'Boundless Power', stats: ['30% increased [Spirit|Spirit]'] };
  const life = { ...node, id: 'life', skill: 42, name: 'Health', stats: ['10% increased maximum Life'] };
  const backend = { translateLines: vi.fn(async (lines: string[]) => lines.map(line => line.includes('Spirit') ? '精魂提高 30%' : line)) };
  const index = await translatePassiveStats(backend, [spirit, life], lang);
  expect(passiveNodeMatches(spirit, '精魂', index)).toBe(true);
  expect(passiveNodeMatches(spirit, 'SPIRIT', index)).toBe(true);
  expect(passiveNodeMatches(life, '精魂', index)).toBe(false);
  expect(backend.translateLines).toHaveBeenCalledWith(['30% increased Spirit', '', '10% increased maximum Life', ''], lang);
  expect(spirit.stats).toEqual(['30% increased [Spirit|Spirit]']);
});

test('stat indexes follow skill IDs within one tree and keep multi-line descriptions inside their node', async () => {
  const first = { ...node, skill: 1, stats: ['First line', 'Continuation'] };
  const second = { ...node, skill: 2, stats: ['Second effect'] };
  const backend = { translateLines: vi.fn(async () => ['完整词条', '', '', '另一词条', '']) };
  const index = await translatePassiveStats(backend, [first, second], 'zh-CN');
  expect(passiveNodeMatches(first, '完整词条', index)).toBe(true);
  expect(passiveNodeMatches(second, '完整词条', index)).toBe(false);
  expect(passiveNodeMatches(second, '另一词条', index)).toBe(true);
  backend.translateLines.mockClear();
  const english = await translatePassiveStats(backend, [first, second], 'en-US');
  expect(passiveNodeMatches(first, 'First line', english)).toBe(true);
  expect(backend.translateLines).not.toHaveBeenCalled();
});

test('stat translation waits for a non-English search and reuses its tree/language request', async () => {
  function Probe({ active, lang, search }: { active: boolean; lang: Lang; search: string }) {
    usePassiveNodeNames([node], lang, active, search);
    return null;
  }
  renderToStaticMarkup(<><Probe active={false} lang="zh-CN" search="critical" />
    <Probe active lang="en-US" search="critical" /><Probe active lang="zh-CN" search=" " /></>);
  ui.effects.splice(0).forEach(effect => effect());
  await Promise.resolve();
  expect(ui.backend.translateLines).not.toHaveBeenCalled();
  renderToStaticMarkup(<Probe active lang="zh-CN" search="暴击" />);
  const effect = ui.effects.pop()!;
  const cleanup = effect();
  await vi.waitFor(() => expect(ui.backend.translateLines).toHaveBeenCalledTimes(1));
  cleanup?.();
  const nextCleanup = effect();
  await Promise.resolve();
  expect(ui.backend.translateLines).toHaveBeenCalledTimes(1);
  nextCleanup?.();
});

test('passive-source displays load one shared skill index per backend and tree version', async () => {
  function Probe({ enabled = true, version }: { enabled?: boolean; version?: string }) {
    usePassiveNodesBySkill(enabled, version);
    return null;
  }
  renderToStaticMarkup(<><Probe enabled={false} /><Probe /><Probe /></>);
  const cleanups = ui.effects.splice(0).map(effect => effect());
  await vi.waitFor(() => expect(ui.backend.loadPassiveTree).toHaveBeenCalledTimes(1));
  expect(ui.backend.loadPassiveTree).toHaveBeenCalledWith(undefined);
  renderToStaticMarkup(<><Probe version="0_4" /><Probe version="0_4" /></>);
  cleanups.push(...ui.effects.splice(0).map(effect => effect()));
  await vi.waitFor(() => expect(ui.backend.loadPassiveTree).toHaveBeenCalledTimes(2));
  expect(ui.backend.loadPassiveTree).toHaveBeenLastCalledWith('0_4');
  cleanups.forEach(cleanup => cleanup?.());
});
