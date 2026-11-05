import type { ComponentProps } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { afterEach, expect, it, vi } from 'vitest';
import { TooltipProvider } from '../components/ui/tooltip';
import { TopbarAction, TopbarHint } from './TopbarHint';

const control = vi.hoisted(() => ({ click: undefined as ComponentProps<'button'>['onClick'] }));
vi.mock('../components/ui/button', () => ({
  Button: ({ onClick, ...props }: ComponentProps<'button'>) => {
    control.click = onClick;
    return <button {...props} onClick={onClick} />;
  },
}));
afterEach(() => { control.click = undefined; vi.unstubAllGlobals(); });

it('keeps a disabled action keyboard-focusable with a tooltip, and blocks activation', () => {
  vi.stubGlobal('window', { clearTimeout });
  const undo = vi.fn();
  const html = renderToStaticMarkup(<TooltipProvider><TopbarAction disabled hint="No changes to undo" onClick={undo}>Undo</TopbarAction></TooltipProvider>);
  expect(html).toContain('aria-disabled="true"');
  expect(html).toContain('data-slot="tooltip-trigger"');
  expect(html).not.toContain(' disabled=');
  expect(html).not.toContain('tabindex="-1"');
  control.click!({} as never);
  expect(undo).not.toHaveBeenCalled();

  const enabled = renderToStaticMarkup(<TooltipProvider><TopbarAction hint="Undo (Ctrl Z)" onClick={undo}>Undo</TopbarAction></TooltipProvider>);
  expect(enabled).not.toContain('aria-disabled');
  control.click!({} as never);
  expect(undo).toHaveBeenCalledOnce();
});

it('lets keyboard users focus the save status to trigger its tooltip', () => {
  const html = renderToStaticMarkup(<TooltipProvider><TopbarHint hint="Saved in this browser"><span tabIndex={0} role="status">Saved in this browser</span></TopbarHint></TooltipProvider>);
  expect(html).toContain('tabindex="0"');
  expect(html).toContain('data-slot="tooltip-trigger"');
  expect(html).toContain('role="status"');
});
