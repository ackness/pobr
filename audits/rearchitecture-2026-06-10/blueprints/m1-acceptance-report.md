# M1 阶段验收报告（W-J 联合收尾）

> 蓝图 `m1-skills-gems.md` §4「阶段整体验收」1–5 逐项核对 + §6 Q3 影响面实测。
> 验收时点：2026-06-11，分支 `m1/skills-gems`，HEAD = W-J 行为接线 commit（47bee7b）。
> 门禁实测：`cargo fmt --check` / `clippy --workspace --all-targets -D warnings` /
> `cargo test --workspace`（1218 passed / 3 ignored）全绿。

## 0. 结论摘要

| # | 验收项（蓝图 §4 原文口径） | 结果 |
|---|---|---|
| 1 | 进攻 ≥40%@5% 且防御 ≥111/117 不降 | **进攻未达标**（22/80 = 27.5%@5%；@10% = 40.0%）；防御达标且上调锁棘轮（114/144@5%、120/144@10% ≥ 旧基线 111/117） |
| 2 | quality-20 宝石 fixture 进 CI（常跑，非 ignore） | ✅ 达标 |
| 3 | statmap 双跑 diff 报告干净 + 切换日志归档 | ✅ 达标 |
| 4 | oracle 对拍 statmap 抽样 ≥50 条全过 | ✅ 达标（71/71） |
| 5 | `skill_stat_map.rs` 与 `is_mappable_stat`（含消费侧兜底）删除、grep 零命中 | ✅ 达标 |

## 1. 进攻 parity（未达标项，缺口与归属）

ninja_parity 实测（18 build，tol 5%）：

```
defensive parity: 114/144 = 79.2% @5%  |  120/144 = 83.3% @10%
offensive parity:  22/80 = 27.5% @5%  |   32/80 = 40.0% @10%
```

防御两档均高于旧基线（111/117）且已锁棘轮（`ninja_parity.rs` BASELINE 常量
114/120，commit 02bbf58）。进攻 @5% 距 ≥40%（≥32/80）缺 10 个命中；@10% 已到
40.0%。**根因**：T2.4 statmap 切换把 Legacy 后缀启发式的「过算抵消欠算」假命中
换成对齐 PoB2 的真实欠算（baseline commit 02bbf58 已显式审查 OFF_HIT5 23→22 的
deadeye 例外；L2 报告 8 个 build 的全部偏移逐条核为"修对"，见
`m1-statmap-switch-log.md` §3）。剩余缺口按补偿清单归属（切换日志 §5 同口径）：

| 缺口 | 量级/例证 | 归属 |
|---|---|---|
| effective 模式 DistanceRamp / `cfg.skillDist`（Close Combat ×1.30 等） | flicker / shield-wall 两 build dps −23% | M3 config_interpreter（skillDist 注入） |
| GlobalEffect tag（buff 域）整族——含 W-J 未选 set global mod、DemonForm/PainOffering 施法速度 buff、Spell Cascade 之外的 per-set buff 条目 | L1 统计：global 22 条 + per-set 133 条带 GlobalEffect | M3 buff_pass（接通后 W-J 通道自动生效，见 §3） |
| `unknown_mod_name` 386（AilmentMagnitude / AreaOfEffect / Duration / EnemyIgniteChance 族） | L1 Unsupported 最大桶 | M4 进攻深化（引擎直通表逐名扩展） |
| MinionModifier LIST 族 | L1 mod_type 60 条 | M5a |
| KeywordFlag TOTEM / WARCRY 位未移植 | UrgentTotems 等 | 后续（pobr-data 位扩随 M4 ModFlags 30 位一并） |
| scalar（checkForScalarMultiplier 反查 mod_db） | Ferocity 等 | M4（mod_db 反查接入后解除固定 1.0） |

roadmap 口径下该缺口在 M3（目标进攻 ≥55%）/M4（≥70%）路径上收敛；本阶段
**不回滚正确行为换命中数**（蓝图 §5 风险表既定纪律）。

## 2. 达标项实证

- **quality-20 fixture（§4-2）**：`crates/pobr-build/tests/gem_quality.rs` 三用例
  全部常跑（无 `#[ignore]`）：trunc 语义单测（rate=0.55,q19→10；负斜率 toward
  zero）、q20 品质段 PoB2 oracle 对拍（Comet/Spark/ArcticArmour golden，
  `quality_stats.lua` 实跑记录在模块头）、stormweaver-comet 15×q20 端到端双跑
  方向断言。`cargo test --workspace` 即覆盖。
- **statmap 双跑 + 切换日志（§4-3）**：`m1-statmap-switch-log.md`（T2b 终版
  f3e2302 + T2.4 终稿 a574028）归档：L1 2437 行分类（`legacy_only` 39 行逐条附
  PoB2 依据核为误映射/超映射）、L2 18 build 全 review（10 个逐字段一致、8 个
  偏移全部"修对"且有据）；切换 commit bf71975 + baseline 独立 commit 02bbf58 +
  纯删除 commit 0c634b4 三段式完成，回退路径（revert 删除 commit）可用；
  `StatMapMode::Compare` 按 §6 Q4 裁决保留为长期观测框架。
- **oracle 抽样（§4-4）**：71/71 PASS（global 59 + per-set 12，探针值 240；
  桶覆盖 div/mult/base/value/multi-mod/Condition/ActorCondition/Multiplier+
  PerStat/flags/conversion/skill_data/per-set），对拍法 = vendor
  `calcs.mergeSkillInstanceMods` 真实 merge 输出经翻译层归一后多重集比较。
  重跑命令见切换日志 §4。
- **删除核验（§4-5）**：`grep -rn is_mappable_stat --include=*.rs` 零命中；
  `skill_stat_map.rs`（751 行）与 `legacy_stat_filter.rs` 已删（0c634b4）。

## 3. W-J 本体交付（未选 set global-only merge）

PoB2 依据 `Modules/CalcActiveSkill.lua`：`isGlobalEffect`（:68-80）、
`mergeSkillInstanceMods` 的 onlyGlobals 路径（:92-141，调用 :124-129、注入条件
:103-107）。

| commit | 内容 |
|---|---|
| c17b668 | pobr-core 引擎：`is_global_effect` / `stat_has_global_mods`（selectedGlobalStats 记账探针）/ `map_stat_global_only`（仅 global modOrGroup 参与 merge；非 global 静默跳过）+ 双 set 语义单测（蓝图 §4 W-J 门禁用例） |
| 9820fac | pobr-build 取数：`BuildData::unselected_set_stats`（buildSkillInstanceStats 表语义：品质逐 set 叠加 trunc + 同 stat 加法合并；vendor 未导出 set 剔除） |
| 47bee7b | 行为接线：编排点 `unselected_set_global_modifiers`（归因 `skill.<效果>.set<k>.<stat>`）+ 选中 set 覆盖键 `selected_set_key` 接进引擎 `set_key`（T2.4 移交项收口） |

**第一批边界（显式登记）**：`GlobalEffect` tag 本身仍在引擎 tag 翻译边界外
（buff 需经 buff effect 缩放与启用条件求值，直接注入=错算）→ 当前未选 set
通道注入恒为零（Unsupported 上报可观测）；M3 buff_pass 接通该 tag 后本通道
**自动**开始产出注入项，无需再改 W-J 代码。ninja 聚合与 golden 逐值持平已实测，
故**无 baseline bump**。

### Q3 影响面实测（蓝图 §6 Q3 要求的统计）

对 18 个 ninja build 的 decoded.xml 宝石清单 × 数据包逐项扫描：

- 多 statSet（vendor 导出 ≥2 set）宝石实例：**53 个**（distinct effect 31 个）；
- 其中**未选 set 含 global-mod stat** 的命中：**4 处 / 3 个 effect**——
  FlameWall set2（投射物附加火/闪电伤 buff；stormweaver-comet 与 shield-wall
  两 build）、OilGrenade set2（油地面曝露）、VineArrow set2（藤蔓减速 debuff）；
- 4 处全部是 GlobalEffect Buff/Debuff 域条目 ⇒ 即使 M1 提前接线数值也为零。
  **Q3「按序执行」裁决正确**，无需提前 W-J；M3 接通后该 4 处自动生效。

### 新增缺口登记（W-J 实施中发现）

- **多 set support 的全 set 全量 merge**：vendor 对 support 效果不传 statSet
  （`CalcActiveSkill.lua:130`，`statSet and {statSet} or grantedEffect.statSets`
  ⇒ 全部 set 全量 merge）；PoBR `support_modifiers` 当前只取主 set。18 build
  无多 set support 载体（上表 53 实例均主动技能），无 parity 影响；建议归属
  M4 进攻深化（与 support 数值面同批），或 M3 buff 域接通时顺带核对。

## 4. 统一门禁三件套核对（roadmap §0）

1. fmt + clippy(-D warnings) + test --workspace：全绿（本报告时点实测）；
2. ninja_parity 18-build 零回归：防御 114/120、进攻 22/32 = 当前 baseline
   常量逐值持平（T2.4 的 23→22 例外已在 02bbf58 独立 commit 显式审查）；
3. 解析/数据面：本阶段 oracle 对拍（quality golden + statmap 71 条）齐备；
   W-J 各 commit 无 regen 产物变更。

## 5. 补刀波（2026-06-11）：逐 build 偏差归因 + 根因登记

> 方法：ninja_parity --nocapture 逐字段 diff × pob2-oracle headless 中间值对拍
> （druid-oracle-comet / coiling-bolts / ice-shot / flicker-strike / DD 五个最差
> build），把进攻偏差拆解为「等级解析 / inc-more 桶 / crit 链 / 配置条件」四类。

### 5.1 已修复（M1 范围内，commit 5961e28）

**宝石等级类别匹配缺口**：`+N to Level of all <category> Skills` 旧实现只支持
单 token 类别，多词类别（`Cold Spell`/`Physical Spell`/`Melee`×多词变体）与
技能名类别（`Shield Wall`/`Ember Fusillade`）整串不匹配被静默丢弃。对齐 PoB2
`ModParser.lua:3480-3496`（gemIdLookup 命中 → 名字匹配；多词 → keywordList）+
`CalcSetup.lua:404-435 applyGemMods`（keywordList 逐 token `gemIsType`，全中
才生效）。5 个 build 的 DPS 向 golden 收敛（comet 0.23x→0.46x、DD 0.05x→0.09x、
双 shield-wall 0.39/0.47x→0.49/0.55x、ember 0.12x→0.18x），hit 计数不变
（深坑未越 5% 阈），零回归。

附带暴露：`cost_multiplier.rs` oracle 锚 301 实为 `floor(232×1.3)`（ER 被
require `GeneratesEnergy` 拒收）与旧注释 `floor(211×1.43)` 的数值巧合；已按
实际链路重锚 479 并在测试 doc 登记 PoB2 golden 577 的余差构成。

### 5.2 根因登记（超出 M1 范围，记录不修）

| 根因 | 证据（oracle/vendor） | 受影响 build/字段 | 归属 |
|---|---|---|---|
| 敌人配置条件链（Critical Weakness 20 层 → enemy `SelfCritChance` BASE +10；cursed-enemy per-curse gain-as ×5 咒；conditionCritRecently 等速度条件） | `ConfigOptions.lua:1889-1894`；coiling 的 `DamageGainAs_Physical=150` = 30%×5 咒（item 词条 per Curse on target） | comet/frost-bomb/coiling/essence-drain 等 CritChance 全线低估；coiling DPS 0.10x 的主因 | M3（config/敌人状态） |
| mode_effective 专属 crit 机制：`CritChanceLucky`（lucky 二掷）、`InevitableCriticalHits`（CritChance 置 100 + 折算 mult，`CalcOffence.lua:3618-3725`） | comet oracle PreEffective 69.92 → final 100 | druid 双 build、frost-bomb 的 CritChance=100 与 CritMultiplier 偏差 | M4（offence 深化） |
| 特殊技能 skill_data：corpse 爆炸 `corpse_explosion_monster_life_permillage_physical`（DD 主伤害来源，statmap 引擎 Unsupported:skill_data_key 上报中） | L2 运行时 outcome 观测 | DD TotalDPS 0.09x（最大单点缺口） | M5b（特殊 statdesc） |
| meta gem 二段 support 形态（Spellslinger → `SupportSpellslingerPlayer`，GemEffects `SecondarySupportName` 通道）+ 能量链（ER/Boundless 的 manaMultiplier 取舍） | oracle skillInfo.supports 含 `SupportSpellslingerPlayer L18`；vendor `act_int.lua:19984-19998` | comet 组 support 裁决/ cost 链余差 | M1 验收已登记缺口（T5 延伸），实施归 M4 前置 |
| DistanceRamp tag（Close Combat II）：核实 PoB2 在 ninja 夹具同环境下 `skillDist=nil`（`enemyDistance` 仅 Placeholder，`CalcActiveSkill.lua:629/642` 只读 configInput）→ PoB2 同样跳过该 mod；PoBR Unsupported:tag 与之同口径，**非缺口** | `ModStore.lua:557-573` | flicker 等近战（无 parity 影响） | 无需修 |

### 5.3 门禁状态（补刀波收尾时点）

fmt --check / clippy -D warnings / test --workspace（1221 passed）全绿；
ninja_parity：防御 114/144=79.2%@5%、进攻 22/80=27.5%@5%（= baseline 常量，
零回归）。M1 进攻 ≥40%@5% 验收目标仍未达成；按上表归因，剩余大头在
M3 配置条件链与 M4 crit/特殊机制，不在 M1 技能/宝石数据链路内。
