import { flushSync } from 'react-dom';
import { prefersReducedMotion } from './tween';

type Starter = (update: () => void) => Pick<ViewTransition, 'ready'>;

/** Runs a state change inside a View Transition where the browser has one; otherwise just runs it. */
export function withViewTransition(update: () => void) {
  const start = (document as Document & { startViewTransition?: Starter }).startViewTransition?.bind(document);
  if (!start || prefersReducedMotion()) { update(); return; }
  const transition = start(() => { flushSync(update); });
  // A second navigation skips the first animation, but both updates still run.
  void transition.ready.catch((error: unknown) => {
    if (!(error instanceof DOMException && error.name === 'AbortError')) reportError(error);
  });
}
