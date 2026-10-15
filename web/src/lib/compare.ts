/**
 * 对比预览：把「改一处后的请求」再算一次，与当前结果逐字段求差。
 * PoB2 的装备对比 / 天赋点收益提示都用这一个原语。
 */

import { STAT_SECTIONS, formatStatValue, type StatFormat, type Lang } from './statDisplay';
import { switchWeapons } from './weaponSets';
import { getBackend } from '../api/backend';
import type { CalculateBuildRequest, CalculateBuildResponse, DisplayStatValue } from '../api/types';

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

/** A missing or unbounded result is never converted to zero, including % from zero. */
export function compareStats(before: DisplayStatValue[], after: DisplayStatValue[]): StatComparison[] {
  const a = new Map(before.map(stat => [stat.id, stat.value]));
  const b = new Map(after.map(stat => [stat.id, stat.value]));
  return [...new Set([...a.keys(), ...b.keys()])].map(id => {
    const before = a.get(id), after = b.get(id);
    const format = statFormats.get(id);
    const unboundedPierce = format === 'pierce' && (typeof before === 'number' && before >= 100 || typeof after === 'number' && after >= 100);
    const finite = !unboundedPierce && typeof before === 'number' && Number.isFinite(before)
      && typeof after === 'number' && Number.isFinite(after);
    const delta = finite ? after! - before! : null;
    const percentBase = typeof before === 'number' && (format === 'modifier' || format === 'chanceMultiplier') ? before - 1 : before;
    return { id, before, after, delta, percent: delta === null || percentBase === 0 ? null : delta / Math.abs(percentBase!) * 100,
      changed: delta === null ? !Object.is(before, after) : Math.abs(delta) >= 0.005 };
  });
}

/** A modifier's delta is a percentage-point difference, not another multiplier. */
export function formatComparisonDelta(delta: number | null, format: StatFormat, lang: Lang): string {
  if (delta === null) return '—';
  const modifier = format === 'modifier' || format === 'chanceMultiplier';
  return `${delta > 0 ? '+' : ''}${formatStatValue(modifier ? delta * 100 : delta, modifier ? 'percent' : format === 'pierce' ? 'int' : format, lang)}`;
}

export type StageState = import('../hooks/useBuildSession').SavedSession['state'];
export type ConfigBasis = 'own' | 'a';

/** Normalizes only the preview's configuration, preserving each main skill and snapshot. */
export function comparisonRequests(a: CalculateBuildRequest, b: CalculateBuildRequest, basis: ConfigBasis): [CalculateBuildRequest, CalculateBuildRequest] {
  const ownA = { ...a, mode_effective: true, config_context: false };
  const ownB = { ...b, mode_effective: true, config_context: false };
  return [ownA, basis === 'own' ? ownB : { ...ownB,
    enemy_tier: a.enemy_tier, config_inputs: a.config_inputs,
    extra_modifiers: a.extra_modifiers, custom_modifier_blocks: a.custom_modifier_blocks,
  }];
}

export function comparisonStats(output: CalculateBuildResponse): DisplayStatValue[] {
  const skillStats = new Set(STAT_SECTIONS.filter(section => section.requiresSkill).flatMap(section => section.rows.map(row => row.id)));
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

export function allocationDiff(a: number[], b: number[]) {
  const aa = new Set(a), bb = new Set(b);
  return { added: [...bb].filter(id => !aa.has(id)).sort((x, y) => x - y),
    removed: [...aa].filter(id => !bb.has(id)).sort((x, y) => x - y),
    shared: [...aa].filter(id => bb.has(id)).sort((x, y) => x - y) };
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
