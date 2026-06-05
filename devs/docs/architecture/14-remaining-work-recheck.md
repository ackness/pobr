# 剩余工作权威重核（2026-06-06）

> 由一次 8-agent 并行审计（按子系统：gem-dps / buff-aura / defence-ehp / ailment /
> minion-trigger / sources-tree-item / parity-app / data-pipeline）综合，**实地核对了
> commit ae9676f（宝石 DPS Phase 2+3）后的工作树**。取代 doc13 中已被超越的「完成声称」。
> 权威性低于代码本身——执行时仍以代码 + agent-docs + vendor PoB2 三方交叉验证为准。

# PoBR — 权威剩余工作清单 (Authoritative Remaining-Work List)

> 综合 8 份子系统审计 + 工作树实地核对（`git status` 中的未提交改动已纳入）。
> **路径已规范化**：审计稿多处写作 `crates/posebr-*`，实际 crate 名为 `crates/pobr-*`。下文统一用真实路径。
> **核对基准**：HEAD = `7c7a543` + 当前未提交工作树（12 文件，+526 行）。**多份审计基于提交时刻的旧快照，已被工作树超越**——见下节。

---

## 与 doc13 / 审计稿的差异（必读：数份审计已过时）

工作树（未提交改动）已推进到 commit `7c7a543` 之后，使若干审计「缺口」**已部分或全部闭合**。核对结论：

### A. doc13/审计称「缺/deferred」但**其实已做**

| 审计声称 | 实地核对 | 证据 |
|---|---|---|
| `granted_effect_stat_sets` 表「下载阻塞 / 未导出 / DPS=0」（data-pipeline / sources-tree / parity 三方审计均如此） | **已导出并入库**。`data/4.5.0.3.4/granted_effect_stat_sets.json` = 2.1M 真实数据；manifest `domains` 含 `granted_effect_stat_sets` | `data/4.5.0.3.4/` 目录列表；`manifest.json` domains |
| `BuildData 未加载 SkillStatSetDef`（gem-dps 审计 item 2，effort S） | **已加载**。`BuildData` 含 `skill_stat_sets: HashMap<String, SkillStatSetDef>`，`load()` 调用 `data.skill_stat_sets()`，`ResolvedSkillLevel` 新增 `base_damage: Vec<SkillDamageStat>` | `build_data.rs:48,61,106-118,174-191` |
| `SkillStatSetDef/Level/SkillDamageStat 未用`（gem-dps item 1） | **已定义且适配器已 join**。`damage_stat_to_mod()` + `skill_base_modifiers()` 把 stat-set 基础伤害映射为 `<Type>DamageMin/Max` BASE 注入 offence | `catalog.rs:208-250`；`calc_orchestrator.rs:238-260` |
| 「主技能基础伤害无端点 / 技能 DPS 恒为 0」（多方） | **主技能伤害已贯通**。`calculate_with_data` line 158-160 注入 `skill_base_modifiers(main_skill)` | `calc_orchestrator.rs:127-206` |
| `mastery_effects 玩家选择注入未做`（sources-tree audit item 1，data-pipeline item 6） | **已做**（doc13:159 正确，doc13:192 的 deferred 标记是陈旧/自相矛盾）。`PassiveTreeSpec.mastery_effects` 存在且 `pobr-tree/src/node.rs:54` 实际读取并按选择注入 | `passive_tree.rs:14-31`；`node.rs:31,54` |
| `SkillUseTime 替换 action_rate 未贯通`（gem-dps item 6） | **主技能已贯通**。`base_input.base_action_rate = 1.0/use_time` | `calc_orchestrator.rs:141-146` |

> **净影响**：gem-dps 审计的 7 项中，3 项（item 1 注入、item 2 加载、item 6 action_rate）对**主技能**已闭合；剩余真实缺口收窄为「per-gem / support-gem 倍率隔离 / 多主技能」（见 P1/P2）。data-pipeline 审计的 stat-set 阻塞项已解除。

### B. doc13/审计称「完成」但**仍有缺口**

| doc13/审计声称完成 | 实际缺口 | 落点 |
|---|---|---|
| Wave2 批次1「PoB catalog fixture + check_pobr_parity CI gate」（doc13:159） | parity catalog 1877 条**全部 `Planned`，0 条 `Computed`**；无 CI gate；无对真实 PoB2 的 golden 回归 | P0-1 |
| Wave2「golden 回归 harness（2 份真实 PoB2 ninja build）」（doc13:96/159/163） | `golden_regression.rs` 只有单元测试，无 PoB2 期望值断言、无 ninja fixture | P0-1 |
| Wave2 批次3「异常维度：跨类型施加完整」（doc13:172） | `cross_type_source_hit()` 已写但 `perform.rs` **从不调用**——FireCanBleed/ChaosCanShock 等静默失效 | P0-3 |
| Wave2 批次3「Shock 叠层 / Chill 叠层完成」（doc13:181） | 二者**均未实现**——只输出标量 effect，无 stacking | P1（异常叠层） |
| Wave2 批次3「召唤物完整化」（doc13:176） | minion 多项字段（skill_list/reservation/tags/hostile）定义但未用；ally-buff 无缩放 | P1/P2 |
| Wave2「`SkillUseTime` 替换 offence action_rate」（doc13） | 仅**主技能**生效，多主技能/副技能无独立 use_time | P2 |

---

## P0 — Correctness Bugs（已编码但静默失效 / 权威性声明落空）

### P0-1 · Parity catalog 全 `Planned`，无 golden 回归 / CI gate
- **一句话**：parity 框架是脚手架——1877 条 stat 全标 `Planned`、0 条 `Computed`，且无任何对真实 PoB2 build 的数值回归，doc13「CI gate 已交付」不成立。
- **证据**：`devs/fixtures/pob/parity/pob-catalog.json`（1877 条 `"parity_status": "Planned"`，`grep -c Computed` = 0）；`crates/pobr-build/tests/golden_regression.rs` 仅含 `full_repo_data_end_to_end_smoke` 等单元测试，无 PoB2 期望值断言；无 `check_pobr_parity` CI 钩子。
- **effort**：M（建框架）→ L（标定全 1877 条）
- **外部阻塞**：是 — 需从 `vendor/PathOfBuilding-PoE2` 跑真实 build 导出 golden 数值作为对照集。

### P0-2 · `resolve_gems()` 硬编码空 `modifier_texts`（per-gem stat 未注入）
- **一句话**：主技能基础伤害已贯通，但**逐宝石**的分等级 stat 与**所有 support 宝石的 more/less 倍率**仍未注入——`resolve_gems` 三处仍传 `Vec::<String>::new()`。
- **证据**：`crates/pobr-build/src/calc_orchestrator.rs:389,397`（`GemModSource::support/active(..., Vec::<String>::new())`）；模块注释 line 372-374 自陈「自身暂不贡献 modifier」。对比：主技能走的是 `skill_base_modifiers()`（line 158），support 宝石不经此路径。
- **影响**：support gem（如「更多伤害」辅助）的 more 倍率完全不作用；多宝石组只有启发式选中的主技能有伤害。
- **effort**：M
- **外部阻塞**：否（数据已在 `skill_stat_sets`，只差把每个 gem 的 `base_damage` → text/mod 并按 SkillTypes tag 注入）。

### P0-3 · `cross_type_source_hit()` 已写但 perform.rs 从不调用
- **一句话**：跨类型异常施加（FireCanBleed / ChaosCanShock 等）的函数已实现却从未被 `fill_ailments` 调用，特性静默失效。
- **证据**：`crates/pobr-core/src/calc/ailment.rs:1210-1233` 定义 `cross_type_source_hit()`；`perform.rs:447-451,500` 用硬编码 `phys_hit` / `chaos_phys_hit` 直接赋值，不经该函数。
- **effort**：S
- **外部阻塞**：否。
- **vendor**：PoB2 `CalcOffence.lua:4809-4825` `canDoAilment` + `<Type>Can<Ailment>` 处理。

### P0-4 · ally-buff 未按召唤物 `BuffEffectOnSelf` 缩放（minion）
- **一句话**：玩家 buff 全额（100%）透传给召唤物，未乘召唤物自身的 `BuffEffectOnSelf`，与 PoB2 不符。
- **证据**：`crates/pobr-core/src/calc/minion.rs:369-372`（直接 `db.add_mod(m.clone())` 无缩放）；对照 PoB2 `CalcPerform.lua:1012-1020` `valueFixed *= minion.BuffEffect/100`。
- **effort**：M
- **外部阻塞**：否（但与 P1「buff/aura ingest」共享 BuffEffect 基建，宜同期做）。

---

## P1 — 解锁下游最多的 Missing Feature（建议优先解锁）

### P1-1 · Buff/Aura ingest 系统（BuffEffect + 光环 + 玩家 debuff 注入 enemy.mod_db）
- **一句话**：整个 buff/aura 子系统为 deferred——`BuffEffectOnSelf` 缩放、光环 Spirit 保留/Presence、Herald autogem、curse/wither/brittleness 注入敌人全部缺失；这是 offence/defence/minion 三方共同的下游瓶颈。
- **证据**：全仓 `grep BuffEffect` 在 `crates/*/src` 零命中；`env.rs:54` & `minion.rs:172` 的 `ally_buff_mods: Vec<Modifier>` 是 stub（`minion.rs:430-473` 置 `vec![]`）；`setup_env.rs:16-18,188-206` 仅有 `reduce_enemy_exposure`，curse/wither 显式 deferred；`OutputTable`（`output.rs:1-144`）无任何 buff 字段。
- **effort**：L（拆分：BuffEffect 标量缩放 M → 光环/保留/Presence L → Herald M → debuff 注入 M）
- **外部阻塞**：否（光环建模为带 reservation 的 Granted Effect，数据已在 granted_effects；但 `has_reservation`/Spirit 解析需补适配器，见 P2-数据）。
- **下游解锁**：P0-4(minion buff)、defence 生存、offence 投资型 build、enemy debuff DPS——**单点解锁面最大**。
- **vendor**：PoB2 `CalcPerform.lua:257-265`（buff×(1+BuffEffectOnSelf/100)）、Spirit/Reservation 段、Herald autogem 段。

### P1-2 · 异常叠层：Ignite / Shock / Chill stacking 未实现
- **一句话**：Bleed/Poison 有 `resolve_stack_config` 叠层，但 Ignite、Shock、Chill 三种均只算单层/标量，无 `*CanStack` / `*Stacks` 聚合。
- **证据**：`perform.rs:485-490`（Ignite 无 `resolve_stack_config` 调用）、`:512-523`（shock 仅标量 `output.shock_effect`，无 `shock_stacked_dps`）、`:491-498`（chill 同）；对比 `:478,506` 的 Bleed/Poison 叠层。
- **effort**：Ignite S（机制同 Bleed），Shock/Chill 各 M（非伤害异常叠层 → DamageTaken 聚合）。
- **外部阻塞**：否。
- **vendor**：`CalcOffence.lua` IgniteCanStack/ShockStacks/ChillStacks。

### P1-3 · 法术型召唤物技能伤害（skill_list 未派生）
- **一句话**：`MinionDef.skill_list`（如骷髅法师的 ArcSkeletonMage）定义但 calc 完全不读——所有召唤物被当纯攻击型，法术召唤物伤害无法派生。
- **证据**：`crates/pobr-core/src/calc/minion.rs` 无 `skill_list` 读取；`MinionWeaponData`（L134-139）只有 physical min/max 无法术伤害字段。
- **effort**：L（需 skill_list → SkillGem/granted_effect 基础伤害注入虚拟武器，与 P0-2 gem 伤害管线复用）。
- **外部阻塞**：是 — 依赖 `minion.json` 入库（见 P2-数据）+ gem 伤害管线（P0-2）。
- **vendor**：PoB2 `CalcActiveSkill.lua:800-900`。

### P1-4 · Secondary（DoT 源）伤害类型未在管线分桶
- **一句话**：`DamageSource` enum 存在但 magnitude 缩放从不按 Attack/Spell/Secondary 分支，secondary base damage 完整性 deferred。
- **证据**：`ailment.rs:155-189`（`bleed_instance`/`poison_instance` 硬编码 mod 名不做 source 过滤）；`DamageComponent.rs:38-49` 定义 enum 但 magnitude 不分支。
- **effort**：M
- **外部阻塞**：是 — 需 `agent-docs/damage-scaling.md` 明确 secondary 定义 + PoB2 `CalcOffence.lua` secondary config。

---

## P2 — Partial 完善（机制已部分实现，补齐边界/集成）

### P2-1 · 支援宝石 more/less 倍率的 SkillTypes tag 隔离
- **一句话**：`SupportGemSpec`（mana_multiplier/supported_skill_types）已定义，但因 P0-2 未注入文本，support 倍率不生效；补全后还需验证 more 倍率仅 tag 被支援技能而不污染全局。
- **证据**：`crates/pobr-core/src/skill_source.rs:180-220`；`offence.rs` 无 SupportGem source 的 tag 隔离逻辑。
- **effort**：M｜**外部阻塞**：否（依赖 P0-2 先落地）。

### P2-2 · 异常 effect/rate 与 DotDpsCap 的 trace 集成
- **一句话**：`apply_effect_and_rate_mod_traced()` / `dps_with_effect_rate_cap_traced()` 已建 TraceGraph，但 `finalize_ailment_dps()` 调的是**非 traced** 版本，归因链断裂；Shock 还完全缺 magnitude 缩放。
- **证据**：`ailment.rs:1168-1193,1308-1344`（traced 版存在）vs `perform.rs:542-545`（无 trace 参数）；`ailment.rs:270` `shock_effect()` 纯函数不读 modifier。
- **effort**：S（trace 接线）+ M（Shock magnitude 缩放）｜**外部阻塞**：否。

### P2-3 · Trigger rate 未注入 ModDb（SkillTriggerRate 缺失）+ 轮转浪费率未折损
- **一句话**：trigger.rs 算出 trigger_rate 但只返回数值结构体，从不写 `SkillTriggerRate` modifier 进 ModDb 供 DPS 引用；`wasted_fraction` 计算出却不乘进 DPS。
- **证据**：`trigger.rs:102-149,304-320,559-570`（仅返回结构体）；`:443-449,472-548`（wasted_fraction 不被应用）。
- **effort**：M（modifier 注入）+ S（浪费率折损）｜**外部阻塞**：否。

### P2-4 · 召唤物：reservation / monster_tags / hostile / base_damage_ignores_attack_speed / Spectre 等级
- **一句话**：MinionDef 多个字段定义但未被 calc 尊重——Spirit/Companion 保留量不消耗、tag 限定 modifier 漏判、敌对 minion 无差别、Spectre 用宝石等级而非区域等级、攻速忽略标志作用范围未在 offence 延续。
- **证据**：`data/minion.rs:202,208-214,218,221`（字段）；`calc/minion.rs` 全文无 reservation/hostile/tags 读取，`:237-239` 攻速标志仅限虚拟武器，`:417-419` Spectre 无特例分支。
- **effort**：reservation M / tags M / hostile S / spectre M / attack_speed M｜**外部阻塞**：部分（依赖 `minion.json` 入库，见 P2-数据）。

### P2-5 · 技能基础参数补全：implicit_stats + 非法力 cost（Spirit/Life/Charges）
- **一句话**：`ResolvedSkillLevel` 缺 `GrantedEffectDef.implicit_stats`（技能自带隐性词条），cost 仅识别 `type==0`(Mana)，Spirit/Life 等资源返回 None。
- **证据**：`build_data.rs:162-172`（cost 仅 `== 0`）；`GrantedEffectDef` 无 implicit_stats join。
- **effort**：M｜**外部阻塞**：部分 — 非法力 cost 需 `CostTypes` 表（注释 `build_data.rs:163` 自陈「待 CostTypes 表下载后」）。

### P2-6 · 数据管线：minion.json / aura / unique_items / CostTypes 未导出
- **一句话**：calc 已有 minion/aura 骨架但**数据未入库**——`pipeline/config.json` 无 minion/aura/unique/CostTypes 表，calc 用硬编码 minion 测试数据。
- **证据**：`pipeline/config.json` 无对应表；`data/4.5.0.3.4/` 无 `minion.json`/`aura`/`unique_items.json`；`vendor/PathOfBuilding-PoE2/src/Data/Minions.lua` 有 32 个 minion 定义未导出。
- **effort**：minion L / unique L / CostTypes S / aura M｜**外部阻塞**：是（部分需 GGG CDN `.dat` 下载，CostTypes 适配器已有模板）。
- **下游解锁**：P1-3(法术召唤物)、P2-4(minion 字段)、P2-5(非法力 cost)、P1-1(光环 reservation)。

### P2-7 · 第二武器组 / JewelSocket 内嵌珠宝 / 物品基底防御值
- **一句话**：sources-tree 三个 deferred 切片——副武器组独立 Spec、装备内嵌珠宝 socket、物品固有 `Armour:`/`Evasion:` 基础值（非词条）未接入防御计算。
- **证据**：`xml_build.rs:22`（deferred 注释）、`:189`（仅交换逻辑无独立 Spec）；`PassiveTreeSpec` 单 spec；`item_text.rs` 不处理 `Armour:`/`Evasion:` 作为 base property。
- **effort**：第二武器组 M / JewelSocket M / 物品防御 M｜**外部阻塞**：否。

### P2-8 · 物品 quality 局部 more 不完整 + WASM 公共 API 未接 JS
- **一句话**：quality→local more 仅覆盖武器物理/护甲，ES/Evasion/jewel/catalyst quality 缺；WASM 有 wasm-bindgen 但无 wasm-pack 构建/JS 消费契约。
- **证据**：`crates/pobr-core/src/item.rs` `ingest_item()` quality 只处理 Weapon+Physical / Armour+Local；`apps/pobr-wasm/src/lib.rs:17-35` 无 wasm feature 定义/构建脚本。
- **effort**：quality S / WASM M｜**外部阻塞**：否。

### P2-9 · 多主技能 use_time（副技能独立施放速率）
- **一句话**：`base_action_rate` 仅对启发式选中的首个主技能赋值，多主技能 build 副技能无独立 use_time 回退。
- **证据**：`calc_orchestrator.rs:218-230`（`resolve_main_skill` 启发式取首个）、`:141-146`（仅 main_skill 赋速率）。
- **effort**：S（但完整多技能框架留待后续 SkillMechanics）｜**外部阻塞**：否。

---

## P3 — Polish

- **P3-1 · DotDpsCap 溢出无 trace 节点**：`finalize_ailment_dps()` 调非 traced `apply_dot_dps_cap()`，cap 命中时无 TraceNode 记录瓶颈来源。`ailment.rs:1339` vs `perform.rs:545`。effort S。
- **P3-2 · PoE2 常量未命名化**：Overwhelm 穿透 cap `75` 内联于 `offence.rs`（应命名常量）；Poise 阈值表 `MONSTER_AILMENT_THRESHOLD_TABLE` 加载但 final calc 未用。`constants.rs` / `offence.rs`。effort S。
- **P3-3 · Desktop GUI (egui) 未实现**：`apps/pobr-desktop/src/lib.rs`(97 行)仅占位注释，无 egui 依赖。effort L｜**外部阻塞**：否（但 headless CI 不验证 GUI，优先级最低）。
- **P3-4 · 应用层测试覆盖极低**：834 测试中 800+ 为 calc 核心；CLI/WASM/Desktop 合计 ≤10 个琐碎测试，无真实 build e2e、无 WASM JS binding 测试。`apps/*/tests/`。effort M。
- **P3-5 · CLI parse-item/calculate-build 已实现但无 golden 集成测试**：`apps/pobr-cli/tests/calculate_cmd.rs` 仅 1 个小测试。effort S（依赖 P0-1 golden 集）。
- **P3-6 · Minion 等级表硬编码 40 项**：`minion.rs:40-51` MINION_LEVEL_TABLE 固定 40 元素，>40 clamp 行为未对照 PoB2 文档。effort M。

---

## 建议的并行实现编排

### 串行主轴（calc 核心 — 必须按序，共享 perform.rs / ModDb 语义）
单线推进，避免 `perform.rs` / `ModDb` 写入冲突：

```
P0-3 (cross-type call, S)
  └─> P1-2 (异常叠层 Ignite→Shock→Chill)
        └─> P2-2 (effect/rate/cap trace 接线) + P3-1 (cap trace)
P1-1 (Buff/Aura ingest — BuffEffect 基建)
  ├─> P0-4 (minion ally-buff 缩放，复用 BuffEffect)
  └─> P2-3 (trigger rate → ModDb 注入)
P0-2 (resolve_gems per-gem 注入)
  └─> P2-1 (support more tag 隔离) ──> P1-3 (法术召唤物伤害，复用 gem 伤害管线)
```

> **关键约束**：P0-3 / P1-2 / P2-2 / P0-2 / P2-1 全部改 `perform.rs` 与 offence/ailment 计算路径，**不可并行**——按上面箭头串行。Buff 链（P1-1→P0-4）与 ailment 链（P0-3→P1-2）改不同函数，**可双流并行**，但 P1-1 落地后再做 P0-4。

### 可独立并行流（独立 crate / 独立文件，无 calc 核心写入冲突）

| 流 | 内容 | 触碰文件 | 与主轴关系 |
|---|---|---|---|
| **数据管线流** | P2-6 (minion/aura/unique/CostTypes 导出) | `pipeline/config.json`、`tools/pobr-data-adapter/`、`data/` | 完全独立；**应最先启动**（解锁 P1-3/P2-4/P2-5/P1-1 reservation） |
| **Parity/golden 流** | P0-1 (catalog 标定 + golden harness + CI gate) | `devs/fixtures/`、`crates/pobr-build/tests/golden_regression.rs` | 独立（只读 OutputTable）；外部阻塞需先建对照集 |
| **物品/装备流** | P2-7 (第二武器组/JewelSocket/物品防御)、P2-8 (quality) | `pobr-core/src/item.rs`、`item_text.rs`、`xml_build.rs`、`build_data.rs` | 独立于 calc 核心；与主轴并行 |
| **应用层流** | P3-3 (Desktop)、P3-4/P3-5 (测试)、P2-8 (WASM JS) | `apps/pobr-desktop/`、`apps/pobr-wasm/`、`apps/pobr-cli/tests/` | 完全独立；优先级最低 |
| **常量/polish 流** | P3-2 (常量命名化) | `constants.rs`、`offence.rs` | 小改，随手可并 |

### 推荐启动顺序
1. **立即并行启动**：数据管线流（P2-6，解锁最多下游）+ Parity 流（P0-1，外部阻塞需早建对照）+ 物品流（P2-7/P2-8）。
2. **calc 主轴起两条并行子链**：ailment 链（P0-3→P1-2→P2-2）与 buff 链（P1-1→P0-4），各自单线。
3. **数据管线流交付后**：解锁 P1-3 / P2-4 / P2-5 / P1-1(reservation 部分)。
4. **gem 链**（P0-2→P2-1→P1-3）在 ailment/buff 链稳定后接入主轴。

**关键路径（最长串行）**：数据管线流(minion.json, L) → P1-1 buff 基建(L) → P0-2 gem 注入(M) → P2-1 support 隔离(M) → P1-3 法术召唤物(L)。其余流可在此关键路径旁完全并行吸收。

---

## 实施修正（2026-06-06，本次会话落地后回填）

实际动手实现时核对发现 recheck 的若干判断需修正（recheck 是 LLM 综合，有误差）：

### 本次已落地
- ✅ **宝石 DPS 通道**（commit `ae9676f`）：见 doc13 Wave 3 批次2。
- ✅ **P1-2 点燃叠层**（commit `7bd176f`）：`IgniteStacks` 叠层接入，对齐 Bleed/Poison。
  Shock/Chill 是**非伤害 magnitude 异常**，非 DoT 叠层概念——recheck 把它们与 DoT 叠层混为一谈，已澄清不在范围。
- ✅ **P2-6 CostTypes / P2-5 非法力消耗**（commit `53dda8c`）：18 种资源入库，`ResolvedSkillLevel.costs`
  完整解析。注：**PoE2 无 Spirit 消耗**（Spirit 仅用于保留）——recheck 的「Spirit cost」不存在。

### recheck 误判修正
- ❌ **P0-3（cross_type_source_hit 从不调用）= 误判**：实地核对 `perform.rs:447-451` 确实调用，
  结果 `phys_hit/fire_hit/...` 用于 gate 异常计算。该项已实现，无需修复。
- ⚠️ **P0-4（minion ally-buff 未缩放）措辞误导**：`ally_buff_mods` 实为**空 stub**（`minion.rs:473` = `vec![]`），
  即当前**没有任何 ally buff 流动**，而非「全额未缩放」。P0-4 不可独立完成——**依赖 P1-1 先提供 buff 源**。

### 关键结论：P0-2 / P1-1 / P0-4 共享同一 L 级前置 = **通用 SkillStatMap 移植**
recheck 把 P0-2 估为 M、P1-1 拆分估算，但三者实际共享一个尚未存在的基建：
**「技能/辅助/光环效果的任意 stat → ModName/ModType/flags」的通用映射**（PoB `SkillStatMap.lua` 105KB 的端口）。

- 当前 `granted_effect_stat_sets.json` **只入库了伤害值 stat**（min/max，`is_damage_value_stat` 过滤）；
  support 宝石的 `more`/`increased` 倍率 stat、光环的 buff stat **未入库**。
- `calc_orchestrator::damage_stat_to_mod` **只映射 flat 伤害族**；通用 `%`/more/flag stat 无映射。

**下一会话建议的关键路径**（一条主线解锁 P0-2 + P1-1 + P0-4）：
1. 扩展 stat-set 适配器：放宽 `is_damage_value_stat`，入库 support/aura 效果的全部数值 stat（或建 `effect_stats.json` 域）。
2. 移植 PoB `SkillStatMap` 的常用子集为 Rust 映射表（`stat_id → (ModName, ModType, flags/tags)`），覆盖 `damage_+%[_final]`、`X_+%`、reservation、buff flag 等高频族。
3. P0-2：`resolve_gems` 对每个 gem（含 support）解析其 effect 的分等级 stat → 经映射注入，support 按 SkillTypes tag 隔离。
4. P1-1：光环/herald 效果（带 reservation 的 GrantedEffect）→ buff mod 聚合 + `BuffEffectOnSelf` 缩放 → 玩家/召唤物（P0-4 随之解锁）。

### 续：SkillStatMap 移植起步 + P0-2 support 注入（2026-06-06，commit `13d4687`）

- **新基建 `pobr-build::skill_stat_map`**：PoB `Data/SkillStatMap.lua`（vendor 已本地化）伤害族子集移植，
  翻译到 PoBR ModName。flat 基础伤害 BASE、`damage_+%`→INC、`_final`→MORE。保守拒绝未知/条件型前缀。
- **数据**：入库 `damage_+%[_final]` + ConstantStats（support 倍率多在常量层）。SocketGroup.gem_skills
  捕获每个宝石 skillId（xml_build）；BuildData::effect_stats（active/support 通用）。
- **P0-2 核心达成**：support 宝石伤害 inc/more/附加 → 注入被支援技能 → DPS。Fireball+FerociousRoar
  (damage_+% 129) 验证击中 ×2.29。
- **剩余**：area/speed/crit/抗性 等**非伤害族**的 SkillStatMap 映射；多技能 **tag 隔离**（当前全局作用域，
  单主技能正确）；support **mana multiplier**（P2-1）。**P1-1 buff/aura 可复用此基建**（光环效果 stat →
  map_skill_stat + reservation/BuffEffect），是下一主线。
