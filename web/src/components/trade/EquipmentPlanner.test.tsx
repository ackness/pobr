import type { ComponentProps } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { beforeEach, expect, test, vi } from 'vitest';
import type { BuildSession } from '../../hooks/useBuildSession';
import { EquipmentPlanner } from './EquipmentPlanner';
import { minionDpsNote } from '../../lib/mainMinion';

const ui = vi.hoisted(() => ({ buttons: [] as ComponentProps<'button'>[], plan: vi.fn() }));
const hooks = await vi.hoisted(async () => {
  const { createHookHarness } = await import('../../testing/hookHarness');
  return createHookHarness({ refs: true, memo: 'deps', callback: 'deps',
    effects: { comparison: 'objectIs', cleanup: 'previous' }, layoutEffect: 'effect' });
});
vi.mock('react', async original => hooks.mockReact(await original<typeof import('react')>()));

vi.mock('../../app/jobs/useJobBeacon', () => ({ useJobBeacon: () => {} }));
vi.mock('../shared/PreviewContext', () => ({ usePreview: () => null }));
vi.mock('../shared/controls', () => ({ AppButton: (props: ComponentProps<'button'>) => { ui.buttons.push(props); return <button {...props} />; },
  AppSelectControl: (props: ComponentProps<'select'>) => <select {...props} /> }));
vi.mock('../../lib/equipmentPlanner', async original => ({ ...await original<typeof import('../../lib/equipmentPlanner')>(),
  planEquipmentUpgrades: (...args: unknown[]) => ui.plan(...args),
}));
beforeEach(() => { hooks.reset(); ui.plan.mockReset(); });

test('recommends a minion-only equipment gain and displays its headline, percentage and per-minion note', async () => {
  const session = { currentRequest: () => ({}), activeWeaponSet: 1, items: [], setItems: vi.fn() } as unknown as BuildSession;
  const candidates = [{ id: 'one', itemId: 'item', label: 'Minion ring', slot: 'ring1', text: 'Candidate' }];
  const catalog = { bases: [], mods: [] }, objective = { stat: 'MainMinionDPS', constraints: [] };
  const damage = { stat: 'MainMinionDPS', label: 'Minion hit DPS', note: minionDpsNote('en-US', 'Wolf', 7) };
  const onRemove = vi.fn();
  const render = () => {
    for (let count = 0; count < 10; count++) {
      hooks.beginRender(); ui.buttons = [];
      const html = renderToStaticMarkup(<EquipmentPlanner session={session} lang="en-US" active objective={objective}
        catalog={catalog} candidates={candidates} onRemove={onRemove} damage={damage} />);
      hooks.runEffects();
      if (!hooks.dirty) return html;
    }
    throw new Error('Render did not settle');
  };
  ui.plan.mockResolvedValue({ baseline: { TotalDPS: 0, CombinedDPS: 0, MainMinionDPS: 100, TotalEHP: 200, Life: 100 },
    plans: [{ candidateIds: ['one'], items: [{ slot: 'ring1', text: 'Candidate' }], unsupported: [],
      stats: { TotalDPS: 0, CombinedDPS: 0, MainMinionDPS: 150, TotalEHP: 200, Life: 100 } }], evaluated: 1 });
  render();
  await ui.buttons.find(button => button.children === 'Compare one- and two-item plans')!.onClick!({} as never);
  const html = render()!;
  expect(ui.plan).toHaveBeenCalledWith(expect.objectContaining({ objective }), { catalog });
  expect(html).toMatch(/<span>Minion hit DPS<\/span>[\s\S]*?\+50%/);
  expect(html).toContain('100 → <b data-plan-stat="MainMinionDPS">150');
  expect(html).toContain('Wolf · per minion · limit 7');
  expect(html).not.toContain('data-plan-stat="TotalDPS"');
  expect(ui.buttons.find(button => button.children === 'Apply equipment plan')?.disabled).toBeFalsy();
});

test.each(['en-US', 'zh-CN', 'zh-TW'] as const)('planned resistance values have exactly one percent sign in %s', async lang => {
  const request = {};
  const session = { activeWeaponSet: 1, currentRequest: () => request } as unknown as BuildSession;
  const objective = { stat: 'Life', constraints: [] }, catalog = { bases: [], mods: [] };
  const candidates = [{ id: 'one', itemId: 'item', label: 'Ring', slot: 'ring1', text: 'Candidate' }];
  const damage = { stat: 'CombinedDPS', label: 'Combined DPS' };
  ui.plan.mockResolvedValue({ baseline: { Life: 100 }, evaluated: 1, plans: [{
    candidateIds: [], items: [], unsupported: [], stats: { Life: 100, FireResist: 75, ColdResist: 75, LightningResist: 75, ChaosResist: -20 },
  }] });
  const render = () => {
    for (let count = 0; count < 10; count++) {
      hooks.beginRender(); ui.buttons = [];
      const html = renderToStaticMarkup(<EquipmentPlanner session={session} lang={lang} active objective={objective}
        catalog={catalog} candidates={candidates} onRemove={() => {}} damage={damage} />);
      hooks.runEffects();
      if (!hooks.dirty) return html;
    }
    throw new Error('Render did not settle');
  };
  render();
  await ui.buttons.find(button => button.className === 'trade-primary')!.onClick!({} as never);
  const resistances = render()!.match(/<p class="equipment-plan-resists">([\s\S]*?)<\/p>/)![1];
  expect(resistances.match(/ 75%<\/span>/g)).toHaveLength(3);
  expect(resistances).toContain(' -20%</span>');
  expect(resistances).not.toContain('%%');
});
