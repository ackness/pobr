import { renderToStaticMarkup } from 'react-dom/server';
import { afterEach, expect, test, vi } from 'vitest';
import { UnincludedModifiers, unincludedGroups } from './UnincludedModifiers';

const harness = vi.hoisted(() => ({ node: null as unknown, effect: undefined as undefined | (() => void | (() => void)), load: vi.fn() }));
vi.mock('react', async original => ({
  ...await original<typeof import('react')>(),
  useState: () => [harness.node, (next: unknown) => { harness.node = next; }],
  useEffect: (effect: typeof harness.effect) => { harness.effect = effect; },
}));
vi.mock('../../api/backend', () => ({ getBackend: async () => ({ loadPassiveTree: harness.load }) }));
vi.mock('../../hooks/useLocalizedLines', async original => ({
  ...await original<typeof import('../../hooks/useLocalizedLines')>(),
  useLocalizedLines: (lines: string[]) => lines,
  useLocalizedNames: (names: { name: string }[]) => names.map(name => name.name),
  useItemDisplayNames: () => [],
}));
afterEach(() => { harness.node = null; harness.effect = undefined; harness.load.mockReset(); });
const render = (source: number) => renderToStaticMarkup(<UnincludedModifiers lines={[`Tree:${source}: Example modifier`]} lang="en-US" />);
const settle = () => new Promise(resolve => setTimeout(resolve, 0));

test.each(['missing', 'rejected'] as const)('a replacement source cannot inherit a resolved node name after a %s lookup', async failure => {
  expect(unincludedGroups(['Tree:123: A', 'Tree:456: B'], 'en-US').map(group => group.identity)).toEqual(['node.123', 'node.456']);
  harness.load.mockResolvedValueOnce([{ skill: 123, id: 'old-node', name: 'Old passive name' }]);
  render(123);
  const cleanup = harness.effect!();
  await settle();
  expect(render(123)).toContain('<h5>Old passive name</h5>');

  // Even the render before the new effect runs must reject state belonging to another source.
  const changed = render(456);
  expect(changed).not.toContain('Old passive name');
  expect(changed).toContain('Passive tree (name unavailable)');
  if (typeof cleanup === 'function') cleanup();
  if (failure === 'missing') harness.load.mockResolvedValueOnce([]);
  else harness.load.mockRejectedValueOnce(new Error('Tree unavailable'));
  harness.effect!();
  await settle();
  expect(render(456)).not.toContain('Old passive name');
  expect(render(456)).toContain('Passive tree (name unavailable)');
});
