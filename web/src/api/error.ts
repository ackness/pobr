import type { ApiErrorJson } from './types';

/**
 * Errors returned by the wasm boundary are JSON-encoded `{code, message, slot?}`
 * because `JsError` only carries strings; parse them back into typed errors.
 * Non-JSON values (legacy wasm builds or non-wasm exceptions) fall back to
 * `internal` with the raw text as `message` so callers always get a usable shape.
 */
export function parseApiError(err: unknown): ApiErrorJson {
  const raw = err instanceof Error ? err.message : String(err);
  try {
    const parsed = JSON.parse(raw) as unknown;
    if (
      parsed !== null &&
      typeof parsed === 'object' &&
      'code' in parsed &&
      'message' in parsed
    ) {
      return parsed as ApiErrorJson;
    }
  } catch {
    // Non-JSON: fall back to `internal`.
  }
  return { code: 'internal', message: raw };
}

/** Single-line display string (prefixed with the slot name when present). */
export function formatApiError(err: unknown): string {
  const e = parseApiError(err);
  return e.slot ? `[${e.slot}] ${e.message}` : e.message;
}
