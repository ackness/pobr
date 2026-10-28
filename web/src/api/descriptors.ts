import type { PobrBackend } from './backend';

/**
 * Gives every calculation response the detail-kind table.
 *
 * The table depends only on the engine build and is about a third of a response, so the
 * engine leaves it out and offers it once (`breakdownDescriptors`). It is fetched here,
 * on the page's side of the worker boundary, and the one object is put into each
 * response: callers still see a complete response, and the worker does not copy 700
 * entries across the channel on every calculation.
 */
export async function withBreakdownDescriptors(backend: PobrBackend): Promise<PobrBackend> {
  const table = await backend.breakdownDescriptors();
  return new Proxy(backend, {
    get(target, property, receiver) {
      if (property !== 'calculateBuild') return Reflect.get(target, property, receiver);
      return async (request: Parameters<PobrBackend['calculateBuild']>[0]) => {
        const response = await target.calculateBuild(request);
        // A request that asked for the table inline already has it.
        response.breakdown_descriptors ??= table;
        return response;
      };
    },
  });
}
