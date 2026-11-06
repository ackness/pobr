import { renderToStaticMarkup } from 'react-dom/server';
import { afterEach, expect, it, vi } from 'vitest';
import type { CalculateBuildResponse } from '../api/types';
import type { BuildSession } from '../hooks/useBuildSession';
import { UiProvider } from '../components/shared/UiContext';
import { TooltipProvider } from '../components/ui/tooltip';
import { ResultsRail } from './ResultsRail';
import type { Route } from './routes';
import { statLabel } from './inspector/statId';

vi.mock('../hooks/useSkillName', () => ({ useSkillCatalog: () => new Map() }));

afterEach(() => vi.unstubAllGlobals());

function render(route: Route, quantity = 1, withIgnite = 100, hasContent = true, dot = 0, lang: 'en-US' | 'zh-CN' | 'zh-TW' = 'en-US', hit = 100) {
  const calc = { stats: Object.entries({ CritMultiplier: 7.36, QuantityMultiplier: quantity, TotalDPS: hit,
    CombinedDPS: 100, TotalDotDPS: dot, TotalDot: 17, Life: 1000, EnergyShield: 500, TotalEHP: 5000, FireResist: 75, ColdResist: 60, LightningResist: 75, ChaosResist: -20, Spirit: 100, SpiritUnreserved: 100, WithIgniteDPS: withIgnite, WithBleedDPS: 100, WithPoisonDPS: 100, CombinedAvgToMonsterLife: 10 }).map(([id, value]) => ({ id, value })),
    main_skill: { skill_id: 'Fireball', hit_damage: [], combined_dps: 100, hit_dps: hit, dot_dps: 0 },
    item_errors: [], unsupported_modifiers: [], minions: [], breakdowns: {}, breakdown_descriptors: { TotalDotDPS: 'steps' },
  } as unknown as CalculateBuildResponse;
  const session = { calcParams: { main_socket_group: 0 }, socketGroups: hasContent ? [{ gems: [] }] : [], hasBuildContent: hasContent, calcMs: 77, dataVersion: '0.5.5.5' } as unknown as BuildSession;
  return renderToStaticMarkup(<UiProvider lang={lang}><TooltipProvider><ResultsRail route={route} session={session}
    view={{ calc, previous: null, stale: false }} lang={lang} onStatClick={() => {}} onNavigate={() => {}} pinned={[]} onTogglePin={() => {}} /></TooltipProvider></UiProvider>);
}

it.each([
  ['en-US', 'Hit DPS', 'Combined DPS'],
  ['zh-CN', '击中 DPS', '综合 DPS'],
  ['zh-TW', '擊中 DPS', '綜合 DPS'],
] as const)('shows one distinct Hit DPS detail and a Combined DPS headline in %s', (lang, hit, combined) => {
  const html = render('overview', 1, 100, true, 0, lang, 80);
  expect(html.match(new RegExp(`class="rail-label">${hit}</span>`, 'g'))).toHaveLength(1);
  expect(html).toMatch(new RegExp(`data-stat="TotalDPS"><span class="rail-label">${hit}</span><span class="rail-value"[^>]*>80</span>`));
  expect(html).toContain(`<span class="rail-hero-label">${combined}</span>`);
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
  // The player's zero is not shown as the build's damage; the selector also names the minion's DPS.
  expect(html).not.toContain('Combined DPS');
  expect(html).not.toContain('Your own combined DPS');
  expect(html.match(/5,307/g)).toHaveLength(2);
  expect(html).toContain('Minion · Wolf Companion');
  // The summoning skill's own offence group would read as the minion's.
  expect(html).not.toContain('Offence</button>');
  expect(html).toContain('Skill mechanics</button>');
});

it('shows core defences before folded details, with no repeated equivalent rows', () => {
  const html = render('overview');
  const essential = html.match(/class="rail-section rail-essentials"[\s\S]*?<\/section>/)![0];
  for (const label of ['Effective HP', 'Life', 'Fire Res.', 'Cold resistance', 'Lightning Res.', 'Chaos Res.', 'Unreserved Spirit']) expect(essential).toContain(label);
  expect(html).not.toContain('>Hit DPS<');
  expect(html).not.toContain('>Spirit<');
  expect(html.indexOf('rail-essentials')).toBeLessThan(html.indexOf('Offence</button>'));
  expect(html).toMatch(/aria-expanded="false"[^>]*>[^]*?Offence<\/button>/);
});
it('loads existing fold preferences, including an explicitly expanded rail', () => {
  vi.stubGlobal('localStorage', { getItem: () => '[]' });
  const html = render('overview');
  expect(html).not.toContain('data-folded');
  vi.stubGlobal('localStorage', { getItem: () => '["Offence","Defence"]' });
  expect(render('overview')).toMatch(/data-folded="true"[^]*?aria-expanded="false"[^]*?Offence/);
});
it('gives a first visit an import/create entry without blank-character stats', () => {
  const html = render('overview', 1, 100, false);
  expect(html).toContain('Import or create a build');
  expect(html).not.toContain('rail-row');
  expect(html).not.toContain('Life');
});

it('keeps total DoT DPS and its inspector action in folded Offence, distinct from skill-only DoT', () => {
  const html = render('overview', 1, 100, true, 40);
  const offence = html.match(/<section[^>]*data-folded="true"[^>]*>\s*<h3[^>]*>[\s\S]*?Offence[\s\S]*?<\/section>/)![0];
  expect(offence).toContain('aria-expanded="false"');
  expect(statLabel('en-US', 'TotalDotDPS')).toBe('Total DoT DPS');
  expect(offence).toContain('class="rail-row is-link" data-stat="TotalDotDPS"');
  expect(offence).toContain('>DoT DPS</span><span class="rail-value">40<');
  expect(offence).toContain('>Skill DoT DPS</span><span class="rail-value">17<');
  expect(render('overview')).not.toContain('data-stat="TotalDotDPS"');
});
