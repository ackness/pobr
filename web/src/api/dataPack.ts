/**
 * Loading the game data pack into the engine.
 *
 * Two things matter on a phone: how much of the pack is held at once, and what a repeat
 * visit has to ask the network for.
 *
 * - The pack is tens of megabytes. Each file is compared with its digest and handed to the
 *   engine as soon as it arrives, and its bytes are dropped; only the few files the page
 *   reads again are kept. Waiting for every file before checking any of them keeps a second
 *   copy of the whole pack in the page's memory beside the engine's.
 * - A file is asked for by its digest (`?h=`), so its URL names exactly one content and the
 *   browser can keep it for good (the host marks those responses immutable). A repeat visit
 *   then fetches the manifest and nothing else.
 */

import { nativeDigest, type DigestFn } from './dataIntegrity';

export interface DataManifest {
  version: string;
  files: string[];
  /** The digest of every listed file as published; absent in a manifest from an older sync. */
  digests?: Record<string, string>;
}

/** How many leading hex digits of a file's digest its URL carries. */
const URL_DIGEST_LENGTH = 16;

/** The URL of a data file, carrying its digest when the manifest has one. */
export function dataUrl(manifest: Pick<DataManifest, 'version' | 'digests'>, rel: string): string {
  const url = `/data/${manifest.version}/${rel}`;
  const digest = manifest.digests?.[rel];
  return typeof digest === 'string' && /^[0-9a-f]{64}$/.test(digest)
    ? `${url}?h=${digest.slice(0, URL_DIGEST_LENGTH)}`
    : url;
}

/**
 * The digests a snapshot manifest lists for its files, or `null` when this cannot be decided
 * here (an unreadable manifest, or one of the older schemas without an inventory): the engine
 * then makes the comparison itself and reports in its own words.
 */
function readInventory(manifestBytes: Uint8Array): Map<string, string> | null {
  let manifest: { schema_version?: unknown; files?: unknown };
  try {
    manifest = JSON.parse(new TextDecoder().decode(manifestBytes));
  } catch {
    return null;
  }
  const listed = manifest.files;
  if (typeof manifest.schema_version !== 'number' || manifest.schema_version < 3
    || listed === null || typeof listed !== 'object' || Array.isArray(listed)) {
    return null;
  }
  const inventory = new Map<string, string>();
  for (const [file, digest] of Object.entries(listed as Record<string, unknown>)) {
    // A digest that is not text can never match; the comparison below reports the file.
    inventory.set(file, typeof digest === 'string' ? digest : '');
  }
  return inventory;
}

export interface LoadDataPackOptions {
  manifest: DataManifest;
  fetchBytes: (url: string) => Promise<Uint8Array>;
  /** Hands a file to the engine (which copies it). */
  stage: (rel: string, bytes: Uint8Array) => void;
  /** The files the page reads again after start-up. Every other file's bytes are dropped once staged. */
  keep: (rel: string) => boolean;
  /** The platform's SHA-256; `null` where there is none. Defaults to the platform's own. */
  digest?: DigestFn | null;
  concurrency?: number;
  onProgress?: (done: number, total: number) => void;
}

export interface LoadedDataPack {
  /**
   * Every file the snapshot manifest lists matched its digest here, so the engine can skip
   * the comparison. `false`: it must do it itself.
   */
  verified: boolean;
  /** The bytes of the files `keep` asked for. */
  kept: Map<string, Uint8Array>;
}

/**
 * Fetches the pack file by file, checks each against the snapshot manifest's digest as it
 * arrives, stages it, and lets go of its bytes. Rejects, with the engine's own messages, when
 * a listed file does not match or was not in the pack.
 */
export async function loadDataPack(options: LoadDataPackOptions): Promise<LoadedDataPack> {
  const { manifest, fetchBytes, stage, keep, onProgress } = options;
  const digest = options.digest === undefined ? nativeDigest() ?? null : options.digest;
  const concurrency = options.concurrency ?? 8;
  const total = manifest.files.length;
  const kept = new Map<string, Uint8Array>();
  let done = 0;
  const take = (rel: string, bytes: Uint8Array) => {
    stage(rel, bytes);
    if (keep(rel)) kept.set(rel, bytes);
    done += 1;
    onProgress?.(done, total);
  };

  // The snapshot manifest comes first: its inventory says what every other file has to be, so
  // each can be compared as it arrives instead of after the whole pack has.
  const snapshot = 'manifest.json';
  let inventory: Map<string, string> | null = null;
  if (manifest.files.includes(snapshot)) {
    const bytes = await fetchBytes(dataUrl(manifest, snapshot));
    inventory = digest ? readInventory(bytes) : null;
    take(snapshot, bytes);
  }

  const queue = manifest.files.filter((rel) => rel !== snapshot);
  const seen = new Set<string>();
  const worker = async () => {
    for (let rel = queue.shift(); rel !== undefined; rel = queue.shift()) {
      const bytes = await fetchBytes(dataUrl(manifest, rel));
      const expected = inventory?.get(rel);
      if (digest && expected !== undefined && await digest(bytes) !== expected) {
        throw new Error(`SHA-256 mismatch: ${rel}`);
      }
      seen.add(rel);
      take(rel, bytes);
    }
  };
  await Promise.all(Array.from({ length: Math.min(concurrency, queue.length) }, worker));

  if (inventory) {
    for (const rel of inventory.keys()) {
      if (!seen.has(rel)) throw new Error(`file not in memory data: ${rel}`);
    }
  }
  return { verified: inventory !== null, kept };
}
