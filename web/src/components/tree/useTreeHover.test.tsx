import type { FocusEvent } from 'react';
import { afterEach, beforeEach, expect, test, vi } from 'vitest';
import { useTreeHover } from './useTreeHover';
import type { PassiveNode } from '../../api/types';

const hooks = await vi.hoisted(async () => {
  const { createHookHarness } = await import('../../testing/hookHarness');
  return createHookHarness({ lazyState: false, functionalUpdates: false, refs: true, effects: 'once' });
});
vi.mock('react', async original => hooks.mockReact(await original<typeof import('react')>()));
const node = { id: 'power', skill: 2, kind: 'notable' } as PassiveNode;
const read = () => { hooks.beginRender(); return useTreeHover(); };
const blur = (inside: boolean) => ({ currentTarget: { contains: () => inside }, relatedTarget: {} }) as unknown as FocusEvent<HTMLDivElement>;
beforeEach(() => { hooks.reset(); vi.useFakeTimers(); });
afterEach(() => { hooks.cleanup?.(); vi.useRealTimers(); });

test('focused change summary survives pointer leave and pending dismissal timers', () => {
  const card = read();
  card.hoverNode(node);
  card.leaveHover();
  vi.advanceTimersByTime(100);
  card.onFocus();
  vi.advanceTimersByTime(1000);
  expect(read().hover).toBe(node);
  card.leaveHover();
  card.onBlur(blur(true)); // Moving focus between the summary and its expanded details.
  card.hoverNode({ ...node, skill: 3 }); // Pointer movement cannot replace the focused card.
  vi.advanceTimersByTime(1000);
  expect(read().hover).toBe(node);
  card.onBlur(blur(false));
  vi.advanceTimersByTime(199);
  expect(read().hover).toBe(node);
  vi.advanceTimersByTime(1);
  expect(read().hover).toBeNull();
});

test('moving from the node to the card cancels dismissal; explicit close releases focus retention', () => {
  const card = read();
  card.hoverNode(node);
  card.leaveHover();
  card.keepHover();
  vi.advanceTimersByTime(1000);
  expect(read().hover).toBe(node);
  card.onFocus(); card.setHover(null);
  const next = { ...node, skill: 3 };
  card.hoverNode(next);
  expect(read().hover).toBe(next);
  card.leaveHover();
  vi.advanceTimersByTime(200);
  expect(read().hover).toBeNull();
});
