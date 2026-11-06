import type { EnemyPresets } from '../../api/enemyPresets';
import type { ConfigInputValue, EnemyTier } from '../../api/types';
import { encounterPreset, encounterValue, encounterValueLabel, presetRatio } from '../../lib/configEncounter';
import { configLabel } from '../../lib/configLabels';
import { bindT, enemyTierLabel, type Lang } from '../../lib/i18n';
import { formatNumber } from '../../lib/statDisplay';

const RESISTANCES = [
  ['enemyFireResist', 'Enemy fire resistance'], ['enemyColdResist', 'Enemy cold resistance'],
  ['enemyLightningResist', 'Enemy lightning resistance'], ['enemyChaosResist', 'Enemy chaos resistance'],
] as const;

// Config option labels end with a colon; the summary lays labels out as a column instead.
const plainLabel = (label: string) => label.replace(/\s*[:：]\s*$/, '');

export function EnemyTierSummary({ table, tier, inputs, inherited, lang }: {
  table: EnemyPresets | null; tier: EnemyTier | 'custom'; inputs: Record<string, ConfigInputValue>;
  inherited: Readonly<Record<string, number>>; lang: Lang;
}) {
  const preset = encounterPreset(table, tier);
  if (!preset || tier === 'custom') return null;
  const tt = bindT(lang);
  const rows: { key: string; label: string; value: string; source: string }[] = RESISTANCES.map(([key, label]) => ({
    key, label: plainLabel(configLabel(lang, { var: key, label })),
    value: encounterValueLabel(lang, { var: key, input_type: 'count' }, encounterValue({ var: key, input_type: 'count' }, inputs[key], inherited)),
    source: tt(inputs[key] === undefined ? 'config.tierPreset' : 'config.quickConfigured'),
  }));
  rows.push({ key: 'minimum', label: tt('config.presetMinLevel'), value: String(preset.min_level), source: tt('config.tierPreset') });
  if (inherited.enemyLevel !== undefined) rows.push({ key: 'level', label: plainLabel(configLabel(lang, { var: 'enemyLevel', label: 'Enemy level' })),
    value: String(inherited.enemyLevel), source: tt(typeof inputs.enemyLevel === 'number' && inputs.enemyLevel > 0 ? 'config.quickConfigured' : 'config.presetLevelSource') });
  for (const [key, label] of [['armour_mult_pct', 'config.presetArmourMultiplier'], ['evasion_mult_pct', 'config.presetEvasionMultiplier']] as const)
    rows.push({ key, label: tt(label), value: `${formatNumber(presetRatio(preset[key]), 2, 0)}%`, source: tt('config.tierPreset') });
  for (const [name, label] of [['CurseEffectOnSelf', 'config.presetCurse'], ['ExposureEffectOnSelf', 'config.presetExposure'], ['DamageTaken', 'config.presetDamageTaken']] as const) {
    preset.enemy_mods.filter(mod => mod.name === name && mod.mod_type === 'MORE').forEach((mod, index) => rows.push({
      key: `${name}-${index}`, label: tt(label),
      value: tt(mod.value < 0 ? 'config.presetLess' : 'config.presetMore').replace('{value}', formatNumber(Math.abs(mod.value), 2, 0)),
      source: tt(mod.effective_only ? 'config.presetCombatOnly' : 'config.tierPreset'),
    }));
  }
  return <div className="cfg-tier-summary">
    <p>{tt('config.tierPreset')}{lang === 'en-US' ? ': ' : '：'}{enemyTierLabel(lang, tier)}</p>
    <p className="config-hint">{tt('config.presetBasis')}</p>
    <dl>{rows.map(row => <div key={row.key}><dt>{row.label}</dt><dd><span>{row.value}</span><small>{row.source}</small></dd></div>)}</dl>
    <p className="config-hint">{tt('config.presetScalingLimit')}</p>
  </div>;
}
