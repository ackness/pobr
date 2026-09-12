import type { Lang } from './i18n';

const labels = {
  title: ['Edit affixes before comparing', '比較前編輯詞綴', '比较前编辑词缀'],
  hint: ['Simulate filling an open prefix or suffix, or replace an existing modifier. Your equipped item stays unchanged until you apply the comparison.', '模擬補上空前綴／後綴，或替換現有詞綴。重算並套用前不會變更已裝備物品。', '模拟补上空前缀／后缀，或替换现有词缀。重算并应用前不会更改已装备物品。'],
  itemLevel: ['Item level', '物品等級', '物品等级'],
  missingLevel: ['Enter the actual item level to show eligible tiers.', '請輸入物品的實際等級，才能篩選可用階級。', '请输入物品的实际等级，才能筛选可用阶级。'],
  prefix: ['Prefixes', '前綴', '前缀'], suffix: ['Suffixes', '後綴', '后缀'],
  search: ['Search affixes', '搜尋詞綴', '搜索词缀'],
  choose: ['Choose an affix and tier', '選擇詞綴與階級', '选择词缀与阶级'],
  add: ['Add affix', '加入詞綴', '添加词缀'],
  remove: ['Remove affix', '移除詞綴', '移除词缀'],
  replace: ['Replace affix', '替換詞綴', '替换词缀'],
  current: ['Copied roll', '複製的原值', '复制的原值'],
  midpoint: ['Tier midpoint estimate', '該階級數值中點估算', '该阶级数值中点估算'],
  minimum: ['Minimum', '最低', '最低'], maximum: ['Maximum', '最高', '最高'],
  range: ['Roll within tier', '階級內數值', '阶级内数值'],
  requirement: ['Required level', '需求等級', '需求等级'],
  compare: ['Recalculate edited item', '重算修改後物品', '重算修改后物品'],
  reset: ['Discard draft changes', '取消草稿修改', '取消草稿修改'],
  dirty: ['Affix draft has not been recalculated. Compare it before applying.', '詞綴草稿尚未重算；請先比較再套用。', '词缀草稿尚未重算；请先比较再应用。'],
  estimate: ['Midpoints estimate the selected tier only; they are not crafting odds or expected returns. The complete item is recalculated, including modifier interactions.', '中點只估算所選階級的數值，不代表打造機率或機率加權收益。會重算整件物品及詞綴交互作用。', '中点只估算所选阶级的数值，不代表打造概率或概率加权收益。会重算整件物品及词缀交互作用。'],
  unknown: ['Some copied modifiers cannot be assigned to a unique affix group. They are preserved; no free slots are inferred. To simulate crafting, explicitly replace or remove the unresolved lines.', '部分原詞綴無法唯一對應詞綴組，已保留原文，不推測空位。若要模擬打造，請明確替換或移除未確認的行。', '部分原词缀无法唯一对应词缀组，已保留原文，不推测空位。若要模拟打造，请明确替换或移除未确认的行。'],
  unresolved: ['Unresolved affix group', '詞綴組未確認', '词缀组未确认'],
  fractured: ['Fractured affixes cannot be changed or removed.', '破裂詞綴不可修改或移除。', '破裂词缀不可修改或移除。'],
  rarity: ['Manual affix simulation currently supports rare and magic items.', '目前可模擬稀有與魔法物品的詞綴。', '目前可模拟稀有与魔法物品的词缀。'],
  corrupted: ['This item state prevents ordinary affix crafting (corrupted, mirrored, sanctified or unidentified).', '此物品狀態不支援一般詞綴打造（腐化、鏡像、Sanctified 或未鑑定）。', '此物品状态不支持普通词缀打造（腐化、镜像、Sanctified 或未鉴定）。'],
  classification: ['Copied line boundaries could not be verified. Edit the original text and compare again.', '無法確認原詞綴的行邊界。請編輯原文後重新比較。', '无法确认原词缀的行边界。请编辑原文后重新比较。'],
  level: ['A selected tier exceeds the item or character level.', '所選階級超出物品等級或角色需求等級。', '所选阶级超出物品等级或角色需求等级。'],
  'item-level': ['Enter an item level from 1 to 100.', '請輸入 1 至 100 的物品等級。', '请输入 1 至 100 的物品等级。'],
  'affix-limit': ['Affix groups must be unique and prefix/suffix limits must be respected.', '詞綴組不可重複，也不能超出前後綴上限。', '词缀组不可重复，也不能超出前后缀上限。'],
  'invalid-roll': ['Choose a roll from minimum to maximum.', '請選擇階級範圍內的數值。', '请选择阶级范围内的数值。'],
  noOptions: ['No eligible affixes match these filters or the remaining slots.', '目前篩選與剩餘空位下沒有可用詞綴。', '当前筛选与剩余空位下没有可用词缀。'],
  dataMissing: ['This data pack has no affix roll ranges.', '此資料包尚無詞綴數值範圍。', '此数据包尚无词缀数值范围。'],
} as const;

export function affixT(lang: Lang, key: keyof typeof labels): string {
  return labels[key][lang === 'en-US' ? 0 : lang === 'zh-TW' ? 1 : 2];
}
