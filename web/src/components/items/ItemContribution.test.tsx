import type { ComponentProps } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { beforeEach, expect, it, vi } from 'vitest';
import type { AttributionResponse } from '../../api/types';
import type { BuildSession } from '../../hooks/useBuildSession';
import { contributionFields, ItemContribution } from './ItemContribution';

const ui = vi.hoisted(() => ({ button: undefined as ComponentProps<'button'> | undefined }));
const hooks = await vi.hoisted(async () => {
  const { createHookHarness } = await import('../../testing/hookHarness');
  return createHookHarness({ lazyState: false, functionalUpdates: false, refs: { reuse: 'nullish' },
    effects: { comparison: 'strict', cleanup: 'latest' } });
});
vi.mock('react', async original => hooks.mockReact(await original<typeof import('react')>()));
vi.mock('../shared/controls', () => ({ AppButton: (props: ComponentProps<'button'>) => { ui.button = props; return <button {...props} />; } }));
beforeEach(() => { hooks.reset(); ui.button = undefined; });
const report: AttributionResponse = { baseline: { CombinedDPS: 1000, TotalEHP: 500 }, entries: [
  { kind: 'item', id: 'weapon1', deltas: { CombinedDPS: 300, TotalEHP: -20 } },
  { kind: 'item', id: 'ring1', deltas: { CombinedDPS: 10, TotalEHP: 50 } },
] };
const session = () => ({ stateVersion: 1, calc: {}, busy: false, runAttribution: vi.fn().mockResolvedValue(report) }) as unknown as BuildSession;
function render(current: BuildSession, slot = 'weapon1') {
  hooks.beginRender(); ui.button = undefined;
  const html = renderToStaticMarkup(<ItemContribution session={current} slot={slot} lang="en-US" />);
  hooks.runEffects();
  return html;
}

it('reuses the existing slot contribution with signed removal losses and explains swap gains', () => {
  const current = { ...session(), sourceContributions: report };
  const html = render(current);
  expect(html).toContain('Combined DPS 300 · Effective HP -20');
  expect(html).toContain('not the gain from swapping');
  expect(ui.button).toBeUndefined();
  expect(current.runAttribution).not.toHaveBeenCalled();
  expect(render(current, 'ring1')).toContain('Combined DPS 10 · Effective HP 50');
});

it.each([
  ['en-US', 'Loss if removed: Combined DPS 12,346 · Effective HP -20'],
  ['zh-CN', '移除此装备后的损失：综合 DPS 12,346 · 有效生命 -20'],
  ['zh-TW', '移除此裝備後的損失：綜合 DPS 12,346 · 有效生命 -20'],
] as const)('uses DPS and effective HP precision with localized punctuation in %s', (lang, expected) => {
  const current: BuildSession = { ...session(), sourceContributions: { ...report, entries: [
    { kind: 'item', id: 'weapon1', deltas: { CombinedDPS: 12345.67, TotalEHP: -20.49 } },
  ] } };
  hooks.beginRender();
  const html = renderToStaticMarkup(<ItemContribution session={current} slot="weapon1" lang={lang} />);
  expect(html).toContain(expected);
});

it('offers the existing attribution action when the slot result is missing and keeps absent numbers unknown', async () => {
  const current = session();
  expect(render(current)).toContain('No contribution result');
  await ui.button!.onClick!({} as never);
  await vi.waitFor(() => expect(render(current)).toContain('Combined DPS 300 · Effective HP -20'));
  expect(current.runAttribution).toHaveBeenCalledWith(contributionFields);
  expect(render(current, 'helmet')).toContain('No contribution result');
  expect(ui.button?.disabled).toBe(false);
  current.busy = true;
  render(current, 'helmet');
  expect(ui.button?.disabled).toBe(true);
});

it('discards a contribution calculation after the build changes', async () => {
  let resolve!: (report: AttributionResponse) => void;
  const pending = new Promise<AttributionResponse>(done => { resolve = done; });
  const current = session();
  vi.mocked(current.runAttribution).mockReturnValue(pending);
  render(current); ui.button!.onClick!({} as never);
  render(current);
  expect(ui.button?.disabled).toBe(true);
  current.stateVersion++;
  render(current);
  resolve(report); await pending; await Promise.resolve();
  expect(render(current)).toContain('No contribution result');
});
