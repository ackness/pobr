import type { BuildSession } from '../hooks/useBuildSession';

export type ImportSummary = Pick<BuildSession, 'character' | 'classNames' | 'calc'> & { failed?: boolean };
export interface ImportReview { scope: string | null; summary?: ImportSummary }

/** A successful import starts with scope=null. Busy includes automatic skill selection and its recalculation. */
export function advanceImportReview(review: ImportReview | null, scope: string | undefined, session: BuildSession): ImportReview | null {
  if (!review || !scope) return review;
  if (review.scope !== null && review.scope !== scope) return null;
  const next = review.scope === null ? { ...review, scope } : review;
  if (next.summary || session.busy || (!session.calc && !session.calcError)) return next;
  return { ...next, summary: { character: session.character, classNames: session.classNames,
    calc: session.calcError ? null : session.calc, failed: !!session.calcError } };
}
