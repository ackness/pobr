/// <reference lib="webworker" />
/**
 * Worker entry: the WASM backend, served to the page over `postMessage` (see `rpc.ts`).
 * The page-side counterpart is `workerBackend.ts`.
 */
import { serveBackend, type Endpoint } from './rpc';
import { createWasmBackend } from './wasmBackend';

serveBackend(createWasmBackend(), self as unknown as Endpoint);
