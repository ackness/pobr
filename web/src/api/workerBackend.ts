import type { PobrBackend } from './backend';
import { createRpcClient, type Endpoint } from './rpc';

/** How long a worker may take to start before it is given up on. */
const START_TIMEOUT_MS = 15_000;

/**
 * The WASM backend running in a dedicated Web Worker, as a {@link PobrBackend}. Resolves
 * once the worker has started; rejects if it cannot (the caller falls back to running
 * the engine on the page's thread).
 */
export async function createWorkerBackend(): Promise<PobrBackend> {
  const worker = new Worker(new URL('./wasmWorker.ts', import.meta.url), { type: 'module' });
  const client = createRpcClient(worker as unknown as Endpoint);
  worker.onerror = (event) => {
    client.fail(new Error(`Engine worker failed: ${event.message || 'it could not be started'}`));
  };
  worker.onmessageerror = () => client.fail(new Error('Engine worker sent a message that could not be read.'));
  let timer: ReturnType<typeof setTimeout> | undefined;
  try {
    await Promise.race([
      client.ready(),
      new Promise<never>((_, reject) => {
        timer = setTimeout(() => reject(new Error('Engine worker did not start.')), START_TIMEOUT_MS);
      }),
    ]);
  } catch (error) {
    worker.terminate();
    throw error;
  } finally {
    clearTimeout(timer);
  }
  return client.backend;
}
