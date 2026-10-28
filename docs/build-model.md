# 构筑模型、导入与运行时数据

[维护者指南](maintainer-guide.md)的补充：数据快照与会话约束、导入后的 `Build` 结构（珠宝、肢体、武器组、技能序号）、存档格式与 JSON 接口、编排层的不变量与缓存，以及 Web 运行时。各导入格式带什么、缺什么见[导入格式](import-formats.md)。

## 数据快照、身份与会话

schema 3 的运行时域读取与历史树枚举受 manifest inventory 约束；未列文件在原生与内存端均视为不存在，patch 和 overlay-common 保留独立层语义。GameData 首次访问验证并缓存不可变快照，替换磁盘快照后必须创建新实例。触发源子计算复用已准备的 Build 视图，不能再次叠加品质；传入核心的 enabled-group 序号必须映射回原始组索引后才能修改选择。

宝石品质自 `4.5.5.2` 起由 adapter 的 `--gem-quality` 独立生成，沿用历史 `overlay/` 路径。
该入口始终严格校验三张官方输入表及版本收据；不受通用 `--strict-columns` 开关控制，
也不从 vendor 或旧产物动态推导范围。来源、兼容边界与升版步骤见[品质生成说明](../pipeline/gem-quality/README.md)。

活动数据版本由 `pobr_gamedata::data_version()` 在运行时解析：`POBR_DATA_VERSION` → `POBR_DATA_ROOT/CURRENT`（默认仓库 `data/`）。两者都缺失时直接报错，代码里不内置任何版本号。测试和 PoB2 对照值一律针对活动版本：数据升级时用 `examples/demo-bd-test/tools/recapture_golden.py` 与 `recapture_panel_golden.py` 重采对照值，parity 掉格代表引擎与新版 PoB2 的差距，修引擎而不是降基线。仓库只保留活动版本加最近两个版本（`mise run data:prune`）；国际服与国服版本号规则不同，“最近”按快照入库顺序判断，不比较版本号。

schema 3 的 `manifest.json` 列出运行时文件及 SHA-256；`BuildData::load` 在可选域降级前校验完整快照。旧 schema 1/2 保留兼容行为。`patch/` 与 `overlay-common/` 是独立覆盖层，不属于快照哈希清单。正式数据和 Web 同步先完成候选生成/校验，再替换目录；失败恢复旧目录，不自动 bless golden。

`CalculationSession` / `Env` 只执行一次；再次 `perform` 返回 `AlreadyPerformed`。构筑计算入口返回 `CompletedCalculationSession`，只允许读取输出、来源和分解，不能再修改输入；手动组装会话使用 `finish(self)` 完成计算。Full DPS 使用消费式 `with_full_dps` 附加汇总，不开放输入修改。重算应创建新会话，避免再次应用资源转换。属性缩放与资源派生共享包含职业基础值及 INC/MORE 的最终属性；天赋关联的装备、品质和诊断查询都使用所选树。

`GameData::identity()` 对快照、共享覆盖层及有效 patch 的文件字节计算稳定摘要；`BuildData::load` 在加载时固定它，克隆共享缓存。原生目录与 WASM 内存数据使用同一算法。CLI 完整构筑、marginal 和 WASM 完整计算响应中的 `provenance` 包含 `engine_version`、`data_version`、`data_sha256`。摘要用于定位输入数据包，JSON 空白变化也会改变摘要，不代表计算语义变化；同版本开发构建还须保留源码 revision，完整复现还需要构筑输入与所选条件。修改磁盘数据后应创建新的 `GameData`，不能复用已加载实例。

## 导入与构筑结构

导入的损耗由 `pobr_build::ImportReport` 承载，与 `Build` 并列返回（`import_build*`）；`Build` 只描述计算输入，不携带诊断。`Build.jewels` 的每颗珠宝带 `JewelSocket`：树上自有插槽、导入时记下的已分配天赋（`JewelSocket::Node`，构筑码 `<Socket nodeId>` 的节点）、被授予的插槽（Sinister / 含插槽的核心；珠宝的 “Allocates <插槽>” 同样打开它，例如 Megalomaniac 的 “Allocates Zarokh's Gift”）或装备插槽，“珠宝插槽天赋效果”类词条只作用于树上的珠宝插槽天赋。节点不是珠宝插槽时，按节点自己的词条：Lich 的 Crystalline Phylactery（“Can Socket a non-Unique Basic Jewel into the Phylactery”、“100% increased Effect of bonuses gained from Socketed Jewel”，[poe2db Lich](https://poe2db.tw/us/Lich)）让其中的珠宝按两倍计，The Adorned（“Jewel Socket Passive Skills containing Corrupted Rare Jewels”）不作用于它（`calc_orchestrator::collect::jewel_socket_increase`）。超越肢体（PoB 的 `Arm 1/2`、`Leg 1/2`，游戏物品类 `IncursionArm` / `IncursionLeg`）是与装备并列的一组栏位 `Build.limbs`（按 `LimbSlot`）：肢体的 stat 都是全局的，词条在装备之后注入并按栏位归因（`item.arm1.implicit`），宝石等级加成和 “Allocates” 等按文本读取的通道也包含它们。`Build.weapon_set` 是 `items` 里那组武器所属的武器组，另一组的武器在 `Build.other_weapons`。导入时取主技能使用的那一组：武器授予的技能要拿着那把武器才能用（构筑码把它再列成普通组时，按“排在组首、等级与授予等级相同、无品质、未腐化”认出），设为只在一组使用的组用那一组，其他主技能取当前物品集的 `useSecondWeaponSet`；树上的武器组专属天赋随之取那一组（`xml_build::calculation_weapon_set`）。主技能用的不是物品集那一组时，两组都能用的组绑定到物品集那一组，即角色平时拿着的武器。WASM 请求带 `weapon_swap` 时取它的 `active`，`alternate_items` 是另一组的武器。绑定到另一组的技能组（`SocketGroup::weapon_set`，另一组武器授予的技能也绑定到那一组，`granted_skills::mark_item_granted_skills`）按另一组的武器计算宝石等级与品质（`skill::resolve::apply_other_weapon_set_gem_bonuses`），其余计算仍按当前组。编排据此设置 `WeaponSet1` / `WeaponSet2` 条件，“使用武器组 1/2 时”的词条（例如 Rigwald's Ferocity 的 `support_rigwald_*_in_weapon_set_*`）只在对应的组生效。

构筑码的 `mainActiveSkill`（从 1 起）是技能组里可选效果的序号：每颗宝石的授予效果按游戏表 `GemEffects.GrantedEffectDisplayOrder`（`GemEffectDef::granted_effect_display_order`）排列，未列出的接在后面，再去掉辅助效果；辅助宝石授予的主动效果仍计入（`skill_env::mods::group_active_effects`）。宝石的 `skillId` 是它的主效果，不一定是序号 1：弩弹宝石的 1 是射击、2 是装填，旗帜宝石的 1 是旗帜、2 是保留效果，幻影宝石先列生成幻影的效果、再列 meta 主效果。导入、计算、宝石目录和导出共用这一顺序，导出保留原序号；没有显示顺序的旧快照按主效果、附加效果的顺序。构筑码里 `<Gem enabled="false">` 的宝石保留在 `Build` 里（`GemSkillRef::enabled`），准备计算视图时才移除（`calc_orchestrator::stage_build_view`）：辅助判定、技能、buff、精魄保留、触发与召唤物都看不到它，序号只数剩下的效果。

PoB item text's `Implicits: N` is a serialized line boundary, including enchant
and rune lines inside that span. Their destination remains `enchant_texts`,
but they consume the count before the following explicit lines are classified.
Otherwise a wrapped Adorned modifier can be split between implicit and explicit
sections and lose its effect even though neither raw line was dropped. The
earlier whole-corpus structural import audit checks retention and identifiers;
it does not prove correct section assignment or downstream modifier application.

## 武器组与精魄保留

精魄保留按当前计算的武器组计：绑定到另一组的技能（包括它的辅助宝石追加的保留量）不保留，游戏 stat `display_skill_reserves_in_all_weapon_sets` 明确要求在所有武器组保留的技能除外，例如 Blink、Convalescence 和 Eternal Rage。Svalinn 授予的 Cast on Block 本身没有基础保留量，Direstrike II 与 Herbalism II 的 40 和 30 精魄只在拿着该盾的武器组计入，再应用保留效率。tuned Titan His Scattering Calamity 的主技能在第一组，其他常驻技能绑定到平时持有的第二组，因此计算第一组时保留为 0；构筑码参考值按另一组保留，与当前钉定 PoB2 不同。这里只改变保留量：绑定到另一组的常驻技能的 buff、光环仍按生效计算，与 PoB2 和构筑码参考值一致；游戏里切换武器组后这类 buff 是否保持，没有找到说明，也没有游戏内实测。

## 存档格式、技能／配置集与队伍

JSON/WASM 握手版本为 16（`pobr_wasm::SCHEMA_VERSION`，与 Web 的 `EXPECTED_SCHEMA_VERSION` 配对）。解码返回 `config_sets` / `skill_sets` 及各自的活动 id；配置和技能集按 XML 稳定 id 选择，不把稀疏 id 当作文档序号。`xml_merge::manage_named_set` 支持新建、复制、重命名、删除，拒绝不存在的 id 和删除最后一组；旧平铺 Config / Skills 管理时包装为 Default，导出平铺 Skills 同样包装。合并只替换活动片段，保留其他组、未知配置标量、原值拼写、扩展属性和子节点；技能／宝石扩展属性只在来源身份可识别时保留，删除重复组造成歧义时不猜归属。`SkillSet.mainSocketGroup` 记忆各集主技能组（从 1 起，0 为未指定），当前值同时写入 `Build.mainSocketGroup`；未记忆的非活动集首次选择第一组，PoB2 再保存可能丢弃扩展记忆。`loadout::derive_loadouts` 按同名或花括号标识同时关联配置与技能集，只有一组的类别豁免匹配。

游戏 Build Planner v1 `.build` 由 `build_api::decode_build_file_json` 识别，旧国服 `items` 与 WeGame 导入仍可用；`build_api::encode_build_file_json({ code, name, all_loadouts, source })` 返回 `files`（文件名与 JSON 内容）和去重的 `warnings`，可按来源 loadout 分别导出，不切换会话。格式使用游戏的天赋／宝石 id 和武器组，装备栏内容只是提示，不生成装备、药剂或珠宝；角色等级默认为 1，宝石等级／品质默认为 1/0，只识别精确的 `Level N, Q% Quality` 等级提示。`planner_source` 保存作者、链接、说明、等级区间和文本，供存档与再导出使用；区间不参与计算。未知 id 会诊断，导出跳过 meta 宝石及无游戏映射的条目，珠宝与肢体无装备栏映射；历史树须先转换。兼容性由格式契约和合成 fixture 验证，尚无游戏内导入验证。

各导入格式（PoB 码与 poe.ninja 等分享链接、WeGame、Planner `.build`、旧国服 `.build`）各带什么、缺什么、导入器怎样归一，见 [导入格式](import-formats.md)。WeGame 分享（`apps/pobr-wasm/src/build_api/wegame.rs`）与 PoB 码在几处写法不同，导入器逐项归一，没有归一的会进 `notes` 的 `[import]` 警告，不静默丢弃。宝石只有中文名，按 (名字, 是否辅助) 查找：同名的物品授予版本（Herald of Ice 与 Coming Calamity 的那个，目录里的 `item_granted_skills` 变体与 `Unique` 前缀效果）在同名有普通宝石时让位，仍有歧义的（各武器的默认攻击等）报 `Unknown or ambiguous gem`。等级和品质可带注释（`20(最高等级)` 是 20 级），按开头的数字读取。珠宝所在的插槽写 `PassiveJewel_<n>` 或天赋树节点 id：`<n>` 是游戏表 `PassiveJewelSlots` 的行号，该行的 `Slot` 指向 `PassiveSkills`，其 `Id` 即树上节点 id；这张表手工整理在 `data/overlay-common/passive_jewel_slots.json`（带导出哈希，已有一个真实分享验证了 4、5、12、17 行），适配器读到该表后应改为生成。主技能、天赋授予的技能和武器组绑定 WeGame 都不提供，见 `[import]` 提示。

队伍保存在 `Build.party`；`party::parse_party` / `party::party_xml` 保留 `<Party>` 的 `ImportedBuffs` / `ExportedBuffs`，导出启用成员的合并标准块，并用可忽略的 `PoBRMember` 保留成员边界与禁用状态。`party::import_member` / WASM `build_api::import_party_member_json(code)` 先清空成员自身的队伍输入，再算生命、能量护盾、魔力、属性和已支持的外放光环／诅咒；能重算的同名效果替换导出值，其余沿用原始块。队友效果走 `calc::buff_pass` 的接收者缩放、召唤物投递、最强副本与施法者各自的诅咒上限，敌人效果走既有敌人通道；来源 id 为 `party.<成员>.<目标>.<来源>`，参与缓存身份与逐行归因。战吼、链接等不能从原生技能重建的效果依赖导出块或手填文本；仅支持简单的 `parent` / `partyMembers` 资源属性缩放。嵌套 LIST、复杂标签及不支持的 `GlobalEffect` 字段保留并诊断，不能剥掉限制后当作无条件词条；成员导出的跳过项有警告，PoB2 再保存可能丢弃成员边界。

## 编排、来源写入与缓存

逐技能总览（`calculate_skill_metrics`）与 Full DPS 对同一个构筑按每个主动效果各算一遍，两次之间只有主技能组与所选效果不同。`calc_orchestrator/selection_memo.rs` 的 `SelectionScope` 让只读角色词条、天赋树与配置、不读技能组和选择的结果在这一轮里只算一次：作用域按构筑与数据的地址命名，别的构筑（克隆、触发源、另一武器组视图）照常自己计算，作用域关闭即失效。进入作用域的一方保证期间只改技能组与主技能组，且不移动该构筑；在作用域内记忆的函数要在注释里列出它读取的字段，debug 构建（即所有测试）会把每次命中与重新计算的结果比对。目前只有 `gem_property_bonuses` 使用它：在 325 个构筑上逐技能总览的响应逐字节不变，平均耗时 85.8 → 74.1 ms。每个效果仍各自写入一遍来源（写入顺序决定 ModDb 的插入顺序，来源标记是持有所有权的字符串，辅助判定等 helper 还会向 `CalculationContext` 记录 `support_limits`），把整段来源写入缓存而不是记忆单个纯函数，要先解决这些副作用的重放与顺序；`group_judgement` 实测只占一轮 11–15%、单次约 2.6 µs 且结果是克隆的 `HashSet`，记忆它收益很小。

编排前置解析由 `calc_orchestrator/prepare.rs` 返回完整的技能、配置和武器结果，不使用默认值占位后分阶段修改。主技能的伤害标志里，攻击、法术、近战来自技能类型（含辅助追加的类型）；命中算不算投射物、是不是范围伤害由所选 stat set 的游戏无值 stat 决定（`base_is_projectile` / `is_area_damage`，`skill_env::stat_set_hit_flags`），没有这类 stat 的旧快照退回技能类型。词条的关键字（“Damage with Warcries” 的 Warcry、“with Fire Skills” 的 Fire、“of Movement Skills”、“Curse Skills”、陷阱与地雷、元素与持续伤害类型）限定它作用于哪些技能：技能按自己的类型带关键字（Warcry、Movement、Aura、AppliesCurse、Physical / Fire / Cold / Lightning / Chaos 等，`skill_env::skill_keyword_flags`），主技能、战吼、光环与 buff、召唤物的技能都按各自的类型取。规则、stat 映射和特殊词条里的关键字名统一由 `KeywordFlags::from_name` 解释，不认识的名字必须报错，不能丢掉：丢掉关键字的词条会作用于所有技能（例如 “Damage with Warcries” 曾加到 Detonate Dead 上）。`CalculationContext` 显式携带 stat-map catalog、观察模式和触发子计算状态；计算热路径不使用线程局部上下文。需要映射诊断时使用 `calculate_with_data_report`，记录由返回的 `CalculationReport.stat_map_records` 独占（含触发子计算），原 `take_stat_map_compare_records` 接口已移除。

`DataCalcCache::new(&BuildData, &DataOrchestratorOptions, capacity)` is an opt-in,
caller-owned bounded output/trigger memo. Its immutable borrows bind data and
options; complete structural Build identity includes config placeholders.
Compare mode and report/session entry points remain uncached. Manual key
projections destructure every field without `..`; keep that safeguard when
adding inputs. The legacy text `CalcCache` defaults to 64 entries and verifies
structural equality for computation; `peek(u64)` remains hash-only. Neither
cache provides dependency-driven stage recomputation or automatically changes
application caching. `BuildData` retains its flat public fields and constructors,
with lookup implementations organized under `build_data/{skills,equipment,passives,rules}.rs`.

来源写入阶段只持有 `calc_orchestrator/sources.rs::SourceWriter`，不暴露 `mod_db()`、聚合查询或 `Deref`。`finish_sources` 消费写入对象后才开放读取：精神保留读取完整来源（含额外词条），召唤物数量在属性准备与条件桥接之后读取。保留线性编排，不为每个注入函数增加状态类型。

角色属性派生和初始资源快照由 `CalculationSession::prepare_player_stats` 负责；build 只补装备事实、计数和来源，再调用 `bridge_player_conditions`。初始快照位于 buff 展开之前，`perform` 在防御资源转换后刷新 Life/Mana 的 stats 与 multipliers；两处共用 core 的资源池计算。保留这两个时点的既有求值顺序，不能把初始快照当成转换后的最终值。

光环由 `calc/buff_pass.rs` 统一处理；旧的 `buff-pass-aura` Cargo feature 和编排层直接注入光环的通道均已删除。运行时 buff 阶段受 `mode_buffs` 控制，curse/debuff 另受 `mode_effective` 控制；不要按历史迁移文档重新引入旧开关。

## Web 运行时与天赋树规划

Web 的 `api/wasmBackend.ts` 默认在专用 module Worker 中运行（`wasmWorker.ts` / `workerBackend.ts` / `rpc.ts`），计算不占用页面线程；Worker 起不来时回退到页面线程，`?engine=main` 或构建时 `VITE_POBR_WORKER=0` 可强制回退。它在浏览器中加载 WASM，调用 `apps/pobr-wasm/src/build_api/`；`web/src/api/types.ts` 镜像 JSON DTO，`apps/pobr-wasm/tests/contract_golden.rs` 约束契约。启动时由 JS fetch 数据，经 `stageDataBytes`（或文本的 `stageDataFile`）暂存；浏览器用原生 SHA-256 逐个核对清单列出的文件（`api/dataIntegrity.ts`，WASM 内软件哈希同样的数据要约 180 ms），通过后调用 `initStagedDataVerified`，它只省去字节比较，schema、清单、路径、摘要格式和文件齐全仍照常校验，数据身份仍由清单摘要得出；无原生摘要（非安全上下文）或旧清单时改用 `initStagedData` 在 WASM 内哈希。随后经此建立内存 `GameData` / `BuildData`，随后计算读取内存数据。Pages Worker 处理受限的 HTTP 适配，计算仍在浏览器 WASM 中执行。原生调用通过 `GameData::new` 读取文件，WASM 使用 `GameData::from_memory`。

Web 的计算、市场目录和天赋树使用初始化时固定的数据版本；重新初始化清除翻译与词缀层级缓存。节点规划逐节点计算，不能按原始词条文本共享包含位置/半径效应的结果。缺少历史树展示元数据时保留导入和计算，禁用不可靠的历史树编辑/规划。存档导入校验嵌套结构，非法存档不覆盖当前内容；启动失败保留原始字节并提供下载恢复。

历史树显式转换由 `pobr_build::tree_convert::preview_conversion` 生成独立 XML；WASM `preview_tree_conversion_json({ code, all_specs })` 返回新分享码与逐 Spec 的保留、删除及共用／两组武器的断路清单。节点须同时匹配编号、种类与身份；两边有稳定 slug 时比较 slug，否则比较去除展示标记的旧名及是否属于升华，数值平衡不作为删除依据。缺少历史目录时报错。当前版本和未选中的 Spec 原文不变；保留存续插槽、武器组专属点、属性选择和不透明 XML，失效插槽的珠宝留在全局物品池。连通性使用当前树及珠宝授权规则；断路节点保留，等待显式修复或退款，修改后的完整分配仍须通过连通性与点数检查（`pobr_tree::allocation::AllocationTree::edit`）。

`node_power_json` 的可选 `nodes` 每次最多 16 个，返回单点试算的 `delta` 与 `distance: 0`，不替调用方寻找合法路线。省略时沿用热力图的邻接搜索，`max_depth` 默认 5、最多 12；整条路线与每点收益由 Web 的完整变体计算，见 [Web 天赋树契约](web-contracts.md#web-passive-tree-editing)。
