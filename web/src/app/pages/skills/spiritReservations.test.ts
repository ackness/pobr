import { expect, it, vi } from 'vitest';
import type { CalculateBuildRequest, CalculateBuildResponse, SocketGroupInput } from '../../../api/types';
import { spiritReservations } from './spiritReservations';
const group = (id: string): SocketGroupInput => ({ enabled: true, gems: [{ skill_id: id, level: 20, quality: 0 }] });
const response = (reserved: number, sources: Array<[string, number]> = []): CalculateBuildResponse => ({
  stats: [{ id: 'SpiritReserved', value: reserved }], breakdowns: { SpiritReserved: { sources: [{
    name: 'SkillSpiritReservationBase', mods: sources.map(([id, value]) => ({
      active: true, mod_type: 'BASE', value, origin_kind: 'SkillGem', origin_id: `spirit.${id}`,
    })),
  }] } },
}) as unknown as CalculateBuildResponse;
const request = { socket_groups: [group('Aura'), group('Buff'), { ...group('Disabled'), enabled: false }] } as CalculateBuildRequest;
it('keeps engine-confirmed ownership separate from release per enabled group without mutating the build', async () => {
  const calculate = vi.fn().mockResolvedValueOnce(response(160, [['Aura', 60], ['Buff', 100]]))
    .mockResolvedValueOnce(response(100)).mockResolvedValueOnce(response(60));
  const report = await spiritReservations(request, calculate, new AbortController().signal);
  expect(report).toEqual({ groups: new Map([[0, { reserves: true, released: 60 }], [1, { reserves: true, released: 100 }]]), shared: [], limited: false });
  expect(request.socket_groups?.map(group => group.enabled)).toEqual([true, true, false]);
  expect(calculate).toHaveBeenCalledTimes(3);
  expect(calculate.mock.calls[0][0].breakdowns).toEqual(['SpiritReserved']);
  expect(calculate.mock.calls[1][0].socket_groups.map((group: SocketGroupInput) => group.enabled)).toEqual([false, true, false]);
});
it('identifies duplicate groups sharing one engine reservation even when either removal releases zero', async () => {
  const duplicate = { socket_groups: [group('Aura'), group('Aura')] };
  const calculate = vi.fn(async (candidate: CalculateBuildRequest) => candidate.socket_groups?.some(group => group.enabled && group.gems.some(gem => gem.enabled !== false))
    ? response(60, [['Aura', 60]]) : response(0));
  const report = await spiritReservations(duplicate, calculate, new AbortController().signal);
  expect(report.groups).toEqual(new Map([[0, { reserves: true, released: 0 }], [1, { reserves: true, released: 0 }]]));
  expect(report.shared).toEqual([{ skillId: 'Aura', groups: [0, 1] }]);
  expect(report.limited).toBe(false);
  expect(calculate).toHaveBeenCalledTimes(5);
  // Each possible provider is confirmed by an engine request with the other copy disabled.
  expect(calculate.mock.calls.slice(3).map(([request]) => request.socket_groups!.map(group => group.gems[0].enabled !== false))).toEqual([[true, false], [false, true]]);
  expect(duplicate.socket_groups.every(group => group.gems[0].enabled === undefined)).toBe(true);
});
it('does not assign shared ownership to an inactive copy just because its skill ID matches', async () => {
  const duplicate = { socket_groups: [{ ...group('Aura'), weapon_set: 2 as const }, group('Aura')] };
  const calculate = vi.fn(async (candidate: CalculateBuildRequest) => candidate.socket_groups?.some(group => group.enabled && !group.weapon_set && group.gems[0].enabled !== false)
    ? response(60, [['Aura', 60]]) : response(0));
  const report = await spiritReservations(duplicate, calculate, new AbortController().signal);
  expect(report.groups.get(0)?.reserves).toBe(false);
  expect(report.groups.get(1)?.reserves).toBe(true);
  expect(report.shared).toEqual([]);
});
it('reports a limit and leaves duplicates unhighlighted when the engine supplies no ownership evidence', async () => {
  const calculate = vi.fn(async () => response(60));
  const report = await spiritReservations({ socket_groups: [group('Aura'), group('Aura')] }, calculate, new AbortController().signal);
  expect([...report.groups.values()].every(group => !group.reserves)).toBe(true);
  expect(report.shared).toEqual([]);
  expect(report.limited).toBe(true);
});
it('does not mistake a group changing another skill\'s reservation cost for a reserving group', async () => {
  const calculate = vi.fn().mockResolvedValueOnce(response(100, [['Aura', 100]]))
    .mockResolvedValueOnce(response(60)).mockResolvedValueOnce(response(0));
  const report = await spiritReservations({ socket_groups: [group('Damage'), group('Aura')] }, calculate, new AbortController().signal);
  expect(report.groups.get(0)).toEqual({ reserves: false, released: 40 });
  expect(report.groups.get(1)).toEqual({ reserves: true, released: 100 });
  expect(report.limited).toBe(true);
});
it('does not describe independent minion reservation rows as one shared reservation', async () => {
  const calculate = vi.fn(async (candidate: CalculateBuildRequest) => {
    const count = candidate.socket_groups?.filter(group => group.enabled && group.gems[0].enabled !== false).length ?? 0;
    return response(count * 60, Array.from({ length: count }, () => ['Minion', 60]));
  });
  const report = await spiritReservations({ socket_groups: [group('Minion'), group('Minion')] }, calculate, new AbortController().signal);
  expect([...report.groups.values()].every(group => group.reserves)).toBe(true);
  expect(report.shared).toEqual([]);
  expect(report.limited).toBe(true);
});
it('stops between requests when leaving the page', async () => {
  const controller = new AbortController();
  const calculate = vi.fn(async () => { controller.abort(); return response(160); });
  await expect(spiritReservations(request, calculate, controller.signal)).rejects.toThrow();
  expect(calculate).toHaveBeenCalledTimes(1);
});
it('does not invent a zero when the engine omits reservation data', async () => {
  const calculate = vi.fn(async () => ({ stats: [] }) as unknown as CalculateBuildResponse);
  await expect(spiritReservations(request, calculate, new AbortController().signal)).rejects.toThrow('reservation_unavailable');
});
