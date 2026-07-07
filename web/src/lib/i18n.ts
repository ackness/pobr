/**
 * UI 文案三语字典（en-US / zh-TW / zh-CN）。
 *
 * 只覆盖界面文案；游戏名词（宝石/技能/物品名）走数据包 i18n 边车——当前仅有
 * 繁中（zh-TW），简中游戏数据需国服数据源接入（见 TODO.md），此前简中界面
 * 下的游戏名词回退繁中。
 */

export type Lang = 'en-US' | 'zh-TW' | 'zh-CN';

export const LANGS: Lang[] = ['en-US', 'zh-TW', 'zh-CN'];

type Entry = { 'en-US': string; 'zh-TW': string; 'zh-CN': string };

const DICT = {
  // 页签
  'tab.build': { 'en-US': 'Build', 'zh-TW': '構建', 'zh-CN': '构建' },
  'tab.tree': { 'en-US': 'Tree', 'zh-TW': '天賦樹', 'zh-CN': '天赋树' },
  'tab.skills': { 'en-US': 'Skills', 'zh-TW': '技能', 'zh-CN': '技能' },
  'tab.items': { 'en-US': 'Items', 'zh-TW': '裝備', 'zh-CN': '装备' },
  'tab.calcs': { 'en-US': 'Calcs', 'zh-TW': '計算', 'zh-CN': '计算' },
  'tab.config': { 'en-US': 'Config', 'zh-TW': '配置', 'zh-CN': '配置' },
  'tab.notes': { 'en-US': 'Notes', 'zh-TW': '筆記', 'zh-CN': '笔记' },
  'notes.placeholder': { 'en-US': 'Notes (placeholder)', 'zh-TW': 'Notes（佔位）', 'zh-CN': 'Notes（占位）' },

  // Build 页
  'build.character': { 'en-US': 'Character', 'zh-TW': '角色', 'zh-CN': '角色' },
  'build.class': { 'en-US': 'Class', 'zh-TW': '職業', 'zh-CN': '职业' },
  'build.ascendancy': { 'en-US': 'Ascendancy', 'zh-TW': '升華', 'zh-CN': '升华' },
  'build.level': { 'en-US': 'Level', 'zh-TW': '等級', 'zh-CN': '等级' },
  'build.none': { 'en-US': '(none)', 'zh-TW': '（無）', 'zh-CN': '（无）' },
  'build.newHint': {
    'en-US': 'Switching class starts a fresh empty build (allocate passives on the Tree tab). Import a PoB2 code below to replace everything.',
    'zh-TW': '切換職業會開一個全新空 build（樹加點在天賦樹頁點選）；下方可一鍵導入 PoB2 code 替換全部內容。',
    'zh-CN': '切换职业会开一个全新空 build（树加点在天赋树页点选）；下方可一键导入 PoB2 code 替换全部内容。',
  },
  'build.import': { 'en-US': 'Import Build Code', 'zh-TW': '匯入 Build Code', 'zh-CN': '导入 Build Code' },
  'build.importPlaceholder': { 'en-US': 'Paste a PoB2 build code here…', 'zh-TW': '在此貼上 PoB2 Build Code…', 'zh-CN': '在此粘贴 PoB2 Build Code…' },
  'build.importButton': { 'en-US': 'Import', 'zh-TW': '匯入', 'zh-CN': '导入' },
  'build.calculating': { 'en-US': 'Calculating…', 'zh-TW': '計算中…', 'zh-CN': '计算中…' },
  'build.imported': { 'en-US': 'Imported: ', 'zh-TW': '已匯入：', 'zh-CN': '已导入：' },
  'build.passives': { 'en-US': 'passives', 'zh-TW': '天賦點', 'zh-CN': '天赋点' },
  'build.itemsCount': { 'en-US': 'items', 'zh-TW': '件裝備', 'zh-CN': '件装备' },
  'build.unsupported': { 'en-US': 'Unsupported modifiers', 'zh-TW': '未支援詞條', 'zh-CN': '未支持词条' },

  // Skills 页
  'skills.title': { 'en-US': 'Socket Groups', 'zh-TW': '技能組', 'zh-CN': '技能组' },
  'skills.addPlaceholder': { 'en-US': 'Search an active gem to add a group…', 'zh-TW': '搜尋主動技能以新建組…', 'zh-CN': '搜索主动技能以新建组…' },
  'skills.addSupport': { 'en-US': 'Add a support gem…', 'zh-TW': '添加輔助寶石…', 'zh-CN': '添加辅助宝石…' },
  'skills.hint': {
    'en-US': 'Click a group title to make it the main skill; level/quality edits recalc live.',
    'zh-TW': '點組標題設為主技能；等級/品質即改即算。',
    'zh-CN': '点组标题设为主技能；等级/品质即改即算。',
  },
  'skills.empty': {
    'en-US': 'No socket groups yet — add one with the search box above, or import a build code.',
    'zh-TW': '尚無技能組——用上方搜尋框添加，或匯入 build code。',
    'zh-CN': '尚无技能组——用上方搜索框添加，或导入 build code。',
  },
  'skills.emptyGroup': { 'en-US': '(empty)', 'zh-TW': '（空組）', 'zh-CN': '（空组）' },
  'skills.main': { 'en-US': 'MAIN', 'zh-TW': '主技能', 'zh-CN': '主技能' },
  'skills.setMain': { 'en-US': 'Set as main skill', 'zh-TW': '設為主技能', 'zh-CN': '设为主技能' },
  'skills.enabled': { 'en-US': 'on', 'zh-TW': '啟用', 'zh-CN': '启用' },
  'skills.removeGroup': { 'en-US': 'Remove group', 'zh-TW': '刪除組', 'zh-CN': '删除组' },
  'skills.removeGem': { 'en-US': 'Remove gem', 'zh-TW': '移除寶石', 'zh-CN': '移除宝石' },
  'skills.level': { 'en-US': 'Level', 'zh-TW': '等級', 'zh-CN': '等级' },
  'skills.quality': { 'en-US': 'Quality', 'zh-TW': '品質', 'zh-CN': '品质' },
  'picker.all': { 'en-US': 'All', 'zh-TW': '全部', 'zh-CN': '全部' },
  'picker.noResults': { 'en-US': 'No matches', 'zh-TW': '無匹配', 'zh-CN': '无匹配' },
  'picker.active': { 'en-US': 'Active', 'zh-TW': '主動', 'zh-CN': '主动' },
  'picker.support': { 'en-US': 'Support', 'zh-TW': '輔助', 'zh-CN': '辅助' },

  // Items 页
  'items.title': { 'en-US': 'Items', 'zh-TW': '裝備', 'zh-CN': '装备' },
  'items.hint': {
    'en-US': 'Edit each slot as PoB item text (Rarity line + name + base + one mod per line); apply recalcs. Mod lines and base names may be in English or Simplified Chinese (CN-realm text); structural lines (Rarity:) stay PoB-style.',
    'zh-TW': '每個槽位直接編輯 PoB 物品文本（Rarity 行 + 名稱 + 基底 + 詞條逐行），保存即重算。詞條行與基底名支持英文或簡中（國服文本）；結構行（Rarity:）保持 PoB 格式。',
    'zh-CN': '每个槽位直接编辑 PoB 物品文本（Rarity 行 + 名称 + 基底 + 词条逐行），保存即重算。词条行与基底名支持英文或简中（国服文本）；结构行（Rarity:）保持 PoB 格式。',
  },
  'items.edit': { 'en-US': 'Edit', 'zh-TW': '編輯', 'zh-CN': '编辑' },
  'items.add': { 'en-US': 'Add', 'zh-TW': '添加', 'zh-CN': '添加' },
  'items.remove': { 'en-US': 'Remove', 'zh-TW': '移除', 'zh-CN': '移除' },
  'items.apply': { 'en-US': 'Apply', 'zh-TW': '保存並重算', 'zh-CN': '保存并重算' },
  'items.cancel': { 'en-US': 'Cancel', 'zh-TW': '取消', 'zh-CN': '取消' },
  'items.empty': { 'en-US': '(empty)', 'zh-TW': '（空）', 'zh-CN': '（空）' },
  'items.flasks': { 'en-US': 'Flasks / Charms (read-only, from import)', 'zh-TW': '藥劑 / 護符（只讀，來自匯入）', 'zh-CN': '药剂 / 护符（只读，来自导入）' },
  'items.jewels': { 'en-US': 'Jewels (read-only, from import)', 'zh-TW': '珠寶（只讀，來自匯入）', 'zh-CN': '珠宝（只读，来自导入）' },

  // Calcs 页
  'calcs.title': { 'en-US': 'Calculations', 'zh-TW': '計算明細', 'zh-CN': '计算明细' },
  'calcs.hint': {
    'en-US': 'Click a stat to expand its base/inc decomposition and per-source modifiers.',
    'zh-TW': '點擊字段展開 base/inc 分解與逐來源詞條。',
    'zh-CN': '点击字段展开 base/inc 分解与逐来源词条。',
  },
  'calcs.mods': { 'en-US': 'mods', 'zh-TW': '條', 'zh-CN': '条' },
  'calcs.baseTotal': { 'en-US': 'Base total', 'zh-TW': '基礎合計', 'zh-CN': '基础合计' },
  'calcs.incTotal': { 'en-US': 'Increased total', 'zh-TW': '增加合計', 'zh-CN': '增加合计' },
  'calcs.type': { 'en-US': 'Type', 'zh-TW': '類型', 'zh-CN': '类型' },
  'calcs.value': { 'en-US': 'Value', 'zh-TW': '數值', 'zh-CN': '数值' },
  'calcs.modifier': { 'en-US': 'Modifier', 'zh-TW': '詞條', 'zh-CN': '词条' },
  'calcs.source': { 'en-US': 'Source', 'zh-TW': '來源', 'zh-CN': '来源' },
  'calcs.baseDerived': { 'en-US': '(base/derived)', 'zh-TW': '（基底/派生）', 'zh-CN': '（基底/派生）' },
  'calcs.attribution': { 'en-US': 'Source Attribution', 'zh-TW': '來源貢獻歸因', 'zh-CN': '来源贡献归因' },
  'calcs.attributionHint': {
    'en-US': 'Recomputes the build without each source and reports marginal contributions (expensive; click to run).',
    'zh-TW': '對每個來源做「移除後重算」，報告其對關鍵字段的邊際貢獻（計算量大，點擊觸發）。',
    'zh-CN': '对每个来源做「移除后重算」，报告其对关键字段的边际贡献（计算量大，点击触发）。',
  },
  'calcs.runAttribution': { 'en-US': 'Run attribution', 'zh-TW': '計算歸因', 'zh-CN': '计算归因' },
  'calcs.running': { 'en-US': 'Running…', 'zh-TW': '歸因計算中…', 'zh-CN': '归因计算中…' },
  'calcs.baseline': { 'en-US': 'Baseline (full build)', 'zh-TW': '基線（完整 build）', 'zh-CN': '基线（完整 build）' },
  'calcs.group': { 'en-US': 'Group', 'zh-TW': '技能組', 'zh-CN': '技能组' },

  // Tree 页
  'tree.title': { 'en-US': 'Passive Tree', 'zh-TW': '天賦樹', 'zh-CN': '天赋树' },
  'tree.allocated': { 'en-US': 'allocated', 'zh-TW': '已加點', 'zh-CN': '已加点' },
  'tree.hint': { 'en-US': 'Click a node to allocate/deallocate (recalcs live)', 'zh-TW': '點擊節點加點/取消，即時重算', 'zh-CN': '点击节点加点/取消，即时重算' },
  'tree.reset': { 'en-US': 'Reset view', 'zh-TW': '重置視圖', 'zh-CN': '重置视图' },
  'tree.loading': { 'en-US': 'Loading tree…', 'zh-TW': '載入樹資料…', 'zh-CN': '加载树数据…' },

  // Config 页
  'config.title': { 'en-US': 'Configuration', 'zh-TW': '戰鬥配置', 'zh-CN': '战斗配置' },
  'config.enemyTier': { 'en-US': 'Enemy tier', 'zh-TW': '敵人檔位', 'zh-CN': '敌人档位' },
  'config.inputs': { 'en-US': 'Config inputs', 'zh-TW': 'Config 輸入（<Input> 鍵值）', 'zh-CN': 'Config 输入（<Input> 键值）' },
  'config.hint': {
    'en-US': 'Raw config inputs from the build; edits trigger recalculation. Keys match PoB2 config vars (e.g. conditionEnemyChilled).',
    'zh-TW': '來自 build 的原始配置；修改值即重算。鍵名與 PoB2 Config 頁一致（如 conditionEnemyChilled）。',
    'zh-CN': '来自 build 的原始配置；修改值即重算。键名与 PoB2 Config 页一致（如 conditionEnemyChilled）。',
  },
  'config.addTitle': { 'en-US': 'Add config input', 'zh-TW': '新增配置項', 'zh-CN': '新增配置项' },
  'config.keyPlaceholder': { 'en-US': 'key (e.g. enemyDistance)', 'zh-TW': '鍵名（如 enemyDistance）', 'zh-CN': '键名（如 enemyDistance）' },
  'config.key': { 'en-US': 'Config key', 'zh-TW': '配置鍵名', 'zh-CN': '配置键名' },
  'config.valueLabel': { 'en-US': 'Config value', 'zh-TW': '配置值', 'zh-CN': '配置值' },
  'config.addButton': { 'en-US': 'Add & recalc', 'zh-TW': '加入並重算', 'zh-CN': '加入并重算' },
  'config.reset': { 'en-US': 'Reset', 'zh-TW': '還原', 'zh-CN': '还原' },
} as const satisfies Record<string, Entry>;

export type UiKey = keyof typeof DICT;

/** 取 UI 文案。 */
export function t(lang: Lang, key: UiKey): string {
  return DICT[key][lang];
}

/** 组件便捷绑定：`const tt = bindT(lang)`。 */
export function bindT(lang: Lang): (key: UiKey) => string {
  return (key) => t(lang, key);
}
