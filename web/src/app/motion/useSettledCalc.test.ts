import { beforeEach, expect, it, vi } from 'vitest';
import type { CalculateBuildResponse } from '../../api/types';
import type { BuildSession } from '../../hooks/useBuildSession';
import { useSettledCalc } from './useSettledCalc';
const held = vi.hoisted(() => ({ value: null as unknown }));
vi.mock('react', () => ({ useState: () => [held.value, (value: unknown) => { held.value = value; }] }));
beforeEach(() => { held.value = null; });
const result = (dps: number) => ({ stats: [{ id: 'CombinedDPS', value: dps }] }) as CalculateBuildResponse;
function render(calc: CalculateBuildResponse | null, group = 0, effect = 1, scope = 'stage') {
  const session = { calc, calcParams: { main_socket_group: group }, socketGroups: [{ main_active_skill: effect }, {}] } as BuildSession;
  useSettledCalc(session, scope);
  return useSettledCalc(session, scope);
}
it('does not compare skills as edits, including while waiting for their calculations', () => {
  const first = result(100), edit = result(110), otherSkill = result(50), nextEdit = result(55);
  expect(render(first).previous).toBeNull();
  expect(render(edit).previous).toBe(first);
  expect(render(null, 1).previous).toBeNull();
  expect(render(otherSkill, 1).previous).toBeNull();
  expect(render(nextEdit, 1).previous).toBe(otherSkill);
});
it('also resets comparisons on another active effect and another stage', () => {
  render(result(100));
  expect(render(result(50), 0, 2).previous).toBeNull();
  expect(render(result(20), 0, 2, 'other-stage').previous).toBeNull();
});
