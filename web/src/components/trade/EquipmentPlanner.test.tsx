import type { ComponentProps } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { expect, test, vi } from 'vitest';
import type { BuildSession } from '../../hooks/useBuildSession';
import type { CalculateBuildRequest } from '../../api/types';
import type { EquipmentPlanningResult } from '../../lib/equipmentPlanner';
import { EquipmentPlanner } from './EquipmentPlanner';

const ui = vi.hoisted(() => ({ index: 0, snapshot: null as unknown }));
vi.mock('react', async original => ({ ...await original<typeof import('react')>(),
  useState: (initial: unknown) => [ui.index++ === 0 ? ui.snapshot : initial, vi.fn()],
}));
vi.mock('../../app/jobs/useJobBeacon', () => ({ useJobBeacon: () => {} }));
vi.mock('../shared/PreviewContext', () => ({ usePreview: () => null }));
vi.mock('../shared/controls', () => ({
  AppButton: (props: ComponentProps<'button'>) => <button {...props} />,
  AppSelectControl: (props: ComponentProps<'select'>) => <select {...props} />,
}));

test.each(['en-US', 'zh-CN', 'zh-TW'] as const)('planned resistance values have exactly one percent sign in %s', lang => {
  const request: CalculateBuildRequest = {};
  const session = { activeWeaponSet: 1, currentRequest: () => request } as unknown as BuildSession;
  const objective = { stat: 'Life', constraints: [] };
  const catalog = { bases: [], mods: [] };
  const result: EquipmentPlanningResult = { baseline: { Life: 100 }, evaluated: 1, limited: false, plans: [{
    candidateIds: [], items: [], unsupported: [], stats: { Life: 100, FireResist: 75, ColdResist: 75, LightningResist: 75, ChaosResist: -20 },
  }] };
  ui.index = 0;
  ui.snapshot = { key: JSON.stringify([1, request, [], 2, objective, undefined, undefined]), catalog, result };
  const html = renderToStaticMarkup(<EquipmentPlanner session={session} lang={lang} active objective={objective}
    catalog={catalog} candidates={[]} onRemove={() => {}} />);
  const resistances = html.match(/<p class="equipment-plan-resists">([\s\S]*?)<\/p>/)![1];
  expect(resistances.match(/ 75%<\/span>/g)).toHaveLength(3);
  expect(resistances).toContain(' -20%</span>');
  expect(resistances).not.toContain('%%');
});
