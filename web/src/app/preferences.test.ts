import { expect, it } from 'vitest';
import { DEFAULT_PREFERENCES, PREFERENCES_KEY, pageDisplayMode, parsePreferences, readPreferences } from './preferences';
import { ROUTES, readRoute, routePath } from './routes';
it('recovers readable defaults and bounds persisted font sizes', () => {
  expect(parsePreferences('{broken')).toMatchObject({ theme: 'dark', fontSize: 16, lang: 'zh-CN' });
  expect(parsePreferences('{"fontSize":900,"font":"bad","lang":"bad"}')).toMatchObject({ fontSize: 32, font: 'system', lang: 'zh-CN' });
  expect(parsePreferences('{"fontSize":1}').fontSize).toBe(14);
  expect(parsePreferences('{"theme":"light","lang":"zh-TW","density":"compact"}')).toMatchObject({ theme: 'light', lang: 'zh-TW', density: 'compact' });
});
it('migrates the equipment preference and lets the new global choice take precedence', () => {
  for (const itemDisplay of ['grouped', 'compact', 'game']) {
    expect(parsePreferences(JSON.stringify({ itemDisplay })).displayMode).toBe(itemDisplay);
  }
  expect(parsePreferences('{"theme":"light"}')).toMatchObject({ theme: 'light', displayMode: 'grouped' });
  expect(parsePreferences('{"itemDisplay":"unknown"}').displayMode).toBe('grouped');
  expect(parsePreferences(null).displayMode).toBe('grouped');
  expect(parsePreferences('{"itemDisplay":"game","displayMode":"compact"}').displayMode).toBe('compact');
});
it('inherits the global mode, keeps valid page overrides and ignores corrupt saved entries', () => {
  const preferences = parsePreferences(JSON.stringify({ displayMode: 'compact', pageDisplayModes: {
    items: 'game', skills: 'grouped', config: 'unknown', invented: 'game', calcs: null,
  } }));
  expect(preferences.pageDisplayModes).toEqual({ items: 'game', skills: 'grouped' });
  expect(pageDisplayMode(preferences, 'items')).toBe('game');
  expect(pageDisplayMode(preferences, 'config')).toBe('compact');
  expect(pageDisplayMode({ ...preferences, displayMode: 'game' }, 'skills')).toBe('grouped');
  expect(parsePreferences(JSON.stringify(preferences))).toEqual(preferences);
  for (const raw of ['null', 'true', '42', '[]', '"bad"', '{"pageDisplayModes":"game"}']) {
    expect(parsePreferences(raw)).toEqual(DEFAULT_PREFERENCES);
  }
});
it('maps pages to paths at the site root and falls back to the overview', () => {
  expect(readRoute('/calcs')).toBe('calcs'); expect(readRoute('/config/')).toBe('config'); expect(readRoute('/')).toBe('overview');
  expect(readRoute('/unknown')).toBe('overview');
  expect(routePath('overview')).toBe('/'); expect(routePath('items')).toBe('/items');
  for (const route of ROUTES) expect(readRoute(routePath(route))).toBe(route);
});
it('starts a device that only used the first interface from the language it chose', () => {
  const store = (items: Record<string, string>) => ({ getItem: (key: string) => items[key] ?? null });
  expect(readPreferences(store({ 'pobr-lang': 'en-US' }))).toEqual({ ...DEFAULT_PREFERENCES, lang: 'en-US' });
  expect(readPreferences(store({ 'pobr-lang': 'fr' }))).toEqual(DEFAULT_PREFERENCES);
  expect(readPreferences(store({}))).toEqual(DEFAULT_PREFERENCES);
  // Once the current interface has saved anything, its own choice wins.
  expect(readPreferences(store({ 'pobr-lang': 'en-US', [PREFERENCES_KEY]: '{"lang":"zh-TW"}' })).lang).toBe('zh-TW');
});
