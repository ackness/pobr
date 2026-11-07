import { uiStorageKey } from '../../lib/sessionStorage';

export const SIDE_DEFAULT = 27;
const SIDE_MIN = 22;
const SIDE_MAX = 44;
/** The side column never takes more than this share of the panel (the `48%` in pages-tree.css). */
const SIDE_MAX_SHARE = 0.48;
const STEP = 1;
const STEP_LARGE = 4;
const KEY = uiStorageKey('pobr-tree-side-width');

/** The widest the column can be (rem) in a panel of `panelRem`, and the narrowest, which gives way when the panel is small. */
export function sideWidthLimits(panelRem = Infinity): { min: number; max: number } {
  const max = Math.min(SIDE_MAX, Math.floor(SIDE_MAX_SHARE * panelRem * 10) / 10);
  return { min: Math.min(SIDE_MIN, max), max };
}

/** Keep a width inside the limits, to a tenth of a rem so a drag does not store pixel noise. */
export function clampSideWidth(rem: number, panelRem?: number): number {
  const { min, max } = sideWidthLimits(panelRem);
  return Math.round(Math.min(max, Math.max(min, rem)) * 10) / 10;
}

/** The width a key asks for (the column is on the right, so ← widens it), or undefined for any other key. */
export function sideWidthForKey(key: string, shift: boolean, current: number, panelRem?: number): number | undefined {
  const step = shift ? STEP_LARGE : STEP;
  const { min, max } = sideWidthLimits(panelRem);
  switch (key) {
    case 'ArrowLeft': return clampSideWidth(current + step, panelRem);
    case 'ArrowRight': return clampSideWidth(current - step, panelRem);
    case 'Home': return min;
    case 'End': return max;
    default: return undefined;
  }
}

/** A stored value is a convenience: anything that is not a number in range falls back to the default. */
export function parseSideWidth(raw: string | null): number {
  const value = raw === null || raw.trim() === '' ? NaN : Number(raw);
  return Number.isFinite(value) && value >= SIDE_MIN && value <= SIDE_MAX ? value : SIDE_DEFAULT;
}

export function readSideWidth(): number {
  try { return parseSideWidth(localStorage.getItem(KEY)); } catch { return SIDE_DEFAULT; }
}

export function writeSideWidth(rem: number) {
  try { localStorage.setItem(KEY, String(rem)); } catch { /* A remembered width is a convenience only. */ }
}
