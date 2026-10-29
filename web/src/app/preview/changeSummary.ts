import type { StatChange } from '../motion/statChanges';

/** Stats a player decides by, shown first when they move. */
export const HEADLINE_STATS = ['CombinedDPS', 'TotalEHP', 'Life', 'EnergyShield', 'Mana', 'FireResist', 'ColdResist', 'LightningResist', 'ChaosResist', 'SpiritUnreserved'] as const;

/** The few changes worth showing in the bar: headline stats first, then the largest relative moves. */
export function summarizeChanges(changes: Map<string, StatChange>, max = 6): StatChange[] {
  const headline = HEADLINE_STATS.flatMap(id => changes.get(id) ?? []);
  const others = [...changes.values()].filter(change => !(HEADLINE_STATS as readonly string[]).includes(change.id))
    .sort((a, b) => Math.abs(b.percent ?? 0) - Math.abs(a.percent ?? 0) || Math.abs(b.delta) - Math.abs(a.delta));
  return [...headline, ...others].slice(0, max);
}
