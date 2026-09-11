import type { Lang } from './i18n';

const messages = {
  title: ['Find support upgrades automatically', '自动寻优辅助宝石', '自動尋優輔助寶石'],
  hint: ['Automatically checks skill compatibility, character level and gem families. Compares replacements, then explores support combinations.', '自动检查技能兼容性、角色等级与宝石家族。先比较替换收益，再探索辅助搭配。', '自動檢查技能相容性、角色等級與寶石家族。先比較替換收益，再探索輔助搭配。'],
  capacity: ['Unlocked support sockets', '已解锁辅助插槽', '已解鎖輔助插槽'],
  capacityHint: ['Imports do not include unlocked socket counts. Defaults to occupied sockets, or two; adjust only to your actual capacity.', '导入数据不含插槽解锁数量。默认使用当前已装数量，至少两个；请按实际已解锁数量调整。', '匯入資料不含插槽解鎖數量。預設使用目前已裝數量，至少兩個；請按實際已解鎖數量調整。'],
  lineage: ['Include lineage supports', '包含血脉辅助', '包含血脈輔助'],
  eligible: ['Automatically screened candidates', '自动筛选的候选辅助', '自動篩選的候選輔助'],
  excluded: ['Excluded: unknown compatibility / level / incompatible', '已排除：兼容数据未知 / 等级不足 / 不兼容', '已排除：相容資料未知 / 等級不足 / 不相容'],
  run: ['Calculate support combinations', '计算辅助搭配', '計算輔助搭配'],
  selectMain: ['Select this skill to optimize', '选择此技能并寻优', '選擇此技能並尋優'],
  empty: ['No confirmed usable supports for this skill. Missing or unmodeled effects are not recommended.', '当前技能没有已确认可用的辅助。数据缺失或尚未建模的效果不会作为推荐。', '目前技能沒有已確認可用的輔助。資料缺失或尚未建模的效果不會作為推薦。'],
  noGain: ['No fully recalculated improvement found within the search budget and current constraints.', '在当前约束与搜索范围内，没有找到完整重算后更好的搭配。', '在目前約束與搜尋範圍內，沒有找到完整重算後更好的搭配。'],
  limit: ['All candidates receive individual probes. Combination search is bounded and is not a guaranteed global optimum. Minion-specific compatibility remains limited; extra lineage-copy allowances are not modeled.', '所有候选均参与单颗探测；组合采用有限搜索，不保证全局最优。召唤物专属兼容性仍有限制；血脉按每种一份筛选，暂不计额外复制权限。', '所有候選均參與單顆探測；組合採用有限搜尋，不保證全域最佳。召喚物專屬相容性仍有限制；血脈按每種一份篩選，暫不計額外複製權限。'],
  results: ['Complete recalculations', '完整重算次数', '完整重算次數'],
  unsupported: ['Excluded combinations with newly unmodeled effects', '排除含新增未建模效果的搭配', '排除含新增未建模效果的搭配'],
  adjustment: ['Skill adjustment · ordinary supports do not use market search', '技能调整 · 普通辅助不跳转市集搜索', '技能調整 · 普通輔助不跳轉市集搜尋'],
  keepActive: ['Keeps the active gems, replaces the complete support set.', '保留主动宝石，替换整套辅助。', '保留主動寶石，替換整套輔助。'],
  loading: ['Loading support compatibility data…', '正在读取辅助兼容数据…', '正在讀取輔助相容資料…'],
  catalogError: ['Support compatibility data could not be loaded. Reload to try again.', '辅助兼容数据加载失败，请刷新后重试。', '輔助相容資料載入失敗，請重新整理後重試。'],
  baseline: ['Current setup', '当前搭配', '目前搭配'],
} satisfies Record<string, [string, string, string]>;

export function supportText(lang: Lang, key: keyof typeof messages): string {
  return messages[key][lang === 'zh-CN' ? 1 : lang === 'zh-TW' ? 2 : 0];
}
