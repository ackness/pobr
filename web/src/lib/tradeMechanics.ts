export interface SituationalAffix {
  id: string;
  line: string;
  value: number;
  kind: 'projectiles' | 'area' | 'debuff' | 'buff';
  delta?: number;
}

/** Measured utility stays separate from damage scores. Debuff candidates need
 * an explicit enemy state and uptime; recognizing their wording is not a claim
 * that the engine supports every effect or that this build can sustain it.
 */
export function situationalAffix(stat: { id: string; line: string; value: number },
  before: Record<string, number>, after: Record<string, number>): SituationalAffix | undefined {
  // Neither the pinned PoB2 parser nor our engine models Puppet Master uptime.
  // Keep its verified market filter usable without turning chance into damage.
  if (/Surpassing Chance to gain a Puppet Master stack whenever you use a Command Skill/i.test(stat.line)) {
    return { ...stat, kind: 'buff' };
  }
  for (const [metric, kind] of [['ProjectileCount', 'projectiles'], ['AoeRadius', 'area']] as const) {
    const delta = (after[metric] ?? 0) - (before[metric] ?? 0);
    if ((before[metric] ?? 0) > 0 && Number.isFinite(delta) && delta > 0) return { ...stat, kind, delta };
  }
  if (/\b(?:Exposure|Wither|Withered|Intimidate|Unnerve)\b|Curse Enemies|Enemies you .*Curse|chance to (?:Shock|Chill|Freeze|Electrocute)/i.test(stat.line)) {
    return { ...stat, kind: 'debuff' };
  }
  return undefined;
}
