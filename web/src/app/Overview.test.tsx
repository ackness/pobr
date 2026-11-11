import { renderToStaticMarkup } from 'react-dom/server';
import { Children, type ComponentProps, type ReactNode } from 'react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import type { AttributionResponse, CalculateBuildResponse, SocketGroupInput } from '../api/types';
import type { BuildSession } from '../hooks/useBuildSession';
import { UpgradeGoalProvider } from '../hooks/useUpgradeGoal';
import { Overview } from './Overview';
import type { ObjectiveState } from '../components/shared/OptimizerControls';

const ui = vi.hoisted(() => ({ buttons: [] as ComponentProps<'button'>[], setGoal: vi.fn(),
  goal: { preset: 'balanced', resistanceTarget: 75 } as ObjectiveState }));
vi.mock('../components/ui/button', () => ({ Button: (props: ComponentProps<'button'>) => { ui.buttons.push(props); return <button {...props} />; } }));
vi.mock('../hooks/useUpgradeGoal', () => ({ UpgradeGoalProvider: ({ children }: { children: ReactNode }) => children,
  useUpgradeGoal: () => ({ goal: ui.goal, setGoal: ui.setGoal }) }));

vi.mock('../hooks/useSkillName', () => ({ useSkillCatalog: () => new Map() }));
const job = vi.hoisted(() => ({ result: null as { version: number; data: AttributionResponse } | null, running: false }));
vi.mock('./jobs/useCancellableJob', () => ({ useCancellableJob: () => job }));
afterEach(() => { job.result = null; ui.buttons = []; ui.setGoal.mockClear(); });

function render(busy: boolean, lang: 'en-US' | 'zh-CN' | 'zh-TW', overrides: Partial<BuildSession> = {}) {
  const session = { socketGroups: [{ gems: [] }], hasBuildContent: true, character: null, calcParams: {},
    classNames: { classes: {}, ascendancies: {} }, busy, calcError: null, stateVersion: 1, ...overrides } as unknown as BuildSession;
  const html = renderToStaticMarkup(<UpgradeGoalProvider><Overview session={session}
    view={{ calc: null, previous: null, stale: busy }} lang={lang} navigate={() => {}} focusStat={() => {}} />
  </UpgradeGoalProvider>);
  return html.match(/<section[^>]+aria-labelledby="ov-skills"[\s\S]*?<\/section>/)![0];
}

describe('Overview skill damage', () => {
  it.each([
    ['en-US', 'Calculating…'], ['zh-CN', '计算中…'], ['zh-TW', '計算中…'],
  ] as const)('shows pending in %s before the separate skill request finishes', (lang, message) => {
    for (const busy of [true, false]) {
      const html = render(busy, lang);
      expect(html).toContain('aria-busy="true"');
      expect(html).toContain(`role="status">${message}</p>`);
      expect(html).not.toContain('No skill deals damage yet.');
      expect(html).not.toContain('暂时没有造成伤害的技能。');
      expect(html).not.toContain('暫時沒有造成傷害的技能。');
    }
  });

  it('shows an empty state when there are no skills to calculate', () => {
    const html = render(false, 'zh-CN', { socketGroups: [] });
    expect(html).toContain('aria-busy="false"');
    expect(html).toContain('暂时没有造成伤害的技能。');
  });

  it('does not leave a failed calculation pending or claim it is empty', () => {
    const html = render(false, 'zh-CN', { calcError: 'Failed' });
    expect(html).toContain('aria-busy="false"');
    expect(html).toContain('当前计算不可用。');
    expect(html).not.toContain('暂时没有造成伤害的技能。');
  });
});

function summary(stats: Record<string, number>, {
  lang = 'zh-CN', extra = {}, groups = [],
}: { lang?: 'en-US' | 'zh-CN' | 'zh-TW'; extra?: Partial<CalculateBuildResponse>; groups?: SocketGroupInput[] } = {}) {
  const calc = { stats: Object.entries(stats).map(([id, value]) => ({ id, value })),
    item_errors: [], unsupported_modifiers: [], ...extra } as unknown as CalculateBuildResponse;
  const session = { socketGroups: groups, calcParams: {}, hasBuildContent: true, classNames: { classes: {}, ascendancies: {} },
    stateVersion: 1 } as unknown as BuildSession;
  return renderToStaticMarkup(<UpgradeGoalProvider><Overview session={session} view={{ calc, previous: null, stale: false }}
    lang={lang} navigate={() => {}} focusStat={() => {}} /></UpgradeGoalProvider>);
}

it('shows a positive Spirit excess with its limit, the weakest hit and shield-dominant total', () => {
  const html = summary({ Life: 1000, EnergyShield: 5000, Spirit: 100, SpiritUnreserved: -109, PhysicalMaxHit: 19203 });
  expect(html).toContain('精魂超出');
  expect(html).toContain('>109<span class="ov-source">来源 ›</span>');
  expect(html).toContain('上限 100');
  expect(html).toContain('最怕物理：单次最多承受 19,203');
  expect(html).toContain('生命 + 能量护盾 6,000');
  expect(html.match(/class="ov-source"/g)).toHaveLength(4);
  expect(summary({ Life: 5000, EnergyShield: 1000, SpiritUnreserved: 20 })).not.toContain('生命 + 能量护盾');
  expect(summary({ SpiritUnreserved: 0 })).not.toContain('精魂超出');
});

it('keeps the overlap explanation after showing removal losses and gains', () => {
  job.result = { version: 1, data: { baseline: { CombinedDPS: 1000 }, entries: [
    { kind: 'item', id: 'weapon1', deltas: { CombinedDPS: 900 } },
    { kind: 'socket_group', id: '1', deltas: { CombinedDPS: -200 } },
  ] } };
  const html = summary({});
  expect(html).toContain('移除后损失 900');
  expect(html).toContain('移除后增加 200');
  expect(html).toContain('（90.0%）');
  expect(html).toContain('合计可超过 100%');
  expect(html).toContain('装备 · 主手');
  expect(html).toContain('技能组 2');
});

it('uses the main minion damage for the headline and source contributions with shared precision', () => {
  job.result = { version: 1, data: { baseline: { MainMinionDPS: 12345.67 }, entries: [
    { kind: 'item', id: 'weapon1', deltas: { MainMinionDPS: 10999.8 } },
  ] } };
  const html = summary({ MainMinionDPS: 12345.67, ActiveMinionLimit: 7 }, { lang: 'zh-CN', extra: {
    main_minion_index: 4,
    minions: [{ index: 4, name: 'Wolf', dps: 12345.67 } as NonNullable<CalculateBuildResponse['minions']>[number]],
  } });
  expect(html).toContain('召唤物击中 DPS');
  expect(html).toContain('>12,346<span class="ov-source">');
  expect(html).toContain('移除后损失 11,000');
  expect(html).toContain('单只');
  expect(html).toContain('上限 7 只');
});

it('shows all four base resistances in a compact summary', () => {
  const html = summary({ FireResist: 75, ColdResist: 60, LightningResist: 76, ChaosResist: -20 });
  const resistance = html.match(/<dl class="ov-resistances"[\s\S]*?<\/dl>/)![0];
  for (const text of ['火焰抗性', '冰冷抗性', '闪电抗性', '混沌抗性', '75%', '60%', '76%', '-20%']) expect(resistance).toContain(text);
});

it.each([74, 76])('opens resistance upgrades at %s using the returned cap instead of the saved default target', value => {
  summary({ FireResist: value, FireResistMax: 78 }, { lang: 'en-US' });
  const action = ui.buttons.find(button => Children.toArray(button.children).includes('Fix in Upgrades'));
  expect(action).toBeDefined();
  action!.onClick!({} as never);
  expect(ui.setGoal).toHaveBeenCalledWith(expect.objectContaining({ resistanceFirst: true, resistanceTarget: 78 }));
});

it('explains mapping beside the DPS only when the result confirms an energy trigger', () => {
  const energy_trigger = { maximum_energy: 100, source: 'SparkPlayer', at_own_rate: false, missing_source: null } as NonNullable<CalculateBuildResponse['energy_trigger']>;
  const html = summary({}, { lang: 'zh-CN', extra: { energy_trigger, main_skill: { group_index: 0, active_skill_index: 2, skill_id: 'CometPlayer', hit_damage: [], hit_dps: 10, dot_dps: 0, combined_dps: 10 } }, groups: [{ enabled: true, gems: [{ skill_id: 'MetaCastOnCritPlayer', level: 16, quality: 0 }, { skill_id: 'CometPlayer', level: 16, quality: 0 }] }, { enabled: true, gems: [{ skill_id: 'SparkPlayer', level: 16, quality: 0 }] }] });
  const damage = html.match(/<div[^>]+class="ov-figure is-dps"[\s\S]*?<\/div>/)![0];
  expect(damage).toContain('刷图按单个普通怪（力量 1）计算，清群时实际更快充满');
  expect(summary({})).not.toContain('刷图按单个普通怪');
});

it.each([
  ['en-US', 'Energy trigger rate is not modelled', 'DPS is shown as 0. Average hit is still calculated.'],
  ['zh-CN', '尚未建模能量触发频率', 'DPS 显示为 0。平均击中伤害仍会计算。'],
  ['zh-TW', '尚未模擬能量觸發頻率', 'DPS 顯示為 0。仍會計算平均擊中傷害。'],
] as const)('explains the zero main-skill DPS and adds a finding in %s', (lang, title, note) => {
  const html = summary({}, { lang, extra: {
    main_skill: { group_index: 0, active_skill_index: 2, skill_id: 'CometPlayer', hit_damage: [], hit_dps: 0, dot_dps: 0, combined_dps: 0 },
    unmodelled_meta_trigger: { meta_gem_id: 'MetaCastOnDodgePlayer', host_skill_id: 'CometPlayer' },
  } });
  const damage = html.match(/<div[^>]+class="ov-figure is-dps"[^]*?<\/div>/)![0];
  expect(damage).toContain('>0<span');
  expect(damage).toContain(note);
  const finding = html.match(/<ul class="ov-findings"[^]*?<\/ul>/)![0];
  expect(finding).toContain(title);
  expect(finding).toContain(note);
  expect(html).not.toContain('MetaCastOnDodgePlayer');
});
