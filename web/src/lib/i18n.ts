/**
 * UI 文案三语字典（en-US / zh-TW / zh-CN）。
 *
 * 只覆盖界面文案；游戏名词（宝石/技能/物品名）走数据包 i18n 边车——当前仅有
 * 繁中（zh-TW），简中游戏数据需国服数据源接入（见 TODO.md），此前简中界面
 * 下的游戏名词回退繁中。
 */

import { STAT_SECTIONS } from './statDisplay';

export type Lang = 'en-US' | 'zh-TW' | 'zh-CN';

export const LANGS: Lang[] = ['en-US', 'zh-TW', 'zh-CN'];

type Entry = { 'en-US': string; 'zh-TW': string; 'zh-CN': string };

const DICT = {
  // Beta 提示
  'beta.notice': {
    'en-US':
      'Beta preview — calculation results, game data and the wasm/JSON API are still evolving and may change or break without notice.',
    'zh-TW': '測試版預覽——計算結果、遊戲資料與 wasm/JSON API 仍在迭代中，可能隨時變動或不穩定。',
    'zh-CN': '测试版预览——计算结果、游戏数据与 wasm/JSON API 仍在迭代中，可能随时变动或不稳定。',
  },
  'beta.dismiss': { 'en-US': 'Got it', 'zh-TW': '知道了', 'zh-CN': '知道了' },

  // 页签
  'loadout.switch': { 'en-US': 'Loadout', 'zh-TW': '配置組', 'zh-CN': '配置组' },
  'tab.build': { 'en-US': 'Build', 'zh-TW': '構建', 'zh-CN': '构建' },
  'tab.tree': { 'en-US': 'Tree', 'zh-TW': '天賦樹', 'zh-CN': '天赋树' },
  'tab.skills': { 'en-US': 'Skills', 'zh-TW': '技能', 'zh-CN': '技能' },
  'tab.items': { 'en-US': 'Items', 'zh-TW': '裝備', 'zh-CN': '装备' },
  'tab.trade': { 'en-US': 'Trade', 'zh-TW': '市集', 'zh-CN': '市集' },
  'tab.calcs': { 'en-US': 'Calcs', 'zh-TW': '計算', 'zh-CN': '计算' },
  'tab.config': { 'en-US': 'Config', 'zh-TW': '配置', 'zh-CN': '配置' },
  'tab.notes': { 'en-US': 'Notes', 'zh-TW': '筆記', 'zh-CN': '笔记' },

  // 局部注释（装备/技能组/珠宝旁）
  'note.label': { 'en-US': 'Note', 'zh-TW': '備註', 'zh-CN': '备注' },
  'note.add': { 'en-US': 'Add note', 'zh-TW': '加備註', 'zh-CN': '加备注' },
  'note.placeholder': {
    'en-US': 'Why this choice? Key mods, alternatives…',
    'zh-TW': '為什麼選它？核心詞條、替代方案…',
    'zh-CN': '为什么选它？核心词条、替代方案…',
  },
  'note.editHint': { 'en-US': 'Click to edit', 'zh-TW': '點擊編輯', 'zh-CN': '点击编辑' },

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
  'picker.lineage': { 'en-US': 'Lineage', 'zh-TW': '血脈', 'zh-CN': '血脉' },

  // 技能组辅助宝石寻优
  'opt.open': { 'en-US': 'Support optimizer', 'zh-TW': '輔助寶石尋優', 'zh-CN': '辅助宝石寻优' },
  'opt.hint': {
    'en-US': 'Pick candidate supports; every combination is fully recalculated and ranked.',
    'zh-TW': '選一批候選輔助寶石，逐個組合完整重算後按目標排名。',
    'zh-CN': '选一批候选辅助宝石，逐个组合完整重算后按目标排名。',
  },
  'opt.addCandidate': { 'en-US': 'Add a candidate support…', 'zh-TW': '添加候選輔助寶石…', 'zh-CN': '添加候选辅助宝石…' },
  'opt.freeSlots': { 'en-US': 'Free sockets', 'zh-TW': '空槽數', 'zh-CN': '空槽数' },
  'opt.objective': { 'en-US': 'Objective', 'zh-TW': '目標', 'zh-CN': '目标' },
  'opt.objDps': { 'en-US': 'Max total DPS', 'zh-TW': '總 DPS 最高', 'zh-CN': '总 DPS 最高' },
  'opt.objDpsPerMana': { 'en-US': 'Max DPS / mana cost', 'zh-TW': 'DPS÷魔耗最高（效率）', 'zh-CN': 'DPS÷魔耗最高（效率）' },
  'opt.objLife': { 'en-US': 'Max Life', 'zh-TW': '生命最高', 'zh-CN': '生命最高' },
  'opt.objEhp': { 'en-US': 'Max total EHP', 'zh-TW': '總 EHP 最高', 'zh-CN': '总 EHP 最高' },
  'opt.constraint': { 'en-US': 'Constraint', 'zh-TW': '約束', 'zh-CN': '约束' },
  'opt.constraintNone': { 'en-US': '(none)', 'zh-TW': '（無）', 'zh-CN': '（无）' },
  'opt.min': { 'en-US': 'min', 'zh-TW': '下限', 'zh-CN': '下限' },
  'opt.max': { 'en-US': 'max', 'zh-TW': '上限', 'zh-CN': '上限' },
  'opt.run': { 'en-US': 'Optimize', 'zh-TW': '開始尋優', 'zh-CN': '开始寻优' },
  'opt.cancel': { 'en-US': 'Cancel', 'zh-TW': '取消', 'zh-CN': '取消' },
  'opt.running': { 'en-US': 'Optimizing…', 'zh-TW': '尋優中…', 'zh-CN': '寻优中…' },
  'opt.combos': { 'en-US': 'combos', 'zh-TW': '個組合', 'zh-CN': '个组合' },
  'opt.tooMany': {
    'en-US': 'Too many combinations — remove candidates or lower free sockets.',
    'zh-TW': '組合數過多——減少候選或調低空槽數。',
    'zh-CN': '组合数过多——减少候选或调低空槽数。',
  },
  'opt.needCandidates': {
    'en-US': 'Add candidate support gems first.',
    'zh-TW': '先添加候選輔助寶石。',
    'zh-CN': '先添加候选辅助宝石。',
  },
  'opt.baseline': { 'en-US': 'Baseline (no change)', 'zh-TW': '基線（不變）', 'zh-CN': '基线（不变）' },
  'opt.openItem': {
    'en-US': 'Try on items from my library',
    'zh-TW': '試穿物品庫的同槽裝備',
    'zh-CN': '试穿物品库的同槽装备',
  },
  'opt.itemHint': {
    'en-US':
      'Tries every same-slot item in your library on this slot (plus "unequip"), recalculates the build for each, and ranks them by the chosen goal — apply the best with one click.',
    'zh-TW':
      '把物品庫裡能裝進這個槽位的裝備逐件試穿（含「卸下」），每件完整重算，按所選目標排名——點「應用」直接換上。',
    'zh-CN':
      '把物品库里能装进这个槽位的装备逐件试穿（含「卸下」），每件完整重算，按所选目标排名——点「应用」直接换上。',
  },
  'opt.needLibrary': {
    'en-US': 'No candidates for this slot — save a few items to the library first.',
    'zh-TW': '物品庫沒有該槽位的候選——先把幾件裝備存進庫。',
    'zh-CN': '物品库没有该槽位的候选——先把几件装备存进库。',
  },
  'opt.openTree': { 'en-US': 'Node optimizer', 'zh-TW': '天賦節點尋優', 'zh-CN': '天赋节点寻优' },
  'opt.points': { 'en-US': 'Points to spend', 'zh-TW': '可用點數', 'zh-CN': '可用点数' },
  'opt.treeNeedsHeat': {
    'en-US': 'Run the node power heatmap above first to build the candidate list.',
    'zh-TW': '先在上方跑一次節點威力熱力圖，生成候選榜。',
    'zh-CN': '先在上方跑一次节点威力热力图，生成候选榜。',
  },
  'opt.treeHint': {
    'en-US':
      'Tick candidate nodes from the power list; combinations are recalculated in full. Connectivity is NOT validated — pathing is up to you.',
    'zh-TW': '從威力榜勾選候選節點，逐組合完整重算。不驗證連通性——路徑自己負責。',
    'zh-CN': '从威力榜勾选候选节点，逐组合完整重算。不验证连通性——路径自己负责。',
  },
  'opt.score': { 'en-US': 'Score', 'zh-TW': '得分', 'zh-CN': '得分' },
  'opt.apply': { 'en-US': 'Apply', 'zh-TW': '應用', 'zh-CN': '应用' },
  'opt.infeasible': { 'en-US': 'constraint not met', 'zh-TW': '不滿足約束', 'zh-CN': '不满足约束' },

  // Trade 市集页（独立 Tab，PoB2 Trader 的对应物）
  'trade.title': { 'en-US': 'Find upgrades on trade', 'zh-TW': '市集找升級', 'zh-CN': '市集找升级' },
  'trade.hint': {
    'en-US':
      'Pick a goal and a budget, then hit "Find better" on a slot. PoBR measures how much each mod on your current item really contributes, and opens the official trade site sorted by that gain — the top results are the biggest upgrades you can actually buy.',
    'zh-TW':
      '選好目標和預算，點某個槽位的「找更好的」。PoBR 會先算出這件裝備每條詞條對你的實際提升，再打開官方市集、按「對你的提升」排序——排最前面的就是預算內能買到的最大升級。',
    'zh-CN':
      '选好目标和预算，点某个槽位的「找更好的」。PoBR 会先算出这件装备每条词条对你的实际提升，再打开官方市集、按「对你的提升」排序——排最前面的就是预算内能买到的最大升级。',
  },
  'trade.realm': { 'en-US': 'Server', 'zh-TW': '伺服器', 'zh-CN': '服务器' },
  'trade.realmIntl': { 'en-US': 'International', 'zh-TW': '國際服', 'zh-CN': '国际服' },
  'trade.realmCn': { 'en-US': 'CN (Tencent)', 'zh-TW': '國服', 'zh-CN': '国服' },
  'trade.league': { 'en-US': 'League', 'zh-TW': '聯盟', 'zh-CN': '赛季/联盟' },
  'trade.leagueSeason': { 'en-US': 'current season', 'zh-TW': '賽季服', 'zh-CN': '赛季服' },
  'trade.leagueCustom': { 'en-US': 'Custom…', 'zh-TW': '自定義…', 'zh-CN': '自定义…' },
  'trade.budget': { 'en-US': 'Max price', 'zh-TW': '預算上限', 'zh-CN': '预算上限' },
  'trade.budgetAny': { 'en-US': 'no limit', 'zh-TW': '不限', 'zh-CN': '不限' },
  'trade.curExalted': { 'en-US': 'Exalted', 'zh-TW': '崇高石', 'zh-CN': '崇高石' },
  'trade.curDivine': { 'en-US': 'Divine', 'zh-TW': '神聖石', 'zh-CN': '神圣石' },
  'trade.curChaos': { 'en-US': 'Chaos', 'zh-TW': '混沌石', 'zh-CN': '混沌石' },
  'trade.findBetter': { 'en-US': 'Find better', 'zh-TW': '找更好的', 'zh-CN': '找更好的' },
  'trade.openSite': { 'en-US': 'Open trade site', 'zh-TW': '打開市集', 'zh-CN': '打开市集' },
  'trade.details': { 'en-US': 'mod values', 'zh-TW': '詞條價值明細', 'zh-CN': '词条价值明细' },
  'trade.colLine': { 'en-US': 'Mod', 'zh-TW': '詞條', 'zh-CN': '词条' },
  'trade.colWeight': {
    'en-US': 'Gain per point',
    'zh-TW': '每 1 點的提升',
    'zh-CN': '每 1 点的提升',
  },
  'trade.slotEmpty': { 'en-US': '(empty)', 'zh-TW': '（空）', 'zh-CN': '（空）' },
  'trade.emptyHint': {
    'en-US': 'Equip something first — search weights come from the current item’s mods.',
    'zh-TW': '先隨便裝上一件——搜尋權重來自當前裝備的詞條。',
    'zh-CN': '先随便装上一件——搜索权重来自当前装备的词条。',
  },
  'trade.unavailable': {
    'en-US': 'Trade search needs the mod→trade-stat map, which this data pack does not include.',
    'zh-TW': '市集搜尋需要詞條→市集屬性映射表，當前資料包沒有帶。',
    'zh-CN': '市集搜索需要词条→市集属性映射表，当前数据包没有带。',
  },
  'trade.noMapped': {
    'en-US': 'No mods on this item can be searched on the trade site.',
    'zh-TW': '這件裝備沒有詞條能在市集上搜尋。',
    'zh-CN': '这件装备没有词条能在市集上搜索。',
  },
  'trade.noWeights': {
    'en-US': 'No mod on this item helps the chosen goal — try another goal.',
    'zh-TW': '這件裝備沒有詞條對所選目標有幫助——換個目標試試。',
    'zh-CN': '这件装备没有词条对所选目标有帮助——换个目标试试。',
  },

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
  'items.switcher': { 'en-US': 'Switch item', 'zh-TW': '切換裝備', 'zh-CN': '切换装备' },
  'items.runes': { 'en-US': 'Rune sockets', 'zh-TW': '符文插槽', 'zh-CN': '符文插槽' },
  'items.socketAdd': { 'en-US': 'Add socket', 'zh-TW': '加孔', 'zh-CN': '加孔' },
  'items.socketRemove': { 'en-US': 'Remove socket', 'zh-TW': '減孔', 'zh-CN': '减孔' },
  'items.addSockets': { 'en-US': 'Add rune socket', 'zh-TW': '添加符文插槽', 'zh-CN': '添加符文插槽' },
  'items.emptySocket': { 'en-US': '(empty socket)', 'zh-TW': '（空槽）', 'zh-CN': '（空槽）' },
  'items.runeGroup': { 'en-US': 'Runes', 'zh-TW': '符文', 'zh-CN': '符文' },
  'items.soulCoreGroup': { 'en-US': 'Soul Cores', 'zh-TW': '魂核', 'zh-CN': '魂核' },
  'items.unequip': { 'en-US': '(unequip)', 'zh-TW': '（卸下）', 'zh-CN': '（卸下）' },
  'items.currentItem': { 'en-US': '(current item)', 'zh-TW': '（當前物品）', 'zh-CN': '（当前物品）' },
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
  'calcs.search': { 'en-US': 'Search aggregates…', 'zh-TW': '搜尋聚合量…', 'zh-CN': '搜索聚合量…' },
  'calcs.fullDps': { 'en-US': 'Skill DPS', 'zh-TW': '技能 DPS', 'zh-CN': '技能 DPS' },
  'calcs.fullDpsHint': {
    'en-US':
      'DPS of each enabled damage skill group in the full build context. Click a row to make it the main skill; the list refreshes as the build changes.',
    'zh-TW': '每個啟用傷害技能組在完整 build 語境下的 DPS。點擊某行設為主技能；build 變動時列表自動刷新。',
    'zh-CN': '每个启用伤害技能组在完整 build 语境下的 DPS。点击某行设为主技能；build 变动时列表自动刷新。',
  },
  'calcs.fullDpsEmpty': {
    'en-US': 'No enabled damage skill groups.',
    'zh-TW': '沒有啟用的傷害技能組。',
    'zh-CN': '没有启用的伤害技能组。',
  },
  'calcs.skill': { 'en-US': 'Skill', 'zh-TW': '技能', 'zh-CN': '技能' },

  // 侧边栏主技能区
  'sidebar.mainSkill': { 'en-US': 'Main Skill', 'zh-TW': '主技能', 'zh-CN': '主技能' },
  'sidebar.noMainSkill': {
    'en-US': 'No damage skill in this build.',
    'zh-TW': '此 build 沒有可計算的傷害技能。',
    'zh-CN': '此 build 没有可计算的伤害技能。',
  },
  'sidebar.disabledGroup': { 'en-US': '(disabled)', 'zh-TW': '（停用）', 'zh-CN': '（停用）' },
  'sidebar.computedSkill': { 'en-US': 'Computing', 'zh-TW': '實際計算', 'zh-CN': '实际计算' },
  'sidebar.hitDps': { 'en-US': 'Hit DPS', 'zh-TW': '擊中 DPS', 'zh-CN': '击中 DPS' },
  'sidebar.dotDps': { 'en-US': 'DoT DPS', 'zh-TW': '持續傷害 DPS', 'zh-CN': '持续伤害 DPS' },
  'sidebar.combinedDps': { 'en-US': 'Combined DPS', 'zh-TW': '綜合 DPS', 'zh-CN': '综合 DPS' },
  'sidebar.damageShare': {
    'en-US': 'Hit Damage by Type',
    'zh-TW': '擊中傷害構成',
    'zh-CN': '击中伤害构成',
  },

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
  'lib.search': { 'en-US': 'Search items…', 'zh-TW': '搜尋物品…', 'zh-CN': '搜索物品…' },
  'lib.filterSlot': { 'en-US': 'Current slot only', 'zh-TW': '只看當前槽位', 'zh-CN': '只看当前槽位' },
  'lib.noMatch': { 'en-US': 'No matching items', 'zh-TW': '無符合物品', 'zh-CN': '没有匹配的物品' },
  'lib.useJewel': { 'en-US': 'Use', 'zh-TW': '使用', 'zh-CN': '使用' },
  'sets.title': { 'en-US': 'Skill sets', 'zh-TW': '技能組套裝', 'zh-CN': '技能组套装' },
  'sets.save': { 'en-US': 'Save current as set', 'zh-TW': '保存當前為套裝', 'zh-CN': '保存当前为套装' },
  'sets.namePlaceholder': { 'en-US': 'Set name…', 'zh-TW': '套裝名稱…', 'zh-CN': '套装名称…' },
  'sets.apply': { 'en-US': 'Apply', 'zh-TW': '套用', 'zh-CN': '套用' },
  'tree.heat': { 'en-US': 'Heat map', 'zh-TW': '熱力圖', 'zh-CN': '热力图' },
  'tree.heatOff': { 'en-US': 'Off', 'zh-TW': '關', 'zh-CN': '关' },
  'tree.heatDepth': { 'en-US': 'Depth', 'zh-TW': '深度', 'zh-CN': '深度' },
  'tree.heatComputing': { 'en-US': 'computing…', 'zh-TW': '計算中…', 'zh-CN': '计算中…' },
  'tree.heatRun': { 'en-US': 'Compute', 'zh-TW': '計算', 'zh-CN': '计算' },
  'tree.heatStale': { 'en-US': 'stale — recompute', 'zh-TW': '已過期，點「計算」刷新', 'zh-CN': '已过期，点「计算」刷新' },
  'tree.heatHint': {
    'en-US': 'Nodes glow by how much allocating them improves the chosen stat (within depth of your tree)',
    'zh-TW': '節點按「加點後對所選屬性的提升幅度」發亮（僅計算距已加點前沿指定深度內的節點）',
    'zh-CN': '节点按「加点后对所选属性的提升幅度」发亮（仅计算距已加点前沿指定深度内的节点）',
  },
  'tree.questAttr': { 'en-US': 'Quest rewards:', 'zh-TW': '劇情獎勵：', 'zh-CN': '剧情奖励：' },
  'tree.questAllAttr': { 'en-US': '+5 all', 'zh-TW': '+5 全屬性', 'zh-CN': '+5 全属性' },
  'tree.attrHotkeys': {
    'en-US': 'Hotkeys: S/D/I (or 1/2/3) on a hovered attribute node',
    'zh-TW': '快捷鍵：懸停屬性小點按 S/D/I（或 1/2/3）',
    'zh-CN': '快捷键：悬停属性小点按 S/D/I（或 1/2/3）',
  },
  'tree.search': { 'en-US': 'Search nodes…', 'zh-TW': '搜尋節點…', 'zh-CN': '搜索节点…' },
  'tree.matches': { 'en-US': 'matches', 'zh-TW': '個命中', 'zh-CN': '个命中' },
  'tree.nextHit': { 'en-US': 'Next', 'zh-TW': '下一個', 'zh-CN': '下一个' },

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
  'config.extraMods': { 'en-US': 'Custom modifiers', 'zh-TW': '自訂詞綴', 'zh-CN': '自定义词缀' },
  'config.extraModsHint': {
    'en-US':
      'One modifier per line (PoB text, e.g. "20% increased Fire Damage"); applies globally on blur. Unparsable lines show up in the Build tab unsupported list.',
    'zh-TW': '一行一條詞綴（PoB 文本，如「20% increased Fire Damage」），離開輸入框即全域生效；無法解析的行會出現在構建頁的不支援清單。',
    'zh-CN': '一行一条词缀（PoB 文本，如「20% increased Fire Damage」），离开输入框即全局生效；无法解析的行会出现在构建页的不支持列表。',
  },

  // Notes 页
  'notes.hint': {
    'en-US': 'Free-form notes. Saved locally in your browser; importing a build brings in its <Notes> section.',
    'zh-TW': '自由筆記。本地保存在瀏覽器；匯入 build 會帶入其 <Notes> 內容。',
    'zh-CN': '自由笔记。本地保存在浏览器；导入 build 会带入其 <Notes> 内容。',
  },
  'notes.placeholder2': { 'en-US': 'Write anything about this build…', 'zh-TW': '寫點關於這個 build 的東西…', 'zh-CN': '写点关于这个 build 的东西…' },
  'notes.preview': { 'en-US': 'Colored preview', 'zh-TW': '著色預覽', 'zh-CN': '着色预览' },

  // 分享 code
  'share.title': { 'en-US': 'Share Code', 'zh-TW': '分享 Code', 'zh-CN': '分享 Code' },
  'share.generate': { 'en-US': 'Generate code', 'zh-TW': '生成 Code', 'zh-CN': '生成 Code' },
  'share.hint': {
    'en-US':
      'Exports the current edited build (tree/items/skills/config/notes) as a PoB2-format share code.',
    'zh-TW': '把當前編輯態（樹/裝備/技能/配置/筆記）導出為 PoB2 格式分享 code。',
    'zh-CN': '把当前编辑态（树/装备/技能/配置/笔记）导出为 PoB2 格式分享 code。',
  },

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
  'Flask 1': { 'en-US': 'Flask 1', 'zh-TW': '藥劑 1', 'zh-CN': '药剂 1' },
  'Flask 2': { 'en-US': 'Flask 2', 'zh-TW': '藥劑 2', 'zh-CN': '药剂 2' },
  'Charm 1': { 'en-US': 'Charm 1', 'zh-TW': '護符 1', 'zh-CN': '护符 1' },
  'Charm 2': { 'en-US': 'Charm 2', 'zh-TW': '護符 2', 'zh-CN': '护符 2' },
  'Charm 3': { 'en-US': 'Charm 3', 'zh-TW': '護符 3', 'zh-CN': '护符 3' },
};

/** 装备槽稳定 id → 本地化槽位名。 */
export function slotLabel(lang: Lang, slotId: string): string {
  return SLOT_LABELS[slotId]?.[lang] ?? slotId;
}

/**
 * 附赠技能组来源标注（PoB2 自动生成的组：`source="Item:14:Plague Edge, Akoyan Spear"`
 * 或 `"Tree:11641"`）→ 简短徽标文本；玩家手动组（无 source）返回 null。
 * 同一技能多次出现多半来自这类附赠组——标注让重复行可解释。
 */
export function grantedSourceLabel(
  lang: Lang,
  source: string | null | undefined,
): string | null {
  if (!source) return null;
  const [kind, , detail] = source.split(':');
  if (kind === 'Item') {
    const prefix =
      lang === 'en-US' ? 'from item' : lang === 'zh-TW' ? '裝備附贈' : '装备附赠';
    const name = detail?.split(',')[0]?.trim();
    return name ? `${prefix} · ${name}` : prefix;
  }
  if (kind === 'Tree') {
    return lang === 'en-US' ? 'from tree' : lang === 'zh-TW' ? '天賦附贈' : '天赋附赠';
  }
  return source;
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

/** 侧栏目录之外的展示量补充译名（diff 列表 / Calcs 分节里出现的长尾字段）。 */
const STAT_EXTRA_LABELS: Record<string, { 'zh-TW': string; 'zh-CN': string }> = {
  EffectiveActionRate: { 'zh-TW': '有效攻速/施速', 'zh-CN': '有效攻速/施速' },
  ShockEffect: { 'zh-TW': '感電效果', 'zh-CN': '感电效果' },
  LifeReserved: { 'zh-TW': '已保留生命', 'zh-CN': '已保留生命' },
  LifeUnreserved: { 'zh-TW': '未保留生命', 'zh-CN': '未保留生命' },
  ManaReserved: { 'zh-TW': '已保留魔力', 'zh-CN': '已保留魔力' },
  ManaUnreserved: { 'zh-TW': '未保留魔力', 'zh-CN': '未保留魔力' },
  LifeRegen: { 'zh-TW': '生命再生', 'zh-CN': '生命再生' },
  ManaRegen: { 'zh-TW': '魔力再生', 'zh-CN': '魔力再生' },
  EnergyShieldRegen: { 'zh-TW': '能量護盾再生', 'zh-CN': '能量护盾再生' },
  SpellBlockChance: { 'zh-TW': '法術格擋率', 'zh-CN': '法术格挡率' },
  EsRechargeRate: { 'zh-TW': '護盾充能速率', 'zh-CN': '护盾充能速率' },
  EsRechargeDelay: { 'zh-TW': '護盾充能延遲', 'zh-CN': '护盾充能延迟' },
  EsRechargePerSecond: { 'zh-TW': '每秒護盾充能', 'zh-CN': '每秒护盾充能' },
  AvoidAllDamageFromHits: { 'zh-TW': '迴避所有擊中', 'zh-CN': '回避所有击中' },
  AvoidProjectileDamage: { 'zh-TW': '迴避投射物傷害', 'zh-CN': '回避投射物伤害' },
  AvoidStun: { 'zh-TW': '迴避暈眩', 'zh-CN': '回避晕眩' },
  AvoidIgnite: { 'zh-TW': '迴避點燃', 'zh-CN': '回避点燃' },
  AvoidShock: { 'zh-TW': '迴避感電', 'zh-CN': '回避感电' },
  AvoidChill: { 'zh-TW': '迴避冰緩', 'zh-CN': '回避冰缓' },
  AvoidFreeze: { 'zh-TW': '迴避凍結', 'zh-CN': '回避冻结' },
  AvoidPoison: { 'zh-TW': '迴避中毒', 'zh-CN': '回避中毒' },
  AvoidBleeding: { 'zh-TW': '迴避流血', 'zh-CN': '回避流血' },
  TakenMultiPhysical: { 'zh-TW': '物理承傷乘區', 'zh-CN': '物理承伤乘区' },
  TakenMultiFire: { 'zh-TW': '火焰承傷乘區', 'zh-CN': '火焰承伤乘区' },
  TakenMultiCold: { 'zh-TW': '冰冷承傷乘區', 'zh-CN': '冰冷承伤乘区' },
  TakenMultiLightning: { 'zh-TW': '閃電承傷乘區', 'zh-CN': '闪电承伤乘区' },
  TakenMultiChaos: { 'zh-TW': '混沌承傷乘區', 'zh-CN': '混沌承伤乘区' },
  CritExtraDamageReduction: { 'zh-TW': '暴擊額外傷害減免', 'zh-CN': '暴击额外伤害减免' },
  EnemyCritEffect: { 'zh-TW': '敵人暴擊效果', 'zh-CN': '敌人暴击效果' },
  ChargePowerCurrent: { 'zh-TW': '暴擊球（當前）', 'zh-CN': '暴击球（当前）' },
  ChargePowerMaximum: { 'zh-TW': '暴擊球上限', 'zh-CN': '暴击球上限' },
  ChargeFrenzyCurrent: { 'zh-TW': '狂怒球（當前）', 'zh-CN': '狂怒球（当前）' },
  ChargeFrenzyMaximum: { 'zh-TW': '狂怒球上限', 'zh-CN': '狂怒球上限' },
  ChargeEnduranceCurrent: { 'zh-TW': '堅忍球（當前）', 'zh-CN': '坚忍球（当前）' },
  ChargeEnduranceMaximum: { 'zh-TW': '堅忍球上限', 'zh-CN': '坚忍球上限' },
  LifeLeechRate: { 'zh-TW': '生命偷取速率', 'zh-CN': '生命偷取速率' },
  ManaLeechRate: { 'zh-TW': '魔力偷取速率', 'zh-CN': '魔力偷取速率' },
  EsLeechRate: { 'zh-TW': '護盾偷取速率', 'zh-CN': '护盾偷取速率' },
  LifeRecoupRate: { 'zh-TW': '生命回得速率', 'zh-CN': '生命回得速率' },
  EsRecoupRate: { 'zh-TW': '護盾回得速率', 'zh-CN': '护盾回得速率' },
  ChillEffect: { 'zh-TW': '冰緩效果', 'zh-CN': '冰缓效果' },
  FreezeBuildupPct: { 'zh-TW': '凍結積累 %', 'zh-CN': '冻结积累 %' },
  ElectrocuteBuildupPct: { 'zh-TW': '感電麻痺積累 %', 'zh-CN': '感电麻痹积累 %' },
  BleedStackedDPS: { 'zh-TW': '流血疊層 DPS', 'zh-CN': '流血叠层 DPS' },
  BleedActiveStacks: { 'zh-TW': '流血生效層數', 'zh-CN': '流血生效层数' },
  PoisonStackedDPS: { 'zh-TW': '中毒疊層 DPS', 'zh-CN': '中毒叠层 DPS' },
  PoisonActiveStacks: { 'zh-TW': '中毒生效層數', 'zh-CN': '中毒生效层数' },
  AoeRadius: { 'zh-TW': '範圍半徑', 'zh-CN': '范围半径' },
  AoeAreaMod: { 'zh-TW': '範圍面積乘區', 'zh-CN': '范围面积乘区' },
  ProjectileCount: { 'zh-TW': '投射物數量', 'zh-CN': '投射物数量' },
  Cooldown: { 'zh-TW': '冷卻時間', 'zh-CN': '冷却时间' },
  CooldownStoredUses: { 'zh-TW': '冷卻儲存次數', 'zh-CN': '冷却储存次数' },
  LifeCost: { 'zh-TW': '生命消耗', 'zh-CN': '生命消耗' },
  SpiritReserved: { 'zh-TW': '已保留精魂', 'zh-CN': '已保留精魂' },
  TriggerRateCap: { 'zh-TW': '觸發速率上限', 'zh-CN': '触发速率上限' },
  SkillTriggerRate: { 'zh-TW': '技能觸發速率', 'zh-CN': '技能触发速率' },
  BlockChanceMax: { 'zh-TW': '格擋率上限', 'zh-CN': '格挡率上限' },
  SpellBlockChanceMax: { 'zh-TW': '法術格擋率上限', 'zh-CN': '法术格挡率上限' },
  EffectiveBlockChance: { 'zh-TW': '有效格擋率', 'zh-CN': '有效格挡率' },
  EffectiveSpellBlockChance: { 'zh-TW': '有效法術格擋率', 'zh-CN': '有效法术格挡率' },
  BlockEffect: { 'zh-TW': '格擋效果', 'zh-CN': '格挡效果' },
  DeflectionRating: { 'zh-TW': '偏轉值', 'zh-CN': '偏转值' },
  DeflectChance: { 'zh-TW': '偏轉率', 'zh-CN': '偏转率' },
  EvadeChance: { 'zh-TW': '閃避率', 'zh-CN': '闪避率' },
  MeleeEvadeChance: { 'zh-TW': '近戰閃避率', 'zh-CN': '近战闪避率' },
  ProjectileEvadeChance: { 'zh-TW': '投射物閃避率', 'zh-CN': '投射物闪避率' },
  SpellEvadeChance: { 'zh-TW': '法術閃避率', 'zh-CN': '法术闪避率' },
  SpellProjectileEvadeChance: { 'zh-TW': '法術投射物閃避率', 'zh-CN': '法术投射物闪避率' },
  SelfStunChance: { 'zh-TW': '自身被暈眩機率', 'zh-CN': '自身被晕眩几率' },
  StunDuration: { 'zh-TW': '暈眩持續時間', 'zh-CN': '晕眩持续时间' },
  LifeRecoverable: { 'zh-TW': '可恢復生命', 'zh-CN': '可恢复生命' },
  EnergyShieldRecoveryCap: { 'zh-TW': '護盾恢復上限', 'zh-CN': '护盾恢复上限' },
  NumberOfDamagingHits: { 'zh-TW': '致傷擊中次數', 'zh-CN': '致伤击中次数' },
  NumberOfMitigatedHits: { 'zh-TW': '減免後承受次數', 'zh-CN': '减免后承受次数' },
  TotalEHPLowestMaxHit: { 'zh-TW': '有效生命（最弱抗）', 'zh-CN': '有效生命（最弱抗）' },
  TotalHitAvg: { 'zh-TW': '平均擊中', 'zh-CN': '平均击中' },
  IgniteDPS: { 'zh-TW': '點燃 DPS', 'zh-CN': '点燃 DPS' },
};

/** 聚合属性名（breakdown 键 / 归因字段 / diff 列表）→ 本地化。
 * 回退链：breakdown 名表 → 侧栏展示目录标签 → 补充译名表 → 原 id。 */
export function statNameLabel(lang: Lang, id: string): string {
  const fromMods = MOD_NAME_LABELS[id]?.[lang];
  if (fromMods) return fromMods;
  for (const section of STAT_SECTIONS) {
    const row = section.rows.find((r) => r.id === id);
    if (row) return row.label[lang];
  }
  if (lang !== 'en-US') {
    const extra = STAT_EXTRA_LABELS[id]?.[lang];
    if (extra) return extra;
  }
  return id;
}

const DAMAGE_TYPE_LABELS: Record<string, Entry> = {
  Physical: { 'en-US': 'Physical', 'zh-TW': '物理', 'zh-CN': '物理' },
  Fire: { 'en-US': 'Fire', 'zh-TW': '火焰', 'zh-CN': '火焰' },
  Cold: { 'en-US': 'Cold', 'zh-TW': '冰冷', 'zh-CN': '冰冷' },
  Lightning: { 'en-US': 'Lightning', 'zh-TW': '閃電', 'zh-CN': '闪电' },
  Chaos: { 'en-US': 'Chaos', 'zh-TW': '混沌', 'zh-CN': '混沌' },
};

/** 伤害类型名（契约 `main_skill.hit_damage[].damage_type`）→ 本地化。 */
export function damageTypeLabel(lang: Lang, damageType: string): string {
  return DAMAGE_TYPE_LABELS[damageType]?.[lang] ?? damageType;
}

const STAT_CATEGORY_LABELS: Record<string, Entry> = {
  Offence: { 'en-US': 'Offence', 'zh-TW': '攻擊', 'zh-CN': '攻击' },
  HitDamage: { 'en-US': 'Hit Damage', 'zh-TW': '擊中傷害', 'zh-CN': '击中伤害' },
  DotDamage: { 'en-US': 'Damage over Time', 'zh-TW': '持續傷害', 'zh-CN': '持续伤害' },
  Ailment: { 'en-US': 'Ailments', 'zh-TW': '異常狀態', 'zh-CN': '异常状态' },
  SkillMechanics: { 'en-US': 'Skill Mechanics', 'zh-TW': '技能機制', 'zh-CN': '技能机制' },
  Defence: { 'en-US': 'Defence', 'zh-TW': '防禦', 'zh-CN': '防御' },
  Resistance: { 'en-US': 'Resistances', 'zh-TW': '抗性', 'zh-CN': '抗性' },
  Avoidance: { 'en-US': 'Avoidance', 'zh-TW': '迴避', 'zh-CN': '回避' },
  Mitigation: { 'en-US': 'Mitigation', 'zh-TW': '減傷', 'zh-CN': '减伤' },
  Resource: { 'en-US': 'Resources', 'zh-TW': '資源', 'zh-CN': '资源' },
  Recovery: { 'en-US': 'Recovery', 'zh-TW': '恢復', 'zh-CN': '恢复' },
  Degen: { 'en-US': 'Degeneration', 'zh-TW': '衰減', 'zh-CN': '衰减' },
  Cost: { 'en-US': 'Costs', 'zh-TW': '消耗', 'zh-CN': '消耗' },
  Requirement: { 'en-US': 'Requirements', 'zh-TW': '需求', 'zh-CN': '需求' },
  Minion: { 'en-US': 'Minions', 'zh-TW': '召喚物', 'zh-CN': '召唤物' },
  Utility: { 'en-US': 'Utility', 'zh-TW': '功用', 'zh-CN': '功用' },
  Other: { 'en-US': 'Other Aggregates', 'zh-TW': '其他聚合量', 'zh-CN': '其他聚合量' },
};

/** display_catalog 分类名 → 本地化（Calcs 分节标题）。 */
export function statCategoryLabel(lang: Lang, category: string): string {
  return STAT_CATEGORY_LABELS[category]?.[lang] ?? category;
}
