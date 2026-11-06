import type { ReactNode } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { expect, it, vi } from 'vitest';
import { UiProvider } from '../shared/UiContext';
import { GemPicker } from './GemPicker';

// Inspect the portal's content without depending on browser mounting or focus.
vi.mock('../ui/popover', () => ({
  Popover: ({ children }: { children: ReactNode }) => children,
  PopoverAnchor: () => null,
  PopoverContent: ({ children }: { children: ReactNode }) => <div>{children}</div>,
}));

it.each(['en-US', 'zh-CN', 'zh-TW'] as const)('keeps the %s picker count outside the colour segments', lang => {
  const html = renderToStaticMarkup(<UiProvider lang={lang}>
    <GemPicker entries={[]} placeholder="Gem" lang={lang} disabled={false} onPick={() => {}} />
  </UiProvider>);
  const header = html.match(/<div class="gem-picker-chips" role="group" aria-label="[^"]+"><div class="segmented segmented--sm">(.*?)<\/div><span class="gem-picker-count">([^<]+)<\/span><\/div>/);
  expect(header).not.toBeNull();
  expect(header![1].match(/<button\b/g)).toHaveLength(4);
  expect(header![1].match(/aria-pressed="true"/g)).toHaveLength(1);
  expect(header![2]).toContain('0');
});
