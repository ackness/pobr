/** Whether a larger value of a stat is an improvement: 1 yes, -1 no, 0 it depends on the build. */
export type Polarity = 1 | -1 | 0;

const LOWER_IS_BETTER = /Cost$|Degen$|^Req(?:Str|Dex|Int)$|(?:Time|TimeToTrigger)$/;
const DEPENDS = /Duration$|^(?:Presence|AreaOfEffect)/;

export function statPolarity(id: string): Polarity {
  if (LOWER_IS_BETTER.test(id)) return -1;
  if (DEPENDS.test(id)) return 0;
  return 1;
}
