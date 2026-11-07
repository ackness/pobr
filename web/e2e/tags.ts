/**
 * Tags that leave a test out of the default run (playwright.config.ts `grepInvert`).
 * `POBR_E2E_ALL=1` runs them as well; `POBR_E2E_ALL=1 … --grep @quarantine` runs only those. The local release run
 * (`mise run e2e release`) sets `POBR_E2E_ALL=1` with `--grep-invert @quarantine`, so every @extended cell still runs there.
 */

/** Fails on the current interface for a reason other than a product bug and waits for a rewrite or a decision. */
export const QUARANTINE = { tag: '@quarantine' } as const;

/** An extra cell of a matrix whose representative cells stay in the default run, or an exhaustive sweep too slow for it. */
export const EXTENDED = { tag: '@extended' } as const;

/**
 * The quick tier (`mise run e2e smoke`, and the base of `mise run e2e affected`): one test per top-level page and
 * core journey with real WASM, in representative cells only. Keep it near 20 tests and under two minutes at four
 * workers; prefer a cheap test of a journey over a slow one, and never tag an @extended cell.
 */
export const SMOKE = { tag: '@smoke' } as const;

/**
 * Locale × width matrices keep English at desktop width and Simplified Chinese at phone width in the
 * default run; the other cells exercise the same code with other strings or another width.
 */
export const matrixCell = (lang: string, width: number) =>
  (lang === 'en-US' && width === 1440) || (lang === 'zh-CN' && width === 390) ? {} : EXTENDED;
