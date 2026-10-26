import { expect, it } from 'vitest';
import { parsePreferences } from './preferences';
import { isV2Path, readRoute, routePath } from './routes';
it('recovers readable defaults and bounds persisted font sizes', () => {
  expect(parsePreferences('{broken')).toMatchObject({ theme: 'dark', fontSize: 16, lang: 'zh-CN' });
  expect(parsePreferences('{"fontSize":900,"font":"bad","lang":"bad"}')).toMatchObject({ fontSize: 32, font: 'system', lang: 'zh-CN' });
  expect(parsePreferences('{"fontSize":1}').fontSize).toBe(14);
  expect(parsePreferences('{"theme":"light","lang":"zh-TW","density":"compact"}')).toMatchObject({ theme: 'light', lang: 'zh-TW', density: 'compact' });
});
it('keeps V2 routing separate from similarly named legacy paths', () => {
  expect(isV2Path('/v2/calcs')).toBe(true); expect(isV2Path('/v20')).toBe(false);
  expect(readRoute('/v2/config/')).toBe('config'); expect(readRoute('/v2/unknown')).toBe('overview');
  expect(routePath('overview')).toBe('/v2'); expect(routePath('items')).toBe('/v2/items');
});
