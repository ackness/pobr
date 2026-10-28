import type { EnemyTier } from '../../../api/types';
import { AppButton } from '../../../components/shared/controls';
import type { BuildSession } from '../../../hooks/useBuildSession';
import { enemyTierLabel, type Lang } from '../../../lib/i18n';
import { words } from '../../text';
import { resolveEnemyTier } from '../../enemy';
import { SCENARIOS } from './scenario';

/**
 * The fight the numbers are measured against, as one choice. Picking a preset sets the enemy tier
 * and nothing else; a build that takes its enemy from its own imported settings reads as custom.
 */
export function ScenarioBar({ session, lang }: { session: BuildSession; lang: Lang }) {
  const w = (en: string, cn: string, tw?: string) => words(lang, en, cn, tw);
  const current = resolveEnemyTier(session.calcParams.enemy_tier, session.calcParams.config_inputs);
  return <div className="cfg-scenario" role="group" aria-label={w('Enemy to fight', '对战的敌人', '對戰的敵人')}>
    <span className="cfg-scenario-label">{w('Fight', '对手', '對手')}</span>
    <div className="cfg-presets">
      {SCENARIOS.map(scenario => <AppButton key={scenario.tier} type="button" className="cfg-preset" aria-pressed={current === scenario.tier}
        disabled={session.busy} onClick={() => { if (current !== scenario.tier) session.updateParams({ enemy_tier: scenario.tier as EnemyTier }); }}>
        {w(scenario.en, scenario.cn, scenario.tw)}
      </AppButton>)}
    </div>
    <span className="cfg-scenario-note" role="status">
      {current === 'custom'
        ? w('Custom: the enemy comes from this BD’s own settings.', '自定义：敌人取自此 BD 自带的设置。', '自訂：敵人取自此 BD 自帶的設定。')
        : w(`Calculated against: ${enemyTierLabel(lang, current)}`, `当前对手：${enemyTierLabel(lang, current)}`, `目前對手：${enemyTierLabel(lang, current)}`)}
    </span>
  </div>;
}
