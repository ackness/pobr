import { renderToStaticMarkup } from 'react-dom/server';
import { expect, test, vi } from 'vitest';
import { LedgerView } from './LedgerView';
import { mockLineUsage } from './mockLineUsage';
import type { BuildSession } from '../../../hooks/useBuildSession';

const harness = vi.hoisted(() => ({ index: 0, done: null as unknown }));
vi.mock('react', async original => {
  const react = await original<typeof import('react')>();
  return { ...react, useState: (initial: unknown) => harness.index++ === 0 ? [harness.done, () => {}] : react.useState(initial) };
});
vi.mock('../../../hooks/useSkillName', () => ({ useSkillName: () => (id: string) => id }));

test.each([null, 'conditionEnemyChilled'])('waiting-condition heading and action share the resolved configuration field (%s)', explicit => {
  const response = mockLineUsage();
  response.usage.conditions = [{ ...response.usage.conditions[0], condition: 'LowLife', config_option: explicit }];
  response.usage.condition_configs = { LowLife: 'conditionLowLife' };
  const field = explicit ?? 'conditionLowLife';
  harness.done = { response, version: 1, nodeIds: new Map(), options: [{ var: field, input_type: 'check', label: 'Resolved condition setting' }] };
  harness.index = 0;
  const session = { calc: {}, stateVersion: 1, busy: false } as BuildSession;
  const html = renderToStaticMarkup(<LedgerView session={session} lang="en-US" />);
  const waiting = html.slice(html.indexOf('Conditions waiting to be set'));
  expect(waiting).toContain('<b>Resolved condition setting</b>');
  expect(waiting).toContain(`href="/config#config-${field}"`);
  expect(waiting).not.toContain('No configuration field is available');
});
