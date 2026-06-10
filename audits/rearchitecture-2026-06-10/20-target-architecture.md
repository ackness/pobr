# 20 — 最终目标架构：数据-框架彻底分离（裁决版）

> 撰写日期：2026-06-10 · 输入：本目录 10–19 十份领域审计 + `audits/pob2-parity-2026-06-09/FINDINGS.md` + 两份独立重构提案（A：数据化终局派；B：parity 斜率派）
> 定位：本方案是对两份提案的**逐条裁决合并**——采纳 B 的节奏（按 parity 提升斜率排期），采纳 A 的终局（三层数据光谱 + 受限模板 DSL + 求值内核），以 **version-bump-drill** 为分离目标的唯一可执行验收。
> 配套路线图见 [21-roadmap.md](21-roadmap.md)；各领域"数据 vs 逻辑"切分细节见 10–19 各文档的「数据 vs 逻辑切分建议」章节，本文引用其结论且与之保持一致。

---

## 0. 方案一句话

**框架 = 稳定的求值内核（scan 引擎、聚合管线、机制公式、tag/form 求值器、handler 注册表）；数据 = 三层物理目录 `data/<版本>/{base/, overlay/, generated/}` 的全量 JSON；版本更新 = 只跑四步管线（pipeline → adapter → extract-lua → precompile），Rust 零改动编译通过——这个演练（version-bump-drill）本身就是架构的回归测试。**

---

## 1. 架构原则裁决表

每条争议给出裁决与理由，作为后续所有 review 的依据。条目编号在路线图与 PR review checklist 中直接引用（P# = Principle）。

| # | 条目 | 裁决 | 理由 |
|---|------|------|------|
| P1 | 数据分层目录结构 | **采纳 A 的三层物理目录** `data/<版本>/{base/, overlay/, generated/}` + manifest v2 三段 domains；**叠加 B 的 L4 刹车原则**：ModFlag/KeywordFlag 位枚举、SkillType 字符串直传、Display 类型契约留代码，不强行 JSON 化 | A 把分层落成目录与 CI 约束（base 禁手改、overlay 禁手改产物只许工具生成、generated 重生一致校验），比 B 的逻辑分层更可执行；B 的 L4 是必要刹车——位枚举是 PoB 内部语义不是游戏数据（见 15-data-pipeline「四层光谱」结论） |
| P2 | 数据化判据 | **采纳 B 的单一判据**：「随版本的数值/条目变 → JSON；只随机制变 → Rust」；任何争议条目用此判据 + version-bump-drill 反证仲裁 | 比 A 的枚举式论证更可传承；A 的权衡声明也承认"换版本仍要改 Rust 即切分错了" |
| P3 | ModParser 六表（formList 91/nameList 776/flagList 202/preFlagList/tagList 684/specialList 2085） | **终局采纳 A（六表全量入 `overlay/mod_parser_rules.json`，scan 引擎 + 27 种 form 求值分支留 Rust）；节奏采纳 B（推迟到 M6，先做小查表）**。M0–M5 期间只数据化纯查表段（high_precision/flagTypes/suffixTypes/penTypes/local_mods 白名单）并继续在现有 Rust parser 上按 parity 需要补词条 | 硬目标下 nameList/tagList 每版本随新词条增长，留 Rust 必改代码——A 的终局判断正确（对应 10-mod-system Gap1/Gap2）；但该战役无直接 parity 增益且要重写已验证的 parser，在进攻 24%/防御 51% 的当下先打它会拖死项目——B 的排序正确。两案对"scan 引擎/form 求值器是逻辑"的判断一致，无争议 |
| P4 | specialModList 2085 条 | **一致采纳**：纯模板（~90–95%）→ 声明式 JSON（`$1` 占位符 + 受限谓词），真逻辑（DOUBLED/jewelFunc/per-skill）→ handler_id 注册表。叠加 A 的三道护栏：①DSL 扩展需 ≥20 条目受益；②handler 条目数监控（<100，逼近 special 总量 10% 即回看切分）；③未验证条目带 `verified:false` 元数据 | 两案一致；护栏是防"代码复杂度变成数据复杂度"的唯一闸门，写入本文档 §5 与 review checklist。迁移按 B 的"ninja build 命中频率排序、覆盖率指标驱动"分批 |
| P5 | SkillStatMap 954 条 | **一致采纳**：`overlay/skill_stat_map.json` + ~60 行 merge 引擎（`value \| statValue×mult/div+base` + tag 实例化），删 `skill_stat_map.rs` 751 行启发式与 adapter 端 `is_mappable_stat` 白名单（全量 stat 入库）。切换采纳 B 的**双跑对照**：新引擎 vs 旧启发式 diff 报告干净后才删旧码 | 18-skills-gems Gap3 / 15-data-pipeline Gap2 已证它是"数据错放进框架"的最大单点；双跑策略是对 FINDINGS 04-02 教训（理论正确仍可能倒退）的直接应用 |
| P6 | ConfigOptions 542 条 | **一致采纳**：`overlay/config_options.json`（schema 字段 + 声明式 effects[] + imply_conditions ~60 处），~10% 真逻辑标 handler_id；解释器一次性消灭 parse_config 前缀启发式、count 型 condition、customMods、enemyIsBoss、resistancePenalty 接线五个已知缺口 | 两案完全一致；19-config 审计证实 517 个 apply 闭包九成是模板化 NewMod |
| P7 | EvalMod tag/form 求值器 | **一致**：留 Rust 并补齐（tag 5→20 种含 actor/limitActor/PerStat 读 output/globalLimit；form 5→27 种），这是覆盖不足不是分层错误 | 两案一致；语义内核跨版本稳定（对应 10-mod-system Gap3） |
| P8 | 常量/怪物百级表/minions/spectres | **一致**：L1 全量入 base/（game_constants 三段、monster_scaling、minions/spectres、non_damaging_ailments、weapon_types、unarmed、jewel_radii）；删 pobr-data 内嵌 monster.rs(45.6K)/minion.rs(24.5K)/constants.rs(8.1K)/campaign.rs 数值表，calc 全部魔数改读注入常量。执行纪律采纳 B 的**搬迁不变式**：搬迁 commit parity 逐值不变，搬迁与行为改动永远分两个 commit | 两案一致（13-defence/14-triggers/15-pipeline 切分结论一致）；不变式是把 FINDINGS 04-02 教训制度化 |
| P9 | 零 I/O 边界与注入方式 | **采纳 A**：pobr-core 签名改收 `&ParserRules`/`&GameConstants` 等引用参数，由 pobr-gamedata 加载、pobr-build 注入；依赖方向不变、不新增 crate | 唯一与现有 CLAUDE.md 约定（I/O 收口 gamedata、core 零 I/O）兼容的方案，两案实质一致 |
| P10 | stat_id 直通 vs 文本解析 | **一致采纳双通道**：文本通道先达 parity 等价（门禁基准是 PoB2 文本语义），stat_id→Modifier 映射表作第二通道，differential 报告 diff<0.1% 后按域渐进切换 | 这是 PoBR 比 PoB2 更稳的架构增量（PoB2 走文本解析是历史包袱），但不能先于 parity 基准切换 |
| P11 | EHP 口径 | **采纳 B**：parity 口径切 PoB2 `numberOfHitsToDie × 单击伤害`（含规避/格挡/恢复），现有 lowest-max-hit 保留为附加指标 | 13-defence Gap4 证实两者量纲同名语义不同，parity 必须对齐 PoB2 口径；A 未覆盖此项 |
| P12 | 能量元宝石等 PoBR 超前实现 | **采纳 B 的双口径开关**：parity 模式走 PoB2（PoE1 式 CoC）口径，能量模型 feature-gated 进"超越模式"并补游戏实测 fixture | 无 parity 参照的超前代码不得污染门禁；A 未覆盖 |
| P13 | overlay 抽取方式 | **采纳 A**：extract-lua 走 luajit 执行 vendor 代码后序列化（复用 pob2-oracle headless 引导），**不用正则啃 642KB Lua 源码**；CI 定期跑 drift diff；pob2-oracle differential test 是最终裁判 | A 的关键工程洞察；B 只说"结构化 diff 对账"未给抽取实现路径 |
| P14 | display_stats / calc_sections | **折中**：`overlay/calc_sections.json`（展示格↔ModName 分组映射）入 overlay（两案均同意，它随词表演化）；display catalog 保留编译期 Rust + sync-pob-catalog 自动生成/校验（B 的告诫：跨版本相对稳定，留框架可接受），condFunc/warnFunc 用受限谓词、复杂的标 native handler | 取 B 的克制 + A 的谓词设计；避免为低收益域扩 DSL |
| P15 | uniques | **一致**：overlay 双层结构（raw 文本块保 BuildRaw 兼容 + 预解析索引），Tabula 类 hardcode 改 per-unique override 数据；文件头记来源与抽取 commit | 两案一致（16-items 切分结论） |
| P16 | pobr-item 边界 | **一致**：编辑态半边（CustomItem 草稿/variant 门控/applyRange/Craft/rune/catalyst/BuildRaw 序列化）归 pobr-item，只读解析链（item_text/ingest_item）留 pobr-core。验收采纳 B：BuildRaw 往返等价 + golden fixture（编辑态无 parity 可依） | 两案一致，B 补了验收契约 |
| P17 | MH/OH 与 crit 双 pass 对归因的冲击 | **采纳 B 的告诫**：动手前出小型 RFC（pass 为 TraceGraph 子图、combineStat 为合并节点），不在防御阶段顺手改归因结构 | A 完全未触及双 pass×归因的结构冲突，这是 PoBR 核心卖点（source-level 归因）的最大一次模型扩展 |
| P18 | 最终验收 | **采纳 A 的 version-bump-drill 并固化为 `devs/scripts/version-bump-drill.sh`**：给新版本 .dat + vendor Lua，仅跑 pipeline→adapter→extract-lua→precompile 四步，Rust 零改动编译通过且 parity 可运行；演练发现的"必须改代码"项即下一阶段数据化清单 | 这是硬目标唯一的可执行定义，本身就是这套架构的回归测试 |

### 1.1 通用原则（两案共识，直接成文）

- **parity 门禁是经验仲裁者**（FINDINGS 04-02 教训成文）：每阶段合并跑 ninja_parity 18-build 门禁（防御 51%/进攻 24%@5% 为底线不得倒退）+ pob2-oracle 中间值对拍；行为修复必须附 PoB2 一手依据，baseline 更新独立 commit 显式审查。
- **可再生性铁律**：`data/<版本>/` 任何文件禁手改；skill_attack_speed_more 类手补（当前数据中恰 1 条、重跑即丢，见 15-pipeline Gap3）、crit_chance/attack_speed_multiplier 等 3912+3578 个 vendor 抽取值必须迁入 overlay 可重复通道；CI 加"重跑产物 byte-diff 零"门禁。
- **保留并强化 PoBR 增量资产**：mod_db 聚合内核（含已修逐 mod round/子集 flags）、TraceGraph/AttributionReport、build code 编解码不重写；所有新数据表注入路径携带 `SourceId`（GemQuality/BuffDefinition/ConfigOption 各为独立 SourceKind），**数据化反而强化归因粒度**。
- **数据化判据速查**（P2 的操作化）：拿到一段 PoB2 Lua，问"PoE2 出 0.6 时这段会因为*数值/条目*变化而 diff 吗？"——会 → JSON；只会因为*机制*变化而 diff → Rust；既有数值又有分支 → 拆成「数据表 + handler_id」。

### 1.2 数据四层光谱（引用 15-data-pipeline 切分结论，作为全方案的分类法）

| 层 | 定义 | 物理归宿 | 例子 |
|---|------|---------|------|
| L1 纯生成数据 | .dat 可全自动再生 | `base/`（pipeline+adapter 产出，禁手改） | Gems/Skills/Bases/Mod* 词缀池/Misc 常量/Minions/Spectres |
| L2 人工策展数据 | 不在 .dat、PoB2 以手工 Lua 维护、随词表/机制演化 | `overlay/`（extract-lua 产出，禁手改产物、只许工具再生） | SkillStatMap/ConfigOptions/specialModList/Uniques/buff 定义/trigger 配置 |
| L3 生成缓存 | 可由 L1+L2+框架确定性派生 | `generated/`（precompile 产出，CI 校验"重生==已提交"） | parsed_mods.json（ModCache 等价）/special_derived.json |
| L4 框架语义 | PoB 内部语义、跨版本稳定 | Rust 代码 | ModFlag/KeywordFlag 位枚举、SkillType 直传、聚合公式、扣池状态机、Display 类型契约 |

---

## 2. 目标 crate/模块布局（与现状 diff）

**总原则：不新增不删除 crate，依赖方向零变化。** 变化集中在：(1) pobr-data 增 schema、删内嵌数据表；(2) pobr-core 增 `rules/` 解释器层与 calc 机制模块、M6 重写 mod_parser；(3) gamedata 增 ~15 个懒加载域 + overlay merge；(4) 两个 tool 升格、一个新 precompile 工具；(5) 框架内 ~2500 行硬编码数据逐步删除。

### 2.1 crates/pobr-data（零逻辑零 I/O；新增约束："零内嵌大数据表"）

| 项 | 现状 | 目标 |
|---|------|------|
| catalog.rs（22.3K 单文件） | 单文件全部 schema | 拆为 `catalog/` 模块目录：现有内容 → `items.rs`/`skills.rs`/`tree.rs`；**新增** `parser_rules.rs`（FormDef/NameMapDef/FlagPhraseDef/TagPhraseDef/SpecialTemplateDef + 占位符值类型与受限谓词）、`stat_map.rs`（StatMapEntry）、`config_def.rs`（ConfigOptionDef + effects/imply_conditions）、`buffs.rs`（BuffDef）、`constants_def.rs`（GameConstantsDef 三段 + 百级表）、`actors.rs`（MinionDef 挂入 DataManifest/SpectreDef/BossSkillDef）、`triggers.rs`（TriggerConfigDef/MirageConfigDef）、`overlay.rs`（UniqueDef/RuneDef/CatalystDef/ModScalabilityDef/LocalModDef）、`display.rs`（DisplayStatDefinition 扩受限谓词 + CalcSectionDef） |
| monster.rs（45.6K）/minion.rs（24.5K）/constants.rs（8.1K）/campaign.rs | 内嵌 Rust 数值表（违背分离目标的最大单点，见 14-triggers Gap3 / 15-pipeline Gap4） | **删除数值表**：类型定义留下，数据迁 `data/<版本>/`；过渡期降级为 fallback 模块（仅测试与无数据兜底），M0/M5 分批删除 |
| CI 约束 | 无 | 新增 lint：禁止 pobr-data 出现 >N 行字面量数组（"零内嵌大数据表"硬约束） |

### 2.2 crates/pobr-core（纯求值引擎化；签名改收注入的规则/常量引用，保持零 I/O）

| 项 | 现状 | 目标 |
|---|------|------|
| **新增 `rules/` 数据解释器层**（纯函数） | 无 | `config_interpreter.rs`（消费 config_options.json）、`stat_map_engine.rs`（~60 行 merge 公式，替换并删除 skill_stat_map 启发式）、`buff_expander.rs`（doActorMisc 等价，消费 buff_definitions.json）、`special_mod.rs`（special 模板实例化 + **handler_id 注册表**：`fn(num, ctx) -> Vec<Modifier>` 按稳定 id 注册）、`keystone_registry.rs`（MoM/EB/CI/IronReflexes/Unbreakable… 有限稳定分支，开关读 ModDb flag——先接通 perform 写死 false 的 CI） |
| mod_parser.rs（61.4K 单文件，规则硬编码 Rust match） | 与 PoB2 把表硬编码 Lua 同构，只是规模小一个数量级 | **M0–M5 维持现架构**，仅查表段（flagTypes/penTypes/high_precision/local_mods 白名单）改读数据 + 输出补 local_candidate 标注；**M6 重构为模块目录**：`scan.rs`（最早+最长匹配引擎，载入期建 aho-corasick 索引）、`forms.rs`（27 种 form 求值 enum，每分支 ≤20 行）、`template.rs`，引擎签名 `parse_mod(text, &ParserRules)` |
| modifier.rs/config.rs | EvalMod tag 5 种；ModFlags 5 位 | tag 5→20 种（actor/limitActor/PerStat 读 output/globalLimit）；ModFlags 扩到 ~30 位（武器类型位由 weapon_types.json 派生，feature-gated 切换）；GetCondition 补 modDB Flag 回退 |
| mod_db.rs | 缺写侧/查询原语 | 补 ReplaceMod/ConvertMod/ScaleAddMod（取整查 high_precision_mods.json）+ SumPositiveValues/HasMod/cfg.source 来源过滤 |
| `calc/` | offence 先行；无扣池状态机；单手单 pass | 阶段顺序改 PoB2 序（defence 先行 + defence→offence 数据通道）；**新增** `pool_damage.rs`（reducePoolsByDamage 状态机 + 参数化 poolProtected 原语——MoM/Guard/Aegis/Ward bypass/SoulLink 复用同一公式）、`buff_pass.rs`（aura/curse/doActorMisc 编排）、`hand_pass.rs`（MH/OH 双 pass + combineStat，先过归因 RFC，见 P17）、`crit_pass.rs`（暴击/非暴击双聚合）；机制公式全部魔数改读注入 GameConstantsDef |

### 2.3 crates/pobr-gamedata（I/O 唯一收口）

- `loader/` 按域懒加载 ~15 个新表。
- **新增 `overlay.rs`**：base→overlay 确定性 merge（key 级覆盖/数组按 id 合并/冲突报错不静默，merge 规则单测锁定）。
- manifest v2（base/overlay/generated 三段 domains，按域记录 schema 版本）。
- **新增 RuleSet 聚合入口**：一次性产出 `ParserRules`/`GameConstants`/`ConfigCatalog` 供 pobr-build 注入。

### 2.4 crates/pobr-build

- CalcOrchestrator 只做接线不再含数据——quality/support 裁决/mana_multiplier/触发上下文/minion actor/武器组条件注入皆消费数据表。
- XML 导入补 `GemSkillRef.quality`、customMods、enemyIsBoss、count 型 condition、resistancePenalty（接 campaign 既有表）。
- `is_weapon_local_mod` 文本枚举改 local_mods.json 白名单 + 结构化局部结算（16-items 切分结论：PoB2 的局部性判定是"mod name + flag 精确匹配且无 tag"的结构化规则，非文本枚举）。
- **删除 skill_stat_map.rs**（751 行，双跑对照干净后）。

### 2.5 crates/pobr-tree

- 消费 PassiveNodeDef 新字段（is_attribute/options/is_switchable/weapon_set/unlock_constraint）。
- **新增 `node_effect.rs`**：PassiveSkillEffect 缩放管线（HasNoEffect 清空→Effect 缩放→珠宝改写→局部缩放，顺序对照 PoB2 buildModListForNode）——17-tree 审计判定的"该领域 parity 主战场"。
- alloc.rs（分配/寻路 BFS）后置 M7。

### 2.6 crates/pobr-item（占位骨架 → 落地）

编辑态半边：CustomItem 草稿、variant 门控、applyRange（消费 mod_scalability.json）、catalyst/rune 选择、Craft 词缀重建、BuildRaw 序列化。只读解析链（item_text/ingest_item）留 pobr-core（P16）。

### 2.7 tools 三件套 + 新工具

| 工具 | 现状 | 目标 |
|---|------|------|
| tools/pobr-data-adapter | 解析 5 个域的计算核心面；is_mappable_stat 白名单过滤 | 新增 statdesc 渲染器（stat_id+值→英文文本）、minions/spectres、三段常量、GemEffects/SupportGems/GrantedEffectQualityStats 等新表解析；**删 is_mappable_stat 白名单（全量入库）** |
| pipeline/config.json | 缺多张表 | 补下载：GemEffects、GrantedEffectQualityStats、ExcludedActiveSkillTypes、MonsterVarieties、DefaultMonsterStats、GameConstants、StatDescriptions、ShieldTypes、ComponentAttributeRequirements、ClassPassiveSkillOverrides 等 |
| tools/sync-pob-catalog | 属性 catalog 抽取/parity 检查 | **升格为 overlay 抽取/对账工具**：新增 `extract-lua` 子命令（luajit 执行 vendor 后序列化，复用 pob2-oracle headless 引导）；check 扩展为 overlay↔vendor drift diff + handler 覆盖清单（枚举 vendor 闭包条目，未映射 handler_id 即告警）；固化 crit_chance/attack_speed_multiplier 抽取为可重复步骤 |
| tools/pob2-oracle | parity 对拍 | **地位升级**：L2/L3 数据化的 differential test 基座——对同一词条文本对拍 PoB2 parseMod 与 PoBR 输出 |
| **新增 tools/precompile-mods**（或 adapter 子命令，M6） | 无 | 扫全语料词条文本 → `generated/parsed_mods.json` + 解析覆盖率 CI 报表 |

---

## 3. 目标 JSON 表全集

目录形态：`data/<版本>/{base/, overlay/, generated/, i18n/}`，manifest.json v2 三段 domains。schema 全部定义在 `pobr-data::catalog`。

> 状态图例：**已有** = data/4.5.0.3.4/ 现存且字段够用；**需扩展** = 现存但缺字段；**新增** = 当前不存在。

### 3.1 base/（.dat 全自动再生，禁手改）

| 表名 | 状态 | 扩展/内容要点 | 对应 PoB2 数据源 | 消费方 crate |
|------|------|--------------|------------------|--------------|
| base_items.json | 需扩展 | spirit★、armour.block_chance★(ShieldTypes)、socket_limit、req{str,dex,int,level}、quality_cap、movement_penalty、weapon.reload_time_ms/bolt_count★、sub_type、flask{...}/charm{...,buff}、granted_skill、implicit 渲染文本 | Data/Bases/*.lua | pobr-core(item)、pobr-build、pobr-item |
| mods.json | 需扩展 | spawn_weights[{tag,weight}]（列已下载未导出）、group/family、stat_order、affix_kind、rendered_lines[]（经 statdesc，词缀池→Modifier 的钥匙）、trade_hashes | ModItem 系 10 文件 | pobr-core、pobr-item、pobr-trade |
| skill_gems.json | 需扩展 | granted_effect_id★（修复 gem↔effect 无外键）、additional_granted_effect_ids[]、additional_stat_set_ids[]、natural_max_level、tags[]、weapon_requirements[]、gem_family | Data/Gems.lua | pobr-build、pobr-core(skill_source) |
| granted_effects.json | 需扩展 | require_skill_types 改 token 表达式数组（保 AND/OR/NOT）★、exclude/add_skill_types[]★、minion_list[]★、parts[]、weapon_restrictions | Data/Skills/*.lua | pobr-core、pobr-build |
| granted_effect_levels.json | 需扩展 | mana_multiplier★、reservation_multiplier、mana_reservation_percent、spirit_reservation_flat★、stored_uses、level_requirement | 同上 | pobr-core、pobr-build |
| granted_effect_stat_sets.json | 需扩展 | 一 effect 多 set（label/base_flags）★、全量 stat 入库（删 is_mappable 过滤）★、quality_stats[]★、dot 基值族+dotIs* 旗标、radius 三级、dpsMultiplier、castTimeOverridesAttackTime、incremental_effectiveness | 同上 + SkillStatMap 边车 | pobr-core、pobr-build |
| passive_tree.json | 需扩展 | is_attribute★（293 节点，radius 计数正确性前提）、options/is_switchable（78 节点）、unlock_constraint、is_multiple_choice、is_free_allocate、apply_to_armour、weapon_set 归属、classes_start[]、charm_socket | TreeData tree.lua / poe2-skilltree-export | pobr-tree |
| passive_tree_meta.json | 需扩展 | 每职业 start_node_id/integer_id、飞升 internal_id（新 build code 解码必需） | 同上 | pobr-tree、pobr-build |
| stats.json | 已有 | — | — | pobr-core、tools |
| cost_types.json | 已有 | — | Data/Costs.lua | pobr-core |
| manifest.json | 需扩展 | 升 v2：base/overlay/generated 三段 domains，按域记录 schema 版本 | — | pobr-gamedata |
| game_constants.json | **新增** | character/monster/game 三段：ArmourRatio/ResistFloor=-200/EvadeChanceCap=95/DeflectEffect=40/Stun 全套/ES recharge 750/ChillMaxEffect/BaseShockMagnitude/ServerTickRate/AccuracyFalloff/CullingThreshold/leech 上限… | Data/Misc.lua + Modules/Data.lua（源 GameConstants/DefaultMonsterStats .dat） | pobr-core(calc 全域) |
| monster_scaling.json | **新增** | 百级表（accuracy/armour/evasion/life/ailment-threshold/ally 系）+ hiddenDamageFixup 派生输入；**替换 monster.rs** | Data/Misc.lua 怪物表 | pobr-core(setup_env/ehp) |
| minions.json | **新增** | MonsterVarieties 反范式化，对齐现 MinionDef（32 条） | Data/Minions.lua | pobr-core、pobr-build |
| spectres.json | **新增** | 同上（593 条） | Data/Spectres.lua | pobr-core、pobr-build |
| non_damaging_ailments.json | **新增** | chill/shock default/max/precision、buildupTypes、defaultAilmentDamageTypes | Modules/Data.lua:347-351 等 | pobr-core(ailment) |
| stat_descriptions.json | **新增** | statdesc 渲染模板（stat_id+值→文本），M5 链路根 | Data/StatDescriptions/（主文件 3.9MB） | pobr-data-adapter（离线消费为主）、pobr-i18n |
| weapon_types.json | **新增** | 类型→flag/melee/one_hand/range/ModFlag 位派生 | data.weaponTypeInfo（Data/Global.lua 附近） | pobr-core、pobr-build |
| unarmed_data.json | **新增** | per-class 空手 phys/速度/暴击 | data.unarmedWeaponData | pobr-core(offence) |
| gem_quality_stats.json | **新增** | effect_id → [{stat, per_quality_rate}] | GrantedEffectQualityStats.dat / Skills 内 qualityStats | pobr-core、pobr-build |
| jewel_radii.json | **新增** | 按树版本：label/inner/outer 环形档（8 档 inner>0）+ 1.2 距离乘数 | Modules/Data.lua:597-613 + GameConstants | pobr-tree |
| quest_rewards.json | **新增** | 任务奖励/进度反推输入（acts questPoints） | QuestRewards.lua | pobr-build |
| world_areas.json | **新增** | 区域等级数据 | WorldAreas.lua | pobr-build |
| essences.json | **新增** | 精华词缀 | Essence.lua | pobr-item |
| runes.json | **新增** | rune_id → {slot_class → 词条, rank, is_soul_core} | Data/ModRunes.lua（165K） | pobr-item、pobr-core |
| mod_scalability.json | **新增** | 词条模板（#化文本）→ 每数值槽 {is_scalable, format}，applyRange 数据面 | Data/ModScalability.lua（1.3M，源 StatDescriptions） | pobr-item |

### 3.2 overlay/（sync-pob-catalog extract-lua 从 vendor 抽取；产物禁手改、只许工具再生；文件头记来源 commit）

| 表名 | 状态 | 内容要点 | 对应 PoB2 数据源 | 消费方 crate |
|------|------|----------|------------------|--------------|
| skill_stat_map.json | **新增** | 954 条全局 stat_id→[{mod_name,mod_type,flags,kw_flags,tags[],div,mult,base,value?}] + per-statset 覆盖边车；**取代 skill_stat_map.rs** | Data/SkillStatMap.lua（105K）+ ~390 处 per-skill 覆盖 | pobr-core(rules::stat_map_engine) |
| config_options.json | **新增** | 542 条：schema 字段（var/type/label/list/defaultState/section/可见性）+ 声明式 effects[] + imply_conditions ~60 处，~10% 标 handler_id | Modules/ConfigOptions.lua（517 apply 闭包） | pobr-core(rules::config_interpreter)、pobr-build |
| special_mods.json | **新增** | specialModList 2085 条：{pattern, mods[占位符模板], handler_id?, verified}，按 ninja 命中频率分批 | ModParser.lua:2231-6150 + data 驱动派生段 | pobr-core(rules::special_mod) |
| buff_definitions.json | **新增** | Onslaught/Fortify/Adrenaline/UnholyMight/Tailwind…→mods[]+幅度+是否吃 BuffEffectOnSelf | CalcPerform.lua doActorMisc if-chain（L503-765） | pobr-core(rules::buff_expander) |
| base_player_mods.json | **新增** | initEnv ~70 条玩家固有基线 mod | CalcSetup.lua:608-678 | pobr-core(setup_env) |
| character_constants.json | **新增** | 等级/属性派生常量（迁出 character.rs） | data.characterConstants | pobr-core |
| trigger_configs.json | **新增** | 61 项：{trigger_id, source_skill_filter, triggered_skill_filter, chance_stat, use_cast_rate, special_handler_id} | CalcTriggers.lua:882-1418 configTable | pobr-core、pobr-build |
| mirage_configs.json | **新增** | 5 类：{mirage_id, count_stat, less_damage_stat, skill_match} | CalcMirages.lua | pobr-core |
| uniques.json | **新增** | raw 文本块（保 BuildRaw 兼容）+ 预解析索引双层 + per-unique override 特例表（Tabula 类） | Data/Uniques/（~130K，社区手工维护） | pobr-item、pobr-core |
| boss_skills.json | **新增** | boss 技能预设（伤害倍率/穿透/速度/uber 变体） | data.bossSkills | pobr-core(setup_env) |
| enemy_presets.json | **新增** | enemyIsBoss 四档 mod 组 + per-type damage/pen/overwhelm 默认列 | ConfigOptions enemy 段 + ModCache 注入组 | pobr-core、pobr-build |
| high_precision_mods.json | **新增** | MORE 取整与 ScaleAddMod 精度例外 | Data.lua highPrecisionMods/defaultHighPrecision | pobr-core(mod_db) |
| local_mods.json | **新增** | 局部词条 ModName 白名单（结构化局部结算依据） | Item.lua calcLocal 语义归纳 | pobr-core、pobr-build |
| catalysts.json | **新增** | catalyst id/名称/tags 矩阵 | Item.lua:14-29 | pobr-item |
| item_tag_special.json | **新增** | itemTagSpecial/ExclusionPattern（手工数据，标注维护来源） | Data.lua:657+ | pobr-item |
| calc_sections.json | **新增** | 29 个 section 的展示格↔{modName[],modType,cfg} 映射 | Modules/CalcSections.lua（2674 行） | pobr-core(display)、apps |
| skill_overrides.json | **新增** | Export #baseMod/#flags/#set 指令承接通道——skill_attack_speed_more 类手补迁入此处，重跑不丢 | Export 模板 directive | pobr-data-adapter（merge 输入）、pobr-core |
| mod_parser_rules.json | **新增（M6）** | 五段：forms 91 条→27 form_id、name_map 776、flag_phrases 202、pre_flags、tag_phrases 684（special 并入 special_mods） | ModParser.lua 六表 | pobr-core(mod_parser) |
| trade_stat_map.json | **新增（远期）** | trade stat 映射 | QueryMods/TradeSiteStats | pobr-trade |

### 3.3 generated/（确定性缓存；CI 校验"重生 == 已提交"）

| 表名 | 状态 | 内容要点 | 对应 PoB2 | 消费方 crate |
|------|------|----------|-----------|--------------|
| parsed_mods.json | **新增（M6）** | 全语料文本→[Modifier] 预解析缓存 + 解析覆盖率元数据（ModCache 等价且更彻底：热路径零解析） | Data/ModCache.lua（6598 行，运行时回写） | pobr-gamedata→pobr-core |
| special_derived.json | **新增** | per-gem chains/pierce 与 keystone LIST 的数据展开 | ModParser 加载期派生表（skillNameList/per-gem special） | pobr-core |

---

## 4. 数据管线目标形态

```
┌─────────────┐   ┌───────────────────┐   ┌──────────────────────────┐
│ GGG .dat 导出 │──▶│ pipeline/（下载） │──▶│ tools/pobr-data-adapter  │──▶ data/<ver>/base/*.json
└─────────────┘   └───────────────────┘   │  + statdesc 渲染器        │
                                          │  + skill_overrides merge  │
┌─────────────────────┐                   └──────────────────────────┘
│ vendor/PoB2 Lua 源码 │──▶ tools/sync-pob-catalog extract-lua ──▶ data/<ver>/overlay/*.json
└─────────────────────┘    （luajit 执行后序列化，头部记 commit）

data/<ver>/{base/ + overlay/} ──▶ tools/precompile-mods ──▶ data/<ver>/generated/*.json
                                                              （parsed_mods + 覆盖率报表）

data/<ver>/ ──▶ crates/pobr-gamedata（唯一 I/O：按域懒加载 + overlay merge + RuleSet 聚合）
            ──▶ pobr-build 注入 ──▶ pobr-core（纯求值，签名收 &ParserRules/&GameConstants/…）
```

要点：

1. **四步可重复**：`pipeline 下载 → adapter 转换 → extract-lua 抽取 → precompile 预编译`，全部幂等、产物 byte-stable，CI 跑"重跑 byte-diff 零"门禁。version-bump-drill（P18）就是把这四步对新版本输入重放一遍。
2. **merge 语义在 gamedata 一处**：base→overlay 的确定性 merge（key 级覆盖/数组按 id 合并/冲突报错不静默），规则单测锁定。adapter 端的 skill_overrides merge 是构建期等价物，两处规则共享文档。
3. **三道 CI 防线**：①可再生性（重跑 byte-diff 零）；②overlay drift（extract-lua 产物 vs vendor 当前 commit 的 diff 报告）；③generated 一致性（precompile 重生 == 已提交）。
4. **pob2-oracle 为终裁**：所有 L2 数据化（statmap/config/special/parser 六表）以"同输入对拍 PoB2 运行时输出"为正确性标准，不以"源码读得对"为标准。
5. **归因贯穿**：每张新表的注入路径带独立 SourceKind 的 `SourceId`，使"这 0.8% DPS 来自某 config 选项/某宝石品质"在 AttributionReport 中可见——数据化与归因目标互相强化而非冲突。

---

## 5. 受限模板 DSL 边界（写入 review checklist）

special_mods/config effects 的占位符语言**硬边界**：

- 允许：`$1..$n` 数值占位、字面量、`negate/clamp(min,max)/div/mult/base` 五种算子、target(player|enemy|minion)、受限谓词（字段引用 + eq/ne/gt/lt + and/or）。
- 禁止：循环、递归、自由表达式、跨条目引用、字符串拼接求值。
- 扩展闸门：新增任何 DSL 能力需 ≥20 个条目受益，否则该条目走 handler_id。
- 监控：handler 条目数 <100；逼近 special 总量 10% 即判切分失败、回看 P4。
- 元数据：未经 oracle 验证的条目带 `verified:false`，运行时照用但 parity 报告单列。

---

## 6. 权衡总声明（方案立场）

激进数据化的真正收益不是"JSON 比 Rust 好维护"，而是：**版本更新边际成本趋零 + 解析覆盖率可离线审计 + 归因粒度免费增强**。代价是抽取（extract-lua）/merge（overlay）/差分（oracle differential）三套工具链成为新的维护面。仲裁标准始终是 version-bump-drill：换版本仍要改 Rust，即某块数据切分错了——演练脚本本身就是这套架构的回归测试。
