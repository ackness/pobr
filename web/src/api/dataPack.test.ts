import { createHash } from 'node:crypto';
import { execFileSync } from 'node:child_process';
import { mkdtempSync, readFileSync, realpathSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';
import { dataUrl, loadDataPack, type DataManifest } from './dataPack';
import { canonical, fetchReleaseFile, validateRelease, type RuntimeRelease } from './runtimeRelease';
// @ts-ignore -- Node test helper, excluded from the application module graph.
import { runtimePackFixture } from '../../scripts/runtime-pack-fixture.mjs';
// @ts-ignore -- Node sync entry point, exercised without bundling the application.
import { syncData } from '../../scripts/sync-data.mjs';

const sha = (bytes: Uint8Array) => createHash('sha256').update(bytes).digest('hex');
const digest = async (bytes: Uint8Array) => sha(bytes);
const bytes = (text: string) => new TextEncoder().encode(text);
const repoRoot = fileURLToPath(new URL('../../../', import.meta.url));

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
    // A file is found at its plain path or under the path that names its content.
    const path = new URL(url, 'https://x.test').pathname;
    const rel = path.startsWith('/data/h/') ? path.replace(/^\/data\/h\/[0-9a-f]{16}\//, '') : path.replace('/data/1.2/', '');
    const data = files.get(rel);
    if (!data) throw new Error(`fetch ${url}: 404`);
    return data;
  };
  return { files, manifest, requested, fetchBytes };
}

describe('dataUrl', () => {
  it('names a file by its content, and only by a whole digest', () => {
    const digests = { 'base/a.json': 'ab'.repeat(32), 'base/b.json': 'xyz', 'base/c.json': 'AB'.repeat(32) };
    expect(dataUrl({ version: '1.2', digests }, 'base/a.json')).toBe(`/data/h/${'ab'.repeat(8)}/base/a.json`);
    // No digest, or one that is not a SHA-256 in lower-case hex: the plain path, which the host revalidates.
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
    // The snapshot manifest is asked for first, and every file by its content.
    expect(requested[0]).toMatch(/^\/data\/h\/[0-9a-f]{16}\/manifest\.json$/);
    expect(requested.every((url) => /^\/data\/h\/[0-9a-f]{16}\//.test(url))).toBe(true);
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
    expect(requested.every((url) => url.startsWith('/data/1.2/'))).toBe(true);
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

describe('runtime release transport', () => {
  function fixture() {
    const root = realpathSync(mkdtempSync(join(tmpdir(), 'pobr-blocks-')));
    const out = join(root, 'candidate');
    runtimePackFixture(out);
    const manifest = JSON.parse(readFileSync(join(out, 'player/manifest.json'), 'utf8')) as RuntimeRelease;
    const payloads = new Map(manifest.blocks.map(spec => [`/data/h/${spec.sha256.slice(0, 16)}/${spec.path}`,
      new Uint8Array(readFileSync(join(out, 'player', spec.path)))]));
    const requests: string[] = [];
    return { root, out, manifest, payloads, requests, fetchBytes: async (url: string) => {
      requests.push(url);
      const raw = payloads.get(url);
      if (!raw) throw new Error(`unexpected fetch: ${url}`);
      return raw;
    } };
  }

  it('loads Python-produced blocks into the exact CLI materialized paths and bytes', async () => {
    const f = fixture();
    try {
      const files = new Map<string, Uint8Array>();
      const loaded = await loadDataPack({ manifest: f.manifest, fetchBytes: f.fetchBytes, digest,
        stage: (name, raw) => files.set(name, raw), keep: name => name === 'overlay/trade_catalog.json' });
      expect(loaded.verified).toBe(true);
      expect([...loaded.kept.keys()]).toEqual(['overlay/trade_catalog.json']);
      expect([...files.keys()].sort()).toEqual(f.manifest.files);
      expect(f.requests).toHaveLength(3);
      execFileSync('python3', ['-m', 'pipeline.runtime.runtime_pack', 'materialize', '--pack', join(f.out, 'runtime-pack.zip'),
        '--out', join(f.root, 'cli'), '--expected-id', f.manifest.release_id], { cwd: repoRoot });
      for (const [name, raw] of files) {
        const path = name.startsWith('overlay-common/') ? join(f.root, 'cli/data', name) : join(f.root, 'cli/data/9.1', name);
        expect(Buffer.from(raw)).toEqual(readFileSync(path));
      }
      expect(new TextDecoder().decode(files.get('base/skills.json'))).toContain('9223372036854775807');
      expect(JSON.parse(new TextDecoder().decode(files.get('overlay/trade_catalog.json'))).gems).toHaveLength(1);
      const recovered = await fetchReleaseFile(f.manifest, 'base/passive_trees/0_1.json', f.fetchBytes, digest);
      expect(recovered).toEqual(files.get('base/passive_trees/0_1.json'));
      // Repeat startup requests exactly the same immutable URLs; HTTP caching
      // belongs to the browser/host and is not simulated as a timing result.
      await loadDataPack({ manifest: f.manifest, fetchBytes: f.fetchBytes, digest, stage: () => {}, keep: () => false });
      expect(f.requests.slice(-3)).toEqual(f.requests.slice(0, 3));
    } finally { rmSync(f.root, { recursive: true, force: true }); }
  });

  it('rejects corrupt blocks, mixed targets, wrong inventories and missing digest support', async () => {
    const f = fixture();
    try {
      const options = { manifest: f.manifest, fetchBytes: f.fetchBytes, digest, stage: () => {}, keep: () => false };
      await expect(loadDataPack({ ...options, digest: null })).rejects.toThrow('require SHA-256');
      const [url, raw] = [...f.payloads][0];
      f.payloads.set(url, bytes('{}'));
      await expect(loadDataPack(options)).rejects.toThrow('block SHA-256 mismatch');
      f.payloads.set(url, raw);
      const mixed = structuredClone(f.manifest);
      mixed.target.game_patch = '9.2';
      await expect(loadDataPack({ ...options, manifest: mixed })).rejects.toThrow('target mismatch');
      const invalid = structuredClone(f.manifest);
      invalid.blocks[0].files.push('overlay/trade_catalog.json');
      await expect(validateRelease(invalid, digest)).rejects.toThrow('block specification');
      const wrongId = { ...f.manifest, release_id: '0'.repeat(64) };
      await expect(loadDataPack({ ...options, manifest: wrongId })).rejects.toThrow('release identity mismatch');
      // Even a rehashed block/release cannot substitute another snapshot identity.
      const localeSpec = f.manifest.blocks[2];
      const localeUrl = `/data/h/${localeSpec.sha256.slice(0, 16)}/${localeSpec.path}`;
      const block = JSON.parse(new TextDecoder().decode(f.payloads.get(localeUrl)));
      const snapshot = JSON.parse(block.files['manifest.json']);
      snapshot.poe_version = '9.2';
      block.files['manifest.json'] = JSON.stringify(snapshot);
      const changed = bytes(JSON.stringify(block));
      const release = structuredClone(f.manifest);
      release.digests['manifest.json'] = sha(bytes(block.files['manifest.json']));
      Object.assign(release.blocks[2], { bytes: changed.length, sha256: sha(changed) });
      const { release_id: _id, ...payload } = release;
      release.release_id = sha(bytes(canonical(payload)));
      f.payloads.set(`/data/h/${sha(changed).slice(0, 16)}/${localeSpec.path}`, changed);
      await expect(loadDataPack({ ...options, manifest: release })).rejects.toThrow('snapshot identity/inventory mismatch');
    } finally { rmSync(f.root, { recursive: true, force: true }); }
  });

  it.skipIf(!process.env.POBR_TEST_RUNTIME_PACK)('loads the fixed actual candidate through sync and compares every staged byte with CLI materialization', async () => {
    const root = realpathSync(mkdtempSync(join(tmpdir(), 'pobr-actual-blocks-')));
    const packRoot = resolve(process.env.POBR_TEST_RUNTIME_PACK!);
    const source = JSON.parse(readFileSync(join(packRoot, 'manifest.json'), 'utf8')) as RuntimeRelease;
    try {
      const destination = join(root, 'web-data');
      const release = syncData({ packRoot, destRoot: destination, expectedId: source.release_id }) as RuntimeRelease;
      execFileSync('python3', ['-m', 'pipeline.runtime.runtime_pack', 'materialize', '--pack', packRoot,
        '--out', join(root, 'cli'), '--expected-id', release.release_id], { cwd: repoRoot });
      const requests: string[] = [];
      let count = 0;
      const loaded = await loadDataPack({ manifest: release, digest, keep: name => name === 'overlay/trade_catalog.json',
        fetchBytes: async url => {
          requests.push(url);
          return new Uint8Array(readFileSync(join(destination, url.replace(/^\/data\//, ''))));
        },
        stage: (name, raw) => {
          const path = name.startsWith('overlay-common/') ? join(root, 'cli/data', name) : join(root, 'cli/data', release.version, name);
          expect(Buffer.compare(Buffer.from(raw), readFileSync(path))).toBe(0);
          count += 1;
        },
      });
      expect(loaded.verified).toBe(true);
      expect(requests).toHaveLength(3);
      expect(count).toBe(release.files.length);
      const trade = JSON.parse(new TextDecoder().decode(loaded.kept.get('overlay/trade_catalog.json')));
      expect(trade.gems.length).toBeGreaterThan(0);
      expect(release.files.filter(name => name.startsWith('base/passive_trees/'))).toHaveLength(4);
      expect(release.locale).toBe('zh-CN');
    } finally { rmSync(root, { recursive: true, force: true }); }
  }, 30_000);
});
