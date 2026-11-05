import type { ComponentProps } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { expect, test, vi } from 'vitest';
import type { ConfigOption } from '../../api/types';
import { ConfigScopeControl, matchesConfigScope, type ConfigScope } from './ConfigScope';

const primitive = vi.hoisted(() => ({ change: (_value: string) => {} }));
vi.mock('../ui/toggle-group', () => ({
  ToggleGroup: ({ children, onValueChange, value }: { children: React.ReactNode; onValueChange: (value: string) => void; value: string }) => {
    primitive.change = onValueChange;
    return <div data-value={value}>{children}</div>;
  },
  ToggleGroupItem: ({ children, value }: ComponentProps<'button'>) => <button value={value}>{children}</button>,
}));

test('switching between three scopes replaces the filter and cannot clear the selection', () => {
  const options: ConfigOption[] = [
    { var: 'unchangedFlag', input_type: 'check' },
    { var: 'changedFlag', input_type: 'check' },
    { var: 'hiddenCount', input_type: 'count', default: 0 },
    { var: 'hiddenDefault', input_type: 'count', default: 0 },
    { var: 'unknown', input_type: 'count' },
  ];
  const inputs = { unchangedFlag: false, changedFlag: true, hiddenCount: 2 };
  const applicability = (key: string) => key === 'unknown' ? 'unknown' : key.startsWith('hidden') ? 'not_applicable' : 'applicable';
  let scope: ConfigScope = 'relevant';
  const render = () => renderToStaticMarkup(<ConfigScopeControl scope={scope} count={2} lang="en-US" onChange={value => { scope = value; }} />);
  const visible = () => options.filter(option => matchesConfigScope(option, applicability(option.var), inputs, scope)).map(option => option.var);
  expect(render()).toContain('Modified (2)');
  expect(visible()).toEqual(['unchangedFlag', 'changedFlag', 'hiddenCount', 'unknown']);
  primitive.change('modified');
  expect(render()).toContain('data-value="modified"');
  expect(visible()).toEqual(['changedFlag', 'hiddenCount']);
  primitive.change('all');
  expect(render()).toContain('data-value="all"');
  expect(visible()).toHaveLength(5);
  primitive.change('relevant');
  primitive.change('');
  expect(render()).toContain('data-value="relevant"');
  expect(visible()).toEqual(['unchangedFlag', 'changedFlag', 'hiddenCount', 'unknown']);
});

test('Modified compares with catalog defaults, including saved zero, false and typed values', () => {
  const number: ConfigOption = { var: 'count', input_type: 'count', default: 0 };
  expect(matchesConfigScope(number, 'not_applicable', { count: 0 }, 'modified')).toBe(false);
  expect(matchesConfigScope(number, 'not_applicable', { count: '0' }, 'modified')).toBe(true);
  const flag: ConfigOption = { var: 'flag', input_type: 'check', default: { state_bool: true } };
  expect(matchesConfigScope(flag, 'not_applicable', { flag: false }, 'modified')).toBe(true);
  expect(matchesConfigScope(flag, 'applicable', {}, 'modified')).toBe(false);
  expect(matchesConfigScope(flag, 'applicable', { flag: 1 }, 'modified')).toBe(false);
});
