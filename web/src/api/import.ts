/** Resolve a public share before the pure WASM decoder runs. */
export async function resolveBuildInput(input: string): Promise<string> {
  const text = input.trim();
  if (!/^https?:\/\//i.test(text)) return text;
  const url = new URL(text);
  if (url.hostname !== 'www.wegame.com.cn') {
    throw new Error('Unsupported share URL. Paste a PoB2 build code or a WeGame PoE2 share URL.');
  }
  const response = await fetch('/api/import/wegame', {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify({ url: text }),
    signal: AbortSignal.timeout(20000),
  });
  const data = await response.json().catch(() => null);
  if (!response.ok || data?.format !== 'wegame') {
    throw new Error(data?.error ?? 'WeGame import service is unavailable. Run pnpm dev or deploy with the bundled Pages worker.');
  }
  return JSON.stringify(data);
}

/** Classify ownership before validation; a rejected PoBR save must never reach
 * another format's permissive decoder. Presence, not validity, identifies it.
 */
export function buildFileKind(input: string): 'workspace' | 'session' | 'external' {
  let parsed: unknown;
  try { parsed = JSON.parse(input); } catch { return 'external'; }
  if (!parsed || typeof parsed !== 'object' || Array.isArray(parsed)) return 'external';
  if (Object.hasOwn(parsed, 'workspace')) return 'workspace';
  if (Object.hasOwn(parsed, 'state') || 'format' in parsed && parsed.format === 'pobr-build') return 'session';
  // Older session envelopes have no format marker. A damaged envelope may
  // have lost its state, but its version/notes pair still belongs to PoBR.
  if (Object.hasOwn(parsed, 'version') && Object.hasOwn(parsed, 'notes')) return 'session';
  return 'external';
}
