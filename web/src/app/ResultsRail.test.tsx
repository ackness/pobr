import { renderToStaticMarkup } from 'react-dom/server';
import { expect, it, vi } from 'vitest';
import type { CalculateBuildResponse } from '../api/types';
import type { BuildSession } from '../hooks/useBuildSession';
import { UiProvider } from '../components/shared/UiContext';
import { TooltipProvider } from '../components/ui/tooltip';
import { ResultsRail } from './ResultsRail';
import type { Route } from './routes';

vi.mock('../hooks/useSkillName', () => ({ useSkillCatalog: () => new Map() }));

function render(route: Route, quantity = 1, withIgnite = 100) {
  const calc = { stats: Object.entries({ CritMultiplier: 7.36, QuantityMultiplier: quantity, TotalDPS: 100,
    CombinedDPS: 100, WithIgniteDPS: withIgnite, WithBleedDPS: 100, WithPoisonDPS: 100, CombinedAvgToMonsterLife: 10 }).map(([id, value]) => ({ id, value })),
    main_skill: { skill_id: 'Fireball', hit_damage: [], combined_dps: 100, hit_dps: 100, dot_dps: 0 },
    item_errors: [], unsupported_modifiers: [], minions: [], breakdowns: {}, breakdown_descriptors: {},
  } as unknown as CalculateBuildResponse;
  const session = { calcParams: { main_socket_group: 0 }, socketGroups: [{ gems: [] }], calcMs: 77, dataVersion: '0.5.5.5' } as unknown as BuildSession;
  return renderToStaticMarkup(<UiProvider lang="en-US"><TooltipProvider><ResultsRail route={route} session={session}
    view={{ calc, previous: null, stale: false }} lang="en-US" onStatClick={() => {}} onNavigate={() => {}} pinned={[]} onTogglePin={() => {}} /></TooltipProvider></UiProvider>);
}

it('hides the duplicate attention link on Overview and names destinations elsewhere', () => {
  expect(render('overview')).not.toContain('class="rail-health"');
  expect(render('skills')).toContain('View in Overview');
  expect(render('overview')).toContain('Skill values (Calculations)');
});

it('hides neutral quantity and duplicate ailment totals while preserving distinct values', () => {
  const html = render('overview');
  expect(html).not.toContain('Quantity Multiplier');
  expect(html).not.toContain('DPS including Ignite');
  expect(html).not.toContain('DPS including Bleed');
  expect(html).not.toContain('DPS including Poison');
  expect(html).toContain('Critical damage bonus');
  expect(html).toContain('+636%');
  expect(html).toContain('Enemy life dealt per hit');
  expect(render('overview', 2, 150)).toContain('Quantity Multiplier');
  expect(render('overview', 2, 150)).toContain('DPS including Ignite');
});
