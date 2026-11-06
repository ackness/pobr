import { renderToStaticMarkup } from 'react-dom/server';
import { afterEach, describe, expect, it, vi } from 'vitest';
import type { AttributionResponse, CalculateBuildResponse } from '../api/types';
import type { BuildSession } from '../hooks/useBuildSession';
import { UpgradeGoalProvider } from '../hooks/useUpgradeGoal';
import { Overview } from './Overview';

vi.mock('../hooks/useSkillName', () => ({ useSkillCatalog: () => new Map() }));
const job = vi.hoisted(() => ({ result: null as { version: number; data: AttributionResponse } | null, running: false }));
vi.mock('./jobs/useCancellableJob', () => ({ useCancellableJob: () => job }));
afterEach(() => { job.result = null; });

function render(busy: boolean, lang: 'en-US' | 'zh-CN' | 'zh-TW', overrides: Partial<BuildSession> = {}) {
  const session = { socketGroups: [{ gems: [] }], hasBuildContent: true, character: null,
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

function summary(stats: Record<string, number>, lang: 'en-US' | 'zh-CN' | 'zh-TW' = 'zh-CN', extra: Partial<CalculateBuildResponse> = {}) {
  const calc = { stats: Object.entries(stats).map(([id, value]) => ({ id, value })),
    item_errors: [], unsupported_modifiers: [], ...extra } as unknown as CalculateBuildResponse;
  const session = { socketGroups: [], hasBuildContent: true, classNames: { classes: {}, ascendancies: {} },
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
  const html = summary({ MainMinionDPS: 12345.67, ActiveMinionLimit: 7 }, 'zh-CN', {
    main_minion_index: 4,
    minions: [{ index: 4, name: 'Wolf', dps: 12345.67 } as NonNullable<CalculateBuildResponse['minions']>[number]],
  });
  expect(html).toContain('召唤物击中 DPS');
  expect(html).toContain('>12,346<span class="ov-source">');
  expect(html).toContain('移除后损失 11,000');
  expect(html).toContain('单只');
  expect(html).toContain('上限 7 只');
});
