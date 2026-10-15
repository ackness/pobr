// Shared by the browser resolver and Pages Worker. Only these fixed raw-code
// endpoints may be fetched; a pasted URL is never forwarded to fetch directly.
const providers = [
  { hosts: ['pobb.in'], path: /^\/([\w-]{1,128})\/?$/, raw: id => `https://pobb.in/pob/${id}` },
  { hosts: ['poe.ninja', 'poe2.ninja'], path: /^\/(?:poe2\/)?pob\/([\w-]{1,128})\/?$/, raw: id => `https://poe.ninja/poe2/pob/raw/${id}` },
  { hosts: ['pastebin.com', 'pastebinp.com'], path: /^\/(?:raw\/)?([A-Za-z0-9]{1,128})\/?$/, raw: id => `https://pastebin.com/raw/${id}` },
  { hosts: ['rentry.co'], path: /^\/([\w-]{1,128})(?:\/raw)?\/?$/, raw: id => `https://rentry.co/paste/${id}/raw` },
  { hosts: ['poe2db.tw'], path: /^\/pob\/([\w-]{1,128})(?:\/raw)?\/?$/, raw: id => `https://poe2db.tw/pob/${id}/raw` },
  { hosts: ['maxroll.gg'], path: /^\/poe2\/pob\/([\w-]{1,128})\/?$/, raw: id => `https://maxroll.gg/poe2/api/pob/${id}` },
];

export class BuildLinkError extends Error {
  constructor(code, status = 400) {
    super(code);
    this.code = code;
    this.status = status;
  }
}

export function buildCodeTarget(input) {
  let url;
  try { url = new URL(input.trim()); } catch { throw new BuildLinkError('invalid_url'); }
  if (url.protocol !== 'https:' || url.port || url.username || url.password || input.length > 2048) {
    throw new BuildLinkError('invalid_url');
  }
  const provider = providers.find(entry => entry.hosts.includes(url.hostname));
  if (!provider) throw new BuildLinkError('unsupported_host');
  const match = provider.path.exec(url.pathname);
  if (!match) {
    if (['poe.ninja', 'poe2.ninja'].includes(url.hostname) && /\/(?:builds|character)(?:\/|$)/.test(url.pathname)) {
      throw new BuildLinkError('ninja_profile');
    }
    throw new BuildLinkError('invalid_url');
  }
  // Query/fragment tracking parameters are not needed to retrieve a public code.
  return provider.raw(match[1]);
}

export const BUILD_CODE_LIMIT = 1024 * 1024;

export function isBuildCode(value) {
  return typeof value === 'string' && value.length >= 16 && value.length <= BUILD_CODE_LIMIT
    && /^[A-Za-z0-9+/_-]+={0,2}$/.test(value);
}

async function readCode(response, signal) {
  if (Number(response.headers.get('content-length')) > BUILD_CODE_LIMIT) {
    await response.body?.cancel();
    throw new BuildLinkError('too_large', 502);
  }
  if (!response.body) throw new BuildLinkError('invalid_response', 502);
  const reader = response.body.getReader();
  const abort = () => { void reader.cancel().catch(() => {}); };
  signal.addEventListener('abort', abort, { once: true });
  const decoder = new TextDecoder();
  let size = 0, text = '';
  try {
    for (;;) {
      if (signal.aborted) throw new BuildLinkError('timeout', 504);
      const { done, value } = await reader.read();
      if (signal.aborted) throw new BuildLinkError('timeout', 504);
      if (done) break;
      size += value.byteLength;
      if (size > BUILD_CODE_LIMIT) throw new BuildLinkError('too_large', 502);
      text += decoder.decode(value, { stream: true });
    }
    const code = (text + decoder.decode()).trim();
    if (!isBuildCode(code)) throw new BuildLinkError('invalid_response', 502);
    return code;
  } finally {
    signal.removeEventListener('abort', abort);
    await reader.cancel().catch(() => {});
  }
}

export async function fetchBuildCode(input, fetcher = fetch) {
  const target = buildCodeTarget(input);
  const signal = AbortSignal.timeout(15000);
  try {
    const response = await fetcher(target, {
      method: 'GET', redirect: 'manual', credentials: 'omit', signal,
      headers: { accept: 'text/plain', 'user-agent': 'PoBR (+https://github.com/ackness/pobr)' },
    });
    if (!response.ok) {
      await response.body?.cancel();
      if ([404, 410].includes(response.status)) throw new BuildLinkError('not_found', 404);
      if ([401, 403, 429].includes(response.status)) throw new BuildLinkError('unavailable', 502);
      throw new BuildLinkError('upstream', 502);
    }
    return await readCode(response, signal);
  } catch (error) {
    if (error instanceof BuildLinkError) throw error;
    throw new BuildLinkError(signal.aborted ? 'timeout' : 'network', signal.aborted ? 504 : 502);
  }
}
