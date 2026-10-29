import { flushSync } from 'react-dom';
import { prefersReducedMotion } from './tween';

type Starter = (update: () => void) => unknown;

/** Runs a state change inside a View Transition where the browser has one; otherwise just runs it. */
export function withViewTransition(update: () => void) {
  const start = (document as Document & { startViewTransition?: Starter }).startViewTransition?.bind(document);
  if (!start || prefersReducedMotion()) { update(); return; }
  start(() => { flushSync(update); });
}
