import { afterEach, expect, test, vi } from 'vitest';
import { ConfigFieldLink } from './ConfigFieldLink';
import { requestConfigField, takeConfigField } from '../../lib/configNavigation';

const harness = vi.hoisted(() => ({ navigate: vi.fn() }));
vi.mock('../../app/pages/navigate', () => ({ useNavigateToPage: () => harness.navigate }));
afterEach(() => { vi.unstubAllGlobals(); takeConfigField(); harness.navigate.mockReset(); });

test('condition link queues the exact config field after shell navigation', () => {
  vi.stubGlobal('window', new EventTarget());
  harness.navigate.mockImplementation(() => { expect(takeConfigField()).toBeNull(); });
  const preventDefault = vi.fn();
  const anchor = ConfigFieldLink({ field: 'conditionLowLife', lang: 'zh-CN' });
  expect(anchor.props.href).toBe('/config#config-conditionLowLife');
  expect(anchor.props.children).toBe('去配置开启 ›');
  anchor.props.onClick({ preventDefault });
  expect(preventDefault).toHaveBeenCalledOnce();
  expect(harness.navigate).toHaveBeenCalledWith('config');
  expect(takeConfigField()).toBe('conditionLowLife');
  expect(takeConfigField()).toBeNull();
});

test('a field request stays pending until the destination consumes it', () => {
  vi.stubGlobal('window', new EventTarget());
  requestConfigField('conditionEnemyChilled');
  expect(takeConfigField()).toBe('conditionEnemyChilled');
});
