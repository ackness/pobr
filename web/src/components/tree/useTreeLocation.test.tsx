import { renderToStaticMarkup } from 'react-dom/server';
import { beforeEach, expect, test, vi } from 'vitest';
import type { PassiveNode } from '../../api/types';
import { useTreeLocation } from './useTreeLocation';

// Replay mount effects in the node-only test environment. The owner outlives each canvas mount.
const mounted = vi.hoisted(() => ({ effects: [] as (() => void)[] }));
vi.mock('react', async original => ({ ...await original<typeof import('react')>(),
  useEffect: (effect: () => void) => { mounted.effects.push(effect); },
}));

const socket: PassiveNode = { skill: 7960, id: 'socket', kind: 'jewel_socket', x: 100, y: 200 };
const flush = () => mounted.effects.splice(0).forEach(effect => effect());
beforeEach(() => { mounted.effects = []; });

test('a consumed locate request does not select, zoom or focus again after page-return or weapon-set remounts', () => {
  const ownerReceipt = { current: undefined as number | undefined };
  let request = { nonce: 1, node: socket.skill };
  const select = vi.fn(), zoom = vi.fn(), focus = vi.fn();
  function Canvas() {
    useTreeLocation(request, ownerReceipt, socket, true, node => {
      select(node.skill); zoom(node.x, node.y); focus();
    });
    return <svg />;
  }
  const mount = (key: string) => {
    renderToStaticMarkup(<Canvas key={key} />);
    flush();
  };
  mount('weapon-set-1');
  expect(ownerReceipt.current).toBe(1);
  expect(select).toHaveBeenCalledWith(socket.skill);
  expect(zoom).toHaveBeenCalledWith(socket.x, socket.y);
  expect(focus).toHaveBeenCalledTimes(1);

  mount('page-return');
  mount('weapon-set-2');
  mount('weapon-set-1-again');
  expect(select).toHaveBeenCalledTimes(1);
  expect(zoom).toHaveBeenCalledTimes(1);
  expect(focus).toHaveBeenCalledTimes(1);

  request = { ...request, nonce: 2 };
  mount('new-locate-action');
  expect(ownerReceipt.current).toBe(2);
  expect(select).toHaveBeenCalledTimes(2);
  expect(zoom).toHaveBeenCalledTimes(2);
  expect(focus).toHaveBeenCalledTimes(2);
});

test('a request is consumed only once the node and viewport are ready', () => {
  const receipt = { current: undefined as number | undefined };
  const focus = vi.fn();
  const request = { nonce: 1, node: socket.skill };
  useTreeLocation(request, receipt, undefined, true, focus); flush();
  useTreeLocation(request, receipt, { ...socket, x: undefined }, true, focus); flush();
  useTreeLocation(request, receipt, socket, false, focus); flush();
  expect(receipt.current).toBeUndefined();
  expect(focus).not.toHaveBeenCalled();
  useTreeLocation(request, receipt, socket, true, focus); flush();
  expect(receipt.current).toBe(1);
  expect(focus).toHaveBeenCalledWith(socket);
});
