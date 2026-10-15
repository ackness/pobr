import type { Breakdown, CalculateBuildResponse, DetailKind, DetailInput } from '../api/types';
import { statNameLabel, type Lang } from './i18n';

export function detailKind(calc: Pick<CalculateBuildResponse, 'breakdowns' | 'breakdown_descriptors'>, id: string): DetailKind {
  return calc.breakdown_descriptors?.[id] ?? (calc.breakdowns[id] ? 'steps' : 'none');
}

/** Formula ids are replaced at the display boundary, without evaluating formulas in JS. */
export function detailFormula(formula: string, inputs: DetailInput[], lang: Lang): string {
  const ids = new Set(inputs.map(input => input.id));
  return formula.replace(/[A-Za-z][A-Za-z0-9]*/g, id => ids.has(id) ? statNameLabel(lang, id) : id);
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
