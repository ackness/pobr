import { LANGS, type Lang } from '../lib/i18n';
import { uiStorageKey } from '../lib/sessionStorage';
import { isDisplayMode, type DisplayMode } from '../lib/displayModes';
import { ROUTES, type Route } from './routes';
export interface Preferences {
  lang: Lang;
  theme: 'dark' | 'light' | 'system';
  font: 'system' | 'sans' | 'serif';
  fontSize: number;
  density: 'comfortable' | 'compact';
  displayMode: DisplayMode;
  pageDisplayModes: Partial<Record<Route, DisplayMode>>;
}
export const PREFERENCES_KEY = uiStorageKey('pobr-preferences');
export const DEFAULT_PREFERENCES: Preferences = { lang: 'zh-CN', theme: 'dark', font: 'system', fontSize: 16, density: 'comfortable', displayMode: 'grouped', pageDisplayModes: {} };
export function parsePreferences(raw: string | null): Preferences {
  let value: Partial<Preferences> & { itemDisplay?: unknown } = {};
  try { value = JSON.parse(raw ?? '{}') ?? {}; } catch { /* Use readable defaults. */ }
  const mode = value.displayMode ?? value.itemDisplay;
  const overrides = value.pageDisplayModes;
  const pageDisplayModes: Preferences['pageDisplayModes'] = {};
  if (overrides && typeof overrides === 'object') {
    for (const page of ROUTES) if (isDisplayMode(overrides[page])) pageDisplayModes[page] = overrides[page];
  }
  return {
    lang: ['en-US', 'zh-CN', 'zh-TW'].includes(value.lang ?? '') ? value.lang! : DEFAULT_PREFERENCES.lang,
    theme: ['dark', 'light', 'system'].includes(value.theme ?? '') ? value.theme! : 'dark',
    font: ['system', 'sans', 'serif'].includes(value.font ?? '') ? value.font! : 'system',
    fontSize: typeof value.fontSize === 'number' && Number.isFinite(value.fontSize) ? Math.min(32, Math.max(14, value.fontSize)) : 16,
    density: value.density === 'compact' ? 'compact' : 'comfortable',
    displayMode: isDisplayMode(mode) ? mode : DEFAULT_PREFERENCES.displayMode,
    pageDisplayModes,
  };
}

export function pageDisplayMode(preferences: Preferences, page: Route): DisplayMode {
  return preferences.pageDisplayModes[page] ?? preferences.displayMode;
}
/** The language the first interface saved; a device that never opened the current one starts from it. */
const LEGACY_LANGUAGE_KEY = 'pobr-lang';
export function readPreferences(storage: Pick<Storage, 'getItem'>): Preferences {
  const raw = storage.getItem(PREFERENCES_KEY);
  const legacy = storage.getItem(LEGACY_LANGUAGE_KEY);
  if (raw === null && LANGS.includes(legacy as Lang)) return { ...DEFAULT_PREFERENCES, lang: legacy as Lang };
  return parsePreferences(raw);
}
