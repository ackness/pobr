import type { ComponentProps } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { beforeEach, expect, test, vi } from 'vitest';
import type { BuildSession } from '../../hooks/useBuildSession';
import { ItemOptimizer } from './ItemOptimizer';
import type { OptimizerResults } from '../shared/OptimizerControls';

const ui = vi.hoisted(() => ({ states: [] as unknown[], cursor: 0, preset: 'dps',
  buttons: [] as ComponentProps<'button'>[], evaluate: vi.fn(), result: vi.fn() }));
vi.mock('react', async original => ({ ...await original<typeof import('react')>(),
  useState: (initial: unknown) => {
    const index = ui.cursor++;
    if (!(index in ui.states)) ui.states[index] = typeof initial === 'function' ? initial() : initial;
    return [ui.states[index], (next: unknown) => { ui.states[index] = typeof next === 'function' ? next(ui.states[index]) : next; }];
  },
}));
vi.mock('../../hooks/useUpgradeGoal', () => ({ useUpgradeGoal: () => ({ goal: { preset: ui.preset, cStat: '', cMin: '', cMax: '' }, setGoal: vi.fn() }) }));
vi.mock('../../hooks/useLocalizedLines', () => ({ useLocalizedNames: (names: { name: string }[]) => names.map(entry => entry.name) }));
vi.mock('../../app/jobs/useJobBeacon', () => ({ useJobBeacon: () => {} }));
vi.mock('../shared/controls', () => ({ AppButton: (props: ComponentProps<'button'>) => { ui.buttons.push(props); return <button {...props} />; } }));
vi.mock('../../lib/optimize', async original => ({ ...await original<typeof import('../../lib/optimize')>(), evaluateVariants: (...args: unknown[]) => ui.evaluate(...args) }));
vi.mock('../shared/OptimizerControls', async original => ({ ...await original<typeof import('../shared/OptimizerControls')>(),
  ObjectiveEditor: () => null, OptimizerProgress: () => null,
  OptimizerResults: (props: ComponentProps<typeof OptimizerResults>) => { ui.result(props); return null; },
}));
beforeEach(() => { ui.states = []; ui.preset = 'dps'; ui.evaluate.mockReset(); ui.result.mockClear(); });

test.each([false, true])('saved equipment candidates use the same damage goal (summoner: %s) and preserve hit-DPS-per-mana', async summoner => {
  const request = {};
  const session = { currentRequest: () => request, calc: { stats: [],
    main_minion_index: summoner ? 1 : null, minions: summoner ? [{ index: 1 }] : [] },
  } as unknown as BuildSession;
  const render = () => {
    ui.cursor = 0; ui.buttons = [];
    return renderToStaticMarkup(<ItemOptimizer session={session} lang="en-US" slot="ring1"
      candidates={['one', 'two'].map(id => ({ id, kind: 'item', name: id, text: id, slot: 'ring1' }))}
      candidateNames={['one', 'two']} currentText={undefined} onApply={vi.fn()} />);
  };
  ui.evaluate.mockResolvedValue({ baseline: { TotalDPS: 10, CombinedDPS: 100, MainMinionDPS: 100, ManaCost: 1 },
    results: [{ index: 0, stats: { TotalDPS: 10000, CombinedDPS: 120, MainMinionDPS: 110, ManaCost: 1 } },
      { index: 1, stats: { TotalDPS: 1, CombinedDPS: 150, MainMinionDPS: 180, ManaCost: 1 } }] });
  render(); ui.buttons.find(button => button.className?.split(' ').includes('gem-optimizer-toggle'))!.onClick!({} as never); render();
  await ui.buttons.find(button => button.className === 'opt-run')!.onClick!({} as never); render();
  expect(ui.result).toHaveBeenLastCalledWith(expect.objectContaining({ objective: expect.objectContaining({ stat: summoner ? 'MainMinionDPS' : 'CombinedDPS' }),
    ranked: [expect.objectContaining({ index: 1 }), expect.objectContaining({ index: 0 })] }));
  ui.preset = 'dpsPerMana'; render();
  expect(ui.result).toHaveBeenLastCalledWith(expect.objectContaining({ objective: expect.objectContaining({ stat: 'TotalDPS', per: 'ManaCost' }),
    ranked: [expect.objectContaining({ index: 0 }), expect.objectContaining({ index: 1 })] }));
});
