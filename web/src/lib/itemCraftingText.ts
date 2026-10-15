import type { Lang } from './i18n';
const labels = {
  craft: ['Craft item', '打造物品', '打造物品'],
  hint: ['Choose a base, then add affixes and compare before equipping. Base implicits start at maximum; affixes start at their midpoint.', '選擇基底並加入詞綴，裝備前先比較。基底固定詞綴取最大值，新增詞綴預設取中間值。', '选择基底并添加词缀，装备前先比较。基底固定词缀取最大值，新增词缀默认取中间值。'],
  itemClass: ['Item class', '物品類別', '物品类别'],
  base: ['Item base', '物品基底', '物品基底'],
  rarity: ['Rarity', '稀有度', '稀有度'],
  NORMAL: ['Normal', '普通', '普通'],
  MAGIC: ['Magic', '魔法', '魔法'],
  RARE: ['Rare', '稀有', '稀有'],
  itemLevel: ['Item level', '物品等級', '物品等级'],
  quality: ['Quality', '品質', '品质'],
  requirement: ['Required level', '需求等級', '需求等级'],
  create: ['Create draft', '建立草稿', '创建草稿'],
  preview: ['Draft preview', '草稿預覽', '草稿预览'],
  previewHint: ['Compare this draft with your current equipment. Saving to the library does not equip it.', '比較此草稿與目前裝備。儲存至收藏庫不會裝備物品。', '比较此草稿与当前装备。保存到收藏库不会装备物品。'],
  save: ['Save to library', '儲存至收藏庫', '保存到收藏库'],
  saved: ['Saved to library', '已儲存至收藏庫', '已保存到收藏库'],
  close: ['Close crafter', '關閉打造面板', '关闭打造面板'],
  loading: ['Loading item data…', '正在載入物品資料…', '正在加载物品数据…'],
  error: ['Could not create this draft. Check the item data and try again.', '無法建立此草稿，請檢查物品資料後重試。', '无法创建此草稿，请检查物品数据后重试。'],
  missing: ['Some game affixes have no display template and cannot be added:', '部分遊戲詞綴缺少顯示範本，暫時無法加入：', '部分游戏词缀缺少显示模板，暂时无法添加：'],
  normalHint: ['Normal items have no explicit affixes. Choose Magic or Rare to add them.', '普通物品沒有隨機詞綴，選擇魔法或稀有即可加入。', '普通物品没有随机词缀，选择魔法或稀有即可添加。'],
} as const;
export function craftT(lang: Lang, key: keyof typeof labels): string {
  return labels[key][lang === 'zh-TW' ? 1 : lang === 'zh-CN' ? 2 : 0];
}
