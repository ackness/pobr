import type { Lang } from './i18n';

const messages = {
  resume: { 'en-US': 'Resume remaining positions', 'zh-CN': '继续分析剩余位置', 'zh-TW': '繼續分析剩餘位置' },
  refresh: { 'en-US': 'Reanalyze all positions', 'zh-CN': '重新分析所有位置', 'zh-TW': '重新分析所有位置' },
  stopped: { 'en-US': 'Analysis stopped. Completed positions are saved; resume to finish the rest.', 'zh-CN': '分析已停止。已完成的位置会保留，可继续分析剩余位置。', 'zh-TW': '分析已停止。已完成的位置會保留，可繼續分析剩餘位置。' },
} as const;

export const tradeAnalysisText = (lang: Lang, key: keyof typeof messages): string => messages[key][lang];
