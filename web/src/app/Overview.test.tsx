import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it, vi } from 'vitest';
import type { BuildSession } from '../hooks/useBuildSession';
import { UpgradeGoalProvider } from '../hooks/useUpgradeGoal';
import { Overview } from './Overview';

vi.mock('../hooks/useSkillName', () => ({ useSkillCatalog: () => new Map() }));
vi.mock('./jobs/useCancellableJob', () => ({ useCancellableJob: () => ({ result: null, running: false }) }));

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
