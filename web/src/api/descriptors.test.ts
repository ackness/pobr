import { describe, expect, it, vi } from 'vitest';
import type { PobrBackend } from './backend';
import { withBreakdownDescriptors } from './descriptors';
import type { CalculateBuildResponse } from './types';

const table = { Life: 'steps', Str: 'sources' } as const;

function backend(response: () => Partial<CalculateBuildResponse>) {
  const fetchTable = vi.fn(async () => ({ ...table }));
  const calculateBuild = vi.fn(async () => response() as CalculateBuildResponse);
  const target = { breakdownDescriptors: fetchTable, calculateBuild, dataVersion: '0.5', decodeBuild: async () => 'decoded' };
  return { target: target as unknown as PobrBackend, fetchTable, calculateBuild };
}

describe('breakdown descriptors', () => {
  it('fetches the table once and puts that one object into every response', async () => {
    const { target, fetchTable } = backend(() => ({ stats: [], breakdowns: {} }));
    const wrapped = await withBreakdownDescriptors(target);
    const first = await wrapped.calculateBuild({ pob_code: 'a' });
    const second = await wrapped.calculateBuild({ pob_code: 'b' });
    expect(first.breakdown_descriptors).toEqual(table);
    expect(second.breakdown_descriptors).toBe(first.breakdown_descriptors);
    expect(fetchTable).toHaveBeenCalledTimes(1);
  });

  it('leaves a table the response already carries, and every other member, alone', async () => {
    const inline = { Life: 'derived' } as unknown as CalculateBuildResponse['breakdown_descriptors'];
    const { target, calculateBuild } = backend(() => ({ stats: [], breakdowns: {}, breakdown_descriptors: inline }));
    const wrapped = await withBreakdownDescriptors(target);
    const request = { pob_code: 'a', include_descriptors: true };
    expect((await wrapped.calculateBuild(request)).breakdown_descriptors).toBe(inline);
    expect(calculateBuild).toHaveBeenCalledWith(request);
    expect(wrapped.dataVersion).toBe('0.5');
    expect(await wrapped.decodeBuild('x')).toBe('decoded');
    // Awaiting the wrapper (as an async factory's result is) must not treat it as a thenable.
    expect(await Promise.resolve(wrapped)).toBe(wrapped);
  });
});
