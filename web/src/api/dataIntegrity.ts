/**
 * Platform SHA-256 helper used to verify staged data packs and runtime blocks.
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
