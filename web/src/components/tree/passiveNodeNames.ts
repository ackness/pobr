import { useMemo } from 'react';
import type { PobrBackend } from '../../api/backend';
import type { PassiveNode } from '../../api/types';
import { useLocalizedNames } from '../../hooks/useLocalizedLines';
import type { Lang } from '../../lib/i18n';
import type { GameNameRequest } from '../../lib/nativeDisplayNames';

/** Display names keyed by {@link passiveNameKey}: one English name can have several translations. */
export type PassiveNodeNames = ReadonlyMap<string, string>;

/** A node's name identity: its PassiveSkills id and its current effective (possibly transformed) name. */
export const passiveNameKey = (node: Pick<PassiveNode, 'id' | 'name'>): string => `${node.id}\n${node.name ?? ''}`;

/** The id-scoped name request for a node, or none when it has no name. */
export function passiveNameRequest(node: Pick<PassiveNode, 'id' | 'name'>): GameNameRequest[] {
  return node.name ? [{ kind: 'passive', id: node.id, name: node.name }] : [];
}

/** Translate names once for the active tree's search and reports, never its calculation data. */
export function usePassiveNodeNames(nodes: readonly PassiveNode[], lang: Lang, active: boolean): PassiveNodeNames {
  const unique = useMemo(() => {
    const byKey = new Map<string, GameNameRequest>();
    if (active) for (const node of nodes) for (const request of passiveNameRequest(node)) byKey.set(passiveNameKey(node), request);
    return { keys: [...byKey.keys()], requests: [...byKey.values()] };
  }, [nodes, active]);
  const translated = useLocalizedNames(unique.requests, lang);
  return useMemo(() => new Map(unique.keys.map((key, index) => [key, translated[index] ?? unique.requests[index].name])),
    [unique, translated]);
}

/** One node's heading (`node.name`, or its id when unnamed) and modifier lines for a tooltip or
 * details card: the name by its id-scoped identity, the lines as text. Returns [heading, ...lines]. */
export async function translatePassiveCard(
  backend: Pick<PobrBackend, 'translateLines' | 'translateNames'>, node: Pick<PassiveNode, 'id' | 'name'>,
  lines: string[], lang: Lang,
): Promise<string[]> {
  const [name, translated] = await Promise.all([
    node.name ? backend.translateNames(passiveNameRequest(node), lang) : Promise.resolve([node.id]),
    lines.length ? backend.translateLines(lines, lang) : Promise.resolve([]),
  ]);
  return [name[0] ?? node.name ?? node.id, ...lines.map((line, index) => translated[index] ?? line)];
}

export function passiveNodeName(node: PassiveNode, names: PassiveNodeNames): string {
  return node.name ? names.get(passiveNameKey(node)) ?? node.name : node.id;
}

/** Keep original names, stable IDs and English modifiers searchable alongside display names. */
export function passiveNodeMatches(node: PassiveNode, query: string, names: PassiveNodeNames): boolean {
  const normalized = query.trim().toLocaleLowerCase();
  if (!normalized) return true;
  const raw = [passiveNodeName(node, names), node.name, node.id, node.skill, ...(node.stats ?? [])].join('\n');
  const displayed = raw
    .replace(/\[([^\]|]*)\|([^\]]*)\]/g, '$2')
    .replace(/\[([^\]]*)\]/g, '$1')
    .toLocaleLowerCase();
  return raw.toLocaleLowerCase().includes(normalized) || displayed.includes(normalized);
}
