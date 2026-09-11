import type { CalculateBuildRequest, ItemLineJson, VariantInput } from '../api/types';
import { evaluateVariants } from './optimize';
import { basesForSlot, loadTradeCatalog, tradeItemVariant, type TradeBase, type TradeCatalog } from './tradeOptimizer';
import { getBackend } from '../api/backend';

/** Only structural clipboard headers are normalized; effect text remains available for diagnostics. */
export function normalizeCopiedItem(input: string): string {
  if (input.length > 50000) throw new Error('invalid-item');
  const rarity: Record<string, string> = { '普通': 'NORMAL', '魔法': 'MAGIC', '稀有': 'RARE', '传奇': 'UNIQUE', '傳奇': 'UNIQUE', '独特': 'UNIQUE', '獨特': 'UNIQUE' };
  const headers: Record<string, string> = { '品质': 'Quality', '品質': 'Quality', '需求': 'Requirements', '需求等级': 'Level', '需求等級': 'Level',
    '等级': 'Level', '等級': 'Level', '物品等级': 'Item Level', '物品等級': 'Item Level', '护甲': 'Armour', '護甲': 'Armour',
    '闪避值': 'Evasion Rating', '閃避值': 'Evasion Rating', '能量护盾': 'Energy Shield', '能量護盾': 'Energy Shield',
    '力量': 'Str', '敏捷': 'Dex', '智慧': 'Int', '智力': 'Int', '插槽': 'Sockets' };
  const lines = input.trim().replace(/\r/g, '').split('\n').map(line => {
    const value = line.trim();
    const match = value.match(/^(稀有度|[\u3400-\u9fff]+)[:：]\s*(.*)$/);
    if (match?.[1] === '稀有度') return `Rarity: ${rarity[match[2]] ?? match[2]}`;
    if (match && headers[match[1]]) return `${headers[match[1]]}: ${match[2]}`.replace(/^(Requirements:)\s*(等级|等級)\s*[:：]?\s*/, '$1 Level: ').replace(/，/g, ',');
    return value === '已腐化' ? 'Corrupted' : value;
  });
  const start = lines.findIndex(line => /^Rarity:\s*(NORMAL|MAGIC|RARE|UNIQUE)$/i.test(line));
  if (start < 0 || !lines[start + 1] || lines.filter(line => /^Rarity:/i.test(line)).length !== 1) throw new Error('invalid-item');
  return lines.slice(start).join('\n');
}

interface ComparisonDependencies {
  catalog?: TradeCatalog;
  translate?: (lines: string[]) => Promise<string[]>;
  evaluate?: typeof evaluateVariants;
  classify?: (text: string) => Promise<ItemLineJson[]>;
  jewelSocketNodes?: readonly number[];
}

async function replacementContext(request: CalculateBuildRequest, text: string,
  catalog: TradeCatalog, translate: (lines: string[]) => Promise<string[]>) {
  if (/^(Unidentified|未鉴定|未鑑定)$/mi.test(text)) throw new Error('unidentified-item');
  const names = catalog.bases.map(base => base.name);
  const needsTranslation = /[\u3400-\u9fff]/.test(text) || request.items?.some(item => /[\u3400-\u9fff]/.test(item.text));
  const localized = needsTranslation ? await translate(names) : names;
  const entries = catalog.bases.map((base, index) => ({ base, aliases: [base.name, localized[index]] }))
    .sort((a, b) => b.base.name.length - a.base.name.length);
  const findBase = (raw: string) => {
    const lines = raw.split('\n').map(line => line.trim());
    const start = lines.findIndex(line => /^Rarity:/i.test(line));
    if (start < 0) return;
    const magic = /MAGIC/i.test(lines[start]);
    const header = /RARE|UNIQUE/i.test(lines[start]) ? [lines[start + 2] ?? ''] : [lines[start + 1] ?? ''];
    if (magic && lines[start + 3] === '--------') header.push(lines[start + 2] ?? '');
    // Magic clipboard names may include a prefix/suffix around the base name.
    return entries.find(({ aliases }) => aliases.some(name => name && header.some(line => line === name || (magic && (
      line.endsWith(` ${name}`) || line.startsWith(`${name} of `) || line.includes(` ${name} of `) ||
      (/[\u3400-\u9fff]/.test(name) && line.includes(name)))))))?.base;
  };
  const base = findBase(text);
  if (!base) throw new Error('unknown-base');
  const requirements = [...text.matchAll(/(?:^|,|Requirements:)\s*(?:Requires\s+)?(?:Level|LevelReq):?\s*(\d+)/gmi)].map(match => Number(match[1]));
  const requiredLevel = Math.max(base.level, ...requirements);
  if (requiredLevel > (request.character?.level ?? 1)) throw new Error('item-level');
  return { base, requiredLevel, findBase };
}

function validateSlot(request: CalculateBuildRequest, slot: string, base: TradeBase,
  catalog: TradeCatalog, findBase: (text: string) => TradeBase | undefined) {
  if (slot.startsWith('Jewel@') && !(request.allocated_nodes ?? []).includes(Number(slot.slice(6)))) throw new Error('wrong-slot');
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
}

/** Validate the slot before calling the permissive calculation API. */
export async function validateReplacement(request: CalculateBuildRequest, slot: string, text: string,
  catalog: TradeCatalog, translate: (lines: string[]) => Promise<string[]>) {
  const { base, findBase } = await replacementContext(request, text, catalog, translate);
  validateSlot(request, slot, base, catalog, findBase);
  return base;
}

export interface ReplacementPosition {
  slot: string;
  variant: VariantInput;
  stats: Record<string, number>;
  unsupported: string[];
}

export interface ReplacementReport {
  text: string;
  base: TradeBase;
  requiredLevel: number;
  lines: ItemLineJson[];
  baseline: Record<string, number>;
  positions: ReplacementPosition[];
  rejected: { slot: string; reason: string }[];
}

/** Classifier output is authoritative for modifier kind; properties are never counted as affixes. */
export function replacementAffixes(lines: ItemLineJson[]): ItemLineJson[] {
  return lines.filter(line => ['implicit', 'explicit', 'enchant', 'rune'].includes(line.kind)
    && line.text.trim() && !/^-{3,}$|^(?:Rarity|Item Class|Item Level|Requirements|Level|LevelReq|Str|Dex|Int|Quality|Armour|Evasion(?: Rating)?|Energy Shield|Spirit|Ward|Sockets):/i.test(line.text.trim()));
}

/** Discover all relevant positions once and calculate each complete replacement in one batch. */
export async function compareReplacementPositions(request: CalculateBuildRequest, text: string,
  signal?: AbortSignal, dependencies: ComparisonDependencies = {}): Promise<ReplacementReport> {
  const normalized = normalizeCopiedItem(text);
  signal?.throwIfAborted();
  const catalog = dependencies.catalog ?? await loadTradeCatalog();
  const backend = !dependencies.translate || !dependencies.classify ? await getBackend() : undefined;
  const context = await replacementContext(request, normalized, catalog, dependencies.translate ?? backend!.translateLines);
  signal?.throwIfAborted();
  const allocated = new Set(request.allocated_nodes ?? []);
  const sockets = new Set([...(dependencies.jewelSocketNodes ?? []), ...(request.jewels ?? []).map(jewel => jewel.socket_node)]);
  const slots = [...new Set([...(request.items ?? []).map(item => item.slot), ...(request.flasks ?? []).map(item => item.slot),
    ...[...sockets].filter(node => allocated.has(node)).sort((a, b) => a - b).map(node => `Jewel@${node}`)])]
    .filter(slot => basesForSlot(catalog, slot).includes(context.base));
  const rejected: ReplacementReport['rejected'] = [];
  const compatible = slots.filter(slot => {
    try { validateSlot(request, slot, context.base, catalog, context.findBase); return true; }
    catch (error) { rejected.push({ slot, reason: error instanceof Error ? error.message : 'wrong-slot' }); return false; }
  });
  if (!compatible.length) throw new Error(rejected[0]?.reason ?? 'no-compatible-slot');
  const lines = await (dependencies.classify ?? backend!.classifyItemLines)(normalized);
  signal?.throwIfAborted();
  const variants = compatible.map(slot => tradeItemVariant(request, slot, normalized));
  const result = await (dependencies.evaluate ?? evaluateVariants)({ request, signal, variants });
  if (result.aborted || signal?.aborted) throw new DOMException('Cancelled', 'AbortError');
  const positions = result.results.flatMap(row => {
    const slot = compatible[row.index];
    if (!slot) return [];
    if (row.error) { rejected.push({ slot, reason: row.error }); return []; }
    return [{ slot, variant: variants[row.index], stats: row.stats, unsupported: row.unsupported ?? [] }];
  });
  if (!positions.length) throw new Error(rejected[0]?.reason ?? 'No replacement result');
  return { text: normalized, base: context.base, requiredLevel: context.requiredLevel, lines, baseline: result.baseline, positions, rejected };
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
