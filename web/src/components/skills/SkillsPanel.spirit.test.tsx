import { renderToStaticMarkup } from 'react-dom/server';
import { beforeEach, expect, it, vi } from 'vitest';
import type { BuildSession } from '../../hooks/useBuildSession';
import { UiProvider } from '../shared/UiContext';
import { SkillsPanel } from './SkillsPanel';
const report = vi.hoisted(() => ({ value: { groups: new Map([[0, { reserves: true, released: 60 }], [1, { reserves: false, released: 0 }]]), shared: [] as Array<{ skillId: string; groups: number[] }>, limited: false } }));
beforeEach(() => { report.value = { groups: new Map([[0, { reserves: true, released: 60 }], [1, { reserves: false, released: 0 }]]), shared: [], limited: false }; });
vi.mock('../../app/pages/skills/useSpiritReservations', () => ({ useSpiritReservations: () => report.value }));
vi.mock('../../app/pages/skills/useSkillMetrics', () => ({ useSkillMetrics: () => null }));
vi.mock('./GemOptimizer', () => ({ GemOptimizer: () => null }));
vi.mock('./GemPicker', () => ({ GemPicker: () => null }));
vi.mock('./MinionSelection', () => ({ MinionSelection: () => null }));
vi.mock('../shared/NamedSets', () => ({ NamedSets: () => null }));
vi.mock('../shared/WeaponSetControl', () => ({ WeaponSetControl: () => null }));
it('marks only the groups that release Spirit and shows the measured cost in the group list', () => {
  const session = { socketGroups: [{ enabled: true, gems: [] }, { enabled: true, gems: [] }],
    calcParams: { main_socket_group: 0 }, socketGroupRevision: 1, stateVersion: 1, annotations: {}, library: { skillSets: [] },
    classNames: { classes: {}, ascendancies: {} },
  } as unknown as BuildSession;
  const html = renderToStaticMarkup(<UiProvider lang="en-US"><SkillsPanel session={session} lang="en-US" active focusSpirit={1} /></UiProvider>);
  expect(html.match(/data-spirit-reserving="true"/g)).toHaveLength(1);
  expect(html).toContain('Releases 60 Spirit when disabled');
  expect(html).toContain('Highlighted groups reserve Spirit according to engine sources.');
  expect(html).not.toContain('Releases 0 Spirit');
});

it('marks both providers of a shared reservation once without showing zero-cost claims', () => {
  report.value = { groups: new Map([[0, { reserves: true, released: 0 }], [1, { reserves: true, released: 0 }]]),
    shared: [{ skillId: 'Aura', groups: [0, 1] }], limited: true };
  const session = { socketGroups: [{ enabled: true, gems: [] }, { enabled: true, gems: [] }],
    calcParams: { main_socket_group: 0 }, socketGroupRevision: 1, stateVersion: 1, annotations: {}, library: { skillSets: [] },
    classNames: { classes: {}, ascendancies: {} },
  } as unknown as BuildSession;
  const html = renderToStaticMarkup(<UiProvider lang="en-US"><SkillsPanel session={session} lang="en-US" active focusSpirit={1} /></UiProvider>);
  expect(html.match(/data-spirit-reserving="true"/g)).toHaveLength(2);
  expect(html.match(/Groups 1, 2 share the Spirit reservation for Aura/g)).toHaveLength(1);
  expect(html).not.toContain('Releases 0 Spirit');
  expect(html).toContain('Limits: The engine cannot assign every reservation amount to a group.');
});
