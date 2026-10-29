import { createContext, useContext, type ReactNode } from 'react';
import type { Lang } from '../../lib/i18n';
import type { DisplayMode } from '../../lib/displayModes';

const UiContext = createContext<{ lang: Lang; displayMode: DisplayMode }>({ lang: 'en-US', displayMode: 'grouped' });

export function UiProvider({ lang, displayMode = 'grouped', children }: { lang: Lang; displayMode?: DisplayMode; children: ReactNode }) {
  return <UiContext.Provider value={{ lang, displayMode }}>{children}</UiContext.Provider>;
}

export function useUi() { return useContext(UiContext); }

const PanelActiveContext = createContext(true);
export function PanelActivity({ active, children }: { active: boolean; children: ReactNode }) {
  return <PanelActiveContext.Provider value={active}>{children}</PanelActiveContext.Provider>;
}
export function usePanelActive() { return useContext(PanelActiveContext); }
