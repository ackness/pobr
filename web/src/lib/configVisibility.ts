import type { ConfigContext, ConfigInputValue, ConfigOption } from '../api/types';
import { configDefaultValue } from './configDefaults';
import gameRelevance from './configGameRelevance.json' with { type: 'json' };

export type Applicability = 'applicable' | 'not_applicable' | 'unknown';

/** Generated from the same game-data rule as config:coverage. Unknown versions
 * stay visible until their snapshot has been audited. Saved inputs are separate. */
export function configExistsInGame(option: ConfigOption, dataVersion: string | null | undefined): boolean {
  return dataVersion !== gameRelevance.data_version || !Object.hasOwn(gameRelevance.excluded, option.var);
}

/** Resolve dependencies against defaults without turning defaults into saved values. */
export function configVisibilityInputs(options: ConfigOption[], inputs: Record<string, ConfigInputValue>): Record<string, ConfigInputValue> {
  const defaults: Record<string, ConfigInputValue> = {};
  for (const option of options) {
    const value = configDefaultValue(option);
    if (value !== undefined) defaults[option.var] = value;
  }
  return { ...defaults, ...inputs };
}

/** Alternatives within a family are OR; separate families are AND. */
export function configApplicability(
  option: ConfigOption,
  context: ConfigContext | null | undefined,
  inputs: Record<string, ConfigInputValue>,
): Applicability {
  let unknown = false;
  for (const [family, arguments_] of Object.entries(option.visibility ?? {})) {
    if (!arguments_?.length) continue;
    const results = arguments_.map(argument => {
      if (family === 'if_option') {
        // Catalog option truthiness: explicit zero and empty string are values.
        return Object.hasOwn(inputs, argument) && inputs[argument] !== false;
      }
      return context?.predicates[family]?.[argument] ?? null;
    });
    if (results.includes(true)) continue;
    if (results.includes(null)) unknown = true;
    else return 'not_applicable';
  }
  return unknown ? 'unknown' : 'applicable';
}

export function showConfigOption(option: ConfigOption, applicability: Applicability,
  inputs: Record<string, ConfigInputValue>, allOptions: boolean): boolean {
  return allOptions || Object.hasOwn(inputs, option.var) || applicability !== 'not_applicable';
}
