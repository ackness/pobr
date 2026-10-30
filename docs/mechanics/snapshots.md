# 机制约定：配置输入与事件快照

“快照”是用户明确声明的当前状态或一次事件，例如这次攻击已经获得祖灵增幅、此前用药留下多少 Guard。引擎按该状态求结果，不自动补出之前发生了什么。本文列出配置读取方式及事件视图，索引见[机制约定](README.md)。

## 配置项与敌人配置

有 handler id 不代表配置已经影响结果。几个已有入口如下：

- 破甲由 `calc::armour_break::apply_armour_break` 处理。有效模式下，`conditionEnemyArmourBroken` 开启后扣掉 `multiplierArmourBreak`，未填则全部扣掉；允许破到负数时下限为原护甲的相反数。不开状态时，残留数值无效。
- `overrideBuffBlinded` / `overrideBuffElusive` 是百分比，不能超过角色可施加／获得的效果（`calc::blind`、`rules::buff_expander`）。
- Inexorable Critical 用 `SecondsSinceInevitableCrit` 乘每秒加成，受 `support_inevitable_criticals_*_cap` 限制；不足 4 秒另设 `CritRecently`（`calc::crit::apply_inexorable_critical`）。
- Dark Pact 的 `darkPactSkeletonLife` 乘 `skeletal_chains_aoe_%_health_dealt_as_chaos_damage`，得到基础混沌伤害。Herald of Ash 的 `hoaOverkill` 乘 `herald_of_ash_burning_%_overkill_damage_per_minute`，得到火焰 DoT（`calc::configured_damage`）。

`calc_orchestrator::enemy_config::modifiers` 用 OVERRIDE 覆盖敌人等级／类别预设的五种伤害、三种元素穿透、暴击率、暴击伤害和 `enemyPhysicalOverwhelm`。显式 0 也覆盖，负数按零，非有限值忽略。来源是 `EnemyConfig`、`config.<输入名>`；被覆盖预设仍留在 `CalculationSession::enemy_hit_mods` 的 EHP／最大承伤明细中，但不计入。

`enemyDamageType` 选择 Average、无类型、近战、投射物、法术、法术投射物命中，供闪避、格挡和承伤上下文使用（`calc::taken::HitCategory`）。DoT 不是命中。`EHPUnluckyWorstOf` 取 1／2／4 次中最差的防御概率估计。`enemySpeed` 只在 Energy 格挡事件没有来袭频率时回退，不是 EHP 通用倍率；`enemyDamageRollRange` 与持续伤害防御视图仍无读取者。

敌人等级由 `skill::mods::resolved_enemy_level` 统一解析，尸体生命和减免共用。优先级为调用方显式值、构筑码 `enemyLevel` Input、Placeholder；均缺失时，普通怪／普通首领跟随角色等级，Pinnacle/Uber 默认 82。最后限制在该档最低等级与 85 之间。尸体生命表在 82／85 级为 32956／36012；Detonate Dead 的 `corpse_explosion_monster_life_permillage_physical = 107` 表示 10.7%。

### 敌人类型、近期消耗与计数

近期合并的敌人类型配置为 `conditionEnemyUndead`、`conditionEnemyDemon`、`conditionEnemyHumanoid`、`conditionEnemyConstruct`、`conditionEnemyBeast`。引擎不从首领档位猜类型；用户声明对应类型后，针对该类型的伤害、暴击率、定身累积或异常时长词条才生效。目录仅在构筑有相关词条时显示选项。

`multiplierEnduranceChargesConsumedRecently` 表示近期已经消耗的耐力球数量，不是当前持球数或最大球数。[Recently 的游戏定义](https://poe2db.tw/us/Recently)是过去 4 秒。活动树节点 1865 / 15494 / 54934 每球增加 3% 火伤，[Flare（43584）](https://poe2db.tw/us/Flare)每球额外获得 2% 火伤；输入 4 分别给 12% 和 8%；缺失／零没有收益。游戏天赋文本与 stat 是来源，配置只是显式状态；不推算获球、消耗频率或再次触发。

实现位于 `overlay-common/enemy_type_and_consumption_config_options.json` 和 `special_mods.json`，新规则为 `exporter: "absent"`。回归见 [passive_tree_coverage.rs](../../crates/pobr-build/tests/skills/passive_tree_coverage.rs)。`multiplier<X>` 的显式 Input 只计一次：已经从 XML 写入 multiplier 表时，不再重复加上 Combat/Effective 标签对应的配置效果。Placeholder 默认值仍按原路径补入；例如近期异常数输入 4，不会变成 8。

### 识别但没有事件模型的词条

当前 `special_mods.json` 已识别多类事件词条，并标为 `batch: "unmodelled"`：Glory 生成、元素灌注持续／拾取、Seal 获得、Puppet Master、Volatility 获得／爆炸延迟、破甲时长／承受破甲、Offering 生命、尸体消耗、护符使用、点燃获怒、充能球获得／消耗、怒气保留／流失时序、法术命中 Hinder 几率／时长、招架时状态、普通 Slam 余震附加及分叉额外投射物等。

识别后它们可离开“无法解析”列表，但仍留在逐行台账和缺口队列。下文某个显式事件视图已支持，不代表整个事件家族都已实现；例如 Volatility 恢复视图不模拟获层过程。也不能因为静态计算暂时不需要，就把它们改成 `no_effect` / S0。

## 祖灵增幅

`calc/session/preparation::bridge_ancestral_boost` 在启用 buff 组装后解出一个共享状态。[游戏关键词](https://poe2db.tw/us/Keywords)规定增幅 Strike 多打两个目标，增幅 Slam 得 30% more 伤害和 25% increased 范围。原生 `GrantedEffectStatSets` 给 Ancestral Call I/II/III 的 `ancestral_call_spirit_strike_interval_ms = 5000/3000/1000`，Fist of War I/II/III 的 `ancestral_slam_interval_duration = 6000/4000/14000`，两者单位都是毫秒。

`double_ancestral_boost_effect` 只把增幅奖励翻一次倍，即 Slam 的 60% more 伤害、50% increased 范围，不把整次攻击或辅助词条再翻倍。[Call III](https://poe2db.tw/us/Ancestral_Call_III) 的 `support_ancestral_call_damage_+%_final` 是无条件 30% less，未增幅攻击也受它影响。增幅时的伤害／范围天赋读取同一状态，不因装备了增幅辅助就常驻。

游戏口径默认未增幅。`conditionAncestrallyBoosted=true` 或 `warcryMode=MAX` 只在存在来源时选择一次增幅攻击。也可填 `multiplierAncestralBoostElapsedSeconds`，表示该辅助上次增幅此技能后经过几秒；不足原生间隔则未准备好，达到间隔则下一次可增幅。计时优先于 MAX，负数／非有限值不算准备好；显式 false 即使有 Cry 也选择未增幅。

这些仍是原始 API 输入：XML 可写 `<Input name="conditionAncestrallyBoosted" boolean="true"/>`；JSON 用 `config_inputs: {"conditionAncestrallyBoosted": true, "multiplierAncestralBoostElapsedSeconds": 5}`。原始 JSON 覆盖导入 XML；没有原始覆盖时，原生调用方可写 `Build.config.conditions["AncestrallyBoosted"]` / `["AncestralCryActive"]` 和 `multipliers["AncestralBoostElapsedSeconds"]`。读取进入配置诊断，但暂无专用 Web/catalog 项或新标签，现有 Max Hit 控件仍可用。

这些输入只描述一次攻击，不能解释轮转平均 DPS。引擎不猜初次是否就绪、冷却恢复缩放、消耗频率或覆盖率；`AVERAGE` 仍保持未增幅，不表示已经求得平均轮转。

[Ancestral Cry](https://poe2db.tw/us/Ancestral_Cry) 还需 `conditionAncestralCryActive=true`，即用户声明其 buff 当前激活。`ancestral_cry_empowered_attacks_strike_X_additional_enemies` 给相同的两目标奖励，技能文字将增幅给近战 Strike 和 Slam；Call/Fist/Cry 共用一次增幅，不相乘。

`ancestral_cry_non_skill_physical_to_fire_convert_%` 通过非技能转换把该增幅攻击 100% 物理转火。Advanced Thaumaturgy 开启的额外品质在同一 buff 状态给全局火伤；普通品质只延长每个消耗球带来的时长。触发攻击（含 Volcanic Steps/Eruption）、图腾／代理、无关技能不继承玩家使用增幅。八秒原生时长、每个可移除耐力球 50% more 时长、Glory、消耗与火山触发均不代表自动覆盖；没有时序实测。

`ancestral_boost_snapshots=false`（由 `on_reference_basis` 选择）保留原有受辅助 Strike 快照；新 Cry/Call III 规则为 `absent`，新 Slam 奖励只在游戏快照启用。参考兼容与游戏口径分别验证。

## Archon

Archon 共享奖励要求 `conditionAffectedByArchon`，默认关闭；有来源不代表 buff 已激活。[Archon 关键词](https://poe2db.tw/us/Archon_Buff)给出 10 秒持续、20 秒恢复期，`ArchonDuration` / `ArchonRecoveryPeriod` 应用 `archon_duration_+%`、`archon_delay_expires_x%_faster`、`archon_no_recovery_period`（Dominion）后显示名义时间。

Invigorating Archon 的移速和元素抗性受 Archon 效果及自身 buff 效果缩放；Energising Archon 的条件充能起始加成不属于 buff，不这样缩放。规则为 `absent`。Command 使用时获得 Archon 的几率只识别来源，不自动开启 buff；触发时序、覆盖率、各 Archon 固有效果尚未由此共享状态实现。

Nature's Archon 仍有证据冲突：关键词称 25% more 伤害，而 `ArchonNature` BuffTemplates 显示 `plant_skill_damage_+% = 25`（increased）。实现其固有伤害前需进一步核对，不能用关键词或 PoB2 任一方直接定案。

## 地面与护符事件

### Olroth's Resolve 的 Guard

[物品文本](https://poe2db.tw/us/Olroths_Resolve)的 `local_unique_flask_gain_%_of_current_ward_as_guard_when_flask_effect_ends` 表示药效结束后，以当时 Ward 获得持续 10 秒的 Guard。已装备药剂即使关闭也保留事件属性。

`olrothGuard` 是用户输入的、先前使用后仍剩余的 Guard，已经考虑期间承伤且仍在十秒内；缺失或零不给 Guard。引擎不从当前／最大 Ward 或药剂覆盖率反推，重新用药也不自动移除旧 Guard。与护符 Guard 取最强，最高为 200% 最大生命，进入 EHP／最大承伤池，不改 Ward。

药效、品质、药剂持续时间不缩放这个结束事件。`Flask<N>GuardAfterEffect` / `GuardDuration` 显示该栏位输入经上限限制的值和名义时长，明细可还原，无伪造 PoB key。规则为 `absent`；没有游戏内时序实测。

### Beira's Anguish 的点燃地面

当前 `UniqueCharmIgniteEnemiesInPresence1` 生成四秒地面，以最大生命的 500% 作为点燃来源火伤。[关键词](https://poe2db.tw/us/Keywords)给地面默认半径 2 米；非命中地面需将易燃累积到 50% 才点燃。[当前物品](https://poe2db.tw/us/Beiras_Anguish)已经替换 0.3 以前立即点燃附近敌人的行为。

每件护符的 `IgnitedGroundDuration`、`IgnitedGroundRadiusMetres`、`IgnitedGroundSourceDamage`、`IgnitedGroundIgniteDPS`、`IgnitedGroundIgniteDuration` 显示潜力，已装备但关闭也可查看。创建快照取当前最大生命，包含保留部分；不借主技能命中、暴击、攻击／法术修正或本地辅助。全局点燃幅度、速度、时长、敌人抗性／承伤照常生效，命中穿透不生效。护符品质、效果和 buff 时长不缩放事件属性；未实测时序。

`conditionBeiraIgniteActive` 明确声明战斗中已建立且正在生效的地面点燃；仅装备或启用不给 DPS。`BeiraIgniteDPS` 取最强护符来源，`CombinedIgniteDPS` 和总 DoT 在它与主技能普通点燃间取强者，不相加。可选 Full DPS 在共享点燃上限内只计一次，与技能数量无关。

若所选／纳入技能支持多个同时点燃，跨来源叠加未验证，Beira 值单列而不加总。使用频率、积累时间、位置、过期与覆盖率均不推算。规则为 `absent`，参考口径排除新来源。

TW 使用已核对的官方用语「貝伊拉的悲痛」「點燃地面」。CN 的「贝拉的苦楚」「燃烧地面」来自钉定 `i18n/zh-CN/words.json` / `stat_lines.json`，混合交易和 PoE2DB 来源，不声称保留了官方 CN KeywordPopups。完整说明文字是应用文案。Headhunter 的偷取怪物词缀没有因此实现。

### 奉献地面

`calc::consecrated_ground` 按[游戏关键词](https://poe2db.tw/us/Keywords)，让明确站在地面上的角色每秒再生当前最大生命的 5%，并获得 50% reduced 诅咒效果（`life_regeneration_rate_per_minute_%`、`curse_effect_on_self_+%`，buff `ground_consecration`）。玩家用 `conditionOnConsecratedGround` 或原生 `OnConsecratedGround` / `Condition:OnConsecratedGround` 声明战斗状态，重复来源只算一次。召唤物需独立的 `conditionMinionsOnConsecratedGround` 或自身条件，不能继承玩家位置。

[Sanguis Heroum](https://poe2db.tw/us/Sanguis_Heroum) 与 `UniqueCharmCreateConsecratedGround1` 给 `charm_create_consecrated_ground_when_used_duration_ms = 4000`、`charm_create_consecrated_ground_when_used_radius = 14`（1.4 米）。关闭的已装备护符也显示 `Charm<N>ConsecratedGroundDuration` / `ConsecratedGroundRadiusMetres`；这不建立地面或把角色放上去。护符效果、品质和时长不缩放独立地面事件。来源规则为 `absent`，基础地面收益 PoB2 也实现。

没有地面所有者／友军地面效果输入，因此 `ConsecratedGroundEffect` 有意不读取，避免把自己的地面修正用于友军地面。不推算使用率、持续覆盖或离地后保留，也未实测。TW 用[奉獻地面](https://poe2db.tw/tw/Keywords)、[嗜血英雄／止血護符](https://poe2db.tw/tw/Sanguis_Heroum)；其他说明和 CN 标签为应用文案，不声称是官方翻译。

## 箭袋、残骸、Volatility 与 Combo

### Cadiro's Gambit

`calc::cadiro_arrows` 读取 `UniquePerandusArrows1`、`each_arrow_fired_gains_random_perdandus_prefix`。用户用 `cadiroArrow` / 原生 `CadiroArrow` 明确选一种箭，0 或未填为无额外效果，1–6 如下：

| 值 | 游戏关键词 | 当前固定效果 |
|---|---|---|
| 1 | [Crescendo](https://poe2db.tw/us/Crescendo_Arrows) | 额外连锁 6 次，每次先前连锁给 60% increased 伤害 |
| 2 | [Splinter](https://poe2db.tw/us/Splinter_Arrows) | 分裂至 6 个目标 |
| 3 | [Reversing](https://poe2db.tw/us/Reversing_Arrows) | 穿透所有目标并返回 |
| 4 | [Diamond](https://poe2db.tw/us/Diamond_Arrows) | 必定暴击、60% increased 暴击伤害加成 |
| 5 | [Covetous](https://poe2db.tw/us/Covetous_Arrows) | 击杀给 600% increased 稀有度 |
| 6 | [Blunt](https://poe2db.tw/us/Blunt_Arrows) | 600% increased 晕眩累积 |

显式零／false／无效输入清除导入选择。只对战斗中的玩家弓攻击、所选 stat set 带 `skill_can_fire_arrows` 时生效；法术、弩、召唤物、代理、非箭次级效果不继承。

Crescendo 用 `skillChainCount` / `Chain` 表示此前连锁次数，受容量限制，默认初击；它是 increased，不是 MORE 或额外单体命中。Diamond 保留第一次命中检查，跳过暴击闪避重检，但仍服从 `NeverCrit`。固定效果不因重复授予而叠加。`CadiroArrowReturn` / `CadiroArrowKillRarity` 保留来源明细，无 PoB key；返回几何／时序与掉落结果未建模。

不假设六种等概率、不同时开启六种、不推算平均 DPS 或返回／连锁额外命中。来源规则 `absent`，参考口径排除它；普通箭袋攻速不变。箭名使用对应 [TW](https://poe2db.tw/tw/Crescendo_Arrows) / [CN](https://poe2db.tw/cn/Crescendo_Arrows) KeywordPopups，完整帮助为应用文案，未实测时序或几何。

### 残骸拾取

`calc::remnant_pickup` 区分拾取者 Arcane Remnants 的回魔与创造者 Empowering Remnants 的重复效果。天赋 46124 / 40985 给 `remnant_recover_%_mana_on_pickup = 3`（每次拾取 3% 最大魔力）和 `created_remnants_have_%_chance_to_duplicate_pick_up_results = 15`。

[当前 Empowering Remnants 文本](https://poe2db.tw/us/Empowering_Remnants)说自己创造的残骸有几率给两次效果。旧 `stat_descriptions` 将其写作额外一次拾取，不能据此让拾取者自己的 Arcane 回魔重复。当前回魔不乘复制几率或 RemnantEffect；它是瞬间拾取恢复，不是再生，也不乘恢复速率。

| 输入／原生 multiplier | 含义 |
|---|---|
| `remnantPickupsPerSecond` / `RemnantPickupsPerSecond` | 玩家每秒拾取次数，可来自任何创造者 |
| `createdRemnantPickupsPerSecond` / `CreatedRemnantPickupsPerSecond` | 玩家创造的残骸每秒被拾取数，可由其他人拾取 |
| `remnantPickupMissingMana` / `RemnantPickupMissingMana` | 每次拾取前的缺魔量，可选 |

恢复以最大魔力百分比计算，但不超过显式缺魔量或未保留容量。满魔／显式零缺魔／`CannotGainMana` 给零，`NoManaRegen` 不阻止瞬间拾取。未填缺魔时明确展示“潜在恢复”，不加进再生、持续回复或 EHP。输入频率意味着每次事件前都有同样缺口，不模拟中间消耗；未填／零频率不从技能使用推算拾取。

一次自创残骸被拾取，预期固有效果次数为 `1 + clamp(chance, 0, 100) / 100`。每次技能使用的潜在次数复用 `effect_terms.remnants_per_use()`，前提是产生的残骸全部被拾取；这不证明发生了拾取。次数不是 buff 幅度、时长、层数或 DPS，也不递归生成新拾取。数值溢出饱和到最大有限值并记入明细。

XML、JSON 与原生输入一致；显式 false／零／无效原始输入清除旧值，配置读取即使缺失也记录。事件输出要求战斗，来源比例／几率仍可查看。两条规则为 `absent`，无时序或复制交互实测。TW 使用游戏「魔力痕跡／生命痕跡」中的「痕跡」，其余完整标签与 CN 说明为应用文案。

### Volatility 恢复

`calc::volatility_events` 读取 Instability（43944）的 `volatility_refresh_%_chance = 25`，以及 Controlled Chaos 的 `maximum_volatility_allowed = 30`。[关键词](https://poe2db.tw/us/Volatility#VolatilityRef)默认上限 200、四秒爆炸计时、每得一层重置，最多每 0.1 秒得一次；爆炸消耗层数，刷新成功后恢复同量。关键词中的每层 100 物理自身伤害、十秒 Volatile Power 不由这个视图模拟。

`volatilityExplosionQuantity` / `VolatilityExplosionQuantity` 必须为有限正整数，受最终上限限制。Controlled Chaos 替换上限而非增加，亦见 [0.3.0 补丁](https://poe2db.tw/us/Version_0.3.0)。每次预期恢复为“封顶层数 × 总刷新几率（限制 0–100%）”。可选 `volatilityExplosionsPerSecond` / `VolatilityExplosionsPerSecond` 声明同样层数的合格爆炸频率，未填时每秒值为零，无效值和溢出拒绝。

不从攻速或重置计时猜频率，恢复层数不反馈到无限链，即使几率 100% 也如此。当前层数、伤害、防御、Volatile Power、到期和覆盖率都不改变。没有来源时全部为零；有玩家来源时，几率和上限在非战斗或数量无效时仍可查看，事件结果为零；召唤物不适用。

显式原始 false／零／无效值清除原生旧设置，缺失则保留；两项配置均记录读取。两条规则为 `absent`，参考口径排除新视图，明细保留公式／来源且无伪造 PoB key。TW「[易爆](https://poe2db.tw/tw/Volatility)」、CN「[无常](https://poe2db.tw/cn/Volatility)」来自游戏词条；完整文案为应用用语。刷新是否绕过获层限制未实测，调用方必须提供合格事件。

### Combo

`calc::combo` 为当前接收连击的技能展示 `ComboMaximum` 和 `ComboGainPerEligibleEvent`。[Combo](https://poe2db.tw/us/Combo)是部分技能的计数，由同一绑定武器击中敌人积累，切武器会丢失。[Tempest Bell](https://poe2db.tw/us/Tempest_Bell) 和 [Mantra of Destruction](https://poe2db.tw/us/Mantra_of_Destruction)明确要求其他技能积累。

活动 `granted_effect_stat_sets.json` 给上述技能需求 4／8，Combo Finisher I/II 要 3／5，Culmination I/II 和 Vruun's Inevitability 给显式上限 10。只有没有显式上限时才把需求当容量，不能相加。不同需求取最大，带符号品质先调整主动技能本身需求；Finisher 需求和显式上限各取最大。辅助仍须通过原有资格检查。

视图描述**从空计数开始的一次成功、合格事件**，只读全局获得修正，不借接收技能自己的命中／施法频率。[Martial Adept](https://poe2db.tw/us/Martial_Adept)（41751）的 `additional_combo_gain_on_hit` 一次增加整数 a；`additional_combo_gain_chance_%` 合为 0–100% 的概率 p，成功只多一层。因此容量 c 下的期望为 `(1-p) × min(c, 1+a) + p × min(c, 2+a)`。计数输入为非负整数；相加、非递归的读法来自文本及共同 stat，但组合交互未实测。没有溢出机制，也不推算当前层数、衰减、频率、消耗、回复或 DPS。

Practiced Combo 修改的是产生连击的那次攻击，不从别组借给接收技能，也不进入这个仅全局视图。Finality 禁止获得连击；`conditionFinality` / 原生 `Finality` 是当前状态，显式 false 清除旧 true，原生明确条件优先于 `Condition:Finality` 标志。无接收技能、非战斗、Finality 中或不支持的代理／召唤物时，获得为零且不读取全局增益。该控件不实现 Finality 的其他奖励。

新获得规则为 `absent`；参考口径保留原有 Culmination 选定层数倍率，但排除新生成视图。Calcs 公式可还原且无虚构 PoB key。TW 使用「[連擊／終極](https://poe2db.tw/tw/Combo)」，CN「连击／绝杀」取保留的 `i18n/zh-CN/stat_lines.json`；完整标签和帮助为应用文案。
