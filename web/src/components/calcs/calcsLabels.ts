import type { Lang } from '../../lib/i18n';

const labels = {
  metrics: ['Metrics & details', '指标与明细', '指標與明細'],
  context: ['Inspection context', '当前查看对象', '目前檢視對象'],
  categories: ['Metric categories', '指标分类', '指標分類'],
  all: ['All categories', '全部分类', '全部分類'],
  choose: ['Choose a metric to inspect its calculation.', '选择一个指标，查看计算过程。', '選擇一個指標，檢視計算過程。'],
  detailHint: ['Details follow the selected actor and skill. Open one metric to load its explanation.', '明细对应当前对象与技能，选择指标后加载解释。', '明細對應目前對象與技能，選擇指標後載入說明。'],
  noCalc: ['Calculate a build to inspect its metrics.', '完成构筑计算后查看指标。', '完成構築計算後檢視指標。'],
  fullDps: ['Full DPS', '完整 DPS', '完整 DPS'],
  fullDpsEmpty: ['Include skills in Full DPS from the Skills page to see their combined contribution here.', '在技能页将技能计入完整 DPS，即可在此查看合计贡献。', '在技能頁將技能計入完整 DPS，即可在此檢視合計貢獻。'],
  steps: ['Calculation steps', '计算步骤', '計算步驟'],
  sources: ['Modifier sources', '词条来源', '詞綴來源'],
  derived: ['Formula & inputs', '公式与输入', '公式與輸入'],
  none: ['Value only', '仅数值', '僅數值'],
  back: ['Back to metrics', '返回指标列表', '返回指標清單'],
  stale: ['Updating the calculation…', '正在更新计算…', '正在更新計算…'],
  socket_group: ['Skill group', '技能组', '技能組'],
  limb: ['Limb', '肢体', '肢體'],
} as const;

export function calcsLabel(lang: Lang, key: keyof typeof labels): string {
  return labels[key][lang === 'en-US' ? 0 : lang === 'zh-CN' ? 1 : 2];
}
