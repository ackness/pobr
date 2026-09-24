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
  'trade.jewelType': { 'en-US': 'Jewel type', 'zh-TW': '珠寶類型', 'zh-CN': '珠宝类型' },
  'trade.baseJewel': { 'en-US': 'Basic jewels', 'zh-TW': '普通珠寶', 'zh-CN': '普通珠宝' },
  'trade.radiusJewel': { 'en-US': 'Time-Lost jewel search', 'zh-TW': '時迭珠寶搜尋', 'zh-CN': '时迭珠宝搜索' },
  'trade.radiusHint': { 'en-US': 'Scores use the allocated passives around this socket in the active weapon set. New jewels use Small radius; an equipped jewel retains its radius upgrades. Paste a market item to compare the complete jewel.', 'zh-TW': '依此插槽周圍、目前武器組已配置的天賦評分。新珠寶以小範圍估算，已裝備珠寶保留半徑升級。貼上市集物品可比較完整珠寶。', 'zh-CN': '按此插槽周围、当前武器组已分配的天赋评分。新珠宝以小范围估算，已装备珠宝保留半径升级。粘贴市集物品可比较完整珠宝。' },
  'trade.goalHelp': { 'en-US': 'How this goal is evaluated', 'zh-TW': '目標如何評估', 'zh-CN': '目标如何评估' },
  'items.position': { 'en-US': 'Equipment position', 'zh-TW': '裝備位置', 'zh-CN': '装备位置' },
  'skills.editMain': { 'en-US': 'Edit main skill', 'zh-TW': '編輯主技能', 'zh-CN': '编辑主技能' },
  'config.configuredOnly': { 'en-US': 'Configured only', 'zh-TW': '僅顯示已設定', 'zh-CN': '仅显示已配置' },
  'config.defaultsHint': { 'en-US': 'Fixed quest rewards and combat defaults follow PoB2. Choice rewards default to None; imported settings take priority. Check rewards, resources and combat conditions against your actual character.', 'zh-TW': '固定任務獎勵與戰鬥預設依照 PoB2；多選獎勵預設為「無」，匯入設定優先。請依角色實際情況核對獎勵、資源與戰鬥條件。', 'zh-CN': '固定任务奖励与战斗默认值依照 PoB2；多选奖励默认为“无”，导入设置优先。请按角色实际情况核对奖励、资源与战斗条件。' },
  'config.importReview': { 'en-US': 'Build imported. Review quest rewards, enemy tier and combat conditions in Config so the calculations match your character and the fight you want to model.', 'zh-TW': '構築已匯入。建議到「配置」核對任務獎勵、敵人階級與戰鬥狀態，讓數值符合角色實際情況與預期戰鬥。', 'zh-CN': '构筑已导入。建议到“配置”核对任务奖励、敌人档位与战斗状态，让数值符合角色实际情况与预期战斗。' },
  'config.review': { 'en-US': 'Review configuration', 'zh-TW': '檢查配置', 'zh-CN': '检查配置' },
  'config.reviewDismiss': { 'en-US': 'Dismiss reminder', 'zh-TW': '關閉提醒', 'zh-CN': '关闭提醒' },
  'config.showAll': { 'en-US': 'Show all options', 'zh-TW': '顯示全部選項', 'zh-CN': '显示全部选项' },
  'tree.editOptions': { 'en-US': 'Attributes & planning', 'zh-TW': '屬性與規劃', 'zh-CN': '属性与规划' },

  "editor.draft": {"en-US": "Draft", "zh-TW": "草稿", "zh-CN": "草稿"},
  "editor.draftHint": {"en-US": "Draft kept while switching pages in this session. Save to apply it to calculations, sharing and backups; Cancel discards it.", "zh-TW": "切換頁面會保留本次開啟期間的草稿。儲存後才會計入數值、分享與備份；取消會丟棄草稿。", "zh-CN": "切换页面会保留本次打开期间的草稿。保存后才会计入数值、分享与备份；取消会丢弃草稿。"},
  "editor.reviewItems": {"en-US": "Review item drafts", "zh-TW": "查看裝備草稿", "zh-CN": "查看装备草稿"},
  "editor.reviewJewels": {"en-US": "Review jewel drafts", "zh-TW": "查看珠寶草稿", "zh-CN": "查看珠宝草稿"},
  "lib.wrongSlot": {"en-US": "Select a compatible equipment position first", "zh-TW": "請先選擇相容的裝備位置", "zh-CN": "请先选择兼容的装备位置"},
  "build.code": {"en-US": "Build code", "zh-TW": "構築代碼", "zh-CN": "构筑代码"},
  "build.importHint": {"en-US": "Paste a PoB2 code, WeGame share link or .build JSON. Importing replaces the current character, equipment, skills and tree. Download a backup first to keep a copy.", "zh-TW": "貼上 PoB2 代碼、WeGame 分享連結或 .build JSON。匯入會替換目前角色、裝備、技能與天賦；如需保留，請先下載備份。", "zh-CN": "粘贴 PoB2 代码、WeGame 分享链接或 .build JSON。导入会替换当前角色、装备、技能与天赋；如需保留，请先下载备份。"},
  "build.importing": {"en-US": "Importing build…", "zh-TW": "正在匯入構築…", "zh-CN": "正在导入构筑…"},
  "build.confirmClassChange": {"en-US": "Start an empty level 1 build with this class? Current equipment, skills, passives, notes and combat settings will be cleared. Download a backup first if you want to keep them.", "zh-TW": "要以此職業新建 1 級空白構築嗎？目前裝備、技能、天賦、筆記與戰鬥配置將被清除。如需保留，請先下載備份。", "zh-CN": "要以此职业新建 1 级空白构筑吗？当前装备、技能、天赋、笔记与战斗配置将被清除。如需保留，请先下载备份。"},
  "build.itemError": {"en-US": "Item could not be parsed and is excluded from calculations", "zh-TW": "物品解析失敗，未計入數值", "zh-CN": "物品解析失败，未计入数值"},
  "share.generating": {"en-US": "Generating share code…", "zh-TW": "正在生成分享碼…", "zh-CN": "正在生成分享码…"},
  "share.stale": {"en-US": "Your build has changed. Generate a new share code to include your latest edits.", "zh-TW": "構築已修改，請重新生成分享碼以包含最新修改。", "zh-CN": "构筑已修改，请重新生成分享码以包含最新修改。"},
  "common.copyFailed": {"en-US": "Copy failed. Select the text below and copy it manually.", "zh-TW": "複製失敗，請選取下方文字後手動複製。", "zh-CN": "复制失败，请选中下方文本后手动复制。"},
  "common.manualCopy": {"en-US": "Text to copy manually", "zh-TW": "手動複製文字", "zh-CN": "手动复制文本"},
  "common.clearSearch": {"en-US": "Clear search", "zh-TW": "清除搜尋", "zh-CN": "清除搜索"},
  "common.noResults": {"en-US": "No matches. Try a different keyword or clear the search.", "zh-TW": "沒有符合的結果，請更換關鍵字或清除搜尋。", "zh-CN": "没有匹配的结果，请更换关键词或清除搜索。"},
  "common.retry": {"en-US": "Retry", "zh-TW": "重試", "zh-CN": "重试"},
  "config.loading": {"en-US": "Loading combat options…", "zh-TW": "正在載入戰鬥配置…", "zh-CN": "正在加载战斗配置…"},
  "ui.loadingPanel": {"en-US": "Loading panel…", "zh-TW": "正在載入面板…", "zh-CN": "正在加载面板…"},
  "config.loadFailed": {"en-US": "Combat options could not be loaded. Retry to show the settings.", "zh-TW": "戰鬥配置載入失敗，請重試。", "zh-CN": "战斗配置加载失败，请重试。"},
  "config.editHint": {"en-US": "Checkboxes and lists apply immediately. Number and text edits apply when you leave the field or press Enter. Reset restores the imported value, or the default for a new build.", "zh-TW": "勾選與下拉選擇立即生效；數字和文字在離開輸入框或按 Enter 後生效。還原會恢復匯入值，新構築則使用預設值。", "zh-CN": "勾选与下拉选择立即生效；数字和文本在离开输入框或按 Enter 后生效。还原会恢复导入值，新构筑则使用默认值。"},
  "picker.colour": {"en-US": "Gem attribute filter", "zh-TW": "寶石屬性篩選", "zh-CN": "宝石属性筛选"},
  "trade.situational": {"en-US": "Utility and conditional affixes", "zh-TW": "功能與條件詞條", "zh-CN": "功能与条件词条"},
  "trade.situationalHint": {"en-US": "These are not fixed DPS gains. Tick an affix to require it in the market search without assigning an invented weight. For debuffs, review the enemy settings on Config; effect, trigger rate and uptime must match your build.", "zh-TW": "這些不等於固定 DPS 提升。勾選後要求市集商品具備該詞條，不虛設分數。負面狀態需到配置頁核對敵人條件、效果與覆蓋率。", "zh-CN": "这些不等于固定 DPS 提升。勾选后要求市集商品具备该词条，不虚设分数。负面状态需到配置页核对敌人条件、效果与覆盖率。"},
  "trade.mechanic.projectiles": {"en-US": "Expected projectiles per use; not same-target hits", "zh-TW": "每次使用的投射物期望數；不等於同目標命中次數", "zh-CN": "每次使用的投射物期望数；不等于同目标命中次数"},
  "trade.mechanic.area": {"en-US": "Calculated radius change; damage depends on overlap", "zh-TW": "計算半徑變化；傷害取決於重疊", "zh-CN": "计算半径变化；伤害取决于重叠"},
  "trade.mechanic.debuff": {"en-US": "Needs enemy conditions; benefit not quantified", "zh-TW": "需確認敵人條件，收益尚未量化", "zh-CN": "需确认敌人条件，收益尚未量化"},
  "trade.mechanic.buff": {"en-US": "Command trigger and stack uptime are not modeled; search requirement only", "zh-TW": "尚未計算號令觸發與層數覆蓋率；可作搜尋條件", "zh-CN": "尚未计算号令触发与层数覆盖率；可作搜索条件"},

  "weapons.active": {"en-US": "Active weapon set", "zh-TW": "目前武器組", "zh-CN": "当前武器组"},
  "weapons.set": {"en-US": "Set", "zh-TW": "武器組", "zh-CN": "武器组"},
  "weapons.binding": {"en-US": "Skill weapon set", "zh-TW": "技能武器組", "zh-CN": "技能武器组"},
  "weapons.both": {"en-US": "Follow active set", "zh-TW": "跟隨目前武器組", "zh-CN": "跟随当前武器组"},
  "weapons.hint": {"en-US": "Each set has its own main hand, off hand and exclusive passives. The selected skill uses its assigned set for calculations and trade scores.", "zh-TW": "每組分別保存主手、副手及專屬天賦。計算和市集評分會使用主技能綁定的武器組。", "zh-CN": "每组分别保存主手、副手及专属天赋。计算和市集评分会使用主技能绑定的武器组。"},

  "trade.resistanceHint": {"en-US": "Resistance priority reduces the combined fire, cold and lightning gap first; set the target to your build’s resistance cap.", "zh-TW": "抗性優先會先縮小火焰、冰冷與閃電的總缺口；目標可按角色的抗性上限調整。", "zh-CN": "抗性优先会先缩小火焰、冰冷与闪电的总缺口；目标可按角色的抗性上限调整。"},
  "trade.balancedHint": {"en-US": "Balanced score = geometric mean of DPS and EHP. The EHP floor applies to evaluated references; market weights cannot guarantee it for a complete listing. Resistance priority first reduces the total gap to your target, then compares balanced gains.", "zh-TW": "均衡分取 DPS 與 EHP 的幾何平均。生存底線用於參考換裝重算，市集詞條權重無法保證整件商品滿足底線。開啟抗性優先後，先縮小三抗總缺口，再比較均衡提升。", "zh-CN": "均衡分取 DPS 与 EHP 的几何平均。生存底线用于参考换装重算，市集词条权重无法保证整件商品满足底线。开启抗性优先后，先缩小三抗总缺口，再比较均衡提升。"},
  "trade.resistanceGap": {"en-US": "Total resistance gap", "zh-TW": "元素抗性總缺口", "zh-CN": "元素抗性总缺口"},
  "trade.resistanceTarget": {"en-US": "Resistance target", "zh-TW": "抗性目標", "zh-CN": "抗性目标"},
  "trade.resistanceFirst": {"en-US": "Prioritize capped elemental resistances", "zh-TW": "優先補滿元素抗性", "zh-CN": "优先补满元素抗性"},
  "trade.keepEhp": {"en-US": "Keep at least current EHP", "zh-TW": "EHP 不低於目前角色", "zh-CN": "EHP 不低于当前角色"},
  "trade.balanced": {"en-US": "Balanced DPS / EHP", "zh-TW": "均衡 DPS / EHP", "zh-CN": "均衡 DPS / EHP"},
  "trade.referenceItem": {"en-US": "View the reference combination", "zh-TW": "查看參考詞條組合", "zh-CN": "查看参考词条组合"},
  "trade.partialAnalysis": {"en-US": "Some positions could not be analyzed. Select a position to see details; the ranking contains completed results only.", "zh-TW": "部分位置分析失敗，可選取位置查看詳情；排序只包含已完成的結果。", "zh-CN": "部分位置分析失败，可选择位置查看详情；排序只包含已完成的结果。"},
  "trade.noPriority": {"en-US": "No improving reference found among the analyzed positions. Try another goal or inspect individual affix scores.", "zh-TW": "已分析位置暫未找到更好的參考方案。可切換目標或查看各位置的詞條評分。", "zh-CN": "已分析位置暂未找到更好的参考方案。可切换目标或查看各位置的词条评分。"},
  "trade.positions": {"en-US": "positions analyzed", "zh-TW": "個位置已分析", "zh-CN": "个位置已分析"},
  "trade.referencePotential": {"en-US": "Reference gain", "zh-TW": "參考提升", "zh-CN": "参考提升"},
  "trade.nonUnique": {"en-US": "Excludes uniques", "zh-TW": "排除傳奇", "zh-CN": "排除传奇"},
  "trade.includeUnique": {"en-US": "Include unique items", "zh-TW": "包含傳奇物品", "zh-CN": "包含传奇物品"},
  "trade.priorityHint": {"en-US": "Each position replaces one item with a bounded affix-combination reference and recalculates your build. Ranked by the selected goal, with life and EHP changes shown below. This is potential, not a priced listing: budget and level filter the market links. Buying one upgrade changes the order; analyze again afterwards.", "zh-TW": "每個位置以詞條組合參考方案替換一件物品並重算，按目標提升排序，下方同時顯示生命與 EHP 變化。這是提升空間，並非在售商品；預算與等級用於市集篩選。買入一件後請重新分析。", "zh-CN": "每个位置以词条组合参考方案替换一件物品并重算，按目标提升排序，下方同时显示生命与 EHP 变化。这是提升空间，并非在售商品；预算与等级用于市集筛选。买入一件后请重新分析。"},
  "trade.priorityOrder": {"en-US": "Upgrade priority", "zh-TW": "升級優先順序", "zh-CN": "升级优先顺序"},
  "trade.overviewHint": {"en-US": "Analyze equipped positions, allocated jewel sockets and gems for the selected skill together.", "zh-TW": "一次分析已裝備位置、已配置珠寶插槽與目前技能的寶石。", "zh-CN": "一次分析已装备位置、已配置珠宝插槽与当前技能的宝石。"},
  "trade.overviewTitle": {"en-US": "Where should I upgrade first?", "zh-TW": "先升級哪個位置？", "zh-CN": "先升级哪个位置？"},
  "ui.buildTitle": {"en-US": "Your build", "zh-TW": "我的構築", "zh-CN": "我的构筑"},
  "ui.buildHint": {"en-US": "Manage your character, import a build, and save your next upgrade plan.", "zh-TW": "管理角色、匯入配置，記錄下一步的升級計畫。", "zh-CN": "管理角色、导入配置，记录下一步的升级计划。"},
  "ui.itemsHint": {"en-US": "Choose a position to inspect or edit an item. Compare saved items before equipping them.", "zh-TW": "選擇位置查看、編輯裝備，從物品庫比較替換後的變化。", "zh-CN": "选择位置查看、编辑装备，从物品库比较替换后的变化。"},
  "ui.equipped": {"en-US": "Equipped items", "zh-TW": "目前裝備", "zh-CN": "当前装备"},
  "ui.configHint": {"en-US": "Set the encounter and combat conditions used to calculate damage and survivability.", "zh-TW": "設定敵人與戰鬥條件，讓傷害和生存計算符合實際場景。", "zh-CN": "设置敌人与战斗条件，让伤害和生存计算符合实际场景。"},
  "ui.showStats": {"en-US": "Show character stats", "zh-TW": "展開角色數據", "zh-CN": "展开角色数据"},
  "ui.hideStats": {"en-US": "Hide character stats", "zh-TW": "收起角色數據", "zh-CN": "收起角色数据"},

  // Beta 提示
  "beta.notice": {"en-US": "Beta: some effects are not included in calculations. Check unsupported modifiers before comparing upgrades.", "zh-TW": "測試版：部分效果尚未計入數值，比較提升前請核對未支援詞條。", "zh-CN": "测试版：部分效果尚未计入数值，比较提升前请核对未支持词条。"},
  'beta.dismiss': { 'en-US': 'Got it', 'zh-TW': '知道了', 'zh-CN': '知道了' },

  // 页签
  'loadout.switch': { 'en-US': 'Loadout', 'zh-TW': '配置組', 'zh-CN': '配置组' },
  "loadout.new": {"en-US": "Duplicate this stage…", "zh-TW": "複製目前階段…", "zh-CN": "复制当前阶段…"},
  'loadout.rename': { 'en-US': 'Rename…', 'zh-TW': '重新命名…', 'zh-CN': '重命名…' },
  'loadout.remove': { 'en-US': 'Delete this stage', 'zh-TW': '刪除此階段', 'zh-CN': '删除此阶段' },
  'loadout.namePrompt': {
    'en-US': 'Stage name (e.g. "1-30", "mapping"). Tree, gear and skills are grouped by this name.',
    'zh-TW': '階段名稱（如「1-30 級」「mapping」）。天賦、裝備、技能以同名成組。',
    'zh-CN': '阶段名称（如「1-30 级」「mapping」）。天赋、装备、技能以同名成组。',
  },
  'loadout.confirmRemove': {
    'en-US': 'Delete this stage? Its tree, gear and skill set are removed from the build.',
    'zh-TW': '刪除此階段？其天賦、裝備與技能組會一併從 build 移除。',
    'zh-CN': '删除此阶段？其天赋、装备与技能组会一并从 build 移除。',
  },
  "loadout.confirmDiscard": {"en-US": "This reloads the original imported build and discards local edits, including auto-saved changes. Export a backup first if you want to keep them. Continue?", "zh-TW": "此操作會重新載入原始構築，捨棄本地修改（包括自動儲存的修改）。若要保留，請先下載備份。繼續？", "zh-CN": "此操作会重新加载原始构筑，丢弃本地修改（包括自动保存的修改）。如需保留，请先下载备份。继续？"},
  'tab.build': { 'en-US': 'Build', 'zh-TW': '構築', 'zh-CN': '构筑' },
  'tab.tree': { 'en-US': 'Tree', 'zh-TW': '天賦樹', 'zh-CN': '天赋树' },
  'tab.skills': { 'en-US': 'Skills', 'zh-TW': '技能', 'zh-CN': '技能' },
  'tab.items': { 'en-US': 'Items', 'zh-TW': '裝備', 'zh-CN': '装备' },
  'app.source': { 'en-US': 'View source on GitHub (opens in a new tab)', 'zh-TW': '在 GitHub 查看原始碼（另開分頁）', 'zh-CN': '在 GitHub 查看源码（新标签页打开）' },
  'tab.trade': { 'en-US': 'Upgrades', 'zh-TW': '提升', 'zh-CN': '提升' },
  'tab.guidance': { 'en-US': 'Build references', 'zh-TW': '流派參考', 'zh-CN': '流派参考' },
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
  "build.newHint": {"en-US": "Changing class starts an empty level 1 build. Set passives on the Tree tab and add gems on Skills.", "zh-TW": "更換職業會新建 1 級空白構築。可到天賦樹加點、技能頁添加寶石。", "zh-CN": "更换职业会新建 1 级空白构筑。可到天赋树加点、技能页添加宝石。"},
  'build.import': { 'en-US': 'Import Build', 'zh-TW': '匯入配置', 'zh-CN': '导入配置' },
  'build.importPlaceholder': { 'en-US': 'Paste a PoB2 build code, WeGame PoE2 share URL, or .build JSON…', 'zh-TW': '貼上 PoB2 Build Code、WeGame 分享連結或 .build JSON…', 'zh-CN': '粘贴 PoB2 Build Code、WeGame 分享链接或 .build JSON…' },
  "build.importButton": {"en-US": "Import & replace build", "zh-TW": "匯入並替換構築", "zh-CN": "导入并替换构筑"},
  'build.calculating': { 'en-US': 'Calculating…', 'zh-TW': '計算中…', 'zh-CN': '计算中…' },
  'build.imported': { 'en-US': 'Imported: ', 'zh-TW': '已匯入：', 'zh-CN': '已导入：' },
  'build.passives': { 'en-US': 'passives', 'zh-TW': '天賦點', 'zh-CN': '天赋点' },
  'build.itemsCount': { 'en-US': 'items', 'zh-TW': '件裝備', 'zh-CN': '件装备' },
  'build.unsupported': { 'en-US': 'Unsupported modifiers', 'zh-TW': '未支援詞條', 'zh-CN': '未支持词条' },
  'build.unsupportedHint': { 'en-US': 'These effects are not included in the calculation. For example, Guard granted by charms is not yet modeled; keep it in mind when comparing survival upgrades.', 'zh-TW': '以下效果尚未計入數值。例如護符授予的 Guard 尚未建模，比較生存提升時需另外考慮。', 'zh-CN': '以下效果尚未计入数值。例如护符授予的 Guard 尚未建模，比较生存提升时需另外考虑。' },

  // Skills 页
  'skills.title': { 'en-US': 'Socket Groups', 'zh-TW': '技能組', 'zh-CN': '技能组' },
  'skills.addPlaceholder': { 'en-US': 'Search an active gem to add a group…', 'zh-TW': '搜尋主動技能以新建組…', 'zh-CN': '搜索主动技能以新建组…' },
  'skills.addSupport': { 'en-US': 'Add a support gem…', 'zh-TW': '添加輔助寶石…', 'zh-CN': '添加辅助宝石…' },
  'skills.hint': {
    'en-US': 'Use the star to select the main skill; expand a group to edit gems and weapon bindings.',
    'zh-TW': '點擊星標選擇主技能，展開技能組編輯寶石和武器綁定。',
    'zh-CN': '点击星标选择主技能，展开技能组编辑宝石和武器绑定。',
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
  "trade.title": {"en-US": "Plan your next upgrade", "zh-TW": "規劃下一步提升", "zh-CN": "规划下一步提升"},
  "trade.hint": {"en-US": "Compare equipment, support combinations and passive paths under one goal. Check real changes before applying an upgrade.", "zh-TW": "以同一目標比較裝備、輔助組合與天賦路徑，確認實際變化後再套用。", "zh-CN": "以同一目标比较装备、辅助组合与天赋路径，确认实际变化后再应用。"},
  "trade.marketCoverage": { 'en-US': "Listings evaluated / market matches", 'zh-TW': "評估商品 / 市集匹配", 'zh-CN': "评估商品 / 市集匹配" },
  "trade.rejected": { 'en-US': "Unreadable items skipped", 'zh-TW': "略過無法解析商品", 'zh-CN': "跳过无法解析商品" },
  "trade.marketHint": { 'en-US': "Each search recalculates up to 20 live listings; weights shortlist candidates, not their final ranking. Gains use your selected skill and combat settings. Prices and availability can change.", 'zh-TW': "每次搜尋重算最多 20 件在售商品，權重只用於篩選。提升取決於主技能和戰鬥設定；價格與庫存可能變動。", 'zh-CN': "每次搜索重算最多 20 件在售商品，权重只用于筛选。提升取决于主技能和战斗设置；价格与库存可能变动。" },
  "trade.sort": { 'en-US': "Sort", 'zh-TW': "排序", 'zh-CN': "排序" },
  "trade.sortGain": { 'en-US': "Largest improvement", 'zh-TW': "提升最多", 'zh-CN': "提升最多" },
  "trade.sortValue": { 'en-US': "Gain per selected currency", 'zh-TW': "每單位所選通貨提升", 'zh-CN': "每单位所选通货提升" },
  "trade.scanEquipped": { 'en-US': "Search equipped slots", 'zh-TW': "搜尋已裝備位置", 'zh-CN': "搜索已装备位置" },
  "trade.overall": { 'en-US': "Best single purchases across searched slots", 'zh-TW': "已搜尋位置的單件升級排行", 'zh-CN': "已搜索位置的单件升级排行" },
  "trade.gems": { 'en-US': "Skill and support gems", 'zh-TW': "技能與輔助寶石", 'zh-CN': "技能与辅助宝石" },
  "trade.gemHint": {"en-US": "Ordinary supports are skill adjustments. Tradable skill and lineage gems have market links; open Skills to optimize the complete support setup.", "zh-TW": "普通輔助列為技能調整；可交易技能與血脈寶石提供市集連結。前往技能頁尋優完整輔助搭配。", "zh-CN": "普通辅助列为技能调整；可交易技能与血脉宝石提供市集链接。前往技能页寻优完整辅助搭配。"},
  "trade.gemGroup": { 'en-US': "Skill group", 'zh-TW': "技能組", 'zh-CN': "技能组" },
  "trade.gain": { 'en-US': "Goal gain", 'zh-TW': "目標提升", 'zh-CN': "目标提升" },
  "trade.tryOn": { 'en-US': "Try on", 'zh-TW': "試穿", 'zh-CN': "试穿" },
  "trade.itemDetails": { 'en-US': "Item details", 'zh-TW': "物品詳情", 'zh-CN': "物品详情" },
  "trade.requirements": { 'en-US': "Requirements (check attributes before buying)", 'zh-TW': "需求（購買前核對屬性）", 'zh-CN': "需求（购买前核对属性）" },
  "trade.uncertain": { 'en-US': "Calculation has unsupported modifiers; review before buying", 'zh-TW': "計算含未支援詞條，購買前請核對", 'zh-CN': "计算含未支持词条，购买前请核对" },
  'trade.budgetSearch': { 'en-US': 'Anonymous trade rejected weighted search. These are recalculated price candidates near your budget; open the trade site to use full weights while logged in.', 'zh-TW': '未登入查詢無法使用完整權重，本次重算接近預算的價格候選。可登入市集使用完整詞條權重搜尋。', 'zh-CN': '未登录查询无法使用完整权重，本次重算接近预算的价格候选。可登录市集使用完整词条权重搜索。' },
  'trade.realm': { 'en-US': 'Server', 'zh-TW': '伺服器', 'zh-CN': '服务器' },
  'trade.realmIntl': { 'en-US': 'International', 'zh-TW': '國際服', 'zh-CN': '国际服' },
  'trade.realmCn': { 'en-US': 'CN (Tencent)', 'zh-TW': '國服', 'zh-CN': '国服' },
  'trade.league': { 'en-US': 'League', 'zh-TW': '聯盟', 'zh-CN': '赛季/联盟' },
  'trade.leagueSeason': { 'en-US': 'current season', 'zh-TW': '賽季服', 'zh-CN': '赛季服' },
  'trade.leagueCustom': { 'en-US': 'Custom…', 'zh-TW': '自定義…', 'zh-CN': '自定义…' },
  'trade.budget': { 'en-US': 'Max price', 'zh-TW': '預算上限', 'zh-CN': '预算上限' },
  'trade.budgetAny': { 'en-US': 'no limit', 'zh-TW': '不限', 'zh-CN': '不限' },
  'trade.curExaltedEquivalent': { 'en-US': 'Exalted Orb Equivalent', 'zh-TW': '崇高石等價物', 'zh-CN': '崇高石等价物' },
  'trade.curExaltedDivine': { 'en-US': 'Exalted or Divine Orbs', 'zh-TW': '崇高石或神聖石', 'zh-CN': '崇高石或神圣石' },
  'trade.curExalted': { 'en-US': 'Exalted', 'zh-TW': '崇高石', 'zh-CN': '崇高石' },
  'trade.curDivine': { 'en-US': 'Divine', 'zh-TW': '神聖石', 'zh-CN': '神圣石' },
  'trade.curChaos': { 'en-US': 'Chaos', 'zh-TW': '混沌石', 'zh-CN': '混沌石' },
  'trade.curRegal': { 'en-US': 'Regal', 'zh-TW': '富豪石', 'zh-CN': '富豪石' },
  'trade.curAlchemy': { 'en-US': 'Alchemy', 'zh-TW': '點金石', 'zh-CN': '点金石' },
  'trade.curVaal': { 'en-US': 'Vaal', 'zh-TW': '瓦爾寶珠', 'zh-CN': '瓦尔宝珠' },
  'trade.curAnnulment': { 'en-US': 'Annulment', 'zh-TW': '剝離石', 'zh-CN': '剥离石' },
  'trade.curAugmentation': { 'en-US': 'Augmentation', 'zh-TW': '增幅石', 'zh-CN': '增幅石' },
  'trade.curTransmutation': { 'en-US': 'Transmutation', 'zh-TW': '蛻變石', 'zh-CN': '蜕变石' },
  'trade.curMirror': { 'en-US': 'Mirror of Kalandra', 'zh-TW': '卡蘭德的魔鏡', 'zh-CN': '卡兰德的魔镜' },
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
    'en-US': 'No rollable affixes found for this base and item level.',
    'zh-TW': '此基底與物品等級沒有可用詞綴。',
    'zh-CN': '此基底与物品等级没有可用词缀。',
  },
  "trade.noWeights": {"en-US": "No supported affix improves this goal in the current calculation. Try another goal or slot.", "zh-TW": "目前計算中，沒有已支援詞條能改善此目標。可嘗試其他目標或位置。", "zh-CN": "当前计算中，没有已支持词条能改善此目标。可尝试其他目标或位置。"},

  "trade.category": { 'en-US': "Item type", 'zh-TW': "物品類型", 'zh-CN': "物品类型" },
  "trade.base": { 'en-US': "Base", 'zh-TW': "基底", 'zh-CN': "基底" },
  "trade.itemLevel": { 'en-US': "Maximum item level", 'zh-TW': "物品等級上限", 'zh-CN': "物品等级上限" },
  "trade.leagueFallback": { 'en-US': "Live league list unavailable; showing saved defaults. You can enter a league manually.", 'zh-TW': "聯盟列表暫時無法更新，使用備用列表；可手動填寫。", 'zh-CN': "赛季列表暂时无法更新，使用备用列表；可手动填写。" },
  "trade.category.accessory.amulet": { 'en-US': "Amulet", 'zh-TW': "項鍊", 'zh-CN': "项链" },
  "trade.category.accessory.ring": { 'en-US': "Ring", 'zh-TW': "戒指", 'zh-CN': "戒指" },
  "trade.category.accessory.belt": { 'en-US': "Belt", 'zh-TW': "腰帶", 'zh-CN': "腰带" },
  "trade.category.armour.chest": { 'en-US': "Body armour", 'zh-TW': "胸甲", 'zh-CN': "胸甲" },
  "trade.category.armour.helmet": { 'en-US': "Helmet", 'zh-TW': "頭盔", 'zh-CN': "头盔" },
  "trade.category.armour.gloves": { 'en-US': "Gloves", 'zh-TW': "手套", 'zh-CN': "手套" },
  "trade.category.armour.boots": { 'en-US': "Boots", 'zh-TW': "鞋子", 'zh-CN': "鞋子" },
  "trade.category.armour.quiver": { 'en-US': "Quiver", 'zh-TW': "箭袋", 'zh-CN': "箭袋" },
  "trade.category.armour.shield": { 'en-US': "Shield", 'zh-TW': "盾牌", 'zh-CN': "盾牌" },
  "trade.category.armour.focus": { 'en-US': "Focus", 'zh-TW': "法器", 'zh-CN': "法器" },
  "trade.category.armour.buckler": { 'en-US': "Buckler", 'zh-TW': "小盾", 'zh-CN': "小盾" },
  "trade.category.weapon.bow": { 'en-US': "Bow", 'zh-TW': "弓", 'zh-CN': "弓" },
  "trade.category.weapon.crossbow": { 'en-US': "Crossbow", 'zh-TW': "弩", 'zh-CN': "弩" },
  "trade.category.weapon.staff": { 'en-US': "Staff", 'zh-TW': "法杖", 'zh-CN': "法杖" },
  "trade.category.weapon.warstaff": { 'en-US': "Quarterstaff", 'zh-TW': "長杖", 'zh-CN': "长杖" },
  "trade.category.weapon.onemace": { 'en-US': "One hand mace", 'zh-TW': "單手錘", 'zh-CN': "单手锤" },
  "trade.category.weapon.twomace": { 'en-US': "Two hand mace", 'zh-TW': "雙手錘", 'zh-CN': "双手锤" },
  "trade.category.weapon.wand": { 'en-US': "Wand", 'zh-TW': "魔杖", 'zh-CN': "魔杖" },
  "trade.category.weapon.sceptre": { 'en-US': "Sceptre", 'zh-TW': "權杖", 'zh-CN': "权杖" },
  "trade.category.weapon.spear": { 'en-US': "Spear", 'zh-TW': "長矛", 'zh-CN': "长矛" },
  "trade.category.weapon.flail": { 'en-US': "Flail", 'zh-TW': "鏈錘", 'zh-CN': "链锤" },
  "trade.category.weapon.talisman": { 'en-US': "Talisman", 'zh-TW': "魔符", 'zh-CN': "魔符" },

  "trade.jewelSocket": { 'en-US': "Jewel socket", 'zh-TW': "珠寶插槽", 'zh-CN': "珠宝插槽" },
  "trade.jewelHint": { 'en-US': "Allocate a jewel socket on the tree to compare jewel combinations there.", 'zh-TW': "先在天賦樹配置珠寶插槽，即可比較該位置的珠寶組合。", 'zh-CN': "先在天赋树分配珠宝插槽，即可比较该位置的珠宝组合。" },
  "trade.noImprovement": { 'en-US': "No fetched item improves this goal within the search. Try another budget or item type.", 'zh-TW': "本次搜尋的商品未提升此目標，可調整預算或物品類型。", 'zh-CN': "本次搜索的商品未提升此目标，可调整预算或物品类型。" },
  "trade.category.jewel": { 'en-US': "Jewel", 'zh-TW': "珠寶", 'zh-CN': "珠宝" },
  "trade.category.flask.charm": { 'en-US': "Charm", 'zh-TW': "護符", 'zh-CN': "护符" },
  "trade.category.flask.life": { 'en-US': "Life flask", 'zh-TW': "生命藥劑", 'zh-CN': "生命药剂" },
  "trade.category.flask.mana": { 'en-US': "Mana flask", 'zh-TW': "魔力藥劑", 'zh-CN': "魔力药剂" },

  "trade.eyebrow": {"en-US": "UPGRADE PLANNER", "zh-TW": "升級規劃", "zh-CN": "升级规划"},
  "trade.localBadge": {"en-US": "Based on your character", "zh-TW": "根據目前角色", "zh-CN": "根据当前角色"},
  "trade.currency": {"en-US": "Budget currency", "zh-TW": "預算通貨", "zh-CN": "预算通货"},
  "trade.chooseSlot": {"en-US": "Choose a position", "zh-TW": "選擇升級位置", "zh-CN": "选择升级位置"},
  "trade.analyzed": {"en-US": "Analyzed", "zh-TW": "已評分", "zh-CN": "已评分"},
  "trade.gemSubtitle": {"en-US": "Levels, quality and supports", "zh-TW": "等級、品質與輔助", "zh-CN": "等级、品质与辅助"},
  "trade.showEmpty": {"en-US": "Show empty positions", "zh-TW": "顯示空位置", "zh-CN": "显示空位置"},
  "trade.hideEmpty": {"en-US": "Hide empty positions", "zh-TW": "收起空位置", "zh-CN": "收起空位置"},
  "trade.analyzeAll": {"en-US": "Analyze all positions", "zh-TW": "分析所有位置", "zh-CN": "分析所有位置"},
  "trade.allBases": {"en-US": "All bases included", "zh-TW": "不限基底", "zh-CN": "不限基底"},
  "trade.levelLimit": {"en-US": "Required level up to", "zh-TW": "需求等級不超過", "zh-CN": "需求等级不超过"},
  "trade.analyze": {"en-US": "Calculate affix scores", "zh-TW": "計算詞條評分", "zh-CN": "计算词条评分"},
  "trade.recalculate": {"en-US": "Recalculate", "zh-TW": "重新計算", "zh-CN": "重新计算"},
  "trade.analysisSkill": {"en-US": "Skill used for scoring", "zh-TW": "評分使用的技能", "zh-CN": "评分使用的技能"},
  "trade.skillContext": {"en-US": "Uses your current combat settings", "zh-TW": "沿用目前戰鬥設定", "zh-CN": "沿用当前战斗设置"},
  "trade.changeType": {"en-US": "Change the detected item type", "zh-TW": "調整自動識別的物品類型", "zh-CN": "调整自动识别的物品类型"},
  "trade.analyzingSlot": {"en-US": "Scoring", "zh-TW": "正在評分", "zh-CN": "正在评分"},
  "trade.analysisFailed": {"en-US": "Unable to calculate this position.", "zh-TW": "此位置暫時無法完成計算。", "zh-CN": "此位置暂时无法完成计算。"},
  "trade.errorDetails": {"en-US": "Details", "zh-TW": "查看詳情", "zh-CN": "查看详情"},
  "trade.emptyTitle": {"en-US": "See what matters for your next upgrade", "zh-TW": "下一件裝備，該看哪些詞條？", "zh-CN": "下一件装备，该看哪些词条？"},
  "trade.emptyDescription": {"en-US": "We test the affixes this item type can roll against your build. You do not need to choose a base or know which stats to search for.", "zh-TW": "根據你的角色，逐一測試此類物品可生成的詞條。不必選基底，也不必先知道該搜哪些屬性。", "zh-CN": "根据你的角色，逐一测试此类物品可生成的词条。不必选基底，也不必先知道该搜哪些属性。"},
  "trade.stepScore": {"en-US": "Calculate useful affixes", "zh-TW": "計算有用詞條", "zh-CN": "计算有用词条"},
  "trade.stepSearch": {"en-US": "Generate a budgeted search", "zh-TW": "生成預算內搜尋", "zh-CN": "生成预算内搜索"},
  "trade.stepBuy": {"en-US": "Choose on the official market", "zh-TW": "到官方市集挑選", "zh-CN": "到官方市集挑选"},
  "trade.affixHeading": {"en-US": "Affixes worth looking for", "zh-TW": "優先關注這些詞條", "zh-CN": "优先关注这些词条"},
  "trade.affixDescription": {"en-US": "Scores use the reference rolls below. The most valuable affix is rated 100.", "zh-TW": "以下列參考數值評分，最有價值的詞條記為 100 分。", "zh-CN": "以下列参考数值评分，最有价值的词条记为 100 分。"},
  "trade.usefulAffixes": {"en-US": "useful affixes", "zh-TW": "項有用詞條", "zh-CN": "项有用词条"},
  "trade.priority": {"en-US": "Priority", "zh-TW": "推薦分", "zh-CN": "推荐分"},
  "trade.referenceGain": {"en-US": "Estimated contribution", "zh-TW": "貢獻估計", "zh-CN": "贡献估计"},
  "trade.marketWeight": {"en-US": "Trade weight", "zh-TW": "市集權重", "zh-CN": "市集权重"},
  "trade.howItWorks": {"en-US": "How these scores are calculated", "zh-TW": "這些評分如何計算？", "zh-CN": "这些评分如何计算？"},
  "trade.methodDescription": {"en-US": "We keep the rest of your build fixed, test each affix on a reference item and your current item when available, and average its marginal contribution. Trade weights are the contribution per stat point. These are reference estimates, not gains from a complete replacement item; percentages cannot be added together. Budget changes update the search without recalculating weights.", "zh-TW": "保留其餘構築，分別在同類參考物品與目前裝備（如有）上加入詞條重算，取平均邊際貢獻；市集權重是每單位屬性的貢獻。這是詞條參考估計，不是整件換裝的提升，百分比不能直接相加。調整預算只更新搜尋條件，不必重新評分。", "zh-CN": "保留其余构筑，分别在同类参考物品与当前装备（如有）上加入词条重算，取平均边际贡献；市集权重是每单位属性的贡献。这是词条参考估计，不是整件换装的提升，百分比不能直接相加。调整预算只更新搜索条件，不必重新评分。"},
  "trade.currentGoal": {"en-US": "Current baseline", "zh-TW": "目前基準", "zh-CN": "当前基准"},
  "trade.evaluations": {"en-US": "evaluations", "zh-TW": "次重算", "zh-CN": "次重算"},
  "trade.unsupportedHint": {"en-US": "Some mechanics are not supported; scores are estimates", "zh-TW": "部分機制尚未支援，評分僅供參考", "zh-CN": "部分机制尚未支持，评分仅供参考"},
  "trade.nextStep": {"en-US": "NEXT STEP", "zh-TW": "下一步", "zh-CN": "下一步"},
  "trade.searchReady": {"en-US": "Your market search is ready", "zh-TW": "已為你配置好市集搜尋", "zh-CN": "已为你配置好市集搜索"},
  "trade.cnMarketHint": {"en-US": "Opens the CN instant-buy market with your budget, level and affix weights. Sign in there to browse. Click an item's Sum score to sort by weight.", "zh-TW": "已使用國服立即購買模式，帶入預算、等級與詞條權重。到市集登入後挑選；點商品的 Sum 總分可按權重排序。", "zh-CN": "已使用国服立即购买模式，带入预算、等级与词条权重。到市集登录后挑选；点商品的 Sum 总分可按权重排序。"},
  "trade.directMarketHint": {"en-US": "Opens the official market with your budget, level and affix weights. Click an item's Sum score to sort by weight. If few items match, broaden the search.", "zh-TW": "帶入預算、等級與詞條權重。點商品的 Sum 總分可按權重排序；結果太少時可擴大搜尋範圍。", "zh-CN": "带入预算、等级与词条权重。点商品的 Sum 总分可按权重排序；结果太少时可扩大搜索范围。"},
  "trade.broaderSearch": {"en-US": "Broaden search (remove minimum score)", "zh-TW": "擴大搜尋範圍（取消最低分）", "zh-CN": "扩大搜索范围（取消最低分）"},
  "trade.browseMarket": {"en-US": "Browse matching items", "zh-TW": "去市集挑選", "zh-CN": "去市集挑选"},
  "trade.copySearch": {"en-US": "Copy search link", "zh-TW": "複製搜尋連結", "zh-CN": "复制搜索链接"},
  "trade.gemCandidates": {"en-US": "Gem upgrade options", "zh-TW": "寶石升級方案", "zh-CN": "宝石升级方案"},
  "trade.replacesGem": {"en-US": "Replaces", "zh-TW": "替換", "zh-CN": "替换"},
  "trade.addsGem": {"en-US": "Add to an open support socket", "zh-TW": "加入空的輔助插槽", "zh-CN": "加入空的辅助插槽"},
  "trade.noGemUpgrade": {"en-US": "No supported gem replacement improves this goal for the selected skill.", "zh-TW": "目前沒有已支援的寶石替換能改善所選技能的此目標。", "zh-CN": "当前没有已支持的宝石替换能改善所选技能的此目标。"},

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
  "items.apply": {"en-US": "Save & recalculate", "zh-TW": "儲存並重算", "zh-CN": "保存并重算"},
  'items.cancel': { 'en-US': 'Cancel', 'zh-TW': '取消', 'zh-CN': '取消' },
  'items.empty': { 'en-US': '(empty)', 'zh-TW': '（空）', 'zh-CN': '（空）' },
  "items.flasks": {"en-US": "Flasks / Charms", "zh-TW": "藥劑 / 護符", "zh-CN": "药剂 / 护符"},
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
  "calcs.hint": {"en-US": "Expand a stat to inspect its base value, increases and modifier sources. Skill DPS and source contributions are below.", "zh-TW": "展開數值可查看基礎值、加成與詞條來源；下方可查看各技能 DPS 並計算來源貢獻。", "zh-CN": "展开数值可查看基础值、加成与词条来源；下方可查看各技能 DPS 并计算来源贡献。"},
  'calcs.mods': { 'en-US': 'mods', 'zh-TW': '條', 'zh-CN': '条' },
  'calcs.baseTotal': { 'en-US': 'Base total', 'zh-TW': '基礎合計', 'zh-CN': '基础合计' },
  'calcs.incTotal': { 'en-US': 'Increased total', 'zh-TW': '增加合計', 'zh-CN': '增加合计' },
  'calcs.type': { 'en-US': 'Type', 'zh-TW': '類型', 'zh-CN': '类型' },
  'calcs.value': { 'en-US': 'Value', 'zh-TW': '數值', 'zh-CN': '数值' },
  'calcs.modifier': { 'en-US': 'Modifier', 'zh-TW': '詞條', 'zh-CN': '词条' },
  'calcs.source': { 'en-US': 'Source', 'zh-TW': '來源', 'zh-CN': '来源' },
  'calcs.baseDerived': { 'en-US': '(base/derived)', 'zh-TW': '（基底/派生）', 'zh-CN': '（基底/派生）' },
  'calcs.attribution': { 'en-US': 'Source Attribution', 'zh-TW': '來源貢獻歸因', 'zh-CN': '来源贡献归因' },
  'calcs.attributionStale': { 'en-US': 'The build or main skill has changed. Run attribution again to update source contributions.', 'zh-TW': '構築或主技能已變更，請重新計算歸因以更新來源貢獻。', 'zh-CN': '构筑或主技能已变更，请重新计算归因以更新来源贡献。' },
  "calcs.attributionHint": {"en-US": "Recalculate after removing each source to see how much it contributes. This can take a while; run it when you need a detailed comparison.", "zh-TW": "逐一移除裝備、天賦等來源並重算，查看各自的貢獻。計算需要一些時間，可在比較方案時按需執行。", "zh-CN": "逐一移除装备、天赋等来源并重算，查看各自的贡献。计算需要一些时间，可在比较方案时按需执行。"},
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
  'tree.jewelSeedMissing': { 'en-US': 'Known jewel transformations are shown. Seed-dependent passives remain unresolved and are excluded from upgrade recommendations.', 'zh-TW': '已顯示已知的珠寶轉換。依賴種子的天賦尚未解析，提升建議會排除這些效果。', 'zh-CN': '已显示已知的珠宝转换。依赖种子的天赋尚未解析，提升建议会排除这些效果。' },
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
  "lib.selectSlotFirst": {"en-US": "Select an equipment position to compare or equip an item.", "zh-TW": "先選擇裝備位置，即可比較或替換物品。", "zh-CN": "先选择装备位置，即可比较或替换物品。"},
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
  "config.addTitle": {"en-US": "Advanced config inputs", "zh-TW": "進階配置項", "zh-CN": "高级配置项"},
  'config.keyPlaceholder': { 'en-US': 'key (e.g. enemyDistance)', 'zh-TW': '鍵名（如 enemyDistance）', 'zh-CN': '键名（如 enemyDistance）' },
  'config.key': { 'en-US': 'Config key', 'zh-TW': '配置鍵名', 'zh-CN': '配置键名' },
  'config.valueLabel': { 'en-US': 'Config value', 'zh-TW': '配置值', 'zh-CN': '配置值' },
  'config.addButton': { 'en-US': 'Add & recalc', 'zh-TW': '加入並重算', 'zh-CN': '加入并重算' },
  "config.reset": {"en-US": "Restore imported/default value", "zh-TW": "還原匯入值或預設值", "zh-CN": "还原导入值或默认值"},
  'config.search': { 'en-US': 'Search config options…', 'zh-TW': '搜尋配置項…', 'zh-CN': '搜索配置项…' },
  'config.extraMods': { 'en-US': 'Custom modifiers', 'zh-TW': '自訂詞綴', 'zh-CN': '自定义词缀' },
  'config.extraModsHint': {
    'en-US':
      'One modifier per line (PoB text, e.g. "20% increased Fire Damage"); applies globally on blur. Unparsable lines show up in the Build tab unsupported list.',
    'zh-TW': '一行一條詞綴（PoB 文本，如「20% increased Fire Damage」），離開輸入框即全域生效；無法解析的行會出現在構築頁的不支援清單。',
    'zh-CN': '一行一条词缀（PoB 文本，如「20% increased Fire Damage」），离开输入框即全局生效；无法解析的行会出现在构筑页的不支持列表。',
  },

  // Notes 页
  "notes.hint": {"en-US": "Keep build ideas and upgrade plans here. Notes are saved in this browser and included in backups and share codes.", "zh-TW": "記錄構築思路與升級計畫。筆記會儲存在此瀏覽器，並包含在備份和分享碼中。", "zh-CN": "记录构筑思路与升级计划。笔记会保存在此浏览器，并包含在备份和分享码中。"},
  'notes.placeholder2': { 'en-US': 'Write anything about this build…', 'zh-TW': '寫點關於這個 build 的東西…', 'zh-CN': '写点关于这个 build 的东西…' },
  'notes.preview': { 'en-US': 'Colored preview', 'zh-TW': '著色預覽', 'zh-CN': '着色预览' },

  // 分享 code
  "share.title": {"en-US": "Share Code", "zh-TW": "分享碼", "zh-CN": "分享码"},
  "share.generate": {"en-US": "Generate share code", "zh-TW": "生成分享碼", "zh-CN": "生成分享码"},
  "share.hint": {"en-US": "Generate a PoB2 code from your current character, gear, skills, tree and notes. Generate it again after editing.", "zh-TW": "將目前角色、裝備、技能、天賦與筆記生成 PoB2 分享碼。修改構築後需重新生成。", "zh-CN": "将当前角色、装备、技能、天赋与笔记生成 PoB2 分享码。修改构筑后需重新生成。"},

  // 本地存档
  'save.title': { 'en-US': 'Local Save', 'zh-TW': '本地存檔', 'zh-CN': '本地存档' },
  "save.hint": {"en-US": "Build edits are auto-saved in this browser. Download a JSON backup to keep a copy or move devices. Import a backup, .build file or code text file to replace the current build.", "zh-TW": "構築修改會自動儲存在此瀏覽器。下載 JSON 備份可另存或跨裝置使用；匯入備份、.build 或代碼文字檔會替換目前構築。", "zh-CN": "构筑修改会自动保存在此浏览器。下载 JSON 备份可另存或跨设备使用；导入备份、.build 或代码文本文件会替换当前构筑。"},
  "save.export": {"en-US": "Download backup", "zh-TW": "下載備份", "zh-CN": "下载备份"},
  "save.import": {"en-US": "Import from file…", "zh-TW": "從檔案匯入…", "zh-CN": "从文件导入…"},
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
