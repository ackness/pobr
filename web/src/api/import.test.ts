import { afterEach, describe, expect, test, vi } from 'vitest';
import { buildFileKind, buildImportErrorMessage, resolveBuildInput } from './import';
// The exact module deployed to Pages is also exercised by the Vite tests.
// @ts-expect-error Plain worker module has no TypeScript declarations.
import worker, { fetchShare, shareKey } from '../../public/_worker.js';

const url = 'https://www.wegame.com.cn/helper/poe2/#/share/SyntheticShareKey_123456';
afterEach(() => vi.unstubAllGlobals());

describe('WeGame import', () => {
  test('validates the host, scheme and fragment', () => {
    expect(shareKey(url)).toBe('SyntheticShareKey_123456');
    for (const value of [url.replace('https:', 'http:'), url.replace('.cn/', '.cn.evil/'),
      url.replace('/#/share/', '/#/?share='), url.replace('www.', 'user@www.'), url + '/extra']) {
      expect(() => shareKey(value)).toThrow();
    }
  });
  test('resolves a share without forwarding credentials or identity', async () => {
    const payloads: Record<string, object> = {
      GetRoleInfo: { role: { level: 59, class_name: 'Deadeye', openid: 'private', name: 'private' } },
      GetEquipments: { equipments: [{ baseType: 'Linen Belt', id: 'private' }] },
      GetTalentTree: { talent_tree: { hashes: [], quest_stats: [] } },
      GetJewels: { jewel_data: '[]' }, GetSkills: { skills: [] },
    };
    const fetcher = vi.fn(async (target: string, options: RequestInit) => {
      expect(target.startsWith('https://www.wegame.com.cn/api/v1/')).toBe(true);
      expect(options.credentials).toBe('omit');
      expect(options.redirect).toBe('manual');
      return Response.json({ result: { error_code: 0 }, ...payloads[target.split('/').pop()!] });
    });
    const result = await fetchShare(url, fetcher);
    expect(fetcher).toHaveBeenCalledTimes(5);
    expect(result.role).toEqual({ level: 59, class_name: 'Deadeye' });
    expect(JSON.stringify(result)).not.toContain('private');
  });
  test('rejects partial upstream failures', async () => {
    await expect(fetchShare(url, async () => Response.json({ result: { error_code: 1 } }))).rejects.toThrow('expired');
  });
  test('rejects upstream redirects without following them', async () => {
    const fetcher = vi.fn(async (_target: string, options: RequestInit) => {
      expect(options.redirect).toBe('manual');
      return new Response(null, { status: 302, headers: { location: 'https://example.com' } });
    });
    await expect(fetchShare(url, fetcher)).rejects.toThrow('HTTP 302');
    expect(fetcher).toHaveBeenCalledTimes(5);
    expect(fetcher.mock.calls.every(([target]) => target.startsWith('https://www.wegame.com.cn/api/v1/'))).toBe(true);
  });
  test('worker refuses invalid requests before fetching', async () => {
    const fetcher = vi.fn(); vi.stubGlobal('fetch', fetcher);
    const response = await worker.fetch(new Request('https://pobr.test/api/import/wegame', {
      method: 'POST', body: JSON.stringify({ url: 'https://example.com' }),
    }), {});
    expect(response.status).toBe(400);
    expect((await response.json()).error_code).toBe('invalid_url');
    expect(fetcher).not.toHaveBeenCalled();
  });
  test('worker names the failure with a code the client localizes', async () => {
    vi.stubGlobal('fetch', vi.fn(async () => Response.json({ result: { error_code: 1 } })));
    const response = await worker.fetch(new Request('https://pobr.test/api/import/wegame', { method: 'POST', body: JSON.stringify({ url }) }), {});
    expect(response.status).toBe(502);
    expect(await response.json()).toMatchObject({ error_code: 'not_found', error: expect.stringContaining('expired') });
  });
  test('passes codes through and exposes share failures', async () => {
    expect(await resolveBuildInput('  eNexample  ')).toBe('eNexample');
    vi.stubGlobal('fetch', vi.fn(async () => Response.json({ error: 'Share expired' }, { status: 502 })));
    await expect(resolveBuildInput(url)).rejects.toThrow('Share expired');
  });
  test('turns every failure of the import service into a message in the selected language', async () => {
    const message = async () => resolveBuildInput(url).then(() => '', (error: Error) => error.message);
    // A stable code from the worker is localized, not echoed.
    vi.stubGlobal('fetch', vi.fn(async () => Response.json({ error: 'WeGame GetRoleInfo failed; the share may have expired.', error_code: 'not_found' }, { status: 502 })));
    expect(await message()).toBe('build.linkNotFound');
    // A host without the service answers with something that is not JSON.
    vi.stubGlobal('fetch', vi.fn(async () => new Response('<html>Not found</html>', { status: 404 })));
    expect(await message()).toBe('build.wegameUnavailable');
    // The browser's own failures are not shown raw either.
    vi.stubGlobal('fetch', vi.fn(async () => { throw new TypeError('Failed to fetch'); }));
    expect(await message()).toBe('build.linkNetwork');
    for (const lang of ['en-US', 'zh-CN', 'zh-TW'] as const) {
      const text = buildImportErrorMessage('build.wegameUnavailable', lang);
      expect(text).not.toBe('build.wegameUnavailable');
      if (lang !== 'en-US') expect(text).toMatch(/[\u4e00-\u9fff]/);
    }
  });
});


describe('build-file format ownership', () => {
  test.each([
    { version: 1, state: { flasks: {} }, notes: '' },
    { version: 2, state: null },
    { format: 'pobr-build', version: 2, build: null },
    { version: 1, notes: 'Missing state' },
  ])('keeps malformed PoBR saves on the session validation path: %j', input => {
    expect(buildFileKind(JSON.stringify(input))).toBe('session');
  });
  test.each([null, {}, false])('recognizes even a malformed workspace for explicit backup handling: %j', workspace => {
    expect(buildFileKind(JSON.stringify({ version: 1, workspace }))).toBe('workspace');
  });
  test.each([
    'eNexample',
    'https://www.wegame.com.cn/helper/poe2/#/share/SyntheticShareKey_123456',
    JSON.stringify({ format: 'wegame', version: 1, role: { level: 80, class_name: 'Witch' } }),
    JSON.stringify({ name: 'Character Lv.80', ascendancy: 'Witch1', passives: [], items: [], skills: [] }),
  ])('preserves external build import handling: %s', input => {
    expect(buildFileKind(input)).toBe('external');
  });
});
