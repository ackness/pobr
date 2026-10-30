<a id="calcs-display-comparison"></a>

# Calcs 显示对比记录

初次源码核对：2026-09-25。PoB2 固定为
`ce566eac45ea8a86477f513c7ee65a1ebe60014e`，当时 PoBR 活动数据为 `4.5.5.2`。
下文还保留 2026-10-12 随 Calcs 明细实现补充的内容（最后补充提交 `24bf4060`）。
这些源码路径、18 个 fixture 的数量和覆盖结果属于当时记录，不能当作当前界面验收。
现行字段见 [calc-outputs.md](calc-outputs.md)，界面行为见 [web-contracts.md](web-contracts.md)。

本次是**源码与接口对比**，没有运行 PoB2 桌面界面。文末同构筑截图核对仍未完成；
计算测试通过不能代替这项检查。

<a id="observable-differences"></a>

## 可观察差异

| 项目 | 当时的 PoB2 | 当时的 PoBR | 比较前需要对齐 |
|------|-------------|-------------|----------------|
| 技能选择 | Calcs 独立选择技能组、主动效果、part（`CalcsTab.lua:42–101`），MAIN / CALCS 分开建环境（`:494–512`） | 使用会话所选主技能组，点击分组 DPS 行会切换主技能 | 组、主动效果序号、stat set、武器组、part；导入的 `statSetIndexCalcs` 不构成独立选择 |
| Buff 模式 | UNBUFFED / BUFFED / COMBAT / EFFECTIVE（`CalcsTab.lua:12–16,165–166`） | 使用会话配置，没有 Calcs 专用模式 | `mode_buffs`、`mode_combat`、`mode_effective` 与敌人输入 |
| 角色 | 可选召唤物属性（`CalcsTab.lua:101–103,434`） | 使用返回的 display catalog，没有同等独立角色选择器 | 召唤物摘要不能证明逐召唤物计算一致 |
| MORE | 按公式给出倍率分解 | 显示各 MORE 行，`more_total` 为该属性名称集合中生效行的乘积 | 行集合以外的倍率，例如动作速度，若有公式会单独列步骤，不计入该 `more_total` |
| 伤害口径 | Hit、DoT、Combined 各有含义 | `main_skill.hit_dps = output.dps`，`dot_dps = total_dot_dps`，`combined_dps = combined_dps`；命中分量为玩家侧、未暴击、敌方减免前数值 | `TotalDPS` 是命中 DPS，不自动等于 Combined 或全部技能 DPS |
| 独立技能摘要 | 主技能与 Full DPS 配置用途不同 | 逐个计算主动效果，包括辅助授予效果，不要求先选主技能；选行后检查该具体效果 | 不能把所有行相加当作可持续循环；功能技能零值保留，不可计算时为 null 并附诊断 |
| 来源贡献 | 词条 / 公式明细不等于移除来源后重算 | `analysis.rs::attribution_impl` 分别移除装备、禁用技能组、移除 utility 槽，再算“基线 − 变体” | 这是移除后的边际变化，不能相加当作份额，也不覆盖全部天赋 / 珠宝；非线性作用使百分比不必合计 100% |
| 格式 | 按字段显示 | 侧栏使用 `statDisplay.ts`；Calcs 分组用 `float2`，≥1000 四舍五入成整数，原始行最多两位小数，null / 非有限值显示为无穷大 | 先比较未舍入 JSON，再判断是否为公式错误 |

武器组的具体编排见 `docs/web-contracts.md`。
技能组贡献标签按保存顺序列出宝石，已知名用本地化名称，无法识别的槽保留来源名称。
变体移除整个组，不只是第一颗或当前主动宝石；标签不保证每颗宝石都启用或参与计算。

### 词条明细怎样对应计算

PoB2 按公式在 `Modules/CalcSections.lua` 组织明细。
当时 PoBR 的 `calculate.rs::BREAKDOWN_MOD_NAMES` 固定列出可展开的显示 stat id。
`CalculationSession::stat_mod_names` 把每个 stat 映射到计算实际读取的 modifier 名称：

- Life / Mana：`MaximumLife` / `MaximumMana`。
- 抗性：两种名称拼写，以及共享的 `ElementalResist`。
- `ActionRate`：所查技能使用的速度名称。
- 暴击：`CriticalStrikeChance` / `CriticalStrikeMultiplier`。
- `HitChance`：`Accuracy`。
- Armour / Evasion / Energy Shield：自身名称，以及共享的 `ArmourAndEvasion` / `Defences`。

`breakdown_for` 通过 `CalculationSession::evaluated_stat_mods` 列出该名称集合的全部已存词条。
每行有 `active`；生效的 BASE / INC / MORE 行还带实际应用值，包含按属性缩放与共享上限。
`base_total`、`inc_total`、`more_total` 从同一集合的一次 `sum` / `more` 查询得到，
只计算生效行。通常使用会话配置；`ActionRate`、`CritChance`、`CritMultiplier`、
`Accuracy`、`HitChance` 使用主手 pass 的配置，使武器本地词条与实际计算一致。

每份明细可带 `formula`：从起点逐步算到显示值，每步保留运算数与当前结果。
`CalculationSession::stat_formula` 从计算阶段保留的中间项生成步骤；
仅当最终值等于显示值时才返回公式。即使没有词条行，只要有公式也返回明细，
例如武器基础暴击率或技能基础频率。

| 范围 | 步骤 id | 计算来源 |
|------|---------|----------|
| 基础资源、抗性、速度等 | `base`、`flat`、`enemy_flat`、`converted`、`increased`、`more`、`added_time`、`action_speed`、`override`、`cap`、`cooldown_cap`、`server_tick_cap`、`floor`、`bonus_to_multiplier` | `offence::pool_terms`、`resolve_resistance`、`action_rate_terms` |
| 防御与 Spirit | `gear_adjusted`、`gained`、`slot_scaled`、`flat_after`、`round` | `defence::defence_resource_terms`、`defence_panels::spirit_terms` |
| 暴击 | `hit_chance`、`lucky`、`bifurcate`、`inevitable`、`inevitable_penalty`、`bifurcate_bonus`、`enemy_increased` | `crit::crit_chance_terms` / `adjust_crit_chance` / `crit_multiplier_terms` |
| 命中率 | `accuracy_rating`、`vs_evasion`、`always_hits`、`enemy_block` | `offence::accuracy_terms` / `hit_chance_terms` |

暴击另有 `enemy_increase_added`（把敌方 increase 加到玩家 increase）、`never_crit`、
`no_crit_bonus`。Armour / Evasion / Energy Shield 的 `slots` 按装备槽展开
`slot_scaled`（`defence::DefenceSlotPart`）。

### 平均命中与命中 DPS

`TotalHitAvg`、`TotalDPS` 有公式步骤，没有顶层 modifier 行。

`TotalHitAvg.damage` 按伤害类型列出：增幅前范围（武器 / 技能基础、附加、转换和额外获得）、
INC 总和、MORE 乘积、非暴击范围、非暴击与暴击均值。
来自 `offence::hit_pass`、`damage::calculate_component_terms`，经
`CalculationSession::hit_damage_terms` 读取。
公式先 `non_crit_hit`（各行之和），再 `crit_hits`（按暴击率加权暴击列）。

每类型的 `mods` 与表级 `shared_mods` 来自 `damage::aggregate_inc_more`，
经 `CalculationSession::hit_damage_mods` 在非暴击命中配置下求值。
所有类型共有的 `Damage`、attack / spell / projectile / area / melee damage 和技能关键字伤害
只列一次；每类型另列自己的伤害名称，以及适用的 `ElementalDamage`。
生效 INC 行之和等于该行 increase，MORE 行乘积等于该行倍率；
MORE 要考虑计算按 modifier 名称保留两位小数的舍入。

`damage.sources` 还按类型保留转换 / 额外获得前范围、附加伤害总量与效用及其词条，
以及把该类型转换或按比例额外获得其他伤害的行（`damage::source_terms` /
`evaluate_source_lines`）。每行还列出 `more_min` / `more_max` 背后的 MORE 词条。
它没有逐条量化每个转换 / 额外获得词条搬移了多少伤害，只显示前后范围。

`TotalDPS` 步骤为 `average_hit` → `enemy_mitigation` → `uses_per_second` →
`chance_to_hit` → `dps_multiplier`。
`enemy_mitigation` 把敌方抗性、护甲和承受伤害合成一个倍率。
其 `mitigation` 按伤害类型展开：平均命中的份额、敌人抗性、非暴击减免、承受伤害倍率、
最终倍率和经过敌方结算后的份额。元素等伤害使用穿透后抗性；物理使用护甲、固定减伤和 Overwhelm。
来自 `offence::enemy_mitigation_terms`、`CritPassOutput::mitigated_avg`，经
`CalculationSession::hit_mitigation_terms` 读取；各行结果之和等于 `enemy_mitigation` 步骤结果。

`offence::enemy_mitigation_lines` 提供敌人侧抗性词条（含 exposure、诅咒）、
物理的护甲 / 固定减伤、承受伤害词条，以及视抗性为反转值的命中比例。
它不列玩家的穿透、Overwhelm 词条或敌方 `IgnoreArmour`。
物理减免展示的是非暴击部分；暴击更大，护甲减免较少，行级最终倍率仍覆盖两者。

### 持续伤害与 Combined DPS

`TotalDotDPS`、`CombinedDPS` 有步骤，没有自己的 modifier 行。
`skill_dot::skill_dot_terms` 经 `perform::skill_dot_terms` 提供已算中间项：

- `TotalDotDPS`：`skill_dot`（技能自身持续伤害）＋ `poison_dps` ＋ `ignite_dps` ＋
  `bleed_dps`，最后受 `cap` 限制。
- `CombinedDPS`：`hit_dps` ＋ `dot_dps`。

`TotalDotDPS.skill_dot` 按类型列出基础每秒伤害、INC、MORE、持续伤害倍率、光环效果、
敌方倍率和结果；并列该类型在 DoT 配置下读取的 INC / MORE 伤害行及 BASE 倍率行。
读取入口是 `CalculationSession::skill_dot_parts` / `skill_dot_mods`。
侧栏 Hit / DoT / Combined DPS 均可打开对应明细。

### 点燃、流血与中毒

`IgniteDPS`、`BleedDPS`、`PoisonDPS` 在各异常自己的配置下列 `AilmentMagnitude` 行。
步骤来自 `ailment::AilmentTerms`，由 `perform` 存入 `Env::ailment_terms`，再经
`CalculationSession::ailment_stage_terms` 返回：

`ailment_source`（按非暴击 / 暴击加权的施加命中）→ `per_second`（单实例每秒伤害占来源比例）→
`increased` / `more` → `enemy_mitigation` → `active_stacks`（同时生效实例数；
技能没有命中频率时用 `chance_to_apply`）→ `ailment_effect` → `ailment_rate` → `cap`。

`ailment` 对象解释这些步骤的输入：非暴击与暴击来源命中、暴击施加的份额、伤害 roll，
以及实例估计的输入（命中率 × 施加率 × 持续时间 × 每秒命中）和同时造成伤害的数量上限。
`source` 按可施加该异常的伤害类型列来源，各行合计分别等于两个来源总值。
另保留基础持续时间、敌人 expiry 倍率，以及玩家侧施加率、持续时间、效果、
更快 / 更慢伤害词条（`CalculationSession::ailment_lines` 使用异常阶段的名称列表）。

不列敌方增加施加率或速率的 `Self<Ailment>Chance`、`Self<Ailment>Faster` 词条；
敌方 expiry rate 只作为一个倍率展示。

### 双武器与装填

有两次武器 pass 时，各自计算的 `ActionRate`、`HitChance`、`CritChance`、
`CritMultiplier`、`TotalHitAvg`、`TotalDPS` 及双手都能施加的异常会带 `hands`。
每手保存自己的总值、行、步骤和表；顶层公式按阶段原有规则合并
（`hand_pass::combined_steps`）：`main_hand` 后接 `off_hand_average` /
`off_hand_added` / `off_hand_either` / `off_hand_alternating`。
异常使用 `ailment::HandAilmentMerge` 的 `stronger_hand`、`stronger_share`、`weaker_hand`。
顶层词条仍是主手的，各手词条在 `hands` 下。

需要装填的弩会给 `ActionRate` 与 `TotalDPS` 添加 `reload` 步骤。
`reload` 对象保留射击频率、弹数、射击时间、装填时间和最终频率。

### 当时 18 个 fixture 实际覆盖了什么

以下数字属于上述补充记录，没有在本文改写时重测：

- Life、Mana、四种抗性、Energy Shield、Armour、Evasion、Spirit、`ActionRate`、
  `CritChance`、`CritMultiplier`、`Accuracy`、`HitChance`：有明细的都带公式。
  Accuracy 是主手 pass 值。防御 `flat` 含装备值，槽位限定增幅另拆 13 行，
  分布在九个构筑的 11 份明细中。
- 16 个主技能会命中的构筑均有 `TotalHitAvg` / `TotalDPS` 解释。
  这 16 个都存在转换或额外获得伤害，共列 379 条生效来源行；敌方行共 76 条。
- 没有 fixture 用双武器攻击，也没有导入的主技能装填。
  两次 pass 和 reload 由 `contract_golden.rs` 的合成构筑验证：两把单手锤、手雷构筑的弩射击。
- 17 个造成伤害的构筑均有 `CombinedDPS` / `TotalDotDPS` 解释。
  召唤物构筑没有这些值：它们不含召唤物伤害，PoB2 的玩家侧对应值也为 0。
  需要召唤物自身输出和技能数据；当时召唤物阶段未计算这部分。
- 所有会造成点燃 / 流血 / 中毒的构筑都有解释，分别为 16 / 2 / 1 个。
  19 份异常明细中，有 42 条施加率、持续时间、效果、速率生效行，以及 21 条来源行。
  只有一个 fixture 的技能自身有持续伤害。
- 没有移动速度明细，因为引擎当时不计算移动速度输出。
  普通 modifier 总值与 PoB2 `EffectiveMovementSpeedMod` 在 18 个中有 17 个不同。
- 步骤不能重建显示值时 `formula` 为 `null`，当时 fixture 未出现这种情况。
  无公式时页面显示 BASE / INC / MORE 总值，并提示可能还有其他效果。

当时的 PoBR 源码定位：

- `web/src/components/calcs/CalcsPanel.tsx`：分组显示、技能组选择、移除边际值和过期结果处理。
- `apps/pobr-wasm/src/build_api/calculate.rs`：明细字段、白名单、主技能伤害口径。
- `apps/pobr-wasm/src/build_api/analysis.rs`：移除变体和差值方向。
- `crates/pobr-core/src/calc/session.rs::evaluated_mods_named` / `named_totals`：
  按计算上下文求值的分组行与总值。
- `crates/pobr-build/src/build.rs::GemSkillRef`：导入的 stat-set 边界。
- `web/src/lib/statDisplay.ts`：格式与 null 处理。

<a id="remaining-gui-acceptance"></a>

## 尚未完成的界面验收

选择已提交的攻击、法术 / DoT、召唤物构筑各一个，使用同一受支持数据 / vendor 版本。
记录精确 fixture、配置、武器组、所选组 / 序号 / stat set 和 buff 模式，
保存两侧 Calcs 截图及对应未舍入数值。
检查 Life / Mana / ES、抗性、命中率、暴击、动作频率、命中 DPS、DoT、Combined DPS，
各展开一条条件词条和 MORE 词条。
把差异分为选择、角色、公式支持、原始 / 已求值明细或显示舍入。
只有这些界面证据才能完成这项验收。
