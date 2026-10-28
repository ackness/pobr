/**
 * Checks a staged data pack against the SHA-256 digests its snapshot manifest lists.
 *
 * The engine refuses a pack with a listed file that has changed, which protects against a
 * deployment that mixes two versions' files. It does the hashing itself unless told the
 * caller did: in WASM there are no SHA instructions and hashing about 57 MB took 180 ms of
 * a 460 ms start, while the browser's own SHA-256 does it in a small fraction of that. The
 * check is the same one (every listed file, the manifest's digest); only who computes it
 * differs.
 */

export type DigestFn = (bytes: Uint8Array) => Promise<string>;

/** The platform's SHA-256, or `undefined` where there is none (`crypto.subtle` needs a secure context). */
export function nativeDigest(): DigestFn | undefined {
  const subtle = globalThis.crypto?.subtle;
  if (!subtle) return undefined;
  return async (bytes) => {
    const hash = new Uint8Array(await subtle.digest('SHA-256', bytes as BufferSource));
    return Array.from(hash, (byte) => byte.toString(16).padStart(2, '0')).join('');
  };
}

const PARALLEL_DIGESTS = 8;

/**
 * Compares every file the manifest in `files` lists with its digest.
 *
 * Resolves `true` when all of them match, so the engine can skip hashing. Resolves
 * `false` when this cannot be decided here (no native digest, or a manifest without a
 * digest inventory, as in the older schemas), and the engine must do its own check.
 * Rejects when a listed file is missing or does not match, with the message the engine
 * gives for the same fault.
 */
export async function verifySnapshotDigests(
  files: ReadonlyMap<string, Uint8Array>,
  digest: DigestFn | null = nativeDigest() ?? null,
): Promise<boolean> {
  const manifestBytes = files.get('manifest.json');
  if (!digest || !manifestBytes) return false;
  let manifest: { schema_version?: unknown; files?: unknown };
  try {
    manifest = JSON.parse(new TextDecoder().decode(manifestBytes));
  } catch {
    return false; // The engine reports an unreadable manifest in its own words.
  }
  const listed = manifest.files;
  if (typeof manifest.schema_version !== 'number' || manifest.schema_version < 3
    || listed === null || typeof listed !== 'object' || Array.isArray(listed)) {
    return false;
  }
  const queue = Object.entries(listed as Record<string, unknown>);
  const worker = async () => {
    for (let next = queue.shift(); next !== undefined; next = queue.shift()) {
      const [file, expected] = next;
      const bytes = files.get(file);
      if (!bytes) throw new Error(`file not in memory data: ${file}`);
      if (await digest(bytes) !== expected) throw new Error(`SHA-256 mismatch: ${file}`);
    }
  };
  await Promise.all(Array.from({ length: Math.min(PARALLEL_DIGESTS, queue.length) }, worker));
  return true;
}
