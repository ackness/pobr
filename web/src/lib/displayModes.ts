import { bindT, type Lang } from './i18n';

export const DISPLAY_MODES = ['grouped', 'compact', 'game'] as const;
export type DisplayMode = typeof DISPLAY_MODES[number];

export function isDisplayMode(value: unknown): value is DisplayMode {
  return DISPLAY_MODES.some(mode => mode === value);
}

/** The same information, organized for reading, comparison, or recognition. */
export function displayModeOptions(lang: Lang) {
  const tt = bindT(lang);
  return [
    { value: 'grouped', label: tt('settings.grouped'), description: tt('settings.groupedHint') },
    { value: 'compact', label: tt('settings.compactMode'), description: tt('settings.compactModeHint') },
    { value: 'game', label: tt('settings.game'), description: tt('settings.gameHint') },
  ] satisfies { value: DisplayMode; label: string; description: string }[];
}
