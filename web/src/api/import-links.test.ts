import { readFileSync } from 'node:fs';
import { afterEach, expect, test, vi } from 'vitest';
import { buildCodeTarget, fetchBuildCode } from '../../public/build-import.mjs';
import { buildImportErrorMessage, resolveBuildInput } from './import';

const fixtures: { url: string; target: string }[] = JSON.parse(readFileSync('worker-tests/fixtures/build-links.json', 'utf8'));
const code = readFileSync('worker-tests/fixtures/build-code.txt', 'utf8').trim();
afterEach(() => { vi.unstubAllGlobals(); vi.restoreAllMocks(); });

test.each(fixtures)('resolves $url through the local service and preserves the code', async ({ url, target }) => {
  expect(buildCodeTarget(`${url}?tracking=ignored#fragment`)).toBe(target);
  const fetcher = vi.fn(async () => Response.json({ code }));
  vi.stubGlobal('fetch', fetcher);
  expect(await resolveBuildInput(url)).toBe(code);
  expect(fetcher).toHaveBeenCalledWith('/api/import/build-code', expect.objectContaining({
    method: 'POST', body: JSON.stringify({ url }),
  }));
});

test('known aliases use the same fixed endpoints', () => {
  expect(buildCodeTarget('https://poe2.ninja/pob/Fixture_42')).toBe(fixtures[1].target);
  expect(buildCodeTarget('https://pastebinp.com/raw/Fixture42')).toBe(fixtures[2].target);
  expect(buildCodeTarget('https://rentry.co/fixture42/raw')).toBe(fixtures[3].target);
});

test.each([
  'https://pobb.in.evil/abc', 'https://127.0.0.1/abc', 'https://pobb.in@evil.test/abc',
  'https://user:pass@pobb.in/abc', 'https://pobb.in:8443/abc', 'http://pobb.in/abc',
  'https://pobb.in/a%2Fb', 'https://pobb.in/abc/extra', 'https://pobb.in/',
])('rejects %s before sending a request', async url => {
  const fetcher = vi.fn(); vi.stubGlobal('fetch', fetcher);
  await expect(resolveBuildInput(url)).rejects.toThrow(/^build\.link/);
  expect(fetcher).not.toHaveBeenCalled();
});

test('character profiles receive a specific unsupported message', async () => {
  await expect(resolveBuildInput('https://poe.ninja/poe2/builds/league/character/account/name'))
    .rejects.toThrow('build.linkNinjaProfile');
  for (const lang of ['en-US', 'zh-TW', 'zh-CN'] as const) {
    expect(buildImportErrorMessage('build.linkNinjaProfile', lang)).not.toBe('build.linkNinjaProfile');
  }
});

test('ordinary code and build-file input remain on the decoder path', async () => {
  const fetcher = vi.fn(); vi.stubGlobal('fetch', fetcher);
  expect(await resolveBuildInput(` ${code}\n`)).toBe(code);
  expect(await resolveBuildInput(' {"format":"wegame"} ')).toBe('{"format":"wegame"}');
  expect(fetcher).not.toHaveBeenCalled();
});

test.each([
  [404, { error_code: 'not_found' }, 'build.linkNotFound'],
  [502, { error_code: 'unavailable' }, 'build.linkUnavailable'],
  [504, { error_code: 'timeout' }, 'build.linkTimeout'],
  [502, { error_code: 'unexpected' }, 'build.linkUpstream'],
  [200, { code: '<html>provider error</html>' }, 'build.linkInvalidResponse'],
] as const)('exposes a localized error for HTTP %s: %j', async (status, body, message) => {
  vi.stubGlobal('fetch', vi.fn(async () => Response.json(body, { status })));
  await expect(resolveBuildInput(fixtures[0].url)).rejects.toThrow(message);
});

test('network failure has a stable localized error', async () => {
  vi.stubGlobal('fetch', vi.fn(async () => { throw new TypeError('Failed to fetch'); }));
  await expect(resolveBuildInput(fixtures[0].url)).rejects.toThrow('build.linkNetwork');
});

test('upstream request does not follow redirects or forward browser credentials', async () => {
  const fetcher = vi.fn<typeof fetch>(async (_url, options) => {
    expect(options).toMatchObject({ redirect: 'manual', credentials: 'omit', method: 'GET' });
    expect(new Headers(options?.headers).has('cookie')).toBe(false);
    return new Response(null, { status: 302, headers: { location: 'https://evil.test' } });
  });
  await expect(fetchBuildCode(fixtures[0].url, fetcher)).rejects.toMatchObject({ code: 'upstream' });
  expect(fetcher).toHaveBeenCalledTimes(1);
});

test('timeout cancels a stalled response body', async () => {
  const controller = new AbortController();
  vi.spyOn(AbortSignal, 'timeout').mockReturnValue(controller.signal);
  const cancel = vi.fn();
  const result = fetchBuildCode(fixtures[0].url, async () => new Response(new ReadableStream({
    pull() { controller.abort(); }, cancel,
  })));
  await expect(result).rejects.toMatchObject({ code: 'timeout', status: 504 });
  expect(cancel).toHaveBeenCalled();
});
