import { renderToStaticMarkup } from 'react-dom/server';
import { beforeEach, expect, it, vi } from 'vitest';
import type { ReactNode } from 'react';
import type { BuildSession } from '../hooks/useBuildSession';
import { CommandPalette } from './CommandPalette';
const commands = vi.hoisted(() => new Map<string, () => void>());
beforeEach(() => commands.clear());
vi.mock('../hooks/useSkillName', () => ({ useSkillCatalog: () => new Map() }));
vi.mock('../components/ui/dialog', () => {
  const Wrap = ({ children }: { children?: ReactNode }) => <div>{children}</div>;
  return { Dialog: Wrap, DialogContent: Wrap, DialogDescription: Wrap, DialogTitle: Wrap };
});
vi.mock('cmdk', () => {
  const Wrap = ({ children }: { children?: ReactNode }) => <div>{children}</div>;
  return { Command: Object.assign(Wrap, { Input: () => null, List: Wrap, Empty: Wrap, Group: Wrap,
    Item: ({ children, value, onSelect }: { children: ReactNode; value: string; onSelect: () => void }) => { commands.set(value, onSelect); return <div data-command={value}>{children}</div>; } }) };
});
it('shows one calculated-skill command per skill, without duplicating identical groups', () => {
  const session = { socketGroups: ['Spark', 'Spark', 'Fireball'].map(skill_id => ({ enabled: true, gems: [{ skill_id }] })) } as BuildSession;
  const html = renderToStaticMarkup(<CommandPalette open onOpenChange={() => {}} session={session} view={{ calc: null, previous: null, stale: false }} lang="en-US"
    pageLabel={route => route} onNavigate={() => {}} onInspect={() => {}} onSettings={() => {}} onToggleTheme={() => {}} themeLabel="Theme" />);
  expect(html.match(/data-command="skill:/g)).toHaveLength(2);
  expect(html).toContain('1. Spark'); expect(html).not.toContain('2. Spark'); expect(html).toContain('3. Fireball');
});

it('prefers an enabled duplicate and selects its original group index', () => {
  const updateParams = vi.fn();
  const session = { socketGroups: [false, true].map(enabled => ({ enabled, gems: [{ skill_id: 'Spark' }] })), updateParams } as unknown as BuildSession;
  const html = renderToStaticMarkup(<CommandPalette open onOpenChange={() => {}} session={session} view={{ calc: null, previous: null, stale: false }} lang="en-US"
    pageLabel={route => route} onNavigate={() => {}} onInspect={() => {}} onSettings={() => {}} onToggleTheme={() => {}} themeLabel="Theme" />);
  expect(html.match(/data-command="skill:/g)).toHaveLength(1);
  expect(html).toContain('2. Spark'); expect(html).not.toContain('1. Spark');
  commands.get('skill:1')!();
  expect(updateParams).toHaveBeenCalledWith({ main_socket_group: 1 });
});
