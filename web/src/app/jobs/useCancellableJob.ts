import { useEffect, useState, useSyncExternalStore } from 'react';
import { JobController } from './job';

/** A cancellable job tied to the component: leaving the page abandons it. */
export function useCancellableJob<T>() {
  const [controller] = useState(() => new JobController<T>());
  const state = useSyncExternalStore(controller.subscribe, controller.get);
  useEffect(() => () => controller.cancel(), [controller]);
  return { ...state, running: state.status === 'running', start: controller.start, cancel: controller.cancel, reset: controller.reset };
}
