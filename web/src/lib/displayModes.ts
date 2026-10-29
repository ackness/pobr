import type { Lang } from './i18n';

export const DISPLAY_MODES = ['grouped', 'compact', 'game'] as const;
export type DisplayMode = typeof DISPLAY_MODES[number];

export function isDisplayMode(value: unknown): value is DisplayMode {
  return DISPLAY_MODES.some(mode => mode === value);
}

/** The same information, organized for reading, comparison, or recognition. */
export function displayModeOptions(lang: Lang) {
  const w = (en: string, cn: string, tw: string) => lang === 'en-US' ? en : lang === 'zh-TW' ? tw : cn;
  return [
    { value: 'grouped', label: w('Grouped details', '分组详解', '分組詳解'), description: w('Read related information together.', '按用途分组，逐项查看。', '依用途分組，逐項查看。') },
    { value: 'compact', label: w('Compact tables', '紧凑表格', '精簡表格'), description: w('Compare values in aligned rows.', '数值对齐，方便比较。', '數值對齊，方便比較。') },
    { value: 'game', label: w('Game cards', '游戏卡片', '遊戲卡片'), description: w('Focus on skills, items and effects.', '突出技能、装备与效果。', '凸顯技能、裝備與效果。') },
  ] satisfies { value: DisplayMode; label: string; description: string }[];
}
