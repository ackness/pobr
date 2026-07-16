import type { ApiErrorJson } from './types';

/**
 * wasm 接口的 Err 侧是 JSON 编码的 `{code, message, slot?}`（JsError 只能携带
 * 字符串）；解析回类型化错误。非 JSON（旧 wasm 构建 / 非 wasm 来源异常）归入
 * `internal`，原文作 message——调用侧永远拿到可用的结构。
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
    // 非 JSON，走 internal 兜底。
  }
  return { code: 'internal', message: raw };
}

/** 展示用单行文本（有 slot 时带槽位前缀）。 */
export function formatApiError(err: unknown): string {
  const e = parseApiError(err);
  return e.slot ? `[${e.slot}] ${e.message}` : e.message;
}
