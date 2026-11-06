/**
 * 对比预览：把「改一处后的请求」再算一次，与当前结果逐字段求差。
 * PoB2 的装备对比 / 天赋点收益提示都用这一个原语。
 */

import { skillFormName } from './skillNames';
import { hasPosition, isAttributeNode } from './treeGeometry';
import { classStartSkill } from './passiveGraph';
import { STAT_SECTIONS, formatStatValue, type StatFormat, type Lang } from './statDisplay';
import { t } from './i18n';
import { switchWeapons } from './weaponSets';
import { getBackend } from '../api/backend';
import type { CalculateBuildRequest, CalculateBuildResponse, DisplayStatValue, GemCatalogEntry, SocketGroupInput } from '../api/types';

export interface DiffEntry {
  id: string;
  before: number;
  after: number;
  delta: number;
}

/** 两份展示字段的非零差异（忽略 null/∞ 字段与 <0.005 的浮点噪声）。 */
export function diffStats(before: DisplayStatValue[], after: DisplayStatValue[]): DiffEntry[] {
  return compareStats(before, after).filter(row => row.delta !== null && row.changed)
    .map(row => ({ id: row.id, before: row.before as number, after: row.after as number, delta: row.delta! }))
    .sort((a, b) => Math.abs(b.delta) - Math.abs(a.delta));
}

/** 算一次候选请求并与当前结果求差。 */
export async function previewDiff(
  request: CalculateBuildRequest,
  current: CalculateBuildResponse,
): Promise<DiffEntry[]> {
  const backend = await getBackend();
  const next = await backend.calculateBuild(request);
  return diffStats(current.stats, next.stats);
}

export interface StatComparison {
  id: string;
  before: number | null | undefined;
  after: number | null | undefined;
  delta: number | null;
  percent: number | null;
  changed: boolean;
}

const statFormats = new Map(STAT_SECTIONS.flatMap(section => section.rows.map(row => [row.id, row.format] as const)));

export function isPercentageStat(format: StatFormat | undefined): boolean {
  return format === 'percent' || format === 'modifier' || format === 'chanceMultiplier';
}

/** Unknown, explicit null and unbounded results remain unavailable. */
export function compareStats(before: readonly Pick<DisplayStatValue, 'id' | 'value'>[], after: readonly Pick<DisplayStatValue, 'id' | 'value'>[]): StatComparison[] {
  const a = new Map(before.map(stat => [stat.id, stat.value]));
  const b = new Map(after.map(stat => [stat.id, stat.value]));
  return [...new Set([...a.keys(), ...b.keys()])].map(id => {
    const before = a.get(id), after = b.get(id);
    const format = statFormats.get(id);
    const unboundedPierce = format === 'pierce' && (typeof before === 'number' && before >= 100 || typeof after === 'number' && after >= 100);
    const finite = !unboundedPierce && typeof before === 'number' && Number.isFinite(before)
      && typeof after === 'number' && Number.isFinite(after);
    const delta = finite ? after! - before! : null;
    return { id, before, after, delta, percent: delta === null || before === 0 || isPercentageStat(format) ? null : delta / Math.abs(before!) * 100,
      changed: delta === null ? !Object.is(before, after) : Math.abs(delta) >= 0.005 };
  });
}

/** A modifier's delta is a percentage-point difference, not another multiplier. */
export function formatComparisonDelta(delta: number | null, format: StatFormat, lang: Lang): string {
  if (delta === null) return '—';
  const modifier = format === 'modifier' || format === 'chanceMultiplier';
  const value = formatStatValue(modifier ? delta * 100 : delta, modifier ? 'percent' : format === 'pierce' ? 'int' : format, lang);
  return `${delta > 0 ? '+' : ''}${isPercentageStat(format) ? value.replace(/%$/, '') + t(lang, 'compare.pointUnit') : value}`;
}

export type StageState = import('../hooks/useBuildSession').SavedSession['state'];
export type ConfigBasis = 'own' | 'a';

/** Normalizes only the preview's configuration, preserving each main skill and snapshot. */
export function comparisonRequests(a: CalculateBuildRequest, b: CalculateBuildRequest, basis: ConfigBasis): [CalculateBuildRequest, CalculateBuildRequest] {
  const ownA = { ...a, mode_effective: true, config_context: false };
  const ownB = { ...b, mode_effective: true, config_context: false };
  return [ownA, basis === 'own' ? ownB : { ...ownB,
    enemy_tier: a.enemy_tier, config_inputs: a.config_inputs, party: a.party,
    extra_modifiers: a.extra_modifiers, custom_modifier_blocks: a.custom_modifier_blocks,
  }];
}

export function comparisonStats(output: CalculateBuildResponse): DisplayStatValue[] {
  const skillStats = new Set(STAT_SECTIONS.filter(section => section.requiresSkill).flatMap(section => section.rows.map(row => row.id)));
  // CombinedDPS is an output/catalog field, but is not listed in every sidebar section.
  // A zero emitted without a selected skill is absence, not a measured damage result.
  skillStats.add('CombinedDPS');
  skillStats.add('TotalDPS');
  return [...output.stats.filter(stat => output.main_skill || !skillStats.has(stat.id)), ...(output.main_skill ? [
    { id: 'MainSkillHitDPS', value: output.main_skill.hit_dps, category: 'Offence' as const },
    { id: 'MainSkillDotDPS', value: output.main_skill.dot_dps, category: 'Offence' as const },
    { id: 'MainSkillCombinedDPS', value: output.main_skill.combined_dps, category: 'Offence' as const },
  ] : [])];
}

export function resolvedTreeVersion(version: string | null | undefined, dataVersion: string | null): string | null {
  return version && version !== 'Default' ? version : dataVersion ? `0_${dataVersion.split('.')[1]}` : null;
}

export async function compareBuilds(a: CalculateBuildRequest, b: CalculateBuildRequest, basis: ConfigBasis) {
  const backend = await getBackend();
  const [requestA, requestB] = comparisonRequests(a, b, basis);
  // One backend/data snapshot for both; serialized to avoid stacking engine work.
  const before = await backend.calculateBuild(requestA);
  const after = await backend.calculateBuild(requestB);
  return { before, after, stats: compareStats(comparisonStats(before), comparisonStats(after)) };
}

/** Ordered line multiset: repeated modifiers retain their counts and raw text. */
export function diffText(a = '', b = '') {
  const linesA = a ? a.split(/\r?\n/) : [], linesB = b ? b.split(/\r?\n/) : [];
  const remaining = new Map<string, number>();
  for (const line of linesB) remaining.set(line, (remaining.get(line) ?? 0) + 1);
  const before = linesA.map(text => {
    const common = (remaining.get(text) ?? 0) > 0;
    if (common) remaining.set(text, remaining.get(text)! - 1);
    return { text, changed: !common };
  });
  const matched = new Map<string, number>();
  for (const line of before.filter(line => !line.changed)) matched.set(line.text, (matched.get(line.text) ?? 0) + 1);
  const after = linesB.map(text => {
    const common = (matched.get(text) ?? 0) > 0;
    if (common) matched.set(text, matched.get(text)! - 1);
    return { text, changed: !common };
  });
  return { before, after };
}

export type ItemSection = 'items' | 'flasks' | 'limbs';
export type CopySection = { kind: 'item'; section: ItemSection; slot: string }
  | { kind: 'weapon'; set: 1 | 2; slot: string }
  | { kind: 'skill'; index: number } | { kind: 'config'; key: string }
  | { kind: 'enemy' } | { kind: 'modifiers' };

export function itemDifferences(a: StageState, b: StageState) {
  return (['items', 'flasks', 'limbs'] as const).flatMap(section => {
    const aa = new Map(a[section].map(item => [item.slot, item.text]));
    const bb = new Map(b[section].map(item => [item.slot, item.text]));
    return [...new Set([...aa.keys(), ...bb.keys()])].filter(slot => aa.get(slot) !== bb.get(slot) && (section !== 'items' || !['weapon1', 'weapon2'].includes(slot)))
      .map(slot => ({ section, slot, before: aa.get(slot), after: bb.get(slot) }));
  });
}

export function weaponDifferences(a: StageState, b: StageState) {
  return ([1, 2] as const).flatMap(set => {
    const aa = switchWeapons(a, set), bb = switchWeapons(b, set);
    return ['weapon1', 'weapon2'].map(slot => ({ set, slot,
      before: aa.items.find(item => item.slot === slot)?.text,
      after: bb.items.find(item => item.slot === slot)?.text,
    })).filter(row => row.before !== row.after);
  });
}

export function skillDifferences(a: StageState, b: StageState) {
  return Array.from({ length: Math.max(a.socketGroups.length, b.socketGroups.length) }, (_, index) => ({ index,
    before: a.socketGroups[index], after: b.socketGroups[index],
    mainA: a.params.main_socket_group === index, mainB: b.params.main_socket_group === index,
  })).filter(row => JSON.stringify(row.before) !== JSON.stringify(row.after) || row.mainA !== row.mainB);
}

export function configDifferences(a: StageState, b: StageState) {
  const aa = a.params.config_inputs, bb = b.params.config_inputs;
  return [...new Set([...Object.keys(aa), ...Object.keys(bb)])].sort()
    .filter(key => aa[key] !== bb[key]).map(key => ({ key, before: aa[key], after: bb[key] }));
}

/** Copy one displayed section only; no mutation of either saved snapshot. */
export function copySection(a: StageState, b: StageState, section: CopySection): StageState {
  if (section.kind === 'weapon') {
    const item = switchWeapons(b, section.set).items.find(item => item.slot === section.slot);
    const swap = a.weaponSwap ?? { active: 1 as const, alternate_items: [], exclusive_nodes: [[], []] as [number[], number[]] };
    if (swap.active === section.set) return copySection(a, { ...b, items: item ? [item] : [] }, { kind: 'item', section: 'items', slot: section.slot });
    return { ...a, weaponSwap: { ...swap,
      alternate_items: [...swap.alternate_items.filter(item => item.slot !== section.slot), ...(item ? [{ ...item }] : [])],
    } };
  }
  if (section.kind === 'item') {
    const item = b[section.section].find(item => item.slot === section.slot);
    return { ...a, [section.section]: [...a[section.section].filter(item => item.slot !== section.slot), ...(item ? [{ ...item }] : [])] };
  }
  if (section.kind === 'config') {
    const config_inputs = { ...a.params.config_inputs };
    if (Object.hasOwn(b.params.config_inputs, section.key)) config_inputs[section.key] = b.params.config_inputs[section.key];
    else delete config_inputs[section.key];
    return { ...a, params: { ...a.params, config_inputs } };
  }
  if (section.kind === 'enemy') return { ...a, params: { ...a.params, enemy_tier: b.params.enemy_tier } };
  if (section.kind === 'modifiers') return { ...a, params: { ...a.params,
    extra_modifiers: b.params.extra_modifiers?.slice(), custom_modifier_blocks: b.params.custom_modifier_blocks?.map(block => ({ ...block })) } };
  const index = section.index;
  const group = b.socketGroups[index];
  // Equipment-granted groups must be changed through their item, not detached from it.
  if (group?.source || a.socketGroups[index]?.source || index < 0 || !Number.isInteger(index)) return a;
  const socketGroups = a.socketGroups.slice();
  const annotations = { ...a.annotations };
  let main = a.params.main_socket_group;
  if (group) {
    const target = Math.min(index, socketGroups.length);
    socketGroups[target] = structuredClone(group);
    if (b.params.main_socket_group === index) main = target;
  } else if (index < socketGroups.length) {
    socketGroups.splice(index, 1);
    if (main !== undefined) main = main === index ? socketGroups.findIndex(group => group.enabled) : main > index ? main - 1 : main;
    for (const key of Object.keys(annotations)) if (/^skill:\d+$/.test(key)) delete annotations[key];
    for (const [key, value] of Object.entries(a.annotations)) {
      const match = /^skill:(\d+)$/.exec(key);
      if (match && Number(match[1]) !== index) annotations[`skill:${Number(match[1]) > index ? Number(match[1]) - 1 : match[1]}`] = value;
    }
  }
  return { ...a, socketGroups, annotations, params: { ...a.params, main_socket_group: main !== undefined && main >= 0 ? main : undefined } };
}

export type AllocationSide = 'a' | 'b' | 'both' | 'none';
export interface OverlayNode {
  key: string;
  node: import('../api/types').PassiveNode;
  side: AllocationSide;
  a?: import('../api/types').PassiveNode;
  b?: import('../api/types').PassiveNode;
  jewelA?: string;
  jewelB?: string;
}

/** IDs are shared identities only inside the same version. Missing coordinates stay missing. */
export function compareTreeAllocations(a: StageState, b: StageState,
  nodesA: import('../api/types').PassiveNode[], nodesB: import('../api/types').PassiveNode[], sameVersion: boolean) {
  const aa = new Set(a.allocatedNodes), bb = new Set(b.allocatedNodes);
  const byA = new Map(nodesA.map(node => [node.skill, node])), byB = new Map(nodesB.map(node => [node.skill, node]));
  const jewelsA = new Map(a.jewels.map(jewel => [jewel.socket_node, jewel.text]));
  const jewelsB = new Map(b.jewels.map(jewel => [jewel.socket_node, jewel.text]));
  const nodes: OverlayNode[] = [];
  if (sameVersion) {
    for (const id of new Set([...byA.keys(), ...byB.keys()])) {
      const nodeA = byA.get(id), nodeB = byB.get(id);
      nodes.push({ key: String(id), node: nodeA ?? nodeB!, a: nodeA, b: nodeB,
        side: aa.has(id) ? bb.has(id) ? 'both' : 'a' : bb.has(id) ? 'b' : 'none',
        jewelA: jewelsA.get(id), jewelB: jewelsB.get(id) });
    }
  } else {
    for (const [label, catalog, allocated, jewels] of [['a', nodesA, aa, jewelsA], ['b', nodesB, bb, jewelsB]] as const) {
      for (const node of catalog) nodes.push({ key: `${label}:${node.skill}`, node, [label]: node,
        side: allocated.has(node.skill) ? label : 'none', [label === 'a' ? 'jewelA' : 'jewelB']: jewels.get(node.skill) });
    }
  }
  const missing = (ids: Set<number>, catalog: Map<number, import('../api/types').PassiveNode>) => [...ids].filter(id => {
    const node = catalog.get(id);
    return !node || !hasPosition(node);
  }).sort((x, y) => x - y);
  return { nodes, unplacedA: missing(aa, byA), unplacedB: missing(bb, byB) };
}

export function treeAllocationTotals(state: StageState, nodes: import('../api/types').PassiveNode[]) {
  const byId = new Map(nodes.map(node => [node.skill, node]));
  const starts = new Set(['Druid', 'Mercenary', 'Ranger', 'Warrior', 'Monk', 'Witch'].map(name => classStartSkill(nodes, name)));
  const totals = { passive: 0, ascendancy: 0, unknown: [] as number[], attributes: { str: 0, dex: 0, int: 0, unassigned: 0 } };
  for (const id of new Set(state.allocatedNodes)) {
    const node = byId.get(id);
    if (!node) { totals.unknown.push(id); continue; }
    if (starts.has(id) || node.kind === 'ascendancy_start') continue;
    if (node.ascendancy_id) totals.ascendancy++; else totals.passive++;
    if (isAttributeNode(node)) totals.attributes[state.attributeChoices[String(id)] ?? 'unassigned']++;
  }
  return totals;
}

/** Stage order is the saved before/after order; never infer chronology from names. */
export function defaultCompareSides<T extends { buildId: string; stageId: string }>(entries: T[], activeId: string | undefined, baseline?: T): { a?: T; b?: T } {
  const current = entries.find(entry => entry.stageId === activeId) ?? entries[0];
  if (!current) return {};
  if (baseline) return { a: baseline, b: current };
  const stages = entries.filter(entry => entry.buildId === current.buildId);
  const index = stages.indexOf(current);
  return index > 0 ? { a: stages[index - 1], b: current } : { a: current, b: stages[index + 1] };
}

/** Empty B is not a removal action. Explicit false and zero config values are copyable. */
export function hasComparisonSource(b: StageState, section: CopySection): boolean {
  switch (section.kind) {
    case 'item': return !!b[section.section].find(item => item.slot === section.slot)?.text.trim();
    case 'weapon': return !!switchWeapons(b, section.set).items.find(item => item.slot === section.slot)?.text.trim();
    case 'skill': return b.socketGroups[section.index] !== undefined;
    case 'config': return b.params.config_inputs[section.key] !== undefined;
    case 'enemy': return b.params.enemy_tier !== undefined;
    case 'modifiers': return !!(b.params.custom_modifier_blocks?.length || b.params.extra_modifiers?.length);
  }
}

/** Default forms add no information unless this gem changed form across the comparison. */
export function comparisonGemForm(gem: SocketGroupInput['gems'][number], other: SocketGroupInput['gems'][number] | undefined,
  catalog: ReadonlyMap<string, GemCatalogEntry>, lang: Lang, effectId = gem.skill_id): string | null {
  const index = gem.stat_set_index ?? 1;
  if (index === 1 && (!other || other.skill_id !== gem.skill_id || (other.stat_set_index ?? 1) === 1)) return null;
  const forms = catalog.get(gem.skill_id)?.effect_forms?.find(effect => effect.skill_id === effectId)?.stat_sets;
  const form = forms?.find(form => form.index === index);
  return form?.label ? skillFormName(form.label, form.id, lang, catalog) : `${t(lang, 'compare.form')} ${index}`;
}
