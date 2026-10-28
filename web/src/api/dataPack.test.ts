import { createHash } from 'node:crypto';
import { describe, expect, it } from 'vitest';
import { dataUrl, loadDataPack, type DataManifest } from './dataPack';

const sha = (bytes: Uint8Array) => createHash('sha256').update(bytes).digest('hex');
const digest = async (bytes: Uint8Array) => sha(bytes);
const bytes = (text: string) => new TextEncoder().encode(text);

/** A pack of files with a schema-3 snapshot manifest listing all of them but itself. */
function pack(contents: Record<string, string>, extra: Record<string, unknown> = {}) {
  const files = new Map(Object.entries(contents).map(([rel, text]) => [rel, bytes(text)]));
  const inventory = Object.fromEntries([...files].map(([rel, data]) => [rel, sha(data)]));
  files.set('manifest.json', bytes(JSON.stringify({ schema_version: 3, files: inventory, ...extra })));
  const digests = Object.fromEntries([...files].map(([rel, data]) => [rel, sha(data)]));
  const manifest: DataManifest = { version: '1.2', files: [...files.keys()].sort(), digests };
  const requested: string[] = [];
  const fetchBytes = async (url: string) => {
    requested.push(url);
    const rel = new URL(url, 'https://x.test').pathname.replace('/data/1.2/', '');
    const data = files.get(rel);
    if (!data) throw new Error(`fetch ${url}: 404`);
    return data;
  };
  return { files, manifest, requested, fetchBytes };
}

describe('dataUrl', () => {
  it('names a file by its digest, and only by a whole one', () => {
    const digests = { 'base/a.json': 'ab'.repeat(32), 'base/b.json': 'xyz', 'base/c.json': 'AB'.repeat(32) };
    expect(dataUrl({ version: '1.2', digests }, 'base/a.json')).toBe(`/data/1.2/base/a.json?h=${'ab'.repeat(8)}`);
    // No digest, or one that is not a SHA-256 in lower-case hex: the plain URL, which the host revalidates.
    expect(dataUrl({ version: '1.2', digests }, 'base/b.json')).toBe('/data/1.2/base/b.json');
    expect(dataUrl({ version: '1.2', digests }, 'base/c.json')).toBe('/data/1.2/base/c.json');
    expect(dataUrl({ version: '1.2' }, 'base/a.json')).toBe('/data/1.2/base/a.json');
  });
});

describe('loadDataPack', () => {
  it('compares each file as it arrives, stages it, and keeps only what the page reads again', async () => {
    const { manifest, fetchBytes, requested, files } = pack({ 'base/a.json': '[1]', 'base/b.json': '[2]', 'i18n/c.json': '{}' });
    const staged: string[] = [];
    const progress: number[] = [];
    const loaded = await loadDataPack({
      manifest, fetchBytes, digest,
      stage: (rel, data) => { expect(data).toBe(files.get(rel)); staged.push(rel); },
      keep: (rel) => rel === 'i18n/c.json',
      onProgress: (done, total) => { expect(total).toBe(manifest.files.length); progress.push(done); },
    });
    expect(loaded.verified).toBe(true);
    expect([...loaded.kept.keys()]).toEqual(['i18n/c.json']);
    expect(staged.sort()).toEqual(manifest.files);
    expect(progress).toEqual([1, 2, 3, 4]);
    // The snapshot manifest is asked for first, and every file by its digest.
    expect(requested[0]).toMatch(/\/manifest\.json\?h=[0-9a-f]{16}$/);
    expect(requested.every((url) => /\?h=[0-9a-f]{16}$/.test(url))).toBe(true);
  });

  it('rejects a file that does not match its digest, with the engine\'s words', async () => {
    const { manifest, fetchBytes } = pack({ 'base/a.json': '[1]', 'base/b.json': '[2]' });
    const tampered = async (url: string) => (url.includes('base/b.json') ? bytes('[3]') : fetchBytes(url));
    await expect(loadDataPack({ manifest, fetchBytes: tampered, digest, stage: () => {}, keep: () => false }))
      .rejects.toThrow('SHA-256 mismatch: base/b.json');
  });

  it('rejects a pack that lacks a file the manifest lists', async () => {
    const { manifest, fetchBytes } = pack({ 'base/a.json': '[1]', 'base/b.json': '[2]' });
    const missing = { ...manifest, files: manifest.files.filter((rel) => rel !== 'base/b.json') };
    await expect(loadDataPack({ manifest: missing, fetchBytes, digest, stage: () => {}, keep: () => false }))
      .rejects.toThrow('file not in memory data: base/b.json');
  });

  it('leaves the comparison to the engine where there is no digest to make it with', async () => {
    const { manifest, fetchBytes } = pack({ 'base/a.json': '[1]' });
    const staged: string[] = [];
    const loaded = await loadDataPack({ manifest, fetchBytes, digest: null, stage: (rel) => staged.push(rel), keep: () => false });
    expect(loaded.verified).toBe(false);
    expect(staged.sort()).toEqual(manifest.files);
  });

  it('leaves it to the engine for an older schema, which lists no digests', async () => {
    const { manifest, fetchBytes } = pack({ 'base/a.json': '[1]' }, { schema_version: 2 });
    const loaded = await loadDataPack({ manifest, fetchBytes, digest, stage: () => {}, keep: () => false });
    expect(loaded.verified).toBe(false);
  });

  it('works from a manifest without digests, with plain URLs', async () => {
    const { manifest, fetchBytes, requested } = pack({ 'base/a.json': '[1]' });
    const plain: DataManifest = { version: manifest.version, files: manifest.files };
    const loaded = await loadDataPack({ manifest: plain, fetchBytes, digest, stage: () => {}, keep: () => false });
    expect(loaded.verified).toBe(true);
    expect(requested.every((url) => !url.includes('?'))).toBe(true);
  });

  it('never has more files in flight than it was allowed', async () => {
    const contents = Object.fromEntries(Array.from({ length: 20 }, (_, i) => [`base/f${i}.json`, `[${i}]`]));
    const { manifest, fetchBytes } = pack(contents);
    let inFlight = 0;
    let peak = 0;
    const slow = async (url: string) => {
      inFlight += 1;
      peak = Math.max(peak, inFlight);
      await new Promise((resolve) => setTimeout(resolve, 1));
      inFlight -= 1;
      return fetchBytes(url);
    };
    await loadDataPack({ manifest, fetchBytes: slow, digest, stage: () => {}, keep: () => false, concurrency: 3 });
    expect(peak).toBeLessThanOrEqual(3);
  });
});
