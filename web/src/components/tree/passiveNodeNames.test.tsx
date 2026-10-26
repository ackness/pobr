import { renderToStaticMarkup } from 'react-dom/server';
import { beforeEach, expect, test, vi } from 'vitest';
import type { PassiveNode } from '../../api/types';
import type { Lang } from '../../lib/i18n';
import { passiveNodeMatches, passiveNodeName, usePassiveNodeNames } from './passiveNodeNames';

const translate = vi.hoisted(() => vi.fn<(lines: string[], lang: Lang) => string[]>());
vi.mock('../../hooks/useLocalizedLines', () => ({ useLocalizedLines: translate }));
const node: PassiveNode = Object.freeze({ skill: 6898, id: 'templar_druid_notable1', name: 'Relentless Vindicator',
  kind: 'notable', stats: ['10% increased [Critical|Critical Hit Chance]'] });
const translations = {
  'zh-CN': new Map([[node.name!, '无休仲裁者'], ['Devoted Protector', '献身卫士']]),
  'zh-TW': new Map([[node.name!, '無休仲裁者'], ['Devoted Protector', '獻身衛士']]),
  'en-US': new Map<string, string>(),
};

beforeEach(() => {
  translate.mockReset().mockImplementation((lines, lang) => lines.map(line => translations[lang].get(line) ?? line));
});

test.each(['zh-CN', 'zh-TW', 'en-US'] as const)('searches display names, original names, IDs and English modifiers in %s', lang => {
  const names = translations[lang];
  expect(passiveNodeName(node, names)).toBe(names.get(node.name!) ?? node.name);
  for (const query of ['', '  ', passiveNodeName(node, names), ' RELENTLESS ', 'templar_druid_notable1', '6898', 'Critical Hit Chance', '[Critical|']) {
    expect(passiveNodeMatches(node, query, names), query).toBe(true);
  }
  expect(passiveNodeMatches(node, 'missing node', names)).toBe(false);
  expect(node.name).toBe('Relentless Vindicator');
  expect(node.stats).toEqual(['10% increased [Critical|Critical Hit Chance]']);
});

test('uses the effective transformed name, preserves unknown names and falls back to stable ID', () => {
  expect(passiveNodeName({ ...node, name: 'Devoted Protector' }, translations['zh-CN'])).toBe('献身卫士');
  expect(passiveNodeName({ ...node, name: 'Unknown transformed power' }, translations['zh-CN'])).toBe('Unknown transformed power');
  expect(passiveNodeName({ ...node, name: undefined }, translations['zh-CN'])).toBe(node.id);
});

test('batches unique effective names without stats or IDs, and gates inactive trees', () => {
  const nodes = [node, { ...node, skill: 42 }, { ...node, skill: 43, name: 'Devoted Protector' }, { ...node, name: undefined }];
  function Probe({ active }: { active: boolean }) {
    const names = usePassiveNodeNames(nodes, 'zh-CN', active);
    return <span>{passiveNodeName(node, names)}</span>;
  }
  expect(renderToStaticMarkup(<Probe active />)).toContain('无休仲裁者');
  expect(translate).toHaveBeenLastCalledWith(['Relentless Vindicator', 'Devoted Protector'], 'zh-CN');
  expect(renderToStaticMarkup(<Probe active={false} />)).toContain('Relentless Vindicator');
  expect(translate).toHaveBeenLastCalledWith([], 'zh-CN');
});
