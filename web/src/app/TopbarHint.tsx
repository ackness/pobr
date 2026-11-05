import type { ComponentProps, ReactElement } from 'react';
import { Button } from '../components/ui/button';
import { Tooltip, TooltipContent, TooltipTrigger } from '../components/ui/tooltip';

export function TopbarHint({ hint, children }: { hint: string; children: ReactElement }) {
  return <Tooltip><TooltipTrigger asChild>{children}</TooltipTrigger><TooltipContent side="bottom">{hint}</TooltipContent></Tooltip>;
}

/** Disabled actions remain focusable so keyboard users can read why they are unavailable. */
export function TopbarAction({ hint, disabled, onClick, ...props }: ComponentProps<typeof Button> & { hint: string }) {
  return <TopbarHint hint={hint}><Button {...props} aria-disabled={disabled || undefined}
    onClick={event => { if (!disabled) onClick?.(event); }} /></TopbarHint>;
}
