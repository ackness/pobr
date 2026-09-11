import { afterEach, describe, expect, test, vi } from 'vitest';
import { resolveBuildInput } from './import';
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
      expect(options.redirect).toBe('error');
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
  test('worker refuses invalid requests before fetching', async () => {
    const fetcher = vi.fn(); vi.stubGlobal('fetch', fetcher);
    const response = await worker.fetch(new Request('https://pobr.test/api/import/wegame', {
      method: 'POST', body: JSON.stringify({ url: 'https://example.com' }),
    }), {});
    expect(response.status).toBe(400);
    expect(fetcher).not.toHaveBeenCalled();
  });
  test('passes codes through and exposes share failures', async () => {
    expect(await resolveBuildInput('  eNexample  ')).toBe('eNexample');
    vi.stubGlobal('fetch', vi.fn(async () => Response.json({ error: 'Share expired' }, { status: 502 })));
    await expect(resolveBuildInput(url)).rejects.toThrow('Share expired');
  });
});
