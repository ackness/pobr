import type { CalculateBuildResponse, EnemyTier } from '../../../api/types';
import { EnergyTriggerNote } from '../../../components/config/EnergyTriggerNote';
import { EnemyTierSummary } from '../../../components/config/EnemyTierSummary';
import type { EnemyPresets } from '../../../api/enemyPresets';
import { AppButton } from '../../../components/shared/controls';
import type { BuildSession } from '../../../hooks/useBuildSession';
import { bindT, enemyTierLabel, type Lang } from '../../../lib/i18n';
import { resolveEnemyTier } from '../../enemy';
import { SCENARIOS } from './scenario';

/**
 * The fight the numbers are measured against, as one choice. Picking a preset sets the enemy tier
 * and nothing else; a build that takes its enemy from its own imported settings reads as custom.
 */
export function ScenarioBar({ session, lang, calc, presets = null, inherited = {} }: { session: BuildSession; lang: Lang; calc?: CalculateBuildResponse | null;
  presets?: EnemyPresets | null; inherited?: Readonly<Record<string, number>> }) {
  const tt = bindT(lang);
  const current = resolveEnemyTier(session.calcParams.enemy_tier, session.calcParams.config_inputs);
  return <div className="cfg-scenario" role="group" aria-label={tt('compare.enemy')}>
    <span className="cfg-scenario-label">{tt('compare.enemy')}</span>
    <div className="cfg-presets segmented">
      {SCENARIOS.map(scenario => <AppButton key={scenario.tier} type="button" className="cfg-preset" aria-pressed={current === scenario.tier}
        disabled={session.busy} onClick={() => { if (current !== scenario.tier) session.updateParams({ enemy_tier: scenario.tier as EnemyTier }); }}>
        {enemyTierLabel(lang, scenario.tier)}
      </AppButton>)}
    </div>
    {presets && current !== 'custom' && <details className="cfg-preset-details">
      <summary className="disclosure">{tt('config.viewEnemyPreset')}{inherited.enemyLevel !== undefined && ` · ${tt('config.enemyLevelSummary').replace('{level}', String(inherited.enemyLevel))}`}</summary>
      <EnemyTierSummary table={presets} tier={current} inputs={session.calcParams.config_inputs} inherited={inherited} lang={lang} />
    </details>}
    <EnergyTriggerNote calc={calc} groups={session.socketGroups} lang={lang} />
    {current === 'custom' && <span className="cfg-scenario-note" role="status">
      {tt('config.customEnemyHint')}
    </span>}
  </div>;
}
