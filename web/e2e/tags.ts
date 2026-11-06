/**
 * Tags that leave a test out of the default run (playwright.config.ts `grepInvert`).
 * `POBR_E2E_ALL=1` runs them as well; `POBR_E2E_ALL=1 … --grep @quarantine` runs only those. The full CI E2E step
 * sets `POBR_E2E_ALL=1` with `--grep-invert @quarantine`, so every @extended cell still runs there.
 */

/** Fails on the current interface for a reason other than a product bug and waits for a rewrite or a decision. */
export const QUARANTINE = { tag: '@quarantine' } as const;

/** An extra cell of a matrix whose representative cells stay in the default run. */
export const EXTENDED = { tag: '@extended' } as const;

/**
 * Locale × width matrices keep English at desktop width and Simplified Chinese at phone width in the
 * default run; the other cells exercise the same code with other strings or another width.
 */
export const matrixCell = (lang: string, width: number) =>
  (lang === 'en-US' && width === 1440) || (lang === 'zh-CN' && width === 390) ? {} : EXTENDED;
