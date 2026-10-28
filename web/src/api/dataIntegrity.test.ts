import { describe, expect, it } from 'vitest';
import { nativeDigest, verifySnapshotDigests } from './dataIntegrity';

const encode = (text: string) => new TextEncoder().encode(text);
const digest = nativeDigest()!;

async function pack(overrides: Record<string, string> = {}, schema = 3) {
  const contents: Record<string, string> = { 'base/a.json': '{"a":1}', 'overlay/b.json': '[1,2,3]' };
  const files = new Map<string, Uint8Array>();
  const listed: Record<string, string> = {};
  for (const [name, text] of Object.entries(contents)) {
    listed[name] = await digest(encode(text));
    files.set(name, encode(overrides[name] ?? text));
  }
  files.set('manifest.json', encode(JSON.stringify({ schema_version: schema, files: listed })));
  return files;
}

describe('snapshot digests', () => {
  it('has a native SHA-256 here, and it is the real one', async () => {
    expect(await digest(encode('abc'))).toBe(
      'ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad',
    );
  });

  it('accepts a pack whose every listed file matches', async () => {
    expect(await verifySnapshotDigests(await pack())).toBe(true);
  });

  it('rejects a changed file, naming it as the engine does', async () => {
    await expect(verifySnapshotDigests(await pack({ 'overlay/b.json': '[1,2,4]' }))).rejects.toThrow(
      'SHA-256 mismatch: overlay/b.json',
    );
  });

  it('rejects a listed file that was never staged', async () => {
    const files = await pack();
    files.delete('base/a.json');
    await expect(verifySnapshotDigests(files)).rejects.toThrow('file not in memory data: base/a.json');
  });

  it('leaves the check to the engine when it cannot decide', async () => {
    // No native digest, an older schema without an inventory, no or unreadable manifest.
    expect(await verifySnapshotDigests(await pack(), null)).toBe(false);
    expect(await verifySnapshotDigests(await pack({}, 2))).toBe(false);
    expect(await verifySnapshotDigests(new Map())).toBe(false);
    expect(await verifySnapshotDigests(new Map([['manifest.json', encode('not json')]]))).toBe(false);
    expect(await verifySnapshotDigests(new Map([['manifest.json', encode('{"schema_version":3}')]]))).toBe(false);
  });

  it('verifies many files at once without mixing them up', async () => {
    const files = new Map<string, Uint8Array>();
    const listed: Record<string, string> = {};
    for (let index = 0; index < 40; index++) {
      const bytes = encode(`{"file":${index}}`);
      files.set(`base/f${index}.json`, bytes);
      listed[`base/f${index}.json`] = await digest(bytes);
    }
    files.set('manifest.json', encode(JSON.stringify({ schema_version: 3, files: listed })));
    expect(await verifySnapshotDigests(files)).toBe(true);
    files.set('base/f17.json', encode('{"file":18}'));
    await expect(verifySnapshotDigests(files)).rejects.toThrow('SHA-256 mismatch: base/f17.json');
  });
});
