import { useMemo } from 'react';
import type { PassiveNode } from '../../api/types';
import { useLocalizedLines } from '../../hooks/useLocalizedLines';
import type { Lang } from '../../lib/i18n';

export type PassiveNodeNames = ReadonlyMap<string, string>;

/** Translate names once for the active tree's search and reports, never its calculation data. */
export function usePassiveNodeNames(nodes: readonly PassiveNode[], lang: Lang, active: boolean): PassiveNodeNames {
  const names = useMemo(() => active
    ? [...new Set(nodes.flatMap(node => node.name ? [node.name] : []))]
    : [], [nodes, active]);
  const translated = useLocalizedLines(names, lang);
  return useMemo(() => new Map(names.map((name, index) => [name, translated[index] ?? name])), [names, translated]);
}

export function passiveNodeName(node: PassiveNode, names: PassiveNodeNames): string {
  return node.name ? names.get(node.name) ?? node.name : node.id;
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
