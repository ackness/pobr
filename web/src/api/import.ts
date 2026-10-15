import { BuildLinkError, buildCodeTarget, isBuildCode } from '../../public/build-import.mjs';
import { t, type Lang, type UiKey } from '../lib/i18n';

const LINK_ERRORS = {
  invalid_url: 'build.linkInvalid', unsupported_host: 'build.linkHost', ninja_profile: 'build.linkNinjaProfile',
  too_large: 'build.linkTooLarge', invalid_response: 'build.linkInvalidResponse', timeout: 'build.linkTimeout',
  not_found: 'build.linkNotFound', unavailable: 'build.linkUnavailable', upstream: 'build.linkUpstream', network: 'build.linkNetwork',
} satisfies Record<string, UiKey>;

/** Import errors keep stable keys until rendered in the selected UI language. */
export function buildImportErrorMessage(message: string, lang: Lang): string {
  return Object.values(LINK_ERRORS).includes(message as typeof LINK_ERRORS[keyof typeof LINK_ERRORS])
    ? t(lang, message as UiKey) : message;
}

/** Resolve a public share before the pure WASM decoder runs. */
export async function resolveBuildInput(input: string): Promise<string> {
  const text = input.trim();
  if (!/^https?:\/\//i.test(text)) return text;
  let url: URL;
  try { url = new URL(text); } catch { throw new Error(LINK_ERRORS.invalid_url); }
  if (url.hostname !== 'www.wegame.com.cn') {
    try { buildCodeTarget(text); } catch (error) {
      throw new Error(error instanceof BuildLinkError ? LINK_ERRORS[error.code] : LINK_ERRORS.invalid_url);
    }
    const signal = AbortSignal.timeout(20000);
    let response: Response;
    let data: { code?: unknown; error_code?: string } | null;
    try {
      response = await fetch('/api/import/build-code', {
        method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify({ url: text }), signal,
      });
      data = await response.json().catch(() => null);
      if (signal.aborted) throw new Error(LINK_ERRORS.timeout);
    } catch {
      throw new Error(signal.aborted ? LINK_ERRORS.timeout : LINK_ERRORS.network);
    }
    if (!response.ok) {
      throw new Error(data?.error_code && Object.hasOwn(LINK_ERRORS, data.error_code)
        ? LINK_ERRORS[data.error_code as keyof typeof LINK_ERRORS] : LINK_ERRORS.upstream);
    }
    if (!isBuildCode(data?.code)) throw new Error(LINK_ERRORS.invalid_response);
    return data.code;
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
