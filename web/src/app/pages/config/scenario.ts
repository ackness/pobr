import type { ConfigContext, ConfigOption, EnemyTier } from '../../../api/types';
import { configApplicability, type Applicability } from '../../../lib/configVisibility';

/** The fights a build can be measured against, in the order a player thinks of them. */
export const SCENARIOS = [
  { tier: 'none', en: 'Mapping', cn: '刷图', tw: '刷圖' },
  { tier: 'boss', en: 'Boss', cn: 'Boss', tw: '頭目' },
  { tier: 'pinnacle', en: 'Pinnacle', cn: '巅峰 Boss', tw: '巔峰頭目' },
  { tier: 'uber', en: 'Uber', cn: '终极 Boss', tw: '終極頭目' },
] as const satisfies ReadonlyArray<{ tier: EnemyTier; en: string; cn: string; tw: string }>;

/**
 * Which preset is active. A build whose enemy comes from its own imported settings carries no
 * tier of its own: that is "custom", and choosing a preset replaces it.
 */
export function activeScenario(tier: EnemyTier | undefined): EnemyTier | 'custom' {
  return SCENARIOS.some(scenario => scenario.tier === tier) ? tier as EnemyTier : 'custom';
}

/** A rule means the option only matters to builds with some skill, flag or condition. */
export function hasVisibilityRules(option: Pick<ConfigOption, 'visibility'>): boolean {
  return Object.values(option.visibility ?? {}).some(values => values?.length);
}

/**
 * Splits options into those the engine has confirmed matter to this build and the rest. An option
 * is confirmed only when it carries a rule and every rule family is satisfied by the calculation's
 * own context; an option with no rule applies to every build and is never singled out, and one
 * whose rules cannot be evaluated yet is not promised to matter. Order is preserved in both halves.
 */
export function partitionRelevant<T extends Pick<ConfigOption, 'visibility'>>(
  options: readonly T[], applicability: (option: T) => Applicability,
): { relevant: T[]; rest: T[] } {
  const relevant: T[] = [];
  const rest: T[] = [];
  for (const option of options) (hasVisibilityRules(option) && applicability(option) === 'applicable' ? relevant : rest).push(option);
  return { relevant, rest };
}

/**
 * What `partitionRelevant` asks of each option: does the calculation's own context confirm it? The values
 * of other options are left out on purpose. An option that depends on another one (`if_option`) is shown
 * in its own group once that one is set; promoting it as well would move it to the top of the page, away
 * from the control just changed, and out of sight when the leading group is folded.
 *
 * `placed` holds the options the reader has changed, as whether they were in the leading group when they
 * were. The context can start to read an option because it was set, so such a row keeps its place instead
 * of jumping under the pointer.
 */
export function confirmedByBuild(context: ConfigContext | null | undefined, placed: Readonly<Record<string, boolean>> = {}): (option: ConfigOption) => Applicability {
  return option => option.var in placed ? (placed[option.var] ? 'applicable' : 'not_applicable') : configApplicability(option, context, {});
}
