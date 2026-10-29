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

  it('presses nothing and says so when the enemy comes from an input the app does not recognise', () => {
    const html = render(undefined, 'en-US', { enemyIsBoss: 'Standard' });
    expect(pressed(html)).toEqual([]);
    expect(html).toContain('Custom');
  });

  it('names the enemy being calculated, in the chosen language', () => {
    expect(render('pinnacle')).toContain('Pinnacle Boss');
    expect(render('pinnacle', 'zh-CN')).toContain('巅峰 Boss');
    expect(render('none', 'zh-TW')).toContain('刷圖');
  });
});
