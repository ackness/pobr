import type { ComponentProps } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { expect, test, vi } from 'vitest';
import type { CalculateBuildResponse } from '../../api/types';
import { NavigateContext } from '../../app/pages/navigate';
import { bindT } from '../../lib/i18n';
import { FullDpsView } from './CalcsPanel';

const button = vi.hoisted(() => ({ click: undefined as ComponentProps<'button'>['onClick'] }));
vi.mock('../shared/controls', () => ({
  AppButton: ({ onClick, ...props }: ComponentProps<'button'>) => {
    button.click = onClick;
    return <button onClick={onClick} {...props} />;
  },
}));
vi.mock('../../hooks/useSkillName', () => ({ useSkillName: () => (id: string) => id }));

test.each(['en-US', 'zh-CN', 'zh-TW'] as const)('Full DPS empty state opens Skills through shell navigation in %s', lang => {
  const goTo = vi.fn();
  const tt = bindT(lang);
  for (const calc of [null, { full_dps: { dps: 0, dot_dps: 0, skills: [] } }]) {
    const html = renderToStaticMarkup(<NavigateContext.Provider value={goTo}>
      <FullDpsView calc={calc as CalculateBuildResponse | null} lang={lang} />
    </NavigateContext.Provider>);
    expect(html).toContain(tt('calcs.goToSkills'));
    expect(html).not.toContain(tt('skills.fullDpsHint'));
    button.click!({} as never);
    expect(goTo).toHaveBeenLastCalledWith('skills');
  }
});

test('Full DPS displays count rules only when contributions exist', () => {
  const calc = { full_dps: { dps: 10, dot_dps: 0, skills: [{ skill_id: 'Fireball', group_index: 0,
    active_skill_index: 1, count: 2, dps: 10, error: null, unsupported_modifiers: [] }] } } as unknown as CalculateBuildResponse;
  const html = renderToStaticMarkup(<FullDpsView calc={calc} lang="en-US" />);
  expect(html).toContain(bindT('en-US')('skills.fullDpsHint'));
  expect(html).not.toContain('Go to Skills');
  expect(html).toContain('Fireball');
});
