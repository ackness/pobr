import { useEffect, useMemo, useState } from 'react';
import { getBackend } from '../../api/backend';
import { useItemDisplayNames, itemLines, useLocalizedLines, useLocalizedNames } from '../../hooks/useLocalizedLines';
import type { SlotItemInput } from '../../api/types';
import { bindT, slotLabel, type Lang } from '../../lib/i18n';
import { unsupportedLine } from '../../lib/diagnosticText';
import { isContinuation } from '../../lib/continuedLines';
import { ModifierText } from '../calcs/ModifierText';

/** Keep each source's adjacent lines together for whole-description translation. */
export function unincludedGroups(lines: readonly string[], lang: Lang, items: readonly SlotItemInput[] = []) {
  const groups = new Map<string, { identity: string; source: string | null; node: number | null; item?: SlotItemInput; lines: string[] }>();
  for (const raw of lines) {
    const parsed = unsupportedLine(raw, lang);
    const match = /^Tree:(\d+): /.exec(raw);
    const matches = parsed.source ? [] : items.filter(item => itemLines(item.text).some(line => line === parsed.text));
    const item = matches.length === 1 ? matches[0] : undefined;
    const key = match ? `node.${match[1]}` : item ? `item.${item.slot}` : parsed.source ?? '';
    const group = groups.get(key) ?? { identity: key, source: parsed.source, node: match ? Number(match[1]) : null, item, lines: [] };
    group.lines.push(parsed.text); groups.set(key, group);
  }
  return [...groups.values()].map(group => ({ ...group, lines: joinWrappedModifiers(group.lines) }));
}

function UnincludedGroup({ group, lang }: { group: ReturnType<typeof unincludedGroups>[number]; lang: Lang }) {
  const tt = bindT(lang);
  const translated = useLocalizedLines(group.lines, lang);
  const [node, setNode] = useState<{ source: number; id: string; name: string } | null>(null);
  useEffect(() => {
    setNode(null);
    if (group.node === null) return;
    let cancelled = false;
    getBackend().then(backend => backend.loadPassiveTree()).then(nodes => {
      const found = nodes.find(node => node.skill === group.node);
      if (!cancelled && found) setNode({ source: group.node!, id: found.id, name: found.name ?? '' });
    }).catch(() => {});
    return () => { cancelled = true; };
  }, [group.node]);
  const names = useLocalizedNames(node?.source === group.node ? [{ kind: 'passive', id: node.id, name: node.name }] : [], lang);
  const itemNames = useItemDisplayNames([group.item?.text], lang);
  const source = group.item ? `${itemNames[0]} · ${slotLabel(lang, group.item.slot)}` : group.node === null ? group.source ?? (lang === 'en-US' ? 'Source unavailable' : lang === 'zh-TW' ? '未提供來源' : '未提供来源') : names[0] ?? (lang === 'en-US' ? 'Passive tree (name unavailable)' : lang === 'zh-TW' ? '天賦樹（缺少名稱）' : '天赋树（缺少名称）');
  return <li>{source && <h5>{source}{lang !== 'en-US' && /[A-Za-z]{3}/.test(source) && <small className="modifier-fallback">{tt('calcs.untranslated')}</small>}</h5>}<ul className="unincluded-lines">{group.lines.map((line, index) => isContinuation(line, translated[index]) ? null : <li key={index}>
    <ModifierText raw={line} translated={translated[index]} lang={lang} rawDetails={false} />
  </li>)}</ul></li>;
}

export function UnincludedModifiers({ lines, lang, items }: { lines: readonly string[]; lang: Lang; items?: readonly SlotItemInput[] }) {
  const groups = useMemo(() => unincludedGroups(lines, lang, items), [lines, lang, items]);
  return <div className="unincluded-modifiers"><p>{bindT(lang)('calcs.unincludedReason')}</p><ul className="unincluded-sources">{groups.map(group => <UnincludedGroup key={group.identity} group={group} lang={lang} />)}</ul>
    {lines.length > 0 && <details className="unincluded-raw"><summary className="disclosure" data-disclosure-level="inline">{bindT(lang)('calcs.originalModifiers')}</summary><pre>{lines.join('\n')}</pre></details>}
  </div>;
}

/** This complete wrapped game description is known to arrive as two diagnostic rows. */
export function joinWrappedModifiers(lines: readonly string[]): string[] {
  const result: string[] = [];
  for (const line of lines) {
    const previous = result.at(-1);
    const whole = previous && /^((?:Bonded: )?\d+(?:\.\d+)?% chance when collecting an Elemental Infusion to gain an)\n(?:Bonded: )?(additional Elemental Infusion of the same type)$/.exec(`${previous}\n${line}`);
    if (whole) result[result.length - 1] = `${whole[1]}\n${whole[2]}`;
    else result.push(line);
  }
  return result;
}
