# Modifier 系统（解析/存储/聚合）

> 重构审计 · 2026-06-10 · 领域 10
> 范围：PoB2 `ModParser / ModTools / ModStore / ModDB / ModList / ModCache` ↔ pobr `mod_parser.rs / mod_db.rs / modifier.rs / mod_cache.rs`
> 前置：已读 `audits/pob2-parity-2026-06-09/FINDINGS.md`，本文不重复其已修结论（01-01～01-06：flags 子集语义、MatchAll keyword、round(·,2) more 聚合、override 后写覆盖、Multiplier floor、per-slot 防御桶等）。

---

## PoB2 代码结构（结构地图）

**PoB2 Modifier 系统 = 「文本解析（ModParser）→ 创建/工具（ModTools）→ 存储/求值（ModStore→ModDB/ModList）→ 预编译缓存（ModCache）」四层。**

```
词条文本（物品/天赋/宝石）
  └→ parseMod（先查 ModCache 命中表）
       └→ mod 表 {name, type, value, flags, keywordFlags, tags...}
            └→ ModDB:AddList / ScaleAddList
                 └→ calc 期 Sum/More/Flag/Override(cfg)
                      └→ EvalMod 按 cfg + actor 求 tag → 数值
```

### 1. `Modules/ModParser.lua`（7133 行 / 642KB）——词条文本 → mod 列表

| 区段 | 行号 | 内容 |
|---|---|---|
| `formList` | :62-154 | 91 条正则 → **27 种 form**（INC/RED/MORE/LESS/BASE/GAIN/LOSE/GRANTS/REMOVES/CHANCE/PEN/REGENFLAT/REGENPERCENT/DEGEN*/DMG*/TOTALCOST/BASECOST/FLAG/OVERRIDE/DOUBLED 等） |
| `modNameList` | :157-961 | **776** 个英文短语 → 内部 ModName 或名集 |
| `modFlagList` | :964-1171 | **202** 条短语 → ModFlag 位 / tag（如 `with maces` → `bor(ModFlag.Mace, ModFlag.Hit)`） |
| `preFlagList` | :1174-1421 | 行首前缀 → flag / wrapper |
| `modTagList` | :1424-2136 | **684** 条 per-X / 条件短语 → tag 模板 |
| `specialModList` | :2231-6150 | **2085** 个特殊词条 → mod 模板（含 triggerExtraSkill / grantedExtraSkill 等辅助构造器） |
| 数据派生区 | :6151-6361 | **加载期由游戏数据生成**：keystone 名来自 `data.keystones`（:6151-6158，每个产一条 `Keystone LIST` mod）；`skillNameList`/`preSkillNameList` 与 per-gem 特殊模式（chains :6340-6342 / pierce :6349-6350 / additional projectiles :6353-6355）按 `data.gems` 的 tags 自动生成（:6302-6361）；另有 `suffixTypes`(:6166)/`penTypes`(:6215)/`resourceTypes`(:6223)/`regenTypes`(:6264)/`flagTypes`(:6268) 等小查找表 |
| `scan()` | :6362-6385 | 核心匹配引擎：对 pattern 表做**「最早开始 + 最长结束」**匹配并切除命中段 |
| `parseMod(line, order)` | :6389-6755 | 主流程：jewelFuncList → unsupported → specialModList（锚定全行）→ preFlagList → preSkillNameList → formList → modTagList×2 →（按 form 选 modNameList/penTypes/costTypes/flagTypes）→ skillNameList → modFlagList → 按 form 定 value/type/suffix → 合并 flags/keywordFlags/tagList → 生成 modList，最后按 misc 标记包装成 ExtraAuraEffect/ExtraAura/MinionModifier/ExtraSkillMod/EnemyModifier 等 LIST mod |
| cache 闭包 | :7109-7133 | 每行先查缓存，miss 时跑 parseMod 两个 pass（order=1/2 技能名扫描时机不同）；失败行可写 `unsupported.txt` 审计 |

### 2. `Modules/ModTools.lua`（267 行）

`modLib.createMod`（可变参构造 mod）、`compareModParams`（name/type/flags/keywordFlags/tags 全等判定，MergeMod 的依据）、format 族（mod → 稳定字符串，缓存与序列化用）、`parseTags`。

### 3. `Classes/ModStore.lua`（907 行）——存储基类

- `parent` 链 + `actor` 引用 + `multipliers`/`conditions` 表。
- `ScaleAddMod`（:45-81）：按 `highPrecisionMods`/`defaultHighPrecision` 的确定性取整缩放，+level 类 floor。
- `GetCondition`（:268-274）：overrideCond → conditions → parent → `Flag("Condition:var")` 四级回退。
- `GetMultiplier`（:276-278）：Override → multipliers + parent + Sum(BASE)。
- `GetStat`（:280-324）：读 `actor.output`，含特例。
- **`EvalMod`（:325-885）：20 种 tag 的求值器**（分支行号逐一核实）：

| tag 类型 | 行号 | tag 类型 | 行号 |
|---|---|---|---|
| Multiplier | :331 | SocketedIn | :691 |
| MultiplierThreshold | :409 | SkillName | :735 |
| PerStat | :440 | SkillId | :770 |
| PercentStat | :489 | SkillPart | :774 |
| StatThreshold | :539 | SkillType | :795 |
| DistanceRamp | :557 | BaseFlag | :813 |
| Limit | :574 | SlotName | :826 |
| Condition | :576 | ModFlagOr | :847 |
| ActorCondition | :607 | KeywordFlagAnd | :854 |
| ItemCondition | :634 | MonsterTag | :861 |

支持 `tag.actor`/`limitActor` 跨 actor（getActor :37-43）、varList/divVar/limitVar/limitTotal/invert/base 偏置；末尾做 globalLimit 跨 mod 累计限幅。

### 4. `Classes/ModList.lua`（271 行，平铺数组）与 `Classes/ModDB.lua`（357 行，按 name 分桶）

同一套 `SumInternal/MoreInternal/FlagInternal/OverrideInternal/ListInternal/TabulateInternal/HasModInternal`：flags 子集匹配 + MatchKeywordFlags + 可选 source 前缀过滤（`mod.source:match("[^:]+") == source`）+ 带 tag 的 mod 走 `context:EvalMod`，全部递归 parent 链。MoreInternal 逐 modName 桶 round(·,2)，highPrecisionMods 例外走 `floor(·×10^p)/10^p`（ModList.lua:131-144 已核实）。写侧另有 `ReplaceMod/ConvertMod/MergeMod`（ModStore.lua:114-127 + ModDB/ModList 各自 Internal）。

### 5. `Data/ModCache.lua`（6598 行）——**生成物**

`Main.lua:128` 把它灌进 parseMod 的 cache 表（`LoadModule("Data/ModCache", modLib.parseModCache)`），`Main.lua:293-297 SaveModCache` 把新解析结果回写源文件，使已知词条行启动后零解析开销。

---

## pobr 实现现状

对应实现共四个文件 ~2248 行：`crates/pobr-core/src/mod_parser.rs`(1516)、`mod_db.rs`(502)、`modifier.rs`(189)、`mod_cache.rs`(41)。

**聚合内核（mod_db.rs / modifier.rs）质量较高**。经上轮审计（2026-06-09）修复后，以下语义已对齐 PoB2：

- flags 子集语义（modifier.rs:140 `is_subset_of`）、MatchAll keyword（:146 `matches_context`）；
- 逐 modName round(·,2) 的 more 聚合（mod_db.rs:8 `round_more`）、override 后写覆盖；
- Multiplier 的 `floor(base/div + 0.0001)`（modifier.rs:182）、get_multiplier 三段语义、per-slot 防御桶、max_of 曝光语义；
- 并叠加了 PoB2 没有的 traced 查询族（`sum_traced/more_traced/flag_traced/override_traced` + `contributions`）支撑 source-level 归因。

但仍有结构性缺口：highPrecisionMods 例外未实现（round_more 无例外分支）；ModList parent 链只有 sum（mod_db.rs:494-500）；无 ReplaceMod/ConvertMod/MergeMod/ScaleAddMod 写侧原语（add API 仅 add_mod/add_list，mod_db.rs:31-42）；ModTag 仅 5 变体（Condition/Multiplier/DamageType/SkillTypes/SlotName，modifier.rs:37-65）vs PoB2 20 种；无 actor 引用与 globalLimit。

**解析器（mod_parser.rs）是手写的窄子集**。`parse_form`（:629-683）仅 INC/RED/MORE/LESS/BASE 五种 + 附加伤害区间/转换/gain-as-extra/override 等专用函数；`parse_name`（:1168-1516）约 93 个 match 臂 / ~118 个短语（PoB2 为 776）；flag/keyword/scope/per-stat 后缀剥离（:728/:770/:808/:1066/:1140）合计数十条；`parse_keystone_special`（:488-）仅 CI/无蓝等数条。所有表硬编码为 Rust match——能正确覆盖 ninja 常见 build 的高频词条，但 PEN/REGENFLAT/CHANCE/FLAG/GRANTS/DOUBLED 等整类 form 与 specialModList 几乎全缺；未识别词条静默归 `ParseStatus::Unsupported`（runtime 会被收集并经 CLI/WASM 曝露，但无离线全语料覆盖率审计）。

**mod_cache.rs 形同虚设**。实现了 normalize + HashMap 缓存（parse_or_insert），但生产路径（calc_orchestrator.rs、session.rs、passive.rs、item.rs、skill_source.rs）全部直接调 `parse_mod`，缓存只被 tests/mod_parser.rs 引用；也没有 PoB2 `Data/ModCache.lua` 那样的离线预编译产物。注意 pobr-build 另有 whole-build 粒度的 `CalcCache`（calc_cache.rs，按 BuildSnapshot 内容哈希缓存 OutputTable），同输入重算不重复解析；但 build 一旦变化即全量重 ingest / 重解析。

**数据管线侧**。`data/4.5.0.3.4/` 的 mods.json / stats.json 是 GGG 词缀池（stat_id + 掷值区间），与 ModParser 的解析规则表是两回事；解析规则与预解析缓存目前都没有 JSON 化落点，`PassiveNodeDef.stats` 等仍以英文文本行入库、运行时解析。

---

## 缺口清单

| # | 标题 | 严重度 | 类型 | PoB2 证据 | pobr 位置 | 说明 |
|---|---|---|---|---|---|---|
| 1 | ModParser 六张 pattern 表只移植了极小子集，且全部硬编码在 Rust 里 | 🔴 high | design | ModParser.lua:62-154/157-961/964-1171/1174-1421/1424-2136/6362-6385/7109-7133 | mod_parser.rs:629-683/1168-1516/728-806/808-1064 | 27 种 form 仅实现 5 种；name 覆盖率约 1/7；数据写死在框架代码里，违背项目「数据 JSON 化」目标 |
| 2 | specialModList（2085 模板 + data 驱动派生模式）基本缺失 | 🔴 high | missing | ModParser.lua:2231-6150/6151-6158/6302-6361 | mod_parser.rs:488-/171-177 | 关键石/唯一物/触发类词条无解析通道，无 `Keystone LIST` 机制 |
| 3 | EvalMod tag 求值：PoB2 20 种 vs pobr 5 种；缺 actor 引用、PerStat 读 output、globalLimit | 🔴 high | partial | ModStore.lua:325-885/:37-43/:280-324 | modifier.rs:37-65/:173-188、config.rs:116 | 跨 actor、阈值门控、限幅类词条全部丢失或错算 |
| 4 | ModCache 预编译：PoB2 持久化生成物 + 全量预热，pobr 运行时 HashMap 且无生产调用方 | 🟡 medium | missing | ModParser.lua:7109-7133、Main.lua:128/:293-297、Data/ModCache.lua | mod_cache.rs:1-41、calc_cache.rs | build 变更后全量重解析；缺离线预解析产物与覆盖率审计 |
| 5 | ModFlags 位宽 5 vs PoB2 ~30：武器类型 flag 降级为全局 Condition/派生名，无 per-hand 语义 | 🟡 medium | partial | Data/Global.lua:222-259、ModParser.lua:964-1171 | pobr-data modifier.rs:34-42、mod_parser.rs:1017-1029/:1206、calc_orchestrator.rs:946-967 | 双持异种武器词条无法限定到对应手；Hit/Dot/Cast/Thorns/Ailment 维 flag 缺失 |
| 6 | 写侧原语缺失：ReplaceMod/ConvertMod/MergeMod/ScaleAddMod 精度规则 | 🟡 medium | missing | ModStore.lua:45-81/:114-127、ModList.lua:131-144、Data.lua:413-415+ | mod_db.rs:31-42、skill_source.rs | 宝石缩放无统一取整规则（±0.01~1 漂移）；mod 转换机制无落点 |
| 7 | GetCondition 的 modDB Flag 回退缺失：`Condition:X` FLAG mod 不能自动驱动条件 | 🟡 medium | partial | ModStore.lua:268-274/:25-28、ModParser.lua:6268-6293 | config.rs:100-116、modifier.rs:150-154、calc_orchestrator.rs:576-577 | 条件联动链断裂，仅 Bonded 一处手工桥接；parent 链仅 sum 穿透 |
| 8 | 查询原语缺口：SumPositiveValues / Combine / HasMod / cfg.source 来源过滤 | 🟢 low | missing | ModStore.lua:134/:168/:257、ModList.lua:126 | mod_db.rs | 「from Equipped items」类口径无原生查询；影响面窄 |

---

## 缺口详述

### Gap 1（🔴 high / design）ModParser 六张 pattern 表只移植极小子集，且硬编码在 Rust 里

**证据**：PoB2 `formList` 含 GAIN/LOSE/GRANTS/REMOVES/CHANCE/PEN/REGENFLAT/REGENPERCENT/DEGEN*/TOTALCOST/BASECOST/FLAG/OVERRIDE/DOUBLED 等共 27 种 form（ModParser.lua:62-154 实测 91 条正则）；pobr `parse_form`（mod_parser.rs:629-683）只支持 INC/RED/MORE/LESS/BASE（加附加伤害/转换/override 少量专用函数）。`modNameList` 776 条 vs pobr `parse_name` ~118 个短语，覆盖率约 1/7。

**影响**：再生（`Regenerate N Life per second`）、穿透（`Damage Penetrates X%...`）、消耗、chance、双倍伤害等大类词条**整类落入 Unsupported，静默丢值不报错**。已全仓 grep 验证：消费侧 offence.rs:769-778 在读 `FirePenetration`/`ElementalPenetration` 等 ModName，但解析侧没有任何路径能从词条文本产出它们（mod_parser.rs 中 "penetrat" 零命中）；`DoubleDamage`/`XDoubled` 全仓零命中。

**架构问题更关键**：pobr 把这些「数据」写成 Rust match 分支，与 PoB2 把表写在 Lua 里是同构问题，每加一条词条都要改框架代码、违背本项目「数据 JSON 化、框架稳定」的核心目标。此外 PoB2 的 `scan()` 是位置无关的「最早开始 + 最长结束」匹配（:6362-6385 已核实），pobr 是固定顺序的 strip_prefix/strip_suffix 链，对词序变体鲁棒性差。

**修复方向**：把六张表声明式 JSON 化（见「数据 vs 逻辑切分建议」），框架只保留 scan 引擎 + 模板实例化；按 form 大类分批补齐（PEN/REGEN/CHANCE/FLAG/DOUBLED 优先，因消费侧已有对应 ModName 落点）。

### Gap 2（🔴 high / missing）specialModList 与 data 驱动派生模式基本缺失

**证据**：PoB2 把无法走通用 form 流程的词条（关键石、唯一物专属、『Nearby allies…』、triggerExtraSkill/grantedExtraSkill 等）全部收进 `specialModList`（实测 2085 个条目，:2231-6150），并在加载期按游戏数据自动派生：每个 keystone 名 → 一条 `Keystone LIST` mod（:6151-6158，由 CalcSetup 二次展开为分配该节点）、每个非 support 技能按 tags 生成 chains/pierce/projectile/totem/aura/curse 模式（:6302-6361，逐段核实）。pobr 只硬编码了 `parse_keystone_special` 的数条 OVERRIDE 特例；全仓 grep 证实无 `Keystone LIST` 通道（pobr-core/pobr-build/pobr-tree 中 keystone 仅见于 EHP 的 CI 选项与注释）。

**影响**：物品授予关键石（如 `Malachai's Awakening`）、大量唯一物/升华词条丢失。

**修复方向**：这些模板 95% 是「捕获数值代入固定 mod 结构」的纯数据，应 JSON 模板化而非继续往 Rust 加分支；per-gem/keystone 派生模式应放进离线管线（pobr-data-adapter），按同样模板规则展开成 JSON——这正印证了「PoB2 的 Lua 大量是数据生成的代码」这一判断。`Keystone LIST` 通道需要框架支持（解析产出 LIST mod → calc setup 期展开为节点分配），属于少量真逻辑。

### Gap 3（🔴 high / partial）EvalMod tag 求值：20 种 vs 5 种

**证据**：ModStore.lua:325-885 的 EvalMod 实为 **20 种** tag（分支行号见结构地图，逐一核实）；pobr `ModTag` 仅 Condition/Multiplier/DamageType/SkillTypes/SlotName 5 变体（modifier.rs:37-65）。

**缺失的关键语义**：

1. **tag.actor / limitActor 跨 actor 求值**（如 `per Poison on the Enemy` 读 enemy.modDB；minion 倍率受 player 限幅）——pobr 的 Multiplier 只读本 actor cfg；
2. **PerStat 从 actor.output 读已算出的 stat**（与 Multiplier 的区别正在于此）。pobr 把两者合并成一个 Multiplier tag、靠编排层把 stat 预灌进 cfg.multipliers——凡编排层没灌的 var 一律得 0 且静默（config.rs:116 `unwrap_or(0.0)` 已核实）；
3. varList/divVar/limitVar/limitTotal/limitNegTotal/invert/tag.base 偏置全部缺失；
4. PercentStat 的 m_ceil、StatThreshold/MultiplierThreshold 门控缺失（`while you have at least 100 Str` 类词条丢失）——全仓 grep `StatThreshold|ActorCondition|global_limit` 仅命中一条注释（mod_parser.rs:930）；
5. **globalLimit/globalLimitKey 跨 mod 累计限幅缺失**——这是 chance-to-deal-Double-Damage（DOUBLED form）整个机制的依赖。

**影响**：所有依赖这些 tag 的词条要么丢失要么数值错。

**修复方向**：扩展 ModTag 枚举至完整 20 种 + 求值上下文从 `&CalcConfig` 升级为可访问 actor output / 对端 actor 的求值环境；PerStat 与 Multiplier 拆开；globalLimit 在聚合层做跨 mod 累计。这是聚合内核下一阶段的最大工程，但 tag 语义本身是稳定逻辑（不随版本变），适合留在框架。

### Gap 4（🟡 medium / missing）ModCache 预编译机制缺位

**证据**：PoB2 把 parseMod 结果持久化为 `Data/ModCache.lua`（6598 行；启动加载 Main.lua:128、新词条回写 :293-297），运行时绝大多数词条 0 解析开销。pobr 的 `ModCache` 实现了 parse_or_insert，但生产路径（calc_orchestrator.rs、session.rs、passive.rs、item.rs、skill_source.rs）一律绕过它直接 `parse_mod`，缓存只被测试引用。

**核查修正（三处）**：(a) 归因的 filtered-recompute 走 `ModDb::filtered`，复用已解析 Modifier、零重解析——「归因放大解析开销」的说法不成立；(b) pobr-build 有 whole-build 粒度 `CalcCache`（按内容哈希缓存 OutputTable），同输入重算零解析——真正的重复解析发生在 **build 任一处变更后的全量重 ingest**（所有来源词条全部重新 parse_mod），编辑迭代场景下与「加速计算」目标仍相悖；(c) unsupported 上报 pobr 已有 runtime 面（`session.unsupported_modifier_texts` → CLI/WASM 输出），缺的是 PoB2 unsupported.txt 那种**离线全语料审计**。

**修复方向**：离线管线产出 `parsed_mods.json`（ModCache 的 JSON 化：把 data 内全部词条行预解析为「文本 → [Modifier]」表）+ 解析覆盖率指标，运行时只查表；增量 ingest（仅重解析变更来源）作为补充优化。

### Gap 5（🟡 medium / partial）ModFlags 位宽 5 vs ~30，武器类型语义降级

**证据**：PoB2 ModFlag（Data/Global.lua:222-259，逐行核实）含 Attack/Spell/Hit/Dot/Cast/Thorns/Melee/Area/Projectile/Ailment/MeleeHit/Weapon + Axe..Talisman 16 种武器 + WeaponMelee/WeaponRanged/Weapon1H/Weapon2H/WeaponMask，约 30 位；modFlagList 把『with maces』映射为 `bor(ModFlag.Mace, ModFlag.Hit)`。pobr `ModFlags` 仅 ATTACK/SPELL/MELEE/PROJECTILE/AREA 5 位（pobr-data modifier.rs:34-42）；KeywordFlags 仅 13 常量（modifier.rs:87-117），缺 Attack/Spell/Totem/Trap/Mine/Minion/Brand/元素位。

**影响**：PoB2 中武器类型是 cfg.flags 的位——进攻聚合按主手/副手分别构造 cfg（每只手带自己的武器位），『increased Damage with Maces』只进入持锤那只手的乘区。pobr 走两条降级路径：伤害族转派生 ModName（`MaceDamage` 等，mod_parser.rs:1206 + orchestrator:946-967 按主手类别消费）、非伤害族转全局 Condition（`UsingMace` 等，orchestrator 按主手置真）。单武器 build 等价，但 pobr 当前无 per-hand 计算结构，**双持异种武器时无法把词条限定到对应手**。Hit/Dot/Cast/Thorns/Ailment 维度的 flag 也缺（用 ModName 后缀如 `*DamageTaken` 或 keyword 兜，覆盖不全）。

**修复方向**：扩展 ModFlags 至 PoB2 全位宽（u64 bitflags + 武器位段），引入 per-hand cfg 构造；这是 audit 01-02（子集语义已修）之外的位宽问题，且是双持支持的前置。

### Gap 6（🟡 medium / missing）写侧原语：ReplaceMod/ConvertMod/MergeMod/ScaleAddMod

**证据**：PoB2 的 mod 注入不只是 append——天赋/升华转换珠宝走 `ConvertMod`、buff 覆写走 `ReplaceMod`、宝石按等级/品质缩放 mod 值走 `ScaleAddMod`，且有一套确定性取整规则（高精度查 `data.highPrecisionMods[name][type]` 走 `floor(v*scale*10^p)/10^p`、+level 类 floor、默认 `m_modf(round(·,2))` 截整——ModStore.lua:45-81 逐行核实；ReplaceMod/ConvertMod 在 :114-127）。pobr 全仓 grep 证实无 scale_add/replace_mod/convert_mod/merge_mod 任何实现（mod_db.rs:31-42 仅 add_mod/add_list）。

**影响**：宝石 ingest（skill_source.rs）自行缩放但无统一精度规则，与 PoB2 在非整缩放值上产生 ±0.01~1 的漂移并在 more 乘区放大；mod 转换类机制（ConvertMod 通道）无落点。more 聚合的 highPrecisionMods 例外（ModList.lua:131-144）在 audit 01-01 修复时未含，至今未补（round_more 无例外分支，已核实）。

**修复方向**：在 ModDb 补四个写侧原语 + 统一 ScaleAddMod 取整规则；`highPrecisionMods`/`defaultHighPrecision`（Data.lua:413-415+）与 ModScalability 表 JSON 化为 `high_precision_mods.json`。

### Gap 7（🟡 medium / partial）GetCondition 的 modDB Flag 回退缺失

**证据**：PoB2 大量词条产出 `Condition:Fortified`/`Condition:Phasing` 等 FLAG mod 入 modDB（formList 的 FLAG form + flagTypes 表，ModParser.lua:6268-6293 已核实），其它词条的 Condition tag 经 `GetCondition` 的第四级回退 `self:Flag(cfg, "Condition:var")`（ModStore.lua:268-274 逐字核实）自动看见它们——『You have Phasing』词条能点亮所有『while Phasing』词条，无需 UI 勾选。pobr 的 `Modifier::matches` 只收 `&CalcConfig`、结构上无法回查 ModDb；条件只来自编排层预置 HashMap（config.rs:100-116）。

**补充（核查发现）**：pobr 已有该回退的一次性手工等价物——`session.has_flag("Condition:CanUseBondedModifiers")` → `set_condition`（calc/session.rs:53 + calc_orchestrator.rs:576-577，仅 Bonded 一处）。说明问题已被意识到，但只逐条 hardcode 接线，每新增一种 flag-driven 条件都要改 orchestrator，**条件联动链整体仍断裂**。对应的 parent 链（minion 继承 player 条件/乘数，ModStore.lua:30-35）也缺：pobr ModList 的 parent 链只实现了 sum，more/flag/override/list 均不穿透 parent（mod_db.rs:494-500 已核实）。

**修复方向**：在条件求值处加通用的 `Flag("Condition:X")` 回退（求值环境需能回查 ModDb，与 Gap 3 的求值环境升级是同一工程）；parent 链补全 more/flag/override/list 穿透。

### Gap 8（🟢 low / missing）查询原语：SumPositiveValues / Combine / HasMod / cfg.source 过滤

PoB2 ModStore 的 Combine(:134)/SumPositiveValues(:168)/HasMod(:257)，及 Sum/More Internal 的 `mod.source:match("[^:]+") == source` 来源类过滤（ModList.lua:126 亲见），被 calc 用于『from Equipped items』『from Passives』类口径。pobr 的 origin(ModifierSource) 信息齐全但聚合 API 不暴露按来源过滤的查询（`filtered()` 可绕但需手工构 db 副本，热路径成本高）。影响面较窄。

---

## 数据 vs 逻辑切分建议

**该领域「数据 vs 逻辑」的本质切分**——这是用户的核心关注点，也是本领域最大的架构机会：PoB2 ModParser.lua 642KB 中超过 90% 的体积是数据表，pobr 目前以小一个数量级的规模复刻了同样的「数据硬编码进框架」反模式。

### 属于数据（应 JSON 化、随版本更新）

1. **ModParser 六张巨型表**：`formList`（91 条正则→27 种 form）、`modNameList`（776 短语→ModName/名集）、`modFlagList`（202 条）、`preFlagList`、`modTagList`（684 条）、`specialModList`（2085 条目）。这些表 95% 以上的 value 是「纯数据模板」——即使 PoB2 写成 Lua 闭包 `function(num) return { mod("X","MORE",num,...) } end`，本质也只是「捕获数值代入固定模板」，可用带占位符的声明式 JSON 表达，如：
   ```json
   { "name": "Damage", "type": "MORE", "value": "$1", "tags": [...] }
   ```
   只有极少数闭包含真逻辑（如 DOUBLED 的双 mod 生成、jewelFuncList 的节点遍历闭包）需留在框架。
2. **数据 × 模板的派生表**：`skillNameList`/`preSkillNameList` 与 per-gem 的 specialModList 扩展（chains/pierce/additional projectiles，:6302-6361）由 `data.gems` 在加载期生成；keystone 条目由 `data.keystones` 生成（:6151-6158）。这印证了「数据生成代码」的判断——PoBR 应在离线管线（pobr-data-adapter）里按同样模板规则展开成 JSON，而不是运行时派生。
3. **小查找表**：`suffixTypes`(:6166)/`penTypes`(:6215)/`resourceTypes`(:6223)/`regenTypes`(:6264)/`flagTypes`(:6268)、`unsupportedModList`(:6161)、Data.lua 的 `highPrecisionMods`/`defaultHighPrecision`(:413-415+) 与 ModScalability——全是数据。
4. **`Data/ModCache.lua`（6598 行）**：纯生成物（parseMod 结果的持久化），等价于「预解析 mod 行 → 解析结果」的离线缓存表。

### 属于逻辑（留在框架）

`scan()` 最早+最长匹配引擎（:6362-6385）、`parseMod` 两 pass 编排与 flag/tag 合并、addToAura/addToMinion/applyToEnemy 等 LIST 包装、`ModStore:EvalMod` 20 种 tag 求值器、ModList/ModDB 聚合内核、ModTools 的 createMod/compare/format、ScaleAddMod 缩放取整算法（其精度例外表是数据）。

### PoB2 现在如何混在一起（反面教材）

全部数据表以 Lua 字面量硬编码在 ModParser.lua 内（642KB 单文件），与 scan/parseMod 逻辑同文件；表的 value 里混用纯 table 与闭包；派生表在模块加载期就地生成；ModCache.lua 由运行时回写源码目录。版本更新时这些表必须改代码。

### pobr 当前 JSON schema（catalog.rs + data/4.5.0.3.4/）还缺的表/字段

| 缺失项 | 建议 schema | 说明 |
|---|---|---|
| **mod 解析规则表**（完全缺失） | `mod_parser_rules.json`：formList / nameList / flagList / tagList / specialList 五段，value 用占位符模板 | 目前 form 模式、name 映射、flag/tag 短语、special 模板全部硬编码在 mod_parser.rs 的 Rust match 里——与 PoB2 把表硬编码在 Lua 里是同构问题。框架只保留 scan 引擎与模板实例化逻辑 |
| **`parsed_mods.json`（ModCache 等价物）** | 「词条文本 → [Modifier] 序列化」缓存表 | 离线工具把 mods.json / passive_tree.json / granted_effect_* 里出现的全部英文词条行预解析；运行时零解析，且把**解析覆盖率变成离线可审计指标**（PoB2 unsupported.txt 的用途；pobr 目前只有 runtime 上报） |
| **stat 语义映射表** | `stat_id → (ModName, ModType, tags)` 映射 | mods.json 已有 `stats: [{stat_id, min, max}]` 但没有到内部 ModName 的映射——PoB2 被迫走英文文本解析是历史包袱，PoBR 长期可**直接由 stat_id 入库绕过文本解析**（比 PoB2 更稳的架构）；该映射表本身是数据 |
| **`high_precision_mods.json`** | name → type → 精度位数；附 mod 可缩放性（ModScalability） | MORE 取整与 ScaleAddMod 精度例外（Gap 6 的数据侧） |
| `PassiveNodeDef.stats` / 物品词条的入库形态 | 由英文文本行迁向 stat_id / 预解析形态 | 短期合理，但等价于把「数据」以待解析文本形态入库；配合 parsed_mods.json 可平滑过渡 |

**优先级建议**：`parsed_mods.json`（立即把覆盖率变成可量化指标，且与「加速计算」目标直接对齐）→ `mod_parser_rules.json`（把 Gap 1/2 的补齐工作从「改框架」变成「填数据」）→ stat_id 直通映射（长期摆脱文本解析）。

---

## 附录：核查说明（verification_notes）

核查范围：3 条 high 全部 + 4 条 medium（gap 4/5/6/7）逐条打开双侧代码验证，gap 8（low）抽查，共 8 条全覆盖。先读了 `audits/pob2-parity-2026-06-09/FINDINGS.md` 确认 01-01～01-06 已修内容与本报告声明一致（本报告未与已知结论重复）。

**查实保留（描述微修）**：
- Gap 1：formList 实测 91 条 / 27 种 form、modNameList 776 条、modFlagList 202、modTagList 684——与描述一致。pobr parse_form 确仅 5 形态（mod_parser.rs:629-683 逐行读过）；全仓 grep 证实 PEN/REGENFLAT/DOUBLED 零解析落点，且消费侧 offence.rs:769-778 在读 FirePenetration 等名而解析侧产不出——「丢值」断言成立。修正 pobr 行号（parse_form 606-660→629-683；parse_name 1145-1257→1168-1516，~110 名→~93 臂/~118 短语）。
- Gap 2：specialModList 实测 2085 条目；keystone→`Keystone LIST` 派生（:6151-6158）与 per-gem chains/pierce/projectile（:6340-6355）逐段读过；pobr 全仓 grep keystone 仅 parse_keystone_special + EHP CI 选项，无 Keystone LIST 通道。成立。
- Gap 3：EvalMod 分支逐一 grep 核对，实为 **20 种 tag 而非 19**（原报告列举本身就是 20 项，计数笔误），起始行 :325 非 :326——已修正。pobr ModTag 5 变体、cfg.multiplier `unwrap_or(0.0)` 静默、全仓 grep StatThreshold/ActorCondition/global_limit 仅一条注释命中——成立。
- Gap 5：Global.lua:222-259 ModFlag 表逐行读过（~30 位）；pobr ModFlags 5 位、KeywordFlags 13 常量核实；weapon_type_tags 实际在 mod_parser.rs:1017-1029（原报告写 995-1016，已修正）。补充核实 "damage with maces"→MaceDamage 派生名路径（:1206 + orchestrator:946-967），双重降级均为主手全局语义、无 per-hand 结构——结论成立。
- Gap 6：ScaleAddMod :45-81 逐行核对（unscalable / level-floor / highPrecision / m_modf 全部如描述）；ReplaceMod/ConvertMod 实际在 :114-127（原报告写 102-134，已修正）；ModList MoreInternal highPrecision 例外在 :131-144 核实；pobr 全仓 grep 无四原语、round_more 无例外分支。成立。

**修正后保留**：
- Gap 4：ModCache 仅测试使用、生产五处直调 parse_mod——查实（grep 全部调用方）。但三处 detail 断言修正：(a) 「归因 filtered-recompute 放大解析开销」不成立——attribution 走 ModDb::filtered 复用已解析 Modifier，零重解析；(b) 原报告漏报 pobr-build/calc_cache.rs 的 whole-build OutputTable 缓存，重复解析仅发生在 build 变更后的全量重 ingest；(c) 「缺 unsupported 审计面」过强——runtime 上报已存在，缺的是离线全语料覆盖率审计。核心架构论点（无离线预解析产物）仍成立，维持 medium。
- Gap 7：GetCondition Flag 回退 :268-274 逐字核实、flagTypes :6268+ 核实、pobr matches() 结构上无法回查 ModDb 核实、parent 链仅 sum 核实。发现原报告漏了 pobr 已有的一次性手工桥接（session.has_flag + calc_orchestrator.rs:576-577，Bonded 专用）——已增补，结论从「完全缺失通道」修正为「无通用回退、仅逐条 hardcode 桥接」，severity 维持 medium。
- Gap 8：抽查通过——SumPositiveValues:168 / Combine:134 / HasMod:257 存在；source 前缀过滤模式在 ModList.lua MoreInternal:126 亲见同款 `mod.source:match("[^:]+")`。保留。

**无删除项**：8 条全部成立或修正后成立，无一条需要降级或删除；唯一接近降级的是 Gap 4（两处影响断言被驳），但其架构主张（生产路径零缓存、无离线预编译）经查属实且与项目核心目标直接相关，维持 medium。

其余字段同步修正：pob2_structure 中 EvalMod 19→20 种、parseMod 行号 6388→6389、cache 闭包 7105→7109；pobr_status 中 mod_parser.rs 1493→1516 行、parse_name 计数更新、补 CalcCache 与 runtime unsupported 上报的存在。
