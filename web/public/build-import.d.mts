export type BuildLinkErrorCode = 'invalid_url' | 'unsupported_host' | 'ninja_profile' | 'too_large'
  | 'invalid_response' | 'timeout' | 'not_found' | 'unavailable' | 'upstream' | 'network';
export class BuildLinkError extends Error {
  code: BuildLinkErrorCode;
  status: number;
  constructor(code: BuildLinkErrorCode, status?: number);
}
export function buildCodeTarget(input: string): string;
export const BUILD_CODE_LIMIT: number;
export function isBuildCode(value: unknown): value is string;
export function fetchBuildCode(input: string, fetcher?: typeof fetch): Promise<string>;
