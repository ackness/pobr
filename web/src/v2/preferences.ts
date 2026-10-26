import type { Lang } from '../lib/i18n';
export interface Preferences {
  lang: Lang;
  theme: 'dark' | 'light' | 'system';
  font: 'system' | 'sans' | 'serif';
  fontSize: number;
  density: 'comfortable' | 'compact';
}
export const PREFERENCES_KEY = 'pobr-v2-preferences';
export const DEFAULT_PREFERENCES: Preferences = { lang: 'zh-CN', theme: 'dark', font: 'system', fontSize: 16, density: 'comfortable' };
export function parsePreferences(raw: string | null): Preferences {
  let value: Partial<Preferences> = {};
  try { value = JSON.parse(raw ?? '{}') ?? {}; } catch { /* Use readable defaults. */ }
  return {
    lang: ['en-US', 'zh-CN', 'zh-TW'].includes(value.lang ?? '') ? value.lang! : DEFAULT_PREFERENCES.lang,
    theme: ['dark', 'light', 'system'].includes(value.theme ?? '') ? value.theme! : 'dark',
    font: ['system', 'sans', 'serif'].includes(value.font ?? '') ? value.font! : 'system',
    fontSize: typeof value.fontSize === 'number' && Number.isFinite(value.fontSize) ? Math.min(32, Math.max(14, value.fontSize)) : 16,
    density: value.density === 'compact' ? 'compact' : 'comfortable',
  };
}
