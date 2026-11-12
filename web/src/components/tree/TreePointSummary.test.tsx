import { renderToStaticMarkup } from 'react-dom/server';
import { expect, test } from 'vitest';
import type { TreeEditResponse } from '../../api/types';
import { TreePointSummary } from './TreePointSummary';

const current = { used: 119, max: 121, weapon: [13, 12] as [number, number] };
const render = (preview: TreeEditResponse | null) => renderToStaticMarkup(<TreePointSummary current={current} preview={preview} invalidReason="Disconnected path" lang="en-US" />);
const preview = (used: number): TreeEditResponse => ({ valid: true, reason: null, result: {
  points_used: used, points_max: 121, weapon_points_used: [13, 12], weapon_points_max: 24,
  added: [], removed: [], promoted: [], path: [], allocation: { shared: [], exclusive: [[], []] },
} });

test('one primary counter uses the same budget before, during and after a hover preview', () => {
  expect(render(null)).toContain('Used 119 / 121 points');
  expect(render(preview(120))).toContain('Used 120 / 121 points<span class="tree-count-change">(+1 this change)');
  expect(render(preview(117))).toContain('(-2 this change)');
  expect(render(preview(119))).toContain('(+0 this change)');
  expect(render(preview(120)).match(/class="tree-count"/g)).toHaveLength(1);
  expect(render(null)).toContain('<details class="tree-point-details">');
  expect(render(null)).not.toContain('open=""');
});

test('an invalid preview preserves the counter and announces its visible diagnostic outside point details', () => {
  const html = render({ valid: false, reason: 'disconnected_shared_path', result: null });
  expect(html).toContain('Used 119 / 121 points');
  const disclosure = html.indexOf('<details');
  expect(html.slice(0, disclosure)).toContain('<p class="tree-edit-preview" role="status" aria-live="polite" aria-atomic="true">Disconnected path</p>');
  expect(html.slice(disclosure)).not.toContain('Disconnected path');
  expect(html).not.toContain('this change');
});

test('the preview status stays available for live updates and clears when a preview is valid or dismissed', () => {
  for (const response of [null, preview(120)]) {
    expect(render(response)).toContain('<p class="tree-edit-preview" role="status" aria-live="polite" aria-atomic="true"></p>');
    expect(render(response)).not.toContain('Disconnected path');
  }
});

test('an unavailable engine budget renders as unknown until a preview supplies it', () => {
  const renderUnknown = (preview: TreeEditResponse | null) => renderToStaticMarkup(<TreePointSummary
    current={{ used: 0, max: null, weapon: [0, 0] }} preview={preview} invalidReason="" lang="en-US" />);
  expect(renderUnknown(null)).toContain('Used 0 / — points');
  expect(renderUnknown(preview(1))).toContain('Used 1 / 121 points');
});
