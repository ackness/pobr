import { describe, expect, test, vi } from 'vitest';
import type { PobrBackend } from '../api/backend';
import type { PassiveNode } from '../api/types';
import { passivePlanningContext, withTravelAttributes } from './passivePlanner';
import { evaluateNodePower, filterPowerRows, NODE_POWER_LIMIT, powerCandidates, rankPowerRows, type PowerRow } from './nodePower';
import type { EvaluateOptions, EvaluateResult } from './optimize';

const nodes: PassiveNode[] = [
  { skill: 1, id: 'start', name: 'WITCH', kind: 'normal', connections: [2, 4] },
  { skill: 2, id: 'travel', name: 'Attribute', kind: 'normal', connections: [3], stats: ['+5 to any Attribute'] },
  { skill: 3, id: 'long', name: 'Long route', kind: 'notable', stats: ['Life'] },
  { skill: 4, id: 'short', name: 'Short route', kind: 'keystone', stats: ['Mana'] },
  { skill: 5, id: 'island', name: 'Island', kind: 'notable', stats: ['Life'] },
  { skill: 6, id: 'asc', name: 'Start', kind: 'ascendancy_start', ascendancy_id: 'Witch1', connections: [7] },
  { skill: 7, id: 'asc-notable', name: 'Asc notable', kind: 'notable', ascendancy_id: 'Witch1', stats: ['Life'] },
];
function candidates() { return powerCandidates(nodes, passivePlanningContext(nodes, [], 'Witch'), [], 'Witch1'); }
const probe = vi.fn<PobrBackend['nodePower']>(async (_request, _stat, _depth, ids) => ({ base: 100, entries: (ids ?? []).map((skill: number) => ({ skill, delta: 999, distance: 0 })) }));
const evaluate = async (opts: EvaluateOptions): Promise<EvaluateResult> => ({ baseline: { Life: 100, Mana: 100 }, aborted: false,
  results: opts.variants.map((variant, index) => {
    const ids = variant.allocate_nodes ?? [];
    return { index, label: null, error: null, stats: { Life: 100 + (ids.includes(2) ? 10 : 0) + (ids.includes(3) ? 10 : 0) + (ids.includes(4) ? 12 : 0), Mana: 100 + (ids.includes(3) ? 50 : 0) }, unsupported: [] };
  }),
});

describe('node power routes and ranking', () => {
  test('includes travel cost, own ascendancy routes, and explicit unreachable rows', () => {
    const rows = candidates();
    expect(rows.find(r => r.node.skill === 3)?.path).toEqual([2, 3]);
    expect(rows.find(r => r.node.skill === 4)?.path).toEqual([4]);
    expect(rows.find(r => r.node.skill === 7)?.path).toEqual([7]);
    expect(rows.find(r => r.node.skill === 5)?.status).toBe('unavailable');
    expect(filterPowerRows(rows, '', false).map(r => r.node.skill)).toEqual([3,4,5,7]);
    expect(filterPowerRows(rows, 'attribute', true).map(r => r.node.skill)).toEqual([2]);
    expect(filterPowerRows(rows, '3', false).map(r => r.node.skill)).toEqual([3]);
    const allocated = powerCandidates(nodes, passivePlanningContext(nodes, [4], 'Witch'), [4]);
    expect(allocated.some(r => r.node.skill === 4)).toBe(false);
  });

  test('ranks complete-route gains per point, independent of single-node heat values and selected stat', async () => {
    const request = withTravelAttributes({ character: {class_name:'Witch'}, allocated_nodes:[] }, nodes, 'dex');
    const run = async (stat: string) => (await evaluateNodePower({ request, rows: candidates(), stat, evaluate, probe })).rows;
    const life = await run('Life');
    expect(life.find(r => r.node.skill === 3)).toMatchObject({ gain:20, perPoint:10, single:999 });
    expect(rankPowerRows(life, 'perPoint')[0].node.skill).toBe(4);
    const mana = await run('Mana');
    expect(rankPowerRows(mana, 'perPoint')[0].node.skill).toBe(3);
    expect(probe).toHaveBeenCalledWith(expect.objectContaining({ attribute_choices: { '2': 'dex' } }), 'Mana', 0, expect.any(Array));
  });

  test('ties resolve by point cost then stable ID; unavailable values stay last in both directions', () => {
    const rows = candidates().map(r => ({ ...r, perPoint: r.path ? 10 : null }));
    for (const ascending of [true,false]) expect(rankPowerRows(rows.reverse(), 'perPoint', ascending).map(r => r.node.skill)).toEqual([2,4,7,3,5]);
  });

  test('missing values and newly unsupported mechanics remain unscored', async () => {
    const calc = async (opts: EvaluateOptions) => {
      const out = await evaluate(opts);
      out.results[0].unsupported = ['existing limit'];
      out.results[1].unsupported = ['existing limit'];
      out.results[2].unsupported = ['existing limit', 'unknown effect'];
      out.results[3].stats = {};
      return out;
    };
    const result = await evaluateNodePower({ request: {}, rows:candidates(), stat:'Life', evaluate:calc, probe });
    expect(result.rows[0].status).toBe('ready');
    expect(result.rows[1]).toMatchObject({status:'unsupported', gain:null, perPoint:null, single:null, diagnostics:['unknown effect']});
    expect(result.rows[2]).toMatchObject({status:'error', gain:null, perPoint:null});
  });

  test('caps work, bounds explicit probes, and stops before probing cancelled routes', async () => {
    const many: PowerRow[] = Array.from({length:200}, (_,i) => ({ ...candidates()[0], node:{...nodes[2], skill:100+i}, path:[100+i] }));
    const probes = vi.fn(probe);
    let progress = 0;
    const result = await evaluateNodePower({request:{}, rows:many, stat:'Life', evaluate, probe:probes, onBatch:(_rows,done)=>{progress=done;}});
    expect(result.rows).toHaveLength(NODE_POWER_LIMIT);
    expect(progress).toBe(NODE_POWER_LIMIT);
    expect(probes.mock.calls.every(args => args[3]!.length <= 16)).toBe(true);
    const controller = new AbortController();
    const cancelledProbe = vi.fn(probe);
    const cancelled = await evaluateNodePower({request:{}, rows:many, stat:'Life', signal:controller.signal,
      evaluate:async opts=>{ controller.abort(); return evaluate(opts); }, probe:cancelledProbe });
    expect(cancelled).toEqual({rows:[],cancelled:true});
    expect(cancelledProbe).not.toHaveBeenCalled();
  });
});
