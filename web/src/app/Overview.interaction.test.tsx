import { isValidElement, type ComponentProps, type ReactNode } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { beforeEach, expect, it, vi } from 'vitest';
import type { BuildSession } from '../hooks/useBuildSession';
import type { AttributionResponse, CalculateBuildResponse, SkillMetricsEntry } from '../api/types';
import { Overview } from './Overview';
const h = vi.hoisted(() => ({ buttons: [] as ComponentProps<'button'>[], skills: null as SkillMetricsEntry[] | null,
  contribution: null as { version: number; selection: string; data: AttributionResponse } | null,
  details: { open: false, scrollIntoView: vi.fn(), querySelector: vi.fn() }, summaryFocus: vi.fn(),
}));
vi.mock('react', async original => ({ ...await original<typeof import('react')>(),
  useRef: () => ({ current: h.details }),
  useEffect: () => {},
  useState: (initial: unknown) => [initial === null && h.skills ? { version: 2, skills: h.skills } : initial, () => {}],
}));
vi.mock('../hooks/useSkillName', () => ({ useSkillCatalog: () => new Map() }));
vi.mock('../hooks/useMainMinion', () => ({ useMainMinion: () => null }));
vi.mock('../hooks/useUpgradeGoal', () => ({ useUpgradeGoal: () => ({ goal: {}, setGoal: () => {} }) }));
vi.mock('./jobs/useCancellableJob', () => ({ useCancellableJob: () => ({ result: h.contribution, running: false }) }));
vi.mock('../components/ui/button', () => ({ Button: (props: ComponentProps<'button'>) => { h.buttons.push(props); return <button {...props} />; } }));
const baseSession = { socketGroups: [{ enabled: true, gems: [] }, { enabled: true, gems: [] }], hasBuildContent: true,
  classNames: { classes: {}, ascendancies: {} }, calcParams: { main_socket_group: 1 }, stateVersion: 2,
  inspectSkill: vi.fn(), updateParams: vi.fn(), setSocketGroups: vi.fn(),
} as unknown as BuildSession;
const calc = { stats: [{ id: 'TotalNetRegen', value: -5 }, { id: 'SpiritUnreserved', value: -10 }],
  unsupported_modifiers: ['Some modifier'], item_errors: [], main_skill: { skill_id: 'Fireball', group_index: 1, active_skill_index: 1, combined_dps: 200 },
} as unknown as CalculateBuildResponse;
const focusStat = vi.fn(), onSpirit = vi.fn(), navigate = vi.fn();
function render() {
  h.buttons = [];
  return renderToStaticMarkup(<Overview session={baseSession} view={{ calc, previous: null, stale: false }} lang="en-US" focusStat={focusStat} onSpirit={onSpirit} navigate={navigate} />);
}
function click(label: string) { h.buttons.find(button => renderToStaticMarkup(<>{button.children}</>).includes(label))!.onClick!({} as never); }
beforeEach(() => { vi.clearAllMocks(); h.skills = null; h.contribution = null; h.details.open = false; h.details.querySelector.mockReturnValue({ focus: h.summaryFocus }); });
it('takes recovery, modifiers, and Spirit to the actual problem', () => {
  render();
  click('View recovery'); expect(focusStat).toHaveBeenCalledWith('TotalNetRegen');
  click('View modifiers'); expect(h.details.open).toBe(true); expect(h.summaryFocus).toHaveBeenCalledOnce(); expect(h.details.scrollIntoView).toHaveBeenCalledOnce();
  click('View skills'); expect(onSpirit).toHaveBeenCalledOnce();
  expect(navigate).not.toHaveBeenCalled();
});
it('puts the global selection explanation above the ranking and marks the current skill', () => {
  h.skills = [{ group_index: 1, active_skill_index: 1, skill_id: 'Fireball', status: 'calculated', metrics: { combined_dps: 200 } }] as SkillMetricsEntry[];
  const html = render();
  expect(html.indexOf('Select a skill to use it for calculations across all pages.')).toBeLessThan(html.indexOf('class="ov-rank"'));
  expect(html).toContain('aria-pressed="true"'); expect(html).toContain('Current');
});

it.each([0, 200])('retains zero-DPS meta-trigger rows with finite bars when the top DPS is %s', top => {
  h.skills = [
    { group_index: 0, active_skill_index: 2, skill_id: 'CometPlayer', status: 'calculated',
      metrics: { combined_dps: 0 }, unmodelled_meta_trigger: { meta_gem_id: 'MetaCastOnDodgePlayer', host_skill_id: 'CometPlayer' } },
    { group_index: 1, active_skill_index: 2, skill_id: 'FireballPlayer', status: 'calculated',
      metrics: { combined_dps: 0 }, unmodelled_meta_trigger: { meta_gem_id: 'MetaCastFireSpellOnHitPlayer', host_skill_id: 'FireballPlayer' } },
    { group_index: 0, active_skill_index: 1, skill_id: 'SparkPlayer', status: 'calculated', metrics: { combined_dps: top } },
  ] as SkillMetricsEntry[];
  const ranking = render().match(/<ol class="ov-rank"[\s\S]*?<\/ol>/)![0];
  expect(ranking.match(/DPS is shown as 0\. Average hit is still calculated\./g)).toHaveLength(2);
  expect(ranking.match(/class="ov-rank-value">0<\/span>/g)).toHaveLength(2);
  expect(ranking.match(/style="width:0%"/g)).toHaveLength(2);
  expect(ranking.indexOf('Comet')).toBeLessThan(ranking.indexOf('Fireball'));
  expect(ranking).not.toMatch(/NaN|Infinity/);
  if (top > 0) {
    expect(ranking.indexOf('Spark')).toBeLessThan(ranking.indexOf('Comet'));
    expect(ranking).toContain('style="width:100%"');
  } else expect(ranking).not.toContain('Spark');
});
it('explains that source analysis needs recalculating after a skill switch', () => {
  h.contribution = { version: 1, selection: '0:1', data: { baseline: {}, entries: [] } };
  const html = render();
  expect(html).toContain('Calculated skill changed; run the analysis again');
  expect(html).not.toContain('The build changed since this was measured.');
});

it('shows the completed empty analysis when every source has zero removal loss', () => {
  h.contribution = { version: 2, selection: '1:1', data: {
    baseline: { CombinedDPS: 200 },
    entries: [0, 1].map(index => ({ kind: 'socket_group', id: String(index), deltas: { CombinedDPS: 0 } })),
  } };
  const html = render();
  expect(html).toContain('Nothing changes this number.');
  expect(html).not.toContain('class="ov-contrib"');
  expect(html).not.toContain('run the analysis again');
});

it('selects the exact active effect atomically and leaves the current row alone', () => {
  h.skills = [0, 1].map(group_index => ({ group_index, active_skill_index: group_index === 0 ? 2 : 1, skill_id: 'Fireball', status: 'calculated', metrics: { combined_dps: 200 } })) as SkillMetricsEntry[];
  const tree = Overview({ session: baseSession, view: { calc, previous: null, stale: false }, lang: 'en-US', focusStat, navigate });
  const buttons: ComponentProps<'button'>[] = [];
  function visit(node: ReactNode) {
    if (Array.isArray(node)) { node.forEach(visit); return; }
    if (!isValidElement<{ children?: ReactNode; 'aria-pressed'?: boolean }>(node)) return;
    if (node.type === 'button' && typeof node.props['aria-pressed'] === 'boolean') buttons.push(node.props);
    visit(node.props.children);
  }
  visit(tree);
  buttons.find(button => button['aria-pressed'] === false)!.onClick!({} as never);
  expect(baseSession.inspectSkill).toHaveBeenCalledWith(0, 2, 2);
  buttons.find(button => button['aria-pressed'] === true)!.onClick!({} as never);
  expect(baseSession.inspectSkill).toHaveBeenCalledTimes(1);
  expect(baseSession.setSocketGroups).not.toHaveBeenCalled();
  expect(baseSession.updateParams).not.toHaveBeenCalled();
});
