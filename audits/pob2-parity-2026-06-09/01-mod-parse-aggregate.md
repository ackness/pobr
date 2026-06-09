# 01 · Modifier 解析与 ModDb 聚合

**Rust 模块**：`pobr-core` — `mod_parser.rs` / `mod_db.rs` / `modifier.rs` / `config.rs`
**对照**：PoB2 `ModList.lua` / `ModStore.lua` / `Global.lua` / `ConfigOptions.lua`
**agent-docs 交叉**：§一 数据来源解析与 ModStore 聚合

## 总评

总体方向与 PoB2 一致：ModDb 的标准属性管线 `(base+Σbase)*(1+Σinc/100)*Π(1+more/100)`、按 name 分桶查询、override 后写覆盖、flag/list 语义都与 PoB2 ModList 的 `SumInternal`/`MoreInternal`/`Override`/`Flag`/`List` 对齐，聚合公式系数无方向性错误。其余机制（override rev 取最后、list、max_of 取最强、per-slot 防御桶）均正确。下列 finding 聚焦真实/潜在偏差。

---

## 01-01 · MORE 聚合缺少 PoB2 的逐-mod round(modResult, 2) 精度归一 — HIGH

**PoB2 行为**：`ModList.lua:118-150` `MoreInternal`：对每个 modName 先把该名下所有 MORE 累乘成 `modResult`，然后 `result = result * round(modResult, 2)`（非高精度 mod 走 round 到小数点后 2 位；高精度 mod 才走 `floor(·*power)/power`）。即每个 modName 桶的 more 乘子在并入总积前会被舍入到最近 0.01。这是 PoB2 确定性数值的一部分。

**PoBR 现状**：`mod_db.rs:161-169` `more()`：`fold(1.0, |product, value| product * (1.0 + value/100.0))` —— 把所有 MORE 值（跨 name、跨来源）一次性连乘，全程不做任何 round。`more_traced(173-213)`、`more_global_only(256-268)`、`more_for_slot(271-283)` 同样无舍入。
- `crates/pobr-core/src/mod_db.rs:161`
- `crates/pobr-core/src/mod_db.rs:168`
- `crates/pobr-core/src/mod_db.rs:181`
- `crates/pobr-core/src/mod_db.rs:267`
- `crates/pobr-core/src/mod_db.rs:282`

**修复方案**：按 PoB2 语义改为 per-modName 分桶：对每个 `ModName` 先算该桶 `Π(1+v/100)`，`round` 到 2 位（除非该 name 在 `data.highPrecisionMods` 内），再并入总积。当前 `more()` 接收 `names: &[ModName]` 却把所有 name 混在一个 fold 里，应改为外层 `for name`、内层算桶、桶级 round。短期可先实现 round-to-2-decimals 的桶级归一以贴近 PoB2 主路径；高精度 mod 表可后置。注意这会改变现有 golden，需配合 oracle 重新对账。

---

## 01-02 · ModFlags 匹配用 intersects（任一重叠），PoB2 要求 mod.flags 是 cfg.flags 子集 — HIGH

**PoB2 行为**：`ModList.lua:103/126/157/178/200/223` 一律用 `band(flags, mod.flags) == mod.flags`：mod 上设置的每一个 flag 都必须在 `cfg.flags` 中出现，mod 才生效（AND 语义/子集判定）。例如 `mod.flags = Attack|Projectile` 时，纯 Attack（非投射）技能 `band(Attack, Attack|Projectile)=Attack ≠ Attack|Projectile` → 拒绝。

**PoBR 现状**：`modifier.rs:137-139` `matches()`：`if !self.flags.is_empty() && !self.flags.intersects(cfg.flags) { return false; }` —— 只要 `mod.flags` 与 `cfg.flags` 有任一位重叠即通过（OR 语义）。mod 有多 flag 时与 PoB2 相反。
- `crates/pobr-core/src/modifier.rs:137`
- `crates/pobr-core/src/modifier.rs:138`
- `crates/pobr-data/src/modifier.rs:52`

**修复方案**：把 `matches()` 的 flag 判定改为 PoB2 子集语义：`self.flags.is_empty() || (cfg.flags.bits() & self.flags.bits()) == self.flags.bits()`。当前 parser 不产生多 flag mod，故现行回归不变（单 flag 下两语义等价），但这是正确性前提：一旦给词条加上 `MELEE`/`PROJECTILE`/`AREA`（enum 已定义）或 ingest 路径产生多 flag mod，intersects 会错误放行。建议改正并补一条多 flag 单测固化语义。

---

## 01-03 · PerStat/Multiplier 缺少 m_floor(base/div+0.0001) 整数化 — MEDIUM

**PoB2 行为**：`ModStore.lua:365`（Multiplier）与 `:460`（PerStat）：`local mult = m_floor(base / (tag.div or 1) + 0.0001)` —— 资源数除以 div 后向下取整再用作乘数。即 `per 10 Strength` 在 95 力量时 `mult=floor(9.5)=9`，而非 9.5。

**PoBR 现状**：`modifier.rs:168-180` `effective_number()`：`let count = cfg.multiplier(var) / div.max(EPSILON); value *= limit.map_or(count, |max| count.min(max));` —— 直接用浮点 count，未向下取整。`per 10 Strength` 在 95 力量会按 9.5 倍缩放，高于 PoB2 的 9 倍。
- `crates/pobr-core/src/modifier.rs:174`
- `crates/pobr-core/src/modifier.rs:175`

**修复方案**：对 `div>1` 的资源型 Multiplier 按 PoB2 加 floor：`let count = (cfg.multiplier(var)/div + 0.0001).floor()`（充能类 div=1 时 floor 无影响，对整数资源也无影响，仅修正 per-N 非整倍场景）。`limit` 应在 floor 之后再 `min`（与 PoB2 一致：先算 mult 再 `min(limit)`）。注意 PoB2 的 PerStat 还有 `tag.base` 偏置与 `limitTotal`，PoBR 暂未支持，可作为后续。

---

## 01-04 · KeywordFlags 不支持 MatchAll，仅实现默认 any 语义 — MEDIUM

**PoB2 行为**：`Global.lua:316-341` `MatchKeywordFlags`：默认 mod 的 keywordFlags 是 ANY 匹配（任一命中即可），但若 mod 带 `KeywordFlag.MatchAll(0x40000000)` 则改为 ALL 匹配（`band(keyword,mod)==mod`）。`ModParser.lua:1058/1088/1202` 等把 `for poison`/`of curse auras`/`poisons inflicted with crits` 设为 MatchAll。

**PoBR 现状**：`modifier.rs:141-143` `matches()`：`!self.keyword_flags.intersects(cfg.keyword_flags)` 只实现了默认 any 语义；`KeywordFlags`（pobr-data）也没有 MatchAll 位概念。同时 parser 当前几乎不产生 keyword_flags（始终 NONE），未把 poison/curse/dot 等关键词解析为 KeywordFlags。
- `crates/pobr-core/src/modifier.rs:141`
- `crates/pobr-data/src/modifier.rs:78`
- `crates/pobr-core/src/mod_parser.rs:1097`

**修复方案**：中期在 `KeywordFlags` 增加 `MATCH_ALL` 常量，`matches()` 据其切换 any/all（与 `MatchKeywordFlags` 等价）。优先级取决于 parser 是否产出 keyword_flags——当前几乎不产出，影响面有限；若后续接入 poison/ignite/dot 等关键词词条，需同步补 MatchAll，否则 `for poison` 类词条会被错误放行到非中毒上下文。

---

## 01-05 · GetMultiplier 不消费 modDB 内的 Multiplier:X，依赖编排层预解析 — LOW

**PoB2 行为**：`ModStore.lua:276-278` `GetMultiplier`：`Override(Multiplier:var)` 或 `self.multipliers[var] + parent + Sum(BASE, Multiplier:var)`。即乘数可由 modDB 里名为 `Multiplier:X` 的 BASE/Override mod 直接提供（如某词条 `+1 to Virulence`→BASE `Multiplier:Virulence`），且支持 parent 链与 override。

**PoBR 现状**：`config.rs:104-106` `multiplier()`：仅 `self.multipliers.get(name).copied().unwrap_or(0.0)`，纯 HashMap 读。所有乘数必须由编排层（`calc_orchestrator.rs:553+` `set_multiplier`）预先算好塞入 cfg，ModDb 里若存在 `Multiplier:X` 形态的 mod 不会被自动纳入。
- `crates/pobr-core/src/config.rs:104`
- `crates/pobr-build/src/calc_orchestrator.rs:553`

**修复方案**：属架构分工差异而非即时 bug：PoBR 把 multiplier 解析上移到编排层，对已覆盖的资源（力/敏/智/充能/等级）数值正确。风险在于词条直接产出 `Multiplier:X` BASE 的场景（少见）会被漏算。建议：要么在 parser 把 `+N to <Multiplier>` 解析为对应 multiplier 注入，要么文档化「ModDb 不承载 Multiplier:X mod，由编排层注入」的契约，避免后续接入此类词条时静默丢失。

---

## 01-06 · 条件默认状态全部为 false，未对齐 PoB2 ConfigOptions 的 defaultState — LOW

**PoB2 行为**：`ConfigOptions.lua` 中部分 Condition 有 `defaultState`（如某些 buff/姿态默认开启），`GetCondition` 在无显式覆盖时回退到这些默认；`EvalMod` 的 Condition tag 据此求值。

**PoBR 现状**：`config.rs:100-102` `condition()`：`self.conditions.get(name).copied().unwrap_or(false)` —— 任何未显式置真的条件一律 false。`matches()` 的 Condition tag 据此求值（`modifier.rs:146-149`）。Empowered 等被有意设默认 false（注释已说明），但通用回退缺少 PoB2 defaultState 表。
- `crates/pobr-core/src/config.rs:100`
- `crates/pobr-core/src/modifier.rs:146`

**修复方案**：多数条件默认 false 与 PoB2 一致（面板默认不勾选）。差异仅在少数有 `defaultState=true` 的条件上。建议核对 `ConfigOptions.lua` 中带 defaultState 的条件清单，由编排层在构造 cfg 时把这些预置为对应默认值（而非全 false 回退），避免默认开启型 buff 被漏算。优先级低，按实际词条覆盖面推进。
