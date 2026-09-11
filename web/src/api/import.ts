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
