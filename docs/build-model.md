# 构筑模型、导入与运行时数据

本文补充[维护者指南](maintainer-guide.md)，说明 `Build` 的输入结构、保存和导出、
计算编排、缓存与浏览器运行时。各来源携带的信息见[导入格式](import-formats.md)，
页面操作见 [Web 契约](web-contracts.md)。
Build 是计算输入，ModDb 是计算词条数据库；stat 是游戏属性标识，stat set 是技能效果的一组数值。
INC 表示相加的 increased／reduced，MORE 表示相乘的 more／less。

## 数据快照、身份与会话

### 活动版本与文件清单

`pobr_gamedata::data_version()` 按以下顺序选择活动版本：先读 `POBR_DATA_VERSION`，
再读 `POBR_DATA_ROOT/CURRENT`；根目录默认是仓库的 `data/`。两者都没有时报错，代码不内置版本号。
仓库保留活动版本及最近入库的两个版本，使用 `mise run data:prune` 清理。
国服与国际服版本号规则不同，所以“最近”按入库顺序，不比较版本字符串。

schema 3 的 `manifest.json` 列出运行时文件和 SHA-256 摘要。
原生文件与 WASM 内存数据都只承认清单中的文件，历史树枚举也受清单约束。
`BuildData::load` 先验证完整快照，再决定可选数据能否降级。旧 schema 1/2 保持兼容行为。
`patch/` 和 `overlay-common/` 是独立覆盖层，不属于快照文件哈希清单。
正式生成和 Web 同步先检查候选目录，再替换；失败恢复旧目录，不自动认可新的 golden 对照值。

`GameData` 首次访问会验证并缓存不可变快照。磁盘数据被替换后必须新建实例。
`GameData::identity()` 对快照、共享覆盖与有效 patch 的文件字节计算稳定摘要；
`BuildData::load` 固定该身份，克隆共享缓存，原生／WASM 使用同一算法。
完整 CLI、marginal 与 WASM 响应的 `provenance` 包含 `engine_version`、`data_version`、`data_sha256`。
摘要定位的是文件字节，JSON 空白变化也会改摘要。复现还需构筑、条件和开发构建的源码 revision。

测试与 PoB2 对照针对活动版本。升版时用
`examples/demo-bd-test/tools/recapture_golden.py`、`recapture_panel_golden.py` 重采对照值。
parity 下降说明需要调查新版差异，不能为通过而降低基线。

宝石品质自 `4.5.5.2` 起由 adapter 的 `--gem-quality` 独立生成，沿用 `overlay/` 路径。
它始终严格校验三张官方输入表和版本记录，不受通用 `--strict-columns` 开关影响，
不从 vendor 或旧产物猜范围。见[品质生成说明](../pipeline/gem-quality/README.md)。

### 一次性计算会话

`CalculationSession`／`Env` 只能执行一次，再次 `perform` 返回 `AlreadyPerformed`。
构筑入口返回只读 `CompletedCalculationSession`，手动组装用 `finish(self)` 完成。
Full DPS 用消费式 `with_full_dps` 附加汇总，不再开放输入修改。
重新计算创建新会话，避免资源转换重复应用。

属性缩放和资源派生共用包含职业基础值及 INC／MORE 的最终属性。
关联天赋的装备、品质和诊断都使用所选树。
触发来源子计算复用已准备的 Build 视图，不能再次加品质。
计算中的启用组序号必须映射回原始组序号，才能改来源选择。

## 导入与构筑结构

`import_build*` 并列返回 `Build` 和 `ImportReport`。
`Build` 只放计算输入，导入损耗留在报告中，不能混入构筑状态。

### 珠宝、肢体和物品原文

`Build.jewels` 的每颗珠宝带 `JewelSocket`，可表示树上插槽、导入记录的节点
（`JewelSocket::Node`，PoB `<Socket nodeId>`）、被授予插槽或装备插槽。
Sinister、带孔核心，以及珠宝的 `Allocates <插槽>` 都可授予孔，
例如 Megalomaniac 的 `Allocates Zarokh's Gift`。
“珠宝插槽天赋效果”只缩放树上真正的珠宝插槽天赋。

非珠宝孔节点按自身规则处理。例如 Lich 的 Crystalline Phylactery 允许插入非暗金基础珠宝，
并使其加成效果提高 100%，所以该珠宝按两倍计算；The Adorned 的
`Jewel Socket Passive Skills containing Corrupted Rare Jewels` 不适用于它。
实现为 `calc_orchestrator::collect::jewel_socket_increase`，原文见 [Lich](https://poe2db.tw/us/Lich)。

超越肢体保存在 `Build.limbs`，按 `LimbSlot` 区分 PoB `Arm 1/2`、`Leg 1/2`，
游戏类别为 `IncursionArm`／`IncursionLeg`。词条全局生效，在装备后注入，按如
`item.arm1.implicit` 的来源归因。宝石等级与 Allocates 等按文本读取的路径也包含肢体。

PoB 的 `Implicits: N` 是序列化行边界，区间内的附魔与符文行也要消耗计数，
虽然最终仍归入 `enchant_texts`。否则换行的 Adorned 词条会被拆进隐式和外延区，
原文没丢却不生效。结构导入审计只证明行与 id 保留，不证明分区或后续效果正确。

### 武器组与主动效果序号

`Build.weapon_set` 标明 `items` 属于哪组，另一对在 `Build.other_weapons`。
PoB 导入按主技能选组：武器授予技能必须使用该武器，显式绑定的组用其绑定，
其余取活动物品集的 `useSecondWeaponSet`（`xml_build::calculation_weapon_set`）。
当武器授予技能又作为普通组出现时，以“组首、等级等于授予等级、零品质、未腐化”等信息识别。
主技能不在物品集活动组时，两组都可用的常驻组绑定到物品集那组，代表平时持有武器。
树上的组专属点随选组变化。

WASM 的 `weapon_swap.active` 明确指定当前组，`alternate_items` 是另一对。
`SocketGroup::weapon_set` 绑定另一组时（含 `granted_skills::mark_item_granted_skills` 标记的物品技能），
宝石等级／品质由 `skill::resolve::apply_other_weapon_set_gem_bonuses` 按另一组武器求得，
其余计算保持当前组上下文。编排设置 `WeaponSet1`／`WeaponSet2` 条件，
Rigwald's Ferocity 的 `support_rigwald_*_in_weapon_set_*` 等只在对应组生效。

`mainActiveSkill` 从 1 起，数技能组里可选择的主动效果，而不是只数主动宝石。
每颗宝石内部先按 `GemEffects.GrantedEffectDisplayOrder`
（`GemEffectDef::granted_effect_display_order`）排列，未列效果接后，再去掉辅助效果。
辅助授予的主动效果也计入，入口为 `skill_env::mods::group_active_effects`。
宝石 `skillId` 是主效果，不保证为序号 1：弩弹先射击再装填，旗帜先旗帜再保留，
幻影先生成效果再 meta 主效果。无显示顺序的旧快照用主效果、附加效果顺序。
导入、计算、目录和导出共用该顺序。

`<Gem enabled="false">` 留在 `Build` 的 `GemSkillRef::enabled`，到
`calc_orchestrator::stage_build_view` 才移除。辅助、技能、buff、精魄、触发和召唤物都看不到它，
主动效果序号只数剩余效果。

## 武器组与精魄保留

精魄按当前计算组保留。绑定另一组的技能及其辅助追加量不保留，
除非游戏 stat `display_skill_reserves_in_all_weapon_sets` 要求所有组保留，如 Blink、Convalescence、Eternal Rage。
Svalinn 的 Cast on Block 自身没有基础保留；Direstrike II 和 Herbalism II 的 40／30 精魄
只在持盾组计入，再应用保留效率。

tuned Titan His Scattering Calamity 的主技能在第一组，其他常驻技能绑定平时持有的第二组，
所以算第一组时保留为 0。来源参考值按另一组保留，与钉定 PoB2 不同。
这里仅改变保留量。另一组常驻技能的 buff／光环仍按生效处理，与 PoB2 和参考值一致。
切武器后这些 buff 在游戏中是否保持，尚无说明或实测，不能据此推断游戏行为。
超出精魄池时的输出规则见[精魄保留](calc-outputs.md#精魄保留)。

## 存档格式、技能／配置集与队伍

### 命名组与 XML 保留

JSON/WASM 握手为 16：`pobr_wasm::SCHEMA_VERSION` 与 Web `EXPECTED_SCHEMA_VERSION` 配对。
解码返回 config_sets／skill_sets 及活动 id，按 XML 稳定 id 选组，不把稀疏 id 当位置。
`xml_merge::manage_named_set` 可新建、复制、重命名、删除，拒绝不存在 id 和删最后一组。
旧平铺 Config／Skills 管理时包装为 Default，平铺 Skills 导出也包装。
合并只替换活动片段，其他组、未知标量、原拼写、扩展属性和子节点保留。
技能／宝石扩展只在来源可识别时保留；删重复组后不能猜归属。

`SkillSet.mainSocketGroup` 记忆各集主组，从 1 起、0 未指定，活动值也写 `Build.mainSocketGroup`。
旧文件未记忆的非活动集首次选第一组，PoB2 再存可能丢掉扩展记忆。
`loadout::derive_loadouts` 按同名或花括号标识关联配置与技能等组，某类别只有一组时豁免匹配。

### Planner 与 WeGame

`build_api::decode_build_file_json` 支持游戏 Planner v1 `.build`、旧国服 items 格式和 WeGame。
`encode_build_file_json({ code, name, all_loadouts, source })` 返回文件名／JSON 内容的 files 与去重 warnings，
可逐来源 loadout 导出，不切会话。
Planner 使用游戏天赋／宝石 id 和武器组。装备只是提示，不生成装备、药剂、珠宝；
角色默认 1 级，宝石默认 1/0，仅精确 `Level N, Q% Quality` 提示可恢复宝石值。
`planner_source` 保存作者、链接、说明、等级区间和文本，区间不参与计算。
未知 id 诊断，导出跳过 meta、无映射项、无装备栏映射的珠宝／肢体，历史树先转换。
已有格式契约和合成测试，尚无游戏内导入验证。

WeGame 在 `apps/pobr-wasm/src/build_api/wegame.rs` 统一平台写法，不能统一的写 `[import]` 笔记。
简中宝石按“名、是否辅助”查找；普通宝石优先同名的 item_granted_skills／Unique 变体，
仍有歧义报 `Unknown or ambiguous gem`。等级／品质读开头数字，如 `20(最高等级)`。
`PassiveJewel_<n>` 指游戏 PassiveJewelSlots 行号，该行 Slot 对应 PassiveSkills.Id。
手工映射在 `data/overlay-common/passive_jewel_slots.json`，带导出哈希；曾用真实分享核对行 4、5、12、17。
adapter 读到原表后应改为生成。WeGame 不提供主技能、授予来源或技能组绑定，必须提示。
详见[格式差异](import-formats.md)和 [WeGame 部署与限制](wegame-import.md)。

### 队伍

`Build.party` 保存成员。`party::parse_party`／`party_xml` 保留 ImportedBuffs／ExportedBuffs，
导出启用成员的合并标准块，用可忽略 PoBRMember 保留成员边界与停用状态。
`party::import_member`／WASM `import_party_member_json(code)` 先清掉成员的队伍输入，
再算资源、属性及已支持的外放光环／诅咒。可重算的同名效果替换导出值，其余沿用原始块。
效果走 `calc::buff_pass` 的接收者缩放、召唤物投递、最强副本和各施法者诅咒上限；
敌人效果用既有敌人路径。来源为 `party.<成员>.<目标>.<来源>`，进入缓存身份与逐行归因。
战吼、链接等仍依赖导出块或手填。只支持简单 parent／partyMembers 资源属性缩放；
嵌套 LIST、复杂标签、未知 GlobalEffect 保留并诊断，不剥限制。PoB2 再存可能丢成员边界。

## 编排、来源写入与缓存

### 计算阶段的约束

`prepare.rs` 一次返回完整的技能、配置和武器结果，不用默认值占位后逐步补。
攻击／法术／近战标志来自含辅助修改的技能类型；投射物／范围命中标志来自所选 stat set 的
`base_is_projectile`／`is_area_damage`，旧快照缺 stat 时退回技能类型。
`skill_env::stat_set_hit_flags`、`skill_keyword_flags` 分别处理这些标志与关键字。
Warcry、Movement、Aura、AppliesCurse、伤害类型等限制各自技能；主技能、buff、战吼、召唤物各用自己的类型。
规则和 stat 映射共用 `KeywordFlags::from_name`，未知名报错；丢关键字会把限定伤害错加给所有技能。

`CalculationContext` 显式携带 stat-map 目录、观察模式和触发子计算状态，热路径不使用线程局部上下文。
映射诊断用 `calculate_with_data_report`，结果由 `CalculationReport.stat_map_records` 独占，包含触发子计算。
旧 `take_stat_map_compare_records` 已移除。

来源阶段只能持有 `sources.rs::SourceWriter`，不开放 mod_db、聚合读取或 Deref。
`finish_sources` 消费写入器后才读；精魄读取含额外词条的完整来源，召唤数量在属性准备与条件桥接后读取。
保持线性流程，不为每个注入函数加状态类型。
`prepare_player_stats` 派生属性和初始资源；build 补装备事实／计数／来源，再 `bridge_player_conditions`。
初始快照在 buff 前，perform 在防御转换后刷新 Life／Mana stats 与 multipliers，两处共用资源池计算。
初始值不能当转换后最终值。

光环统一走 `calc/buff_pass.rs`，由 mode_buffs 控制，curse／debuff 另受 mode_effective 控制。
旧 buff-pass-aura feature 和编排直接注入光环都已删除，不按历史文档恢复。

### 单轮复用与输出缓存

逐技能总览和 Full DPS 对各主动效果独立重算，触发来源也会克隆 Build 重算。
`selection_memo.rs::SelectionScope` 只在这一轮复用不依赖技能选择的纯函数结果，
目前为 gem_property_bonuses、radius_jewel_grant_modifiers、slot_bonus_effect_scales。
作用域按构筑／数据地址区分，关闭即失效；其他装备或武器视图不共用。
`calculate_prepared_with_context`、`attributes_without_skills` 用 enter_unless_named 开作用域；
触发克隆只在计算期间 adopt，进触发缓存前撤销地址。
作用域期间只可改技能选择，不能移动构筑。函数注释列出所读字段，debug 每次命中会重算核对。
每个效果仍写入完整来源：插入顺序、来源字符串所有权、support_limits 等副作用不能省掉。
这不是依赖追踪，也没有跨天赋变体复用触发来源。

此前对 325 个构筑核对过总览、编辑重算和每个 8 种天赋变体，响应逐字节不变。
原生 release 的单变体示例为 Stormweaver 23.4→17.9ms、Blood Mage 10.9→8.8ms、Titan 5.5→4.7ms。
这只是该轮样本。group_judgement 当时占一轮 11–15%、单次约 2.6µs；
触发来源在 Stormweaver Comet 占约 70%，perform 约 15–25%，不能据此声称缓存了完整计算阶段。

`DataCalcCache::new(&BuildData, &DataOrchestratorOptions, capacity)` 是调用方自愿启用、自己持有的有界输出／触发缓存。
不可变借用固定数据与选项，结构身份包含 Build 全部输入及配置 placeholder。
手写缓存键逐字段解构，不能用 `..` 漏新字段。Compare 模式、report/session 入口不缓存。
旧文本 CalcCache 默认 64 项，计算时核对结构相等；peek(u64) 仍只查哈希。
两者都不做按依赖重算，也不会自动改变应用缓存。
BuildData 保留平铺公共字段／构造器，查询分在 build_data/{skills,equipment,passives,rules}.rs。

## Web 运行时与天赋树规划

### 浏览器启动与数据校验

`api/wasmBackend.ts` 默认在专用 module Worker 运行，经 wasmWorker.ts、workerBackend.ts、rpc.ts 调用。
Worker 起不来则回退页面线程，`?engine=main` 或构建时 VITE_POBR_WORKER=0 强制回退。
Web 的 api/types.ts 镜像 JSON 接口，contract_golden.rs 约束其形状。
Pages Worker 只处理受限 HTTP，Rust 计算仍在浏览器。

启动按以下顺序加载：

1. 取 `/data/manifest.json` 的 version、files、digests，再取快照 manifest。
2. `api/dataPack.ts` 最多同时下载 8 个文件，边取边校验 SHA-256。
   `dataIntegrity.ts` 优先用浏览器原生摘要；旧测量中同包在 WASM 软件哈希约需 180ms。
3. 用 stageDataBytes（文本兼容入口 stageDataFile）暂存，随即丢页面字节，只保留 readAgainAfterBoot 指定的文件。
4. 全部通过后用 initStagedDataVerified 建内存 GameData／BuildData。它只跳过字节再比较，
   schema、清单、路径、摘要格式和文件齐全仍检查，身份由摘要确定。
   无原生摘要或旧清单时改用 initStagedData 在 WASM 中哈希。

`init_staged_data*` 用 `GameData::from_memory_releasing`。
BuildData 解析完仅自身读取的 passive_tree、passive_trees/*、skill_gems、granted_effects、
granted_effect_levels、granted_effect_stat_sets 后 release_domain，减少原文与解析结果同时驻留。
键仍保留供身份和枚举；再读已释放文件时 debug panic、release 报错，不能假装缺文件降级。
base_items、mods、stat_descriptions、i18n、overlay 等后续仍读，不能释放。
新增释放项前用 `POBR_DBG_RELEASE_BACKEND=1 cargo test -p pobr-wasm --no-fail-fast`
使宿主测试走相同路径并通过套件。浏览器 WASM 线性内存只增不减，降低峰值有意义。
原生用 GameData::new 读文件，其他内存调用可用 GameData::from_memory。

### 静态缓存与 404

sync-data 为文件生成 `/data/h/<摘要前16位>/<文件>` 副本，副本不列入 files。
`public/_headers` 只对 `/data/h/*` 和带内容哈希的 `/assets/*` 设置
`Cache-Control: public, max-age=31536000, immutable`。
清单与 `/data/<版本>/` 普通路径仍须重新校验。
`_routes.json` 只把 API 发给 Worker，静态头必须放 _headers；该文件按路径匹配，不按查询串。

缺失的长期缓存文件必须返回 404，不能以 200 返回应用外壳再被永久缓存。
Pages 从目录向上寻找最近的 404.html。仓库提供 `public/assets/404.html`，
sync-data 生成 data/h/404.html，分别覆盖这两类路径；平台 404 响应使用 no-store。
**根目录和页面路由目录不能放 404.html**，否则 `/build` 等页面不能回退应用外壳。
规则由 `worker-tests/static-headers.test.mjs` 检查。

计算、市场目录与树使用初始化固定的数据版本；再初始化清翻译和词缀阶级缓存。
非法嵌套存档不覆盖当前内容；启动失败保留原始字节并可下载恢复。
缺历史树显示元数据时保留导入／计算，但禁用不可靠的编辑／规划。

### 历史树转换与热力图

`tree_convert::preview_conversion` 生成独立 XML。
WASM `preview_tree_conversion_json({ code, all_specs })` 返回新码及逐 Spec 的保留、删除、共用／两组断路列表。
节点同时匹配编号、种类、身份：两边有 slug 用 slug，否则比较去标记旧名和升华身份。
词条数值变化不删节点，缺历史目录报错。当前与未选 Spec 原文不变；
存续孔、组专属点、属性、不透明 XML 保留，失效孔珠宝留在物品池。
连通按当前树与珠宝授权判断，断路先保留；明确修复／退款后仍经 AllocationTree::edit 验证连通和预算。

`node_power_json` 显式 nodes 每次最多 16 个，返回单点 delta、distance: 0，不保证合法路线。
省略 nodes 时按邻接搜索，max_depth 默认 5、最多 12。Web 完整变体另算路线／每点收益。
可选 window: {offset, limit} 只算候选段并返回 total；未带 window 时响应保持旧形状，无 total。
候选按节点定义再节点号排序后切片，窗口响应按节点号排序。
Web 每批 64 个，批间让出 Worker，可显示进度／取消；BD 改动中止本轮，已算部分标过期。

只有完整定义和范围珠宝成员关系相同的节点才能共享单点计算。
定义排除 skill、id、坐标、连线，再加入 radius_jewel_membership 的圆内关系；不能只按词条原文共享。
passive_jewel_state 存在 nodes 改写、征服或未解析变换时不共享。
allocation_grants／rings 只改可分配性、不改数值，所以不阻止共享。
node_power_sharing.rs 逐位比较共享 delta 与逐节点请求，并核对通用变体至指标量级 1e-12；
语料抽样为 release ignored 测试。

旧原生 release 样本中，深度 3 约 770–1100 候选、深度 8 约 1900–3100，每次完整重算 4.5–17.8ms。
定义复用后的深度 5 示例为 Stormweaver 35→19s、Blood Mage 10.9→6.2s、Titan 7.8→4.0s，
各带 2–3 颗半径珠宝。这些是特定样本测量，不是所有 BD 的耗时保证。
路线和页面应用限制见[天赋树契约](web-contracts.md#web-passive-tree-editing)。
