import { useEffect, useRef, useState } from 'react';
import { NUMBER_TWEEN_MS, canStartTween, prefersReducedMotion, startTween, tween } from './tween';

/**
 * Eases a displayed number toward its target so a change is noticed rather than missed. Unbounded
 * (null) values, reduced motion and the first value are shown as they are.
 */
export function useAnimatedNumber(target: number | null | undefined): number | null | undefined {
  const [shown, setShown] = useState(target);
  const current = useRef(target);
  useEffect(() => {
    const from = current.current;
    const direct = typeof target !== 'number' || !Number.isFinite(target) || typeof from !== 'number' || !Number.isFinite(from)
      || Math.abs(target - from) < 1e-9 || prefersReducedMotion() || !canStartTween();
    if (direct) { current.current = target; setShown(target); return; }
    const start = performance.now();
    return startTween(now => {
      const value = tween(from, target, now - start);
      current.current = value; setShown(value);
      return now - start < NUMBER_TWEEN_MS;
    });
  }, [target]);
  return shown;
}
