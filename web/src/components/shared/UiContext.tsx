import { createContext, useContext, type ReactNode } from 'react';
import type { Lang } from '../../lib/i18n';

const UiContext = createContext<{ v2: boolean; lang: Lang }>({ v2: false, lang: 'en-US' });

export function UiProvider({ v2 = false, lang, children }: { v2?: boolean; lang: Lang; children: ReactNode }) {
  return <UiContext.Provider value={{ v2, lang }}>{children}</UiContext.Provider>;
}

export function useUi() { return useContext(UiContext); }
