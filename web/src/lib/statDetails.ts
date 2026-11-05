import type { Breakdown, CalculateBuildResponse, DetailKind, DetailInput } from '../api/types';
import { bindT, statNameLabel, type Lang } from './i18n';
import { formatStatValue, statFormat } from './statDisplay';

export function detailKind(calc: Pick<CalculateBuildResponse, 'breakdowns' | 'breakdown_descriptors'>, id: string): DetailKind {
  return calc.breakdown_descriptors?.[id] ?? (calc.breakdowns[id] ? 'steps' : 'none');
}

/** Formula ids are replaced at the display boundary, without evaluating formulas in JS. */
export function detailFormula(formula: string, inputs: DetailInput[], lang: Lang): string {
  const ids = new Set(inputs.map(input => input.id));
  // Hide only the outer float-noise rounding; integer and inner rounding are meaningful.
  let display = formula.trim();
  if (display.startsWith('round(')) {
    let depth = 0;
    let comma = -1;
    let end = -1;
    for (let i = 5; i < display.length; i++) {
      if (display[i] === '(') depth++;
      if (display[i] === ')') depth--;
      if (display[i] === ',' && depth === 1) comma = i;
      if (depth === 0) { end = i; break; }
    }
    if (end === display.length - 1 && comma >= 0 && /^\s*9\s*$/.test(display.slice(comma + 1, end))) {
      display = display.slice(6, comma).trim();
    }
  }
  return display.replaceAll('*', '×').replaceAll('/', '÷')
    .replace(/[A-Za-z][A-Za-z0-9]*/g, id => ids.has(id) ? detailInputLabel(id, inputs, lang) : id);
}

export function detailInputLabel(id: string, inputs: DetailInput[], lang: Lang): string {
  const label = statNameLabel(lang, id);
  if (!id.startsWith('Primary') || !inputs.some(input => input.id.startsWith('Secondary'))) return label;
  return bindT(lang)('calcs.mainSkillInput').replace('{input}', label);
}

/** Pass chances are fractions in derived inputs, unlike the percentage outputs. */
export function detailInputValue(input: DetailInput, lang: Lang): string {
  if (/^(Primary|Secondary)(HitChance|AccuracyChance|CriticalChance|CritRollChance)$/.test(input.id)) {
    return formatStatValue(input.value === null ? null : input.value * 100, 'percent', lang);
  }
  if (/^(Primary|Secondary)ExcessChance$/.test(input.id)) return formatStatValue(input.value, 'percent', lang);
  return formatStatValue(input.value, statFormat(input.id), lang);
}

/** Empty is about the returned rows, not just the stat's static descriptor. */
export function detailEmptyKey(kind: DetailKind, value: number | null | undefined, breakdown?: Breakdown | null): 'calcs.zeroNoRows' | 'calcs.noBreakdown' | null {
  if (kind === 'none') return 'calcs.noBreakdown';
  if (breakdown && hasDetailRows(breakdown)) return null;
  return value === 0 && (kind === 'steps' || kind === 'derived') ? 'calcs.zeroNoRows' : 'calcs.noBreakdown';
}

function hasDetailRows(breakdown: Breakdown): boolean {
  return !!(breakdown.base_total || breakdown.inc_total || breakdown.more_total !== 1
    || breakdown.mods.length || breakdown.formula?.steps.length || breakdown.derived?.inputs.length
    || breakdown.derived?.context.length || breakdown.sources?.some(group => group.mods.length)
    || breakdown.damage || breakdown.skill_dot?.length || breakdown.mitigation?.length
    || breakdown.ailment || breakdown.reload || breakdown.slots?.length || breakdown.hands?.some(hasDetailRows));
}

/** Cache only the current calculation revision. Switching revisions discards both
 * results and promises; late completions cannot repopulate the new revision. */
export class StatDetailCache {
  private revision: unknown;
  private entries = new Map<string, Promise<Breakdown | null>>();

  load(revision: unknown, id: string, compute: () => Promise<Breakdown | null>): Promise<Breakdown | null> {
    if (this.revision !== revision) {
      this.revision = revision;
      this.entries.clear();
    }
    const cached = this.entries.get(id);
    if (cached) return cached;
    const pending = compute().catch(error => {
      if (this.entries.get(id) === pending) this.entries.delete(id);
      throw error;
    });
    this.entries.set(id, pending);
    return pending;
  }
}
