import { displayStatDefinition } from '../../lib/statDisplay';

/** Whether a larger value of a stat is an improvement: 1 yes, -1 no, 0 it depends on the build. */
export type Polarity = 1 | -1 | 0;

export function statPolarity(id: string): Polarity {
  const direction = displayStatDefinition(id)?.higher_is_better;
  return direction === false ? -1 : direction === null ? 0 : 1;
}
