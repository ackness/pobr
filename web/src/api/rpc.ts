/**
 * Runs a {@link PobrBackend} on the other side of a message channel (a Web Worker in
 * the app, a `MessagePort` in tests).
 *
 * The engine does its work synchronously: one recalculation takes tens of milliseconds
 * and a heat map or an optimization takes seconds. On the page's own thread that is a
 * frozen interface; in a worker the page only posts a request and receives a result.
 * Calls are answered in the order they arrive, because the engine has one state.
 */
import type { PobrBackend } from './backend';

/** What a `Worker`, a worker's global scope and a `MessagePort` have in common. */
export interface Endpoint {
  postMessage(message: unknown): void;
  onmessage: ((event: MessageEvent) => void) | null;
}

interface Call {
  id: number;
  method: string;
  args: unknown[];
}

type Reply =
  | { id: number; kind: 'progress'; message: string }
  | { id: number; kind: 'value'; value: unknown; dataVersion?: string }
  | { id: number; kind: 'error'; name: string; message: string };

/** Answered by the server itself, before the backend exists: has the worker started? */
const READY = '__ready';

/**
 * Serves `backend` on `endpoint`. The backend may still be loading; calls wait for it,
 * and a backend that fails to load fails each call with that error.
 */
export function serveBackend(backend: PobrBackend | Promise<PobrBackend>, endpoint: Endpoint): void {
  const loading = Promise.resolve(backend);
  // A rejection is reported through the calls that wait on it, not as an unhandled one.
  loading.catch(() => undefined);
  endpoint.onmessage = (event: MessageEvent) => {
    const { id, method, args } = event.data as Call;
    void (async () => {
      try {
        if (method === READY) {
          endpoint.postMessage({ id, kind: 'value', value: true } satisfies Reply);
          return;
        }
        const target = await loading;
        const operation = (target as unknown as Record<string, unknown>)[method];
        if (typeof operation !== 'function') throw new Error(`Unknown backend method: ${method}`);
        // A callback cannot cross the channel; progress travels as messages instead.
        const callArgs = method === 'init'
          ? [(message: string) => endpoint.postMessage({ id, kind: 'progress', message } satisfies Reply)]
          : args;
        const value = await (operation as (...a: unknown[]) => unknown).apply(target, callArgs);
        endpoint.postMessage({ id, kind: 'value', value, dataVersion: target.dataVersion } satisfies Reply);
      } catch (error) {
        const failure = error instanceof Error ? error : new Error(String(error));
        // `message` is kept as it is: API errors are JSON in it (see `parseApiError`).
        endpoint.postMessage({ id, kind: 'error', name: failure.name, message: failure.message } satisfies Reply);
      }
    })();
  };
}

export interface RpcClient {
  backend: PobrBackend;
  /** Resolves once the other side has started serving. */
  ready(): Promise<void>;
  /** Rejects every call still waiting (the worker died or was given up on). */
  fail(error: Error): void;
}

export function createRpcClient(endpoint: Endpoint): RpcClient {
  let nextId = 1;
  let dataVersion: string | undefined;
  const waiting = new Map<number, {
    resolve: (value: unknown) => void;
    reject: (error: Error) => void;
    onProgress?: (message: string) => void;
  }>();
  endpoint.onmessage = (event: MessageEvent) => {
    const reply = event.data as Reply;
    const call = waiting.get(reply.id);
    if (!call) return;
    if (reply.kind === 'progress') {
      call.onProgress?.(reply.message);
      return;
    }
    waiting.delete(reply.id);
    if (reply.kind === 'value') {
      if (reply.dataVersion !== undefined) dataVersion = reply.dataVersion;
      call.resolve(reply.value);
    } else {
      const error = new Error(reply.message);
      error.name = reply.name;
      call.reject(error);
    }
  };
  const send = (method: string, args: unknown[], onProgress?: (message: string) => void) =>
    new Promise<unknown>((resolve, reject) => {
      const id = nextId++;
      waiting.set(id, { resolve, reject, onProgress });
      try {
        endpoint.postMessage({ id, method, args } satisfies Call);
      } catch (error) {
        // e.g. an argument that cannot be cloned: only this call fails.
        waiting.delete(id);
        reject(error instanceof Error ? error : new Error(String(error)));
      }
    });
  const backend = new Proxy({} as PobrBackend, {
    get(_target, property) {
      // Not a thenable: the backend is returned from async functions and awaited.
      if (typeof property === 'symbol' || property === 'then') return undefined;
      if (property === 'dataVersion') return dataVersion;
      // Synchronous in the interface and used by nothing; there is no engine here to ask.
      if (property === 'translate') return (_lang: string, key: string) => key;
      if (property === 'init') return (onProgress?: (message: string) => void) => send('init', [], onProgress).then(() => undefined);
      return (...args: unknown[]) => send(property, args);
    },
  });
  return {
    backend,
    ready: () => send(READY, []).then(() => undefined),
    fail(error) {
      for (const call of waiting.values()) call.reject(error);
      waiting.clear();
    },
  };
}
