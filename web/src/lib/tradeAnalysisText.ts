import { messages, type Lang } from './i18n';
import type { Messages } from '../locales';

export const tradeAnalysisText = (lang: Lang, key: keyof Messages['tradeAnalysis']): string => messages(lang, 'tradeAnalysis')[key];
