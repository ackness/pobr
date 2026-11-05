import { renderToStaticMarkup } from 'react-dom/server';
import { beforeEach, expect, test, vi } from 'vitest';
import type { PassiveNode } from '../../api/types';
import type { Lang } from '../../lib/i18n';
import type { GameNameRequest } from '../../lib/nativeDisplayNames';
import { passiveNameKey, passiveNodeMatches, passiveNodeName, translatePassiveCard, usePassiveNodeNames } from './passiveNodeNames';

const translate = vi.hoisted(() => vi.fn<(names: readonly GameNameRequest[], lang: Lang) => string[]>());
vi.mock('../../hooks/useLocalizedLines', () => ({ useLocalizedNames: translate }));
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
