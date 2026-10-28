import { afterEach, describe, expect, it } from 'vitest';
import type { PobrBackend } from './backend';
import { parseApiError } from './error';
import { createRpcClient, serveBackend, type Endpoint } from './rpc';

const channels: MessageChannel[] = [];
afterEach(() => {
  for (const channel of channels.splice(0)) {
    channel.port1.close();
    channel.port2.close();
  }
});

/** A client and a server joined by a real message channel, the way a worker joins them. */
function connect(backend: Partial<PobrBackend> | Promise<PobrBackend>) {
  const channel = new MessageChannel();
  channels.push(channel);
  serveBackend(backend as PobrBackend | Promise<PobrBackend>, channel.port1 as unknown as Endpoint);
  return createRpcClient(channel.port2 as unknown as Endpoint);
}

describe('engine RPC', () => {
  it('forwards arguments and returns values, and is not mistaken for a thenable', async () => {
    const client = connect({
      async decodeBuild(code) {
        return { code, upper: code.toUpperCase() } as never;
      },
      async nodePower(request, stat, depth, nodes) {
        return { echoed: [request, stat, depth, nodes] } as never;
      },
    });
    expect(await client.backend.decodeBuild('abc')).toEqual({ code: 'abc', upper: 'ABC' });
    const request = { pob_code: 'x', allocated_nodes: [1, 2] };
    expect(await client.backend.nodePower(request, 'TotalDPS', 5, [7])).toEqual({
      echoed: [request, 'TotalDPS', 5, [7]],
    });
    // Awaiting the backend itself (as `async` factories do) must not hang or unwrap it.
    const awaited = await Promise.resolve(client.backend);
    expect(awaited).toBe(client.backend);
  });

  it('keeps an API error as it is, so parseApiError reads it on the page', async () => {
    const apiError = JSON.stringify({ code: 'bad_request', message: 'unknown breakdown: Nope', slot: 'ring1' });
    const client = connect({
      async calculateBuild() {
        throw new Error(apiError);
      },
    });
    const failure = await client.backend.calculateBuild({}).then(() => undefined, (error: unknown) => error);
    expect(failure).toBeInstanceOf(Error);
    expect(parseApiError(failure)).toEqual({ code: 'bad_request', message: 'unknown breakdown: Nope', slot: 'ring1' });
  });

  it('carries init progress as messages and the loaded data version with the result', async () => {
    let version: string | undefined;
    const client = connect({
      get dataVersion() {
        return version;
      },
      async init(onProgress) {
        onProgress?.('loading');
        version = '0.5';
        onProgress?.('ready');
      },
    });
    expect(client.backend.dataVersion).toBeUndefined();
    const seen: string[] = [];
    await client.backend.init((message) => seen.push(message));
    expect(seen).toEqual(['loading', 'ready']);
    expect(client.backend.dataVersion).toBe('0.5');
  });

  it('answers calls in the order they arrive, each with its own result', async () => {
    const order: string[] = [];
    const client = connect({
      async normalizeItemDraft(text) {
        order.push(text);
        return text;
      },
    });
    const results = await Promise.all(['a', 'b', 'c'].map((text) => client.backend.normalizeItemDraft(text)));
    expect(results).toEqual(['a', 'b', 'c']);
    expect(order).toEqual(['a', 'b', 'c']);
  });

  it('fails an unknown method and an argument that cannot be cloned, one call at a time', async () => {
    const client = connect({ async normalizeItemDraft(text) { return text; } });
    await expect(client.backend.gemCatalog()).rejects.toThrow('Unknown backend method: gemCatalog');
    await expect(
      client.backend.normalizeItemDraft((() => 1) as unknown as string),
    ).rejects.toThrow();
    expect(await client.backend.normalizeItemDraft('still works')).toBe('still works');
  });

  it('reports a backend that failed to load through its calls, and still says the worker is up', async () => {
    const client = connect(Promise.reject(new Error('wasm module not built')));
    await expect(client.ready()).resolves.toBeUndefined();
    await expect(client.backend.decodeBuild('x')).rejects.toThrow('wasm module not built');
  });

  it('fails everything still waiting when the worker is given up on', async () => {
    const client = connect(new Promise<PobrBackend>(() => undefined));
    const stuck = client.backend.decodeBuild('x');
    client.fail(new Error('Engine worker failed'));
    await expect(stuck).rejects.toThrow('Engine worker failed');
  });

  it('translates nothing itself: the key comes back', () => {
    const client = connect({});
    expect(client.backend.translate('zh-CN', 'some.key')).toBe('some.key');
  });
});
