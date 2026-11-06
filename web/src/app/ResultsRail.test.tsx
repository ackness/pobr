import { renderToStaticMarkup } from 'react-dom/server';
import { expect, it, vi } from 'vitest';
import type { CalculateBuildResponse, SocketGroupInput } from '../api/types';
import type { BuildSession } from '../hooks/useBuildSession';
import { UiProvider } from '../components/shared/UiContext';
import { TooltipProvider } from '../components/ui/tooltip';
import { ResultsRail } from './ResultsRail';
import type { Route } from './routes';

vi.mock('../hooks/useSkillName', () => ({ useSkillCatalog: () => new Map() }));

function render(route: Route, quantity = 1, withIgnite = 100, extra: Partial<CalculateBuildResponse> = {}, groups: SocketGroupInput[] = []) {
  const calc = { stats: Object.entries({ CritMultiplier: 7.36, QuantityMultiplier: quantity, TotalDPS: 100,
    CombinedDPS: 100, WithIgniteDPS: withIgnite, WithBleedDPS: 100, WithPoisonDPS: 100, CombinedAvgToMonsterLife: 10 }).map(([id, value]) => ({ id, value })),
    main_skill: { skill_id: 'Fireball', hit_damage: [], combined_dps: 100, hit_dps: 100, dot_dps: 0 },
    item_errors: [], unsupported_modifiers: [], minions: [], breakdowns: {}, breakdown_descriptors: {}, ...extra,
  } as unknown as CalculateBuildResponse;
  const session = { calcParams: { main_socket_group: 0 }, socketGroups: groups.length ? groups : [{ gems: [] }], calcMs: 77, dataVersion: '0.5.5.5' } as unknown as BuildSession;
  return renderToStaticMarkup(<UiProvider lang="en-US"><TooltipProvider><ResultsRail route={route} session={session}
    view={{ calc, previous: null, stale: false }} lang="en-US" onStatClick={() => {}} onNavigate={() => {}} pinned={[]} onTogglePin={() => {}} /></TooltipProvider></UiProvider>);
}

it('explains energy-trigger mapping beside the enemy tier only for a confirmed trigger', () => {
  const energy_trigger = { maximum_energy: 100, source: 'SparkPlayer', at_own_rate: false, missing_source: null } as NonNullable<CalculateBuildResponse['energy_trigger']>;
  const html = render('config', 1, 100, { energy_trigger, main_skill: { group_index: 0, active_skill_index: 2, skill_id: 'CometPlayer', hit_damage: [], hit_dps: 10, dot_dps: 0, combined_dps: 10 } }, [{ enabled: true, gems: [{ skill_id: 'MetaCastOnCritPlayer', level: 16, quality: 0 }, { skill_id: 'CometPlayer', level: 16, quality: 0 }] }, { enabled: true, gems: [{ skill_id: 'SparkPlayer', level: 16, quality: 0 }] }]);
  expect(html).toContain('Mapping is calculated against one normal monster (power 1); clearing packs fills energy faster');
  expect(render('config')).not.toContain('clearing packs');
});

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

it('leads a summoner build with one minion\'s hit DPS and keeps the player\'s own damage secondary', () => {
  const stats = Object.entries({ CombinedDPS: 0, TotalDPS: 0, ActiveMinionLimit: 7, MainMinionDPS: 5307.37 }).map(([id, value]) => ({ id, value, category: 'Offence' }));
  const minion = { index: 0, name: 'Wolf Companion', minion_id: 'wolf', summoned_by: 'WolfPackPlayer', skill_id: 'Bite', dps: 5307.37,
    life: 2261.7, armour: 974, evasion: 209, energy_shield: 0, stats: {}, breakdowns: {}, breakdown_descriptors: { TotalDPS: 'steps', Life: 'steps' } };
  const calc = { stats, main_skill: { skill_id: 'WolfPackPlayer', group_index: 0, active_skill_index: 1, hit_damage: [], combined_dps: 0, hit_dps: 0, dot_dps: 0 },
    item_errors: [], unsupported_modifiers: [], minions: [minion], main_minion_index: 0, breakdowns: {}, breakdown_descriptors: {},
  } as unknown as CalculateBuildResponse;
  const session = { calcParams: { main_socket_group: 0 }, socketGroups: [{ gems: [] }], calcMs: 77, dataVersion: '0.5.5.5' } as unknown as BuildSession;
  const html = renderToStaticMarkup(<UiProvider lang="en-US"><TooltipProvider><ResultsRail route="skills" session={session}
    view={{ calc, previous: null, stale: false }} lang="en-US" onStatClick={() => {}} onNavigate={() => {}} pinned={[]} onTogglePin={() => {}} /></TooltipProvider></UiProvider>);
  expect(html).toContain('<span class="rail-hero-label">Minion hit DPS</span>');
  expect(html).toMatch(/class="rail-hero-value is-link"[^>]*>[^<]*5,307/);
  expect(html).toContain('<span class="rail-hero-note">Wolf Companion · per minion · limit 7</span>');
  // The player's zero is not shown as the build's damage, and the minion's DPS isn't repeated below.
  expect(html).not.toContain('Combined DPS');
  expect(html).not.toContain('Your own combined DPS');
  expect(html.match(/5,307/g)).toHaveLength(1);
  expect(html).toContain('Minion · Wolf Companion');
  // The summoning skill's own offence group would read as the minion's.
  expect(html).not.toContain('Offence</button>');
  expect(html).toContain('Skill mechanics</button>');
});
