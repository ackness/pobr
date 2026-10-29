import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import type { EnemyTier } from '../../../api/types';
import type { BuildSession } from '../../../hooks/useBuildSession';
import { UiProvider } from '../../../components/shared/UiContext';
import { TooltipProvider } from '../../../components/ui/tooltip';
import { ScenarioBar } from './ScenarioBar';

const render = (tier: EnemyTier | undefined, lang: 'en-US' | 'zh-CN' | 'zh-TW' = 'en-US', configInputs: Record<string, unknown> = {}) => renderToStaticMarkup(
  <UiProvider lang={lang}><TooltipProvider>
    <ScenarioBar lang={lang} session={{ calcParams: { enemy_tier: tier, config_inputs: configInputs }, busy: false, updateParams: () => {} } as unknown as BuildSession} />
  </TooltipProvider></UiProvider>);

const pressed = (html: string) => [...html.matchAll(/aria-pressed="(true|false)"[^>]*>([^<]*)</g)].filter(match => match[1] === 'true').map(match => match[2]);

describe('ScenarioBar', () => {
  it('presses exactly the preset that matches the enemy tier', () => {
    expect(pressed(render('boss'))).toEqual(['Boss']);
    expect(pressed(render('none'))).toEqual(['Mapping']);
    expect(pressed(render('uber'))).toEqual(['Uber']);
  });

  it('shows the engine default, a Pinnacle boss, when the build sets no enemy', () => {
    expect(pressed(render(undefined))).toEqual(['Pinnacle']);
  });

  it.each([
    ['en-US', 'Using the build’s enemy settings.'],
    ['zh-CN', '使用构筑中的敌人设置。'],
    ['zh-TW', '使用 BD 中的敵人設定。'],
  ] as const)('explains the custom enemy settings in %s without selecting a preset', (lang, message) => {
    const html = render(undefined, lang, { enemyIsBoss: 'Standard' });
    expect(pressed(html)).toEqual([]);
    expect(html).toContain(`role="status">${message}</span>`);
  });

  it.each([
    ['en-US', 'Pinnacle', 'Mapping'],
    ['zh-CN', '巅峰 Boss', '刷图'],
    ['zh-TW', '巔峰頭目', '刷圖'],
  ] as const)('identifies the selected enemy in %s without a duplicate status message', (lang, pinnacle, mapping) => {
    const html = render('pinnacle', lang);
    expect(pressed(html)).toEqual([pinnacle]);
    expect(html).not.toContain('role="status"');
    expect(pressed(render('none', lang))).toEqual([mapping]);
  });
});
