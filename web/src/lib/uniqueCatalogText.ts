import type { Lang } from './i18n';
const labels = {
  title: ['Unique catalogue', '傳奇物品目錄', '传奇物品目录'],
  close: ['Close unique catalogue', '關閉傳奇目錄', '关闭传奇目录'],
  hint: ['Search names or modifier text, choose a version and rolls, then preview or save the item. Rolls default to the midpoint.', '可搜尋名稱或詞綴文字，選擇版本與數值後預覽或儲存。數值預設為範圍中點。', '可搜索名称或词缀文字，选择版本与数值后预览或保存。数值默认为范围中点。'],
  search: ['Search unique names and modifiers', '搜尋傳奇名稱與詞綴', '搜索传奇名称与词缀'],
  allClasses: ['All classes', '所有類別', '所有类别'],
  allSlots: ['All slots', '所有部位', '所有部位'],
  itemClass: ['Item class', '物品類別', '物品类别'],
  slot: ['Equipment slot', '裝備部位', '装备部位'],
  choose: ['Choose a unique', '選擇傳奇物品', '选择传奇物品'],
  matches: ['Matching items', '符合的物品', '匹配的物品'],
  empty: ['No uniques match these filters.', '沒有符合篩選的傳奇物品。', '没有符合筛选的传奇物品。'],
  variant: ['Variant', '變體', '变体'],
  alt: ['Additional variant', '附加變體', '附加变体'],
  group: ['Variant group', '變體群組', '变体组'],
  version: ['Version', '版本', '版本'],
  rolls: ['Modifier rolls', '詞綴數值', '词缀数值'],
  roll: ['Roll', '數值', '数值'],
  minimum: ['All minimum rolls', '全部最低值', '全部最低值'],
  middle: ['All midpoint rolls', '全部中間值', '全部中间值'],
  maximum: ['All maximum rolls', '全部最高值', '全部最高值'],
  preview: ['Preview unique', '預覽傳奇物品', '预览传奇物品'],
  preparing: ['Preparing item…', '正在準備物品…', '正在准备物品…'],
  error: ['Could not prepare this unique. Check the selection and try again.', '無法準備此傳奇物品，請檢查選擇後重試。', '无法准备此传奇物品，请检查选择后重试。'],
  baseUnavailable: ['This catalogue variant has no matching base. Choose another variant.', '目錄中此變體沒有對應基底，請選擇其他變體。', '目录中此变体没有对应基底，请选择其他变体。'],
  jewel: ['Passive jewel socket', '天賦珠寶插槽', '天赋珠宝插槽'],
  unavailable: ['This base cannot be compared in an equipment slot.', '此基底無法在裝備部位中比較。', '此基底无法在装备部位中比较。'],
} as const;
export function uniqueT(lang: Lang, key: keyof typeof labels): string {
  return labels[key][lang === 'zh-TW' ? 1 : lang === 'zh-CN' ? 2 : 0];
}
