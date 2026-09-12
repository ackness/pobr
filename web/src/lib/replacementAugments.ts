import type { ItemAugmentInfo, RuneCatalogEntry } from '../api/types';

export interface AugmentSelection {
  mode: 'inherit' | 'original' | 'custom';
  sockets?: number;
  runes?: string[];
}

export interface ReplacementAugments {
  info: ItemAugmentInfo;
  selection: AugmentSelection;
  source: 'original' | 'inherited' | 'custom';
  sockets: number;
  runes: string[];
  addedSockets: number;
  skipped: string[];
  limitWarning?: 'unknown-equipped-augments';
}

export interface AugmentUsage {
  catalog: RuneCatalogEntry[];
  items: { slot: string; text: string }[];
  slot: string;
}

/** Limits apply to the resulting active equipment set, including shared-name groups. */
export function applyEquippedAugmentLimits(info: ItemAugmentInfo, usage?: AugmentUsage) {
  const entries = [...(usage?.catalog ?? []), ...info.options];
  const canonical = new Map(entries.map(entry => [entry.name, entry]));
  const global = new Map((usage?.catalog ?? []).map(entry => [entry.name, entry]));
  const aliases = new Map(entries.flatMap(entry => [entry.name, entry.name_zh_cn, entry.name_zh_tw]
    .filter((name): name is string => !!name).map(name => [name, entry.name] as const)));
  const groupOf = (name: string) => {
    const resolved = aliases.get(name) ?? name;
    return global.get(resolved)?.limit_id ?? canonical.get(resolved)?.limit_id ?? resolved;
  };
  const limits = new Map<string, number>();
  for (const entry of usage?.catalog ?? []) {
    if (entry.limit !== undefined) {
      const group = groupOf(entry.name);
      limits.set(group, Math.min(limits.get(group) ?? Infinity, entry.limit));
    }
  }
  // The global catalog stores the original minimum across item contexts. Prefer
  // it over a previously adjusted picker option so repeated views do not deduct twice.
  const globalGroups = new Set(limits.keys());
  for (const entry of info.options) {
    const group = groupOf(entry.name);
    if (entry.limit !== undefined && !globalGroups.has(group)) {
      limits.set(group, Math.min(limits.get(group) ?? Infinity, entry.limit));
    }
  }
  const occupied = new Map<string, number>();
  let uncertain = false;
  for (const item of usage?.items ?? []) {
    if (item.slot === usage?.slot) continue;
    let named = false;
    for (const match of item.text.matchAll(/^[ \t]*(?:Rune|Soul Core|Idol):[ \t]*([^\n\r]*)/gmi)) {
      const name = match[1].trim();
      if (!name || /^none$/i.test(name)) continue;
      named = true;
      const resolved = aliases.get(name);
      if (!resolved) { uncertain = true; continue; }
      const group = groupOf(resolved);
      occupied.set(group, (occupied.get(group) ?? 0) + 1);
    }
    if (!named && /^\s*(?:\{[^}]*\})*\{rune\}/im.test(item.text)) uncertain = true;
  }
  const remaining = new Map([...limits].map(([group, limit]) => [group,
    uncertain ? 0 : Math.max(0, limit - (occupied.get(group) ?? 0))]));
  const adjusted = { ...info, options: info.options.map(option => {
    const limit = remaining.get(groupOf(option.name));
    return limit === undefined ? option : { ...option, limit, limit_id: groupOf(option.name) };
  }) };
  const exceeds = (names: string[]) => {
    const counts = new Map<string, number>();
    return names.filter(Boolean).some(name => {
      const group = groupOf(name), count = (counts.get(group) ?? 0) + 1;
      counts.set(group, count);
      return count > (remaining.get(group) ?? Infinity);
    });
  };
  return { info: adjusted, remaining, groupOf, exceeds,
    ...(uncertain ? { limitWarning: 'unknown-equipped-augments' as const } : {}) };
}

/** Match PoB2's empty-candidate migration; never silently replace a listing's augments. */
export function planReplacementAugments(info: ItemAugmentInfo, current: ItemAugmentInfo | undefined,
  level: number, selection: AugmentSelection = { mode: 'inherit' }, usage?: AugmentUsage): ReplacementAugments {
  const limits = applyEquippedAugmentLimits(info, usage);
  info = limits.info;
  const original: ReplacementAugments = { info, selection, source: 'original', sockets: info.sockets,
    runes: info.runes, addedSockets: 0, skipped: [],
    ...(limits.limitWarning || info.reason === 'unknown_augments' ? { limitWarning: 'unknown-equipped-augments' as const } : {}) };
  const preserveOriginal = () => {
    if (limits.exceeds(info.runes)) throw new Error('augment-limit');
    return original;
  };
  if (selection.mode === 'original' || !info.editable) return preserveOriginal();
  const allowed = new Map(info.options.filter(option => (option.required_level ?? 0) <= level).map(option => [option.name, option]));
  if (selection.mode === 'inherit') {
    if (info.runes.some(Boolean) || !current || (!current.sockets && !current.runes.length)) return preserveOriginal();
    const sockets = Math.max(info.sockets, Math.min(current.sockets, info.max_sockets));
    const names = current.runes.filter(Boolean);
    const counts = new Map<string, number>();
    const excluded: string[] = [];
    const fitting = names.filter(name => {
      const option = allowed.get(name), group = limits.groupOf(name), count = (counts.get(group) ?? 0) + 1;
      if (!option || count > (limits.remaining.get(group) ?? Infinity)) { excluded.push(name); return false; }
      counts.set(group, count); return true;
    });
    const runes = fitting.slice(0, sockets);
    const skipped = [...excluded, ...fitting.slice(sockets)];
    if (!sockets && !runes.length) return { ...original, skipped };
    return { ...original, source: 'inherited', sockets, runes, addedSockets: sockets - info.sockets, skipped };
  }
  const sockets = selection.sockets ?? info.sockets;
  const runes = selection.runes ?? info.runes;
  if (!Number.isInteger(sockets) || sockets < 0 || sockets > info.max_sockets || runes.length > sockets ||
    runes.some(name => name && !allowed.has(name))) throw new Error('invalid-augments');
  if (limits.exceeds(runes)) throw new Error(usage ? 'augment-limit' : 'invalid-augments');
  return { ...original, source: 'custom', sockets, runes, addedSockets: Math.max(0, sockets - info.sockets) };
}
