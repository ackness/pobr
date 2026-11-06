import type { ComponentProps } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { expect, test, vi } from 'vitest';
import type { Breakdown } from '../../api/types';
import { BreakdownTable } from './CalcsPanel';

const harness = vi.hoisted(() => ({ buttons: [] as ComponentProps<'button'>[] }));
vi.mock('../shared/controls', () => ({ AppButton: (props: ComponentProps<'button'>) => {
  harness.buttons.push(props); return <button {...props} />;
} }));
const empty: Breakdown = { base_total: 0, inc_total: 0, more_total: 1, mods: [], formula: null };

test('combined DPS components open their own metric without changing actors', () => {
  harness.buttons = [];
  const select = vi.fn();
  renderToStaticMarkup(<BreakdownTable breakdown={{ ...empty, formula: { value: 120, steps: [
    { op: 'hit_dps', operand: 100, result: 100 }, { op: 'dot_dps', operand: 20, result: 120 },
  ] } }} stat="CombinedDPS" lang="en-US" onSelect={select} />);
  expect(harness.buttons).toHaveLength(2);
  harness.buttons[0].onClick!({} as never);
  expect(select).toHaveBeenLastCalledWith('TotalDPS');
  harness.buttons[1].onClick!({} as never);
  expect(select).toHaveBeenLastCalledWith('TotalDotDPS');
});

test('resistance source controls open the total, maximum and over-cap details', () => {
  harness.buttons = [];
  const select = vi.fn();
  renderToStaticMarkup(<BreakdownTable breakdown={empty} stat="FireResist" lang="en-US" onSelect={select} />);
  expect(harness.buttons).toHaveLength(3);
  harness.buttons.forEach(button => button.onClick!({} as never));
  expect(select.mock.calls).toEqual([['FireResistTotal'], ['FireResistMax'], ['FireResistOverCap']]);
});
