import { renderToStaticMarkup } from 'react-dom/server';
import type { ComponentProps } from 'react';
import { beforeEach, expect, test, vi } from 'vitest';
import { ComparisonBaseline } from './ComparisonBaseline';
import type { BuildSession } from '../hooks/useBuildSession';
import { createWorkspace, defaultStarterSession, recordWorkspaceBaseline } from '../lib/buildWorkspace';

const ui = vi.hoisted(() => ({ buttons: [] as ComponentProps<'button'>[] }));
vi.mock('../components/ui/button', () => ({ Button: (props: ComponentProps<'button'>) => { ui.buttons.push(props); return <button {...props} />; } }));
beforeEach(() => { ui.buttons = []; });

test('save, compare and replace baseline actions call the session and keep the approved copy', () => {
  const workspace = createWorkspace(defaultStarterSession('Ranger'));
  const saveComparisonBaseline = vi.fn(), startBaselineComparison = vi.fn(), onNavigate = vi.fn();
  const session = { workspace, saveComparisonBaseline, startBaselineComparison, busy: false } as unknown as BuildSession;
  expect(renderToStaticMarkup(<ComparisonBaseline session={session} lang="zh-CN" onNavigate={onNavigate} />)).toContain('记下当前');
  ui.buttons[0].onClick!({} as never);
  expect(saveComparisonBaseline).toHaveBeenCalledOnce();
  session.workspace = recordWorkspaceBaseline(workspace, 1000);
  ui.buttons = [];
  const html = renderToStaticMarkup(<ComparisonBaseline session={session} lang="zh-CN" onNavigate={onNavigate} />);
  expect(html).toMatch(/基线：[0-9]{2}:[0-9]{2} 记下/);
  ui.buttons.find(button => button.children === '对比')!.onClick!({} as never);
  expect(startBaselineComparison).toHaveBeenCalledOnce();
  expect(onNavigate).toHaveBeenCalledWith('compare');
  ui.buttons.find(button => button.children === '重新记下')!.onClick!({} as never);
  expect(saveComparisonBaseline).toHaveBeenCalledTimes(2);
});

test('recording is disabled while calculation or loading is busy', () => {
  const session = { workspace: createWorkspace(defaultStarterSession('Ranger')), busy: true } as unknown as BuildSession;
  renderToStaticMarkup(<ComparisonBaseline session={session} lang="en-US" onNavigate={() => {}} />);
  expect(ui.buttons[0].disabled).toBe(true);
});
