import { createContext, useContext, useMemo, type ReactNode } from 'react';
import type { Lang } from '../../lib/i18n';
import type { DisplayMode } from '../../lib/displayModes';

const UiContext = createContext<{ lang: Lang; displayMode: DisplayMode }>({ lang: 'en-US', displayMode: 'grouped' });

export function UiProvider({ lang, displayMode = 'grouped', children }: { lang: Lang; displayMode?: DisplayMode; children: ReactNode }) {
  const value = useMemo(() => ({ lang, displayMode }), [lang, displayMode]);
  return <UiContext value={value}>{children}</UiContext>;
}

export function useUi() { return useContext(UiContext); }

const PanelActiveContext = createContext(true);
export function PanelActivity({ active, children }: { active: boolean; children: ReactNode }) {
  return <PanelActiveContext value={active}>{children}</PanelActiveContext>;
}
export function usePanelActive() { return useContext(PanelActiveContext); }
