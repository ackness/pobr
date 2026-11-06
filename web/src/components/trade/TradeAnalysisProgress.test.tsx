import { renderToStaticMarkup } from 'react-dom/server';
import { expect, test } from 'vitest';
import { remainingMinutes, TradeAnalysisProgress, type SlotProgress } from './TradeAnalysisProgress';

const initial: SlotProgress = { done: 0, total: 11, measuredMs: 0, measuredSlots: 0 };
test('ETA waits for a measured slot, including when resuming cached results', () => {
  expect(remainingMinutes(initial)).toBeNull();
  expect(remainingMinutes({ ...initial, done: 4 })).toBeNull();
  expect(remainingMinutes({ ...initial, done: 2, measuredMs: 40_000, measuredSlots: 2 })).toBe(3);
  expect(remainingMinutes({ ...initial, done: 11, measuredMs: 100_000, measuredSlots: 11 })).toBeNull();
});
test('renders the current slot out of the total without exposing internal evaluation counts', () => {
  const html = renderToStaticMarkup(<TradeAnalysisProgress slots={{ ...initial, done: 3, measuredMs: 90_000, measuredSlots: 3 }}
    position="胸甲" fraction={.5} lang="zh-CN" onStop={() => {}} />);
  expect(html).toContain('第 4/11 个位置');
  expect(html).toContain('预计还需约 4 分钟');
  expect(html).toContain('value="3.5" max="11"');
  expect(html).toContain('停止');
  expect(html).not.toContain('继续');
});
