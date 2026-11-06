import { useEffect, useMemo, useState } from 'react';
import { getBackend, type PobrBackend } from '../../api/backend';
import type { PassiveNode } from '../../api/types';
import { cleanLine, linesToTranslate, localizedLines, useLocalizedNames } from '../../hooks/useLocalizedLines';
import { shownLines } from '../../lib/continuedLines';
import type { Lang } from '../../lib/i18n';
import { cleanLineText } from '../../lib/modifierText';
import type { GameNameRequest } from '../../lib/nativeDisplayNames';

/** Display names keyed by {@link passiveNameKey}: one English name can have several translations. */
export type PassiveNodeNames = ReadonlyMap<string, string>;

/** A node's name identity: its PassiveSkills id and its current effective (possibly transformed) name. */
export const passiveNameKey = (node: Pick<PassiveNode, 'id' | 'name'>): string => `${node.id}\n${node.name ?? ''}`;

/** The id-scoped name request for a node, or none when it has no name. */
export function passiveNameRequest(node: Pick<PassiveNode, 'id' | 'name'>): GameNameRequest[] {
  return node.name ? [{ kind: 'passive', id: node.id, name: node.name }] : [];
}

const passiveStatsKey = (node: Pick<PassiveNode, 'skill'>): string => `stats:${node.skill}`;

type PassiveNodesBySkill = ReadonlyMap<string, PassiveNode>;
const NO_PASSIVE_NODES: PassiveNodesBySkill = new Map();
const passiveTrees = new WeakMap<PobrBackend, Map<string | undefined, Promise<PassiveNodesBySkill>>>();

/** Share the loaded tree and its skill index across passive-source displays. */
export function usePassiveNodesBySkill(enabled: boolean, treeVersion?: string): PassiveNodesBySkill {
  const [loaded, setLoaded] = useState<{ treeVersion?: string; nodes: PassiveNodesBySkill } | null>(null);
  useEffect(() => {
    if (!enabled) return;
    let cancelled = false;
    getBackend().then(backend => {
      let versions = passiveTrees.get(backend);
      if (!versions) { versions = new Map(); passiveTrees.set(backend, versions); }
      let pending = versions.get(treeVersion);
      if (!pending) {
        pending = backend.loadPassiveTree(treeVersion).then(nodes => new Map(nodes.map(node => [String(node.skill), node])));
        versions.set(treeVersion, pending);
        pending.catch(() => { versions.delete(treeVersion); });
      }
      return pending;
    }).then(nodes => { if (!cancelled) setLoaded({ treeVersion, nodes }); }).catch(() => {});
    return () => { cancelled = true; };
  }, [enabled, treeVersion]);
  return loaded && loaded.treeVersion === treeVersion ? loaded.nodes : NO_PASSIVE_NODES;
}

/** Search uses the same complete descriptions and continuation handling as modifier displays. */
export async function translatePassiveStats(backend: Pick<PobrBackend, 'translateLines'>, nodes: readonly PassiveNode[], lang: Lang): Promise<PassiveNodeNames> {
  // A blank separator prevents a multi-line description from spanning two nodes.
  const lines = nodes.flatMap(node => [...(node.stats ?? []).map(cleanLine), '']);
  const sent = linesToTranslate(lines, lang);
  const translated = lang === 'en-US' || !sent.some(Boolean) ? [] : await backend.translateLines(sent, lang);
  const localized = localizedLines(lines, lang, sent.map((line, index) => line ? translated[index] : undefined));
  const result = new Map<string, string>();
  let offset = 0;
  for (const node of nodes) {
    const count = node.stats?.length ?? 0;
    result.set(passiveStatsKey(node), shownLines(lines.slice(offset, offset + count), localized.slice(offset, offset + count)).join('\n'));
    offset += count + 1;
  }
  return result;
}

/** Translate names once for the active tree's search and reports, never its calculation data. */
export function usePassiveNodeNames(nodes: readonly PassiveNode[], lang: Lang, active: boolean, search = ''): PassiveNodeNames {
  const unique = useMemo(() => {
    const byKey = new Map<string, GameNameRequest>();
    if (active) for (const node of nodes) for (const request of passiveNameRequest(node)) byKey.set(passiveNameKey(node), request);
    return { keys: [...byKey.keys()], requests: [...byKey.values()] };
  }, [nodes, active]);
  const translated = useLocalizedNames(unique.requests, lang);
  // Callers may recreate display arrays without changing the stats or their skill identities.
  const statsKey = useMemo(() => JSON.stringify(nodes.map(node => [node.skill, node.stats ?? []])), [nodes]);
  const cache = useMemo(() => new Map<Lang, Promise<PassiveNodeNames>>(), [statsKey]);
  const [stats, setStats] = useState<{ key: string; values: ReadonlyMap<Lang, PassiveNodeNames> } | null>(null);
  const searching = Boolean(search.trim());
  useEffect(() => {
    if (!active || !searching || lang === 'en-US' || !nodes.length) return;
    let pending = cache.get(lang);
    if (!pending) {
      pending = getBackend().then(backend => translatePassiveStats(backend, nodes, lang));
      cache.set(lang, pending);
      pending.catch(() => { cache.delete(lang); });
    }
    let cancelled = false;
    pending.then(values => {
      if (!cancelled) setStats(current => current?.key === statsKey && current.values.get(lang) === values ? current
        : { key: statsKey, values: new Map([...(current?.key === statsKey ? current.values : []), [lang, values]]) });
    }).catch(() => {});
    return () => { cancelled = true; };
  }, [nodes, statsKey, lang, active, searching, cache]);
  return useMemo(() => {
    const result = new Map(unique.keys.map((key, index) => [key, translated[index] ?? unique.requests[index].name]));
    if (stats?.key === statsKey) for (const [key, value] of stats.values.get(lang) ?? []) result.set(key, value);
    return result;
  }, [unique, translated, statsKey, lang, stats]);
}

/** One node's heading (`node.name`, or its id when unnamed) and modifier lines for a tooltip or
 * details card: the name by its id-scoped identity, the lines as text. Returns [heading, ...lines];
 * a line a multi-line description covered is dropped (`lib/continuedLines`). */
export async function translatePassiveCard(
  backend: Pick<PobrBackend, 'translateLines' | 'translateNames'>, node: Pick<PassiveNode, 'id' | 'name'>,
  lines: string[], lang: Lang,
): Promise<string[]> {
  const [name, translated] = await Promise.all([
    node.name ? backend.translateNames(passiveNameRequest(node), lang) : Promise.resolve([node.id]),
    lines.length ? backend.translateLines(lines, lang) : Promise.resolve([]),
  ]);
  return [name[0] ?? node.name ?? node.id, ...shownLines(lines, translated)];
}

export function passiveNodeName(node: PassiveNode, names: PassiveNodeNames): string {
  return node.name ? names.get(passiveNameKey(node)) ?? node.name : node.id;
}

/** Keep original names, stable IDs and English modifiers searchable alongside display names. */
export function passiveNodeMatches(node: PassiveNode, query: string, names: PassiveNodeNames): boolean {
  const normalized = query.trim().toLocaleLowerCase();
  if (!normalized) return true;
  const raw = [passiveNodeName(node, names), node.name, node.id, node.skill, names.get(passiveStatsKey(node)), ...(node.stats ?? [])].join('\n');
  const displayed = cleanLineText(raw).toLocaleLowerCase();
  return raw.toLocaleLowerCase().includes(normalized) || displayed.includes(normalized);
}
