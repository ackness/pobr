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
  'common.copy': { 'en-US': 'Copy', 'zh-TW': '複製', 'zh-CN': '复制' },
  'common.copied': { 'en-US': 'Copied', 'zh-TW': '已複製', 'zh-CN': '已复制' },
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
  'items.jewels': { 'en-US': 'Jewels (edit on the Tree tab by clicking a socket)', 'zh-TW': '珠寶（在天賦樹頁點插槽編輯）', 'zh-CN': '珠宝（在天赋树页点插槽编辑）' },

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
  'tree.ascendancy': { 'en-US': 'Ascendancy', 'zh-TW': '升華', 'zh-CN': '升华' },
  'tree.focusAsc': { 'en-US': 'Focus ascendancy', 'zh-TW': '定位升華盤', 'zh-CN': '定位升华盘' },
  'tree.pickAscHint': {
    'en-US': 'Pick an ascendancy (top right) to show its cluster',
    'zh-TW': '在右上選擇升華以顯示升華盤',
    'zh-CN': '在右上选择升华以显示升华盘',
  },
  'tree.attrPick': { 'en-US': 'Attribute:', 'zh-TW': '屬性：', 'zh-CN': '属性：' },
  'tree.attr.str': { 'en-US': 'Strength', 'zh-TW': '力量', 'zh-CN': '力量' },
  'tree.attr.dex': { 'en-US': 'Dexterity', 'zh-TW': '敏捷', 'zh-CN': '敏捷' },
  'tree.attr.int': { 'en-US': 'Intelligence', 'zh-TW': '智慧', 'zh-CN': '智慧' },
  'tree.attrDistribute': { 'en-US': 'Attribute points', 'zh-TW': '屬性點調配', 'zh-CN': '属性点调配' },
  'tree.attrUnassigned': { 'en-US': 'unassigned', 'zh-TW': '未分配', 'zh-CN': '未分配' },
  'tree.jewel': { 'en-US': 'Jewel socket', 'zh-TW': '珠寶插槽', 'zh-CN': '珠宝插槽' },
  'tree.unallocSocket': { 'en-US': 'Unallocate socket', 'zh-TW': '取消插槽加點', 'zh-CN': '取消插槽加点' },
  'tree.jewelHint': {
    'en-US': 'PoB jewel text (mods may be English or Simplified Chinese). Radius jewels ("... in Radius also grant ...") reshape nearby passives automatically.',
    'zh-TW': 'PoB 珠寶文本（詞條可英文或簡中）。範圍珠寶（「範圍內…同時給予…」）會自動改寫半徑內天賦詞條。',
    'zh-CN': 'PoB 珠宝文本（词条可英文或简中）。范围珠宝（"… in Radius also grant …"）会自动改写半径内天赋词条。',
  },
  'diff.none': { 'en-US': 'no change', 'zh-TW': '無變化', 'zh-CN': '无变化' },
  'diff.ifAlloc': { 'en-US': 'If allocated:', 'zh-TW': '若加點：', 'zh-CN': '若加点：' },
  'diff.ifDealloc': { 'en-US': 'If removed:', 'zh-TW': '若取消：', 'zh-CN': '若取消：' },
  'lib.title': { 'en-US': 'Library', 'zh-TW': '物品庫', 'zh-CN': '物品库' },
  'lib.save': { 'en-US': 'Save to library', 'zh-TW': '存入庫', 'zh-CN': '存入库' },
  'lib.equip': { 'en-US': 'Equip', 'zh-TW': '裝備', 'zh-CN': '装备' },
  'lib.compare': { 'en-US': 'Compare', 'zh-TW': '對比', 'zh-CN': '对比' },
  'lib.delete': { 'en-US': 'Delete', 'zh-TW': '刪除', 'zh-CN': '删除' },
  'lib.empty': {
    'en-US': 'Library is empty — save items/jewels here and switch freely between them.',
    'zh-TW': '庫是空的——把裝備/珠寶存進來即可隨意切換對比。',
    'zh-CN': '库是空的——把装备/珠宝存进来即可随意切换对比。',
  },
  'lib.selectSlotFirst': { 'en-US': 'Select a slot above first', 'zh-TW': '先在上方選中槽位', 'zh-CN': '先在上方选中槽位' },
  'lib.useJewel': { 'en-US': 'Use', 'zh-TW': '使用', 'zh-CN': '使用' },
  'sets.title': { 'en-US': 'Skill sets', 'zh-TW': '技能組套裝', 'zh-CN': '技能组套装' },
  'sets.save': { 'en-US': 'Save current as set', 'zh-TW': '保存當前為套裝', 'zh-CN': '保存当前为套装' },
  'sets.namePlaceholder': { 'en-US': 'Set name…', 'zh-TW': '套裝名稱…', 'zh-CN': '套装名称…' },
  'sets.apply': { 'en-US': 'Apply', 'zh-TW': '套用', 'zh-CN': '套用' },
  'tree.questAttr': { 'en-US': 'Quest rewards:', 'zh-TW': '劇情獎勵：', 'zh-CN': '剧情奖励：' },
  'tree.questAllAttr': { 'en-US': '+5 all', 'zh-TW': '+5 全屬性', 'zh-CN': '+5 全属性' },
  'tree.attrHotkeys': {
    'en-US': 'Hotkeys: S/D/I (or 1/2/3) on a hovered attribute node',
    'zh-TW': '快捷鍵：懸停屬性小點按 S/D/I（或 1/2/3）',
    'zh-CN': '快捷键：悬停属性小点按 S/D/I（或 1/2/3）',
  },

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
  'config.search': { 'en-US': 'Search config options…', 'zh-TW': '搜尋配置項…', 'zh-CN': '搜索配置项…' },

  // Notes 页
  'notes.hint': {
    'en-US': 'Free-form notes. Saved locally in your browser; importing a build brings in its <Notes> section.',
    'zh-TW': '自由筆記。本地保存在瀏覽器；匯入 build 會帶入其 <Notes> 內容。',
    'zh-CN': '自由笔记。本地保存在浏览器；导入 build 会带入其 <Notes> 内容。',
  },
  'notes.placeholder2': { 'en-US': 'Write anything about this build…', 'zh-TW': '寫點關於這個 build 的東西…', 'zh-CN': '写点关于这个 build 的东西…' },

  // 本地存档
  'save.title': { 'en-US': 'Local Save', 'zh-TW': '本地存檔', 'zh-CN': '本地存档' },
  'save.hint': {
    'en-US': 'Every edit is saved to this browser automatically and restored on reload. Export/import a JSON file to back up or move between devices.',
    'zh-TW': '每次編輯都會自動保存到瀏覽器，刷新後自動恢復；可導出/導入 JSON 文件備份或跨設備遷移。',
    'zh-CN': '每次编辑都会自动保存到浏览器，刷新后自动恢复；可导出/导入 JSON 文件备份或跨设备迁移。',
  },
  'save.export': { 'en-US': 'Export JSON', 'zh-TW': '導出 JSON', 'zh-CN': '导出 JSON' },
  'save.import': { 'en-US': 'Import JSON', 'zh-TW': '導入 JSON', 'zh-CN': '导入 JSON' },
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

// ---------------------------------------------------------------------------
// 动态键标签（槽位 / 配置分区 / 敌人档位 / 聚合属性名）——键来自数据，
// 不进 DICT；查不到回退原键。
// ---------------------------------------------------------------------------

const SLOT_LABELS: Record<string, Entry> = {
  weapon1: { 'en-US': 'Main Hand', 'zh-TW': '主手', 'zh-CN': '主手' },
  weapon2: { 'en-US': 'Off Hand', 'zh-TW': '副手', 'zh-CN': '副手' },
  helmet: { 'en-US': 'Helmet', 'zh-TW': '頭盔', 'zh-CN': '头盔' },
  bodyarmour: { 'en-US': 'Body Armour', 'zh-TW': '胸甲', 'zh-CN': '胸甲' },
  gloves: { 'en-US': 'Gloves', 'zh-TW': '手套', 'zh-CN': '手套' },
  boots: { 'en-US': 'Boots', 'zh-TW': '鞋子', 'zh-CN': '鞋子' },
  amulet: { 'en-US': 'Amulet', 'zh-TW': '項鍊', 'zh-CN': '项链' },
  ring1: { 'en-US': 'Ring 1', 'zh-TW': '戒指 1', 'zh-CN': '戒指 1' },
  ring2: { 'en-US': 'Ring 2', 'zh-TW': '戒指 2', 'zh-CN': '戒指 2' },
  ring3: { 'en-US': 'Ring 3', 'zh-TW': '戒指 3', 'zh-CN': '戒指 3' },
  belt: { 'en-US': 'Belt', 'zh-TW': '腰帶', 'zh-CN': '腰带' },
};

/** 装备槽稳定 id → 本地化槽位名。 */
export function slotLabel(lang: Lang, slotId: string): string {
  return SLOT_LABELS[slotId]?.[lang] ?? slotId;
}

const CONFIG_SECTION_LABELS: Record<string, Entry> = {
  General: { 'en-US': 'General', 'zh-TW': '一般', 'zh-CN': '常规' },
  'Quest Rewards': { 'en-US': 'Quest Rewards', 'zh-TW': '任務獎勵', 'zh-CN': '任务奖励' },
  'Skill Options': { 'en-US': 'Skill Options', 'zh-TW': '技能選項', 'zh-CN': '技能选项' },
  'When In Combat': { 'en-US': 'When In Combat', 'zh-TW': '戰鬥狀態', 'zh-CN': '战斗状态' },
  'For Effective DPS': { 'en-US': 'For Effective DPS', 'zh-TW': '有效 DPS 條件', 'zh-CN': '有效 DPS 条件' },
  'Enemy Stats': { 'en-US': 'Enemy Stats', 'zh-TW': '敵人屬性', 'zh-CN': '敌人属性' },
  'Custom Modifiers': { 'en-US': 'Custom Modifiers', 'zh-TW': '自訂詞條', 'zh-CN': '自定义词条' },
};

/** Config 分区名（数据原名）→ 本地化。 */
export function configSectionLabel(lang: Lang, section: string): string {
  return CONFIG_SECTION_LABELS[section]?.[lang] ?? section;
}

const ENEMY_TIER_LABELS: Record<string, Entry> = {
  none: { 'en-US': 'Normal enemy', 'zh-TW': '一般敵人', 'zh-CN': '普通敌人' },
  boss: { 'en-US': 'Boss', 'zh-TW': '頭目', 'zh-CN': 'Boss' },
  pinnacle: { 'en-US': 'Pinnacle Boss', 'zh-TW': '巔峰頭目', 'zh-CN': '巅峰 Boss' },
  uber: { 'en-US': 'Uber Boss', 'zh-TW': '終極頭目', 'zh-CN': '终极 Boss' },
};

/** 敌人档位 → 本地化。 */
export function enemyTierLabel(lang: Lang, tier: string): string {
  return ENEMY_TIER_LABELS[tier]?.[lang] ?? tier;
}

const MOD_NAME_LABELS: Record<string, Entry> = {
  Life: { 'en-US': 'Life', 'zh-TW': '生命', 'zh-CN': '生命' },
  Mana: { 'en-US': 'Mana', 'zh-TW': '魔力', 'zh-CN': '魔力' },
  EnergyShield: { 'en-US': 'Energy Shield', 'zh-TW': '能量護盾', 'zh-CN': '能量护盾' },
  Spirit: { 'en-US': 'Spirit', 'zh-TW': '精魂', 'zh-CN': '精魂' },
  Armour: { 'en-US': 'Armour', 'zh-TW': '護甲', 'zh-CN': '护甲' },
  Evasion: { 'en-US': 'Evasion', 'zh-TW': '閃避', 'zh-CN': '闪避' },
  FireResist: { 'en-US': 'Fire Resistance', 'zh-TW': '火焰抗性', 'zh-CN': '火焰抗性' },
  ColdResist: { 'en-US': 'Cold Resistance', 'zh-TW': '冰冷抗性', 'zh-CN': '冰冷抗性' },
  LightningResist: { 'en-US': 'Lightning Resistance', 'zh-TW': '閃電抗性', 'zh-CN': '闪电抗性' },
  ChaosResist: { 'en-US': 'Chaos Resistance', 'zh-TW': '混沌抗性', 'zh-CN': '混沌抗性' },
  Speed: { 'en-US': 'Attack/Cast Speed', 'zh-TW': '攻擊/施放速度', 'zh-CN': '攻击/施放速度' },
  CritChance: { 'en-US': 'Crit Chance', 'zh-TW': '暴擊率', 'zh-CN': '暴击率' },
  CritMultiplier: { 'en-US': 'Crit Multiplier', 'zh-TW': '暴擊加成', 'zh-CN': '暴击加成' },
  Accuracy: { 'en-US': 'Accuracy', 'zh-TW': '命中', 'zh-CN': '命中' },
  MovementSpeed: { 'en-US': 'Movement Speed', 'zh-TW': '移動速度', 'zh-CN': '移动速度' },
  TotalDPS: { 'en-US': 'Total DPS', 'zh-TW': '總 DPS', 'zh-CN': '总 DPS' },
  TotalEHP: { 'en-US': 'Effective HP', 'zh-TW': '有效生命', 'zh-CN': '有效生命' },
};

const ORIGIN_KIND_LABELS: Record<string, Entry> = {
  CharacterBase: { 'en-US': 'Character', 'zh-TW': '角色基礎', 'zh-CN': '角色基础' },
  Item: { 'en-US': 'Item base', 'zh-TW': '物品基底', 'zh-CN': '物品基底' },
  ItemAffix: { 'en-US': 'Item affix', 'zh-TW': '物品詞綴', 'zh-CN': '物品词缀' },
  ItemImplicit: { 'en-US': 'Implicit', 'zh-TW': '固有詞綴', 'zh-CN': '固有词缀' },
  ItemEnchant: { 'en-US': 'Enchant', 'zh-TW': '附魔', 'zh-CN': '附魔' },
  ItemQuality: { 'en-US': 'Quality', 'zh-TW': '品質', 'zh-CN': '品质' },
  PassiveNode: { 'en-US': 'Passive', 'zh-TW': '天賦節點', 'zh-CN': '天赋节点' },
  AscendancyNode: { 'en-US': 'Ascendancy', 'zh-TW': '升華節點', 'zh-CN': '升华节点' },
  Jewel: { 'en-US': 'Jewel', 'zh-TW': '珠寶', 'zh-CN': '珠宝' },
  SkillGem: { 'en-US': 'Skill gem', 'zh-TW': '技能寶石', 'zh-CN': '技能宝石' },
  SupportGem: { 'en-US': 'Support gem', 'zh-TW': '輔助寶石', 'zh-CN': '辅助宝石' },
  SkillLevel: { 'en-US': 'Skill level', 'zh-TW': '技能等級', 'zh-CN': '技能等级' },
  GemQuality: { 'en-US': 'Gem quality', 'zh-TW': '寶石品質', 'zh-CN': '宝石品质' },
  Config: { 'en-US': 'Config', 'zh-TW': '配置', 'zh-CN': '配置' },
  Buff: { 'en-US': 'Buff', 'zh-TW': '增益', 'zh-CN': '增益' },
  Derived: { 'en-US': 'Derived', 'zh-TW': '派生', 'zh-CN': '派生' },
};

/** 词条来源类别（SourceKind 名）→ 本地化。 */
export function originKindLabel(lang: Lang, kind: string): string {
  return ORIGIN_KIND_LABELS[kind]?.[lang] ?? kind;
}

/** 聚合属性名（breakdown 键 / 归因字段）→ 本地化。 */
export function statNameLabel(lang: Lang, id: string): string {
  return MOD_NAME_LABELS[id]?.[lang] ?? id;
}
