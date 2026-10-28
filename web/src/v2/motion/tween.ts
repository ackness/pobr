export const NUMBER_TWEEN_MS = 280;

export function easeOutCubic(t: number) { return 1 - (1 - t) ** 3; }

/** Value `elapsed` milliseconds into a move from `from` to `to`; exact at the end. */
export function tween(from: number, to: number, elapsed: number, duration = NUMBER_TWEEN_MS) {
  if (duration <= 0 || elapsed >= duration) return to;
  return from + (to - from) * easeOutCubic(Math.max(0, elapsed / duration));
}

export function prefersReducedMotion() {
  try { return matchMedia('(prefers-reduced-motion: reduce)').matches; } catch { return false; }
}

/**
 * One animation frame loop shared by every moving number, so a stage switch that changes a hundred
 * rows costs one callback per frame, and never more than MAX_RUNNING numbers move at once.
 */
export const MAX_RUNNING = 24;
type Tick = (now: number) => boolean;
const running = new Set<Tick>();
let frame = 0;
function loop(now: number) {
  for (const tick of [...running]) if (!tick(now)) running.delete(tick);
  frame = running.size ? requestAnimationFrame(loop) : 0;
}
export function canStartTween() { return running.size < MAX_RUNNING; }
export function startTween(tick: Tick) {
  running.add(tick);
  if (!frame) frame = requestAnimationFrame(loop);
  return () => { running.delete(tick); };
}
