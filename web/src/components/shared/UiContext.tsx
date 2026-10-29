import { createContext, useContext, type ReactNode } from 'react';
import type { Lang } from '../../lib/i18n';
import type { ItemDisplayStyle } from '../../lib/itemDisplay';

const UiContext = createContext<{ lang: Lang; itemDisplay: ItemDisplayStyle }>({ lang: 'en-US', itemDisplay: 'grouped' });

export function UiProvider({ lang, itemDisplay = 'grouped', children }: { lang: Lang; itemDisplay?: ItemDisplayStyle; children: ReactNode }) {
  return <UiContext.Provider value={{ lang, itemDisplay }}>{children}</UiContext.Provider>;
}

export function useUi() { return useContext(UiContext); }

const PanelActiveContext = createContext(true);
export function PanelActivity({ active, children }: { active: boolean; children: ReactNode }) {
  return <PanelActiveContext.Provider value={active}>{children}</PanelActiveContext.Provider>;
}
export function usePanelActive() { return useContext(PanelActiveContext); }
