import type { CalculateBuildRequest } from '../api/types';
import { evaluateVariants } from './optimize';
import { basesForSlot, loadTradeCatalog, tradeItemVariant, type TradeCatalog } from './tradeOptimizer';
import { getBackend } from '../api/backend';

/** Only structural clipboard headers are normalized; effect text remains available for diagnostics. */
export function normalizeCopiedItem(input: string): string {
  const rarity: Record<string, string> = { '普通': 'NORMAL', '魔法': 'MAGIC', '稀有': 'RARE', '传奇': 'UNIQUE', '傳奇': 'UNIQUE', '独特': 'UNIQUE', '獨特': 'UNIQUE' };
  const headers: Record<string, string> = { '品质': 'Quality', '品質': 'Quality', '需求': 'Requirements', '需求等级': 'Level', '需求等級': 'Level',
    '等级': 'Level', '等級': 'Level', '物品等级': 'Item Level', '物品等級': 'Item Level', '护甲': 'Armour', '護甲': 'Armour',
    '闪避值': 'Evasion Rating', '閃避值': 'Evasion Rating', '能量护盾': 'Energy Shield', '能量護盾': 'Energy Shield',
    '力量': 'Str', '敏捷': 'Dex', '智慧': 'Int', '智力': 'Int', '插槽': 'Sockets' };
  const lines = input.trim().replace(/\r/g, '').split('\n').map(line => {
    const value = line.trim();
    const match = value.match(/^(稀有度|[\u3400-\u9fff]+)[:：]\s*(.*)$/);
    if (match?.[1] === '稀有度') return `Rarity: ${rarity[match[2]] ?? match[2]}`;
    if (match && headers[match[1]]) return `${headers[match[1]]}: ${match[2]}`;
    return value === '已腐化' ? 'Corrupted' : value;
  });
  const start = lines.findIndex(line => /^Rarity:\s*(NORMAL|MAGIC|RARE|UNIQUE)$/i.test(line));
  if (start < 0 || !lines[start + 1] || input.length > 50000) throw new Error('invalid-item');
  return lines.slice(start).join('\n');
}

interface ComparisonDependencies {
  catalog?: TradeCatalog;
  translate?: (lines: string[]) => Promise<string[]>;
  evaluate?: typeof evaluateVariants;
}

/** Validate the slot before calling the permissive calculation API. */
export async function validateReplacement(request: CalculateBuildRequest, slot: string, text: string,
  catalog: TradeCatalog, translate: (lines: string[]) => Promise<string[]>) {
  if (/^(Unidentified|未鉴定|未鑑定)$/mi.test(text)) throw new Error('unidentified-item');
  if (slot.startsWith('Jewel@') && !(request.allocated_nodes ?? []).includes(Number(slot.slice(6)))) throw new Error('wrong-slot');
  const names = catalog.bases.map(base => base.name);
  const localized = /[\u3400-\u9fff]/.test(text) ? await translate(names) : names;
  const findBase = (raw: string) => {
    const lines = raw.split('\n');
    const start = lines.findIndex(line => /^Rarity:/i.test(line));
    const header = /RARE|UNIQUE/i.test(lines[start] ?? '') ? [lines[start + 2] ?? ''] : [lines[start + 1] ?? ''];
    // Magic clipboard names may include a prefix/suffix around the base name.
    return catalog.bases.map((base, index) => ({ base, aliases: [base.name, localized[index]] }))
      .sort((a, b) => b.base.name.length - a.base.name.length)
      .find(({ aliases }) => aliases.some(name => name && header.some(line => line === name ||
        line.endsWith(` ${name}`) || line.startsWith(`${name} of `) || line.includes(` ${name} of `)) ))?.base;
  };
  const base = findBase(text);
  if (!base) throw new Error('unknown-base');
  if (!basesForSlot(catalog, slot).includes(base)) throw new Error('wrong-slot');
  if (slot === 'weapon1' || slot === 'weapon2') {
    const current = request.items?.find(item => item.slot === slot)?.text;
    const oldBase = current ? findBase(current) : undefined;
    // A different weapon family needs a coordinated skill/offhand change. This
    // preview deliberately operates on one slot and cannot validate that setup.
    if (oldBase && base.category !== oldBase.category) throw new Error('weapon-type');
    const main = findBase(request.items?.find(item => item.slot === 'weapon1')?.text ?? '');
    if (slot === 'weapon2' && ((base.category === 'armour.quiver' && main?.category !== 'weapon.bow') ||
      (main?.category === 'weapon.bow' && base.category !== 'armour.quiver'))) throw new Error('weapon-type');
  }
  const requirements = [...text.matchAll(/(?:^|,|Requirements:)\s*(?:Level|LevelReq):?\s*(\d+)/gmi)].map(match => Number(match[1]));
  if (Math.max(base.level, ...requirements) > (request.character?.level ?? 1)) throw new Error('item-level');
  return base;
}

export async function compareReplacement(request: CalculateBuildRequest, slot: string, text: string,
  signal?: AbortSignal, dependencies: ComparisonDependencies = {}) {
  const normalized = normalizeCopiedItem(text);
  signal?.throwIfAborted();
  const catalog = dependencies.catalog ?? await loadTradeCatalog();
  const translate = dependencies.translate ?? (await getBackend()).translateLines;
  await validateReplacement(request, slot, normalized, catalog, translate);
  signal?.throwIfAborted();
  const evaluate = dependencies.evaluate ?? evaluateVariants;
  const result = await evaluate({ request, signal, variants: [tradeItemVariant(request, slot, normalized)] });
  if (result.aborted || signal?.aborted) throw new DOMException('Cancelled', 'AbortError');
  const row = result.results[0];
  if (!row || row.error) throw new Error(row?.error ?? 'No replacement result');
  return { text: normalized, baseline: result.baseline, stats: row.stats, unsupported: row.unsupported ?? [] };
}
