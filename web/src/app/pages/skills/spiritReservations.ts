import type { CalculateBuildRequest, CalculateBuildResponse } from '../../../api/types';

export interface SpiritReservationGroup {
  /** Ownership comes from engine source rows, independently of marginal release. */
  reserves: boolean;
  released: number;
}
export interface SpiritReservationReport {
  groups: Map<number, SpiritReservationGroup>;
  shared: Array<{ skillId: string; groups: number[] }>;
  limited: boolean;
}

/** Flat reservation sources already include the engine's support/configuration adjustments.
 * The source identifies an effect, not a socket group. Percentage reservations and sources
 * without this identity cannot be assigned to a group by the current response contract. */
function reservationSources(calc: CalculateBuildResponse) {
  const effects = new Map<string, number[]>();
  for (const source of calc.breakdowns?.SpiritReserved?.sources ?? []) {
    if (source.name !== 'SkillSpiritReservationBase') continue;
    for (const mod of source.mods) {
      if (!mod.active || mod.mod_type !== 'BASE' || !(typeof mod.value === 'number' && mod.value > 0)
        || mod.origin_kind !== 'SkillGem' || !mod.origin_id?.startsWith('spirit.')) continue;
      const id = mod.origin_id.slice('spirit.'.length);
      effects.set(id, [...(effects.get(id) ?? []), mod.value]);
    }
  }
  return effects;
}

/** Keep marginal release separate from reservation ownership. Duplicate effects can reserve
 * once while removing either group releases nothing; isolate each candidate provider and ask
 * the engine for its reservation source before marking it as a provider of that shared effect. */
export async function spiritReservations(request: CalculateBuildRequest, calculate: (request: CalculateBuildRequest) => Promise<CalculateBuildResponse>,
  signal: AbortSignal): Promise<SpiritReservationReport> {
  const run = async (candidate: CalculateBuildRequest) => {
    signal.throwIfAborted();
    const calc = await calculate({ ...candidate, breakdowns: ['SpiritReserved'] });
    signal.throwIfAborted();
    return calc;
  };
  const value = (calc: CalculateBuildResponse) => calc.stats.find(stat => stat.id === 'SpiritReserved')?.value;
  const baselineCalc = await run(request);
  const baseline = value(baselineCalc);
  if (typeof baseline !== 'number') throw new Error('reservation_unavailable');
  const sources = reservationSources(baselineCalc);
  const report: SpiritReservationReport = { groups: new Map(), shared: [],
    limited: Math.abs(baseline - [...sources.values()].flat().reduce((sum, amount) => sum + amount, 0)) > 0.000001 };
  const groups = request.socket_groups ?? [];
  for (const [index, group] of groups.entries()) {
    if (!group.enabled) continue;
    const reserved = value(await run({ ...request, socket_groups: groups.map((entry, i) => i === index ? { ...entry, enabled: false } : entry) }));
    if (typeof reserved !== 'number') throw new Error('reservation_unavailable');
    report.groups.set(index, { reserves: false, released: baseline - reserved });
  }
  for (const [skillId, amounts] of sources) {
    const candidates = groups.flatMap((group, index) => group.enabled
      && group.gems.some(gem => gem.enabled !== false && gem.skill_id === skillId) ? [index] : []);
    const providers: number[] = [];
    if (candidates.length === 1) providers.push(candidates[0]);
    else for (const index of candidates) {
      const isolated = await run({ ...request, socket_groups: groups.map((group, i) => i === index ? group : {
        ...group, gems: group.gems.map(gem => gem.skill_id === skillId ? { ...gem, enabled: false } : gem),
      }) });
      if (reservationSources(isolated).has(skillId)) providers.push(index);
    }
    for (const index of providers) report.groups.get(index)!.reserves = true;
    if (providers.length === 0) report.limited = true;
    // One positive source row, with multiple engine-confirmed providers, is a shared
    // reservation. Multiple rows may be independent minions; never invent a split.
    if (amounts.length === 1 && providers.length > 1) report.shared.push({ skillId, groups: providers });
    else if (amounts.length > 1 && providers.length > 1) report.limited = true;
  }
  if ([...report.groups.values()].some(group => group.released !== 0 && !group.reserves)) report.limited = true;
  return report;
}
