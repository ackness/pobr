import type { Lang } from '../lib/i18n';
export function words(lang: Lang, en: string, cn: string, tw: string = cn) { return lang === 'en-US' ? en : lang === 'zh-TW' ? tw : cn; }
/** Document titles; the navigation itself uses the shorter `tab.*` labels. */
export const pageNames = {
  'en-US': { overview: 'Overview', build: 'Build library', tree: 'Passive tree', skills: 'Skills', items: 'Equipment', trade: 'Upgrades', calcs: 'Calculations', config: 'Combat setup', compare: 'Compare stages' },
  'zh-CN': { overview: 'BD 概览', build: 'BD 管理', tree: '天赋树', skills: '技能搭配', items: '装备', trade: '寻找提升', calcs: '计算明细', config: '战斗配置', compare: '阶段对比' },
  'zh-TW': { overview: 'BD 概覽', build: 'BD 管理', tree: '天賦樹', skills: '技能搭配', items: '裝備', trade: '尋找提升', calcs: '計算明細', config: '戰鬥配置', compare: '階段比較' },
};
