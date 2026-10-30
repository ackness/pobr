# 与 PoB2 的关系：数据来源与已知差异

原则见[维护者指南的“与 PoB2 的关系”](maintainer-guide.md#与-pob2-的关系逐步独立)。本文列出 PoBR 仍依赖 PoB2 的数据、按游戏规则有意与 PoB2 不同的地方，以及冻结语料中逐行解释过的参考值差异。

登记方式：差异的规则与依据写在这里（规则本身属于某个机制时写在对应的[机制约定](mechanics/README.md)并在这里链接）；逐行的构筑与 stat 写进 `devs/scripts/known-differences.json`，`reason` 指向本文或对应的机制文档。导出工具没有实现的词条规则只标 `exporter: "absent"`，由参考口径自动排除，不在这里逐条登记（见[参考口径](diagnostics.md#参考口径导出工具规则与语料)）。

## 数据来源现状与退出顺序

各文件的 `_meta.generator` 可核对来源；下面的路径均相对 `data/<版本>/`，共享规则另放 `data/overlay-common/`。
PoB2 对照版本由 `devs/reference-sources.json` 独立固定，生产产物的 `_meta.vendor_commit` 仍记录其生成来源。
固定官方树输入和独立 adapter 已有单独的候选入口；当前输入存在两处 Druid 替换引用缺口，
尚未切换生产树或退出 vendor 补充，见[数据管线](../pipeline/README.md#固定来源与独立树候选)。

### 直接来自游戏的数据

- `base/` 的物品基底、词缀、stat、天赋树、技能及等级。`ActiveSkills.DisplayedName` 用于 buff 名、`AffectedBy<名字>` 条件和诅咒优先级（`skill_env::buff_skill_name`）。
- stat set 保留 `base_is_projectile` 等无值 stat；召唤物技能另保留按使用者等级成长的效能。
- `base/skill_gems.json` 的效果链接、宝石标签来自 `GemEffects` / `GemTags`；各宝石等级所需角色等级来自 `ItemExperiencePerLevel`。品质另由 `overlay/gem_quality_stats.json` 提供。
- `base/monster_resistances.json` 连接 `MonsterVarieties → MonsterTypes → MonsterResistances`，取普通稀有度的 `Fire1/Cold1/Lightning1/Chaos1`，合并链接并保留负弱点，供召唤物基础抗性使用。

### 仍从 PoB2 抽取的数据

`sync-pob-catalog` 读取钉定 Lua 版本，主要产物如下：

- 解析：`mod_parser_rules.json`、`generated/special_vendor.json`、`mod_scalability.json`。
- 技能：`skill_stat_map.json`、`skill_overrides.json`、`stat_set_labels.json`；以及触发、幻影配置。
- 配置：`config_options.json`，其中 `if*` 与 `tooltip` 只决定界面可见性／说明，不证明选项已参与计算。
- 物品：`uniques.json`、`base_item_overrides.json`、`runes.json`、`catalysts.json`；另有 `curse_priority.json`。
- 描述：`stat_descriptions.json` 与派生的 `stat_id_map.json`。它们来自 PoB2 导出，不是直接从游戏客户端提取。

`gem_effects.json` 仍服务旧快照、几颗宝石共用的效果和显示顺序 `GemEffects.GrantedEffectDisplayOrder`。虽然原生效果链接已在 `base/skill_gems.json`，`pathofexile-dat@15.2.0` 的社区 schema 尚未命名显示顺序列，所以顺序仍由此 overlay 提供。

`overlay/minions.json` 的 `monster_variety_id` 来自 `Export/Minions/Minions.txt` 的 `#monster` 映射。已入库的召唤 stat 只有 monster hash 等数值，不能反查 variety；幽魂／驯兽 ID 本身已是 variety ID。这里借用的是身份映射，抗性值仍读游戏表。

### 手工整理与回归数据

`overlay-common/buff_definitions.json`、`special_mods.json`、`high_precision_mods.json`、`local_mods.json` 包含项目维护的兼容规则及独立游戏规则，不能统称为 PoB2 语义。

游戏来源手工表包括 `item_granted_skills.json`（`ItemInherentSkills.NoReservation` 的基底，adapter 支持该表后应生成）、`boss_poise_thresholds.json` 和 `totem_varieties.json`（保留游戏行／来源）。`energy_config_options.json` 等则是项目定义的外部事件／静态状态输入，合入配置目录。

`examples/demo-bd-test/builds/*/meta.json` 的回归对照值由 `tools/pob2-oracle` 重录。它们用于防回归，不是独立游戏真值。

这些依赖仍限制部分新版本数据的接入，但不限制 PoBR 只能计算 PoB2 已支持的机制。`rules/skill_stats.rs` 的自有映射和带 `game_source` 的规则已支持若干导出器缺失的效果；`exporter: "absent"` 就是对此的明确记录。不要再把“仍依赖部分数据”写成“机制覆盖不会超过 PoB2”。

逐步减少依赖的候选顺序是：直接提取游戏 stat 描述；建立游戏 stat ID 到引擎 modifier 的自有映射；将技能映射和配置改为按游戏数据差异维护；补充游戏内实测回归。`stat_id_map.json` 是从描述与现有 parser 派生的维护资料，不是生产中的独立 stat 注入通道或已验证的游戏语义；迁移需核实映射的单位、作用域与证据，并保留未知项诊断。每项要单独评估，不是一次性重写计划。

## 已知的有意差异

### 首领累积门槛

首领 poise 使用当前游戏表，不用 vendor 预设。0.5.5.4.2 的 Xesht（`MonsterVarieties` 行 1946）`PoiseThreshold = 1700`，即 17 倍；钉定 PoB2 的 838% MORE 是 9.38 倍。`overlay-common/boss_poise_thresholds.json` 保留行与导出 hash。地图首领众数 313 与旧 213% MORE 一致。worked Shaman Spark `5dcf9ecdd375` 的 `HeavyStunBuildupAvg` 为 3.627930% 对 6.578395%，主比例为 `17/9.38 = 1.812367`，余下命中／取整差约 0.05%。Pinnacle/Uber 都用代表性的 Xesht，当前导出无独立 Uber variety；不能说所有首领都相同。该门槛也影响 Freeze/Electrocute，且与独特稀有度 6 倍分开。

### 偷取

偷取每种资源只取恢复速率最高的有效实例（[0.5 补丁说明](https://www.pathofexile.com/forum/view-thread/3932540)：“Only a single Leech instance per resource (Life, Mana, or Energy Shield) can apply at a time.”）；[游戏关键词](https://poe2db.tw/us/Keywords)给出一秒基础恢复时间，偷取量和偷取速度分别修正。

钉定的 PoB2 仍叠加实例，保留按资源池计算的单实例偷取量上限，并把“increased amount … Leeched”当作最大速率加成。tuned 组的 Deadeye（Twister，`bed1edc6d8b2`）`ManaLeechGainRate` 为 PoBR 80.72 对参考值 166.17；Pathfinder（Ice Shot，`728cbde29279`）同一字段为 162.96 对 190.12；Blood Mage（Arc，`5bc8be0fa967`）`LifeLeechGainRate` 为 513.68 对 575.23。

去掉单实例偷取量上限后，原来吻合的三行 `LifeLeechGainRate` 也出现有意差异：Warbringer（Shield Wall，`1e4e329e5e5c`）为 582.978 对 552.40416，Blood Mage（Comet，`bc750d615159`）为 641.43144 对 207.01274，Gemling Legionnaire（Frostbolt，`42f790c0f9ba`）为 0.2 对 0.0635；这些行已登记在 `devs/scripts/known-differences.json`。前述默认速率上限仍是兼容约定，平均命中频率也只是覆盖时间估计；这些数值不是游戏内实测结果。

### 导入符文原文

PoB2 用自己的符文表覆盖物品原文；PoBR 以物品原文为准。同一物品上同一 stat 的符文在原文里合成一行：tuned 组的 Infernalist（Comet）武器上是一行 “45% less maximum Life”，PoBR 按 0.55 计，PoB2 按两块符文的 15% 与 30% 分别连乘（0.595），所以它的生命比参考值低。

### 同名诅咒取最强

同一个诅咒由两个技能组施加时，敌人身上只有最强的那一份（“Debuffs of the same type do not stack, and only the copy with the strongest effect applies”，[Curse](https://www.poe2wiki.net/wiki/Curse)），比较的是各份自身与其辅助宝石的诅咒效果（`skill_env::buff_specs::curse_strength`）。PoB2 按自己的优先级表挑，光环来源优先：tuned 组的 Acolyte of Chayula（Decompose）有 Doedre's Undoing 组和 Blasphemy 组两份 Despair，PoB2 用后者（敌人混沌抗性 −31），PoBR 用更强的前者（−58）。

### 尸体伤害类型

尸体爆炸的伤害类型取自游戏 stat：`corpse_explosion_monster_life_permillage_<类型>` 与 `corpse_explosion_monster_life_%_<类型>` 写明类型；不带类型的 `corpse_explosion_monster_life_%`（Fiery Death）按它的文本 “Explosion deals base Fire Damage equal to {0}% of the Corpse's Maximum Life” 是火焰伤害（`skill_stats::corpse_explosion_damage`），PoB2 按物理计。

### Witchtoad 生成与费用

Doedre's Undoing 的 Witchtoad 每 0.25 秒生成一只，PoB2 把这个间隔乘以施法时间，施法越快爆炸越多且没有上限；PoBR 按“区域数 × 1000 / 间隔”计算，与施法速度无关（见 `skill_stats::granted_skill_modifiers`）。Dark Consequences 自带魔力消耗为 0，但 Archmage 的“Non-Channelling Spells cost an additional … of your maximum Mana”仍给它本身增加消耗；这不是继承诅咒的消耗。tuned 组 Blood Mage 的 5.7% 最大魔力经 Atalui 转成生命、Efficiency II 减少 40% 后为每次爆炸 199 生命，两区域每秒 8 次为 1592。PoB2 的每秒生命消耗却用它的施法速率 2.14/s，得到 425.86；PoBR 保留按爆炸频率支付，伤害计算不变。

### 冻结 Witchtoad 频率

2026-10-26 冻结 Forbidden Rites 语料中，`blood-mage-03/04/06/08/10` 与 `chronomancer-04` 虽以 Impending Doom 命名，实际选中 Dark Consequences。Witchtoad 间隔给每秒 8 次；除去 oracle 的施法速度相关频率后，五个 Blood Mage 的 `TotalDPS` 差异都小于 1.5%。Chronomancer 还差 Hindered 状态，所以只解释频率行。known-difference 同时绑定解码 XML SHA-256，不能仅凭短 rank 名复用到另一快照。

### Focus 等级加成缩放

Instruments of Power 的 “50% reduced bonuses gained from Equipped Focus” 让 focus 上的 stat 按一半给予，“+# to Level of all X Skills” 不低于 1（[Instruments of Power](https://www.poe2wiki.net/wiki/Instruments_of_Power)）：focus 的 +2 法术等级计 +1，+1 仍为 +1（`skill::resolve::scale_slot_gem_property_bonuses`）。PoB2 减半 focus 的其他词条，宝石等级不减半：demo 语料的 Disciple of Varashta（Comet）的法术比参考值低一级，TotalDPS 0.86×，`ninja_parity` 的进攻与持续伤害基线各降 1 / 2 格。

### Bonestorm 命中频率

Bonestorm（“Channel to conjure a swarm of bone spikes in the air, then release to fire them at enemies and explode”，“Can fire up to 20 Projectiles”，[Bonestorm](https://poe2db.tw/us/Bonestorm)）引导时每次施法召出一根骨刺，释放时全部射出：PoBR 按每根骨刺命中一次计，每秒命中数即施法速率；一次引导最多 20 根（`base_number_of_allowed_bone_storm_projectiles`），不改变每秒的骨刺数。PoB2 的引导层数（`Multiplier:BonestormStage`）没有配置时为 0，按最短引导时间 0.3 秒（`base_minimum_channel_time_ms`）命中一次：tuned 组的 Abyssal Lich（Bonestorm）PoB2 每秒 3.33 次，PoBR 11.96 次。**没有游戏内实测**：假设每根骨刺都命中目标。

### Withered 的施加者

“Minions have 10% chance to inflict Withered on Hit” 在没有召唤物的构筑上，PoB2 把这条词条当作玩家自己能施加 Withered，并按配置的层数给敌人加混沌承伤；PoBR 把它交给召唤物，角色自己的技能、Presence 或已有的召唤物能施加 Withered 时才计入配置的层数（`calc/withered.rs`），所以这类构筑会比参考值低。每层 5%、最多 10 层两边一致。

### 不保留精魄的物品授予

Lament / Portent / Absent Amulet 授予的技能不保留精魄（游戏表 `ItemInherentSkills` 的 `NoReservation`，三个基底列在 `overlay-common/item_granted_skills.json`）。PoB2 导入角色时把这个技能再列成一个普通宝石组并按宝石保留 30 / 60 / 100 精魄，所以这类构筑码自带的 `SpiritUnreserved` 常为负数，而游戏里保留量不可能超过精魄总量。

PoBR 把“排在组首、等级与授予等级相同、无品质、未腐化”的那个普通组认作被授予的技能本身，不计保留，也按授予技能的需求降级（见[授予技能](mechanics/items.md#宝石等级品质与授予技能)）；其余同名宝石照常保留（`calc_orchestrator/granted_skills.rs`）。其他物品授予、需要保留精魄的技能也是这样认出的同一个技能，只保留一次；PoB2 的物品组和宝石组各保留一次（tuned 组 Gemling Legionnaire Frostbolt 与 Oracle Grim Pillars 胸甲授予的 Blink：PoB2 120，PoBR 60）。

PoB2 只对仅由物品授予的技能（`fromItem`，如 Purity of Ice）这样合并，所以 Absent Amulet 的 Eternal Rage 在 PoB2 里是两个组：降级的物品组和未降级的宝石组。该技能上的辅助宝石追加的精魄也一并免除，PoB2 仍计入：依据是权杖授予的技能在游戏数据里以 `reservation_multiplier = -100` 表达“不保留”，辅助的追加量随之归零；这一点没有游戏内实测。

### 箭雨与 Hailstorm Rounds

Rain of Arrows 的命中频率按游戏的两条规则计算（`pobr-core/src/calc/rain.rs`）：“One arrow lands every 0.08 seconds”（`rain_of_arrows_delay_per_arrow`，“Modifiers to Projectile speed apply to arrow landing frequency”）和 “Can Hit each enemy only once every 0.12 seconds”（`rain_hit_delay_ms`）。

命中只能来自落下的箭：间隔过后落下的第一支箭命中，所以每 ⌈0.12 ÷ 落箭间隔⌉ 支箭命中一次；一次使用射出 N 支箭（技能的投射物数量），一场箭雨最多命中 ⌈N ÷ 该数⌉ 次，使用得慢、箭雨在下一次使用前已经落完时按每次使用的命中数封顶（“Maximum 1 arrow rain”）。PoB2 把命中间隔记为 max(0.08, 0.12) 秒，等于假设敌人每次能被命中时恰好有箭落下，也不计投射物速度：没有投射物速度时游戏规则给出每秒 6.25 次，PoB2 为 8.33 次；tuned 组的 Rain of Arrows 构筑（169% 增加的投射物速度和 Projectile Acceleration III 的 40% 更多投射物速度，每 0.0212 秒落一支）是每 6 支箭命中一次，每秒 7.85 次。

以下是假设，**没有游戏内实测**：每支落下的箭都打到敌人（箭雨半径 3.6 米、单支箭的冲击半径 0.7 米，对单个敌人实际更少，取决于体型）；落箭节奏均匀，按游戏的数值、不按服务器帧取整；角色持续使用该技能；品质的 “Arrows fall #% faster” 是箭下落的速度，不改落箭频率。同样带 `rain_hit_delay_ms` 的 Firestorm、Icestorm 见下一条；Hailstorm Rounds（`rain_hit_delay_ms` = 200）的每次发射提供一次落点，连续发射时每 ⌈0.2 × 发射速率⌉ 次落点命中一次，再按弹匣与装填周期折算（`perform::fill_crossbow_reload`），一次攻击的多个投射物共用这个命中间隔，命中限制不减慢弹药消耗；`siege_cascade_fall_time_ms`（300）是飞行时间，不是落点间隔。

Hailstorm Rounds 没有模拟散布和弹匣交界处的命中锁定相位，**没有游戏内实测**。参考口径（`on_reference_basis()`）按 PoB2 的计法比较：箭雨每当敌人能再被命中时就算一次，Hailstorm Rounds 取发射速率与每秒 5 次中的较小值。

### 周期命中

球与风暴的命中间隔读游戏 stat，不随施法速率改变。实现为 `pobr-core/src/calc/periodic.rs`，映射在 `rules::spell_hits::modifiers`。

| 技能 | 游戏间隔与当前计算 |
|---|---|
| Ball Lightning | `ball_lightning_base_hit_frequency_ms = 200`，每 200 ms 命中范围内敌人 |
| Solar Orb | `solar_orb_base_pulse_frequency_ms = 1400`，每 1400 ms 脉冲一次 |
| Firestorm / Icestorm | `fire_storm_fireball_delay_ms` 分别为 200 / 150；共用 `rain_hit_delay_ms = 150`，按上一条箭雨的落点规则限制命中 |
| Thunderstorm | `lightning_storm_hit_frequency_ms = 350`，且同一敌人在 `lightning_storm_hit_prevention_duration_ms = 500` 内只受一次命中；计算同时满足两个间隔的潜在频率，不模拟 `lightning_storm_hit_frequency_variance_ms` 的随机间隔 |

角色维持一场风暴或一个球，多个不叠加；有持续时间的按 min(持续时间 × 每秒使用次数, 实例上限) 计覆盖率（最多 1），Ball Lightning 没有这类持续时间 stat，假设目标一直在范围内、不估算球的行进。Ball Lightning、Solar Orb 的其他 stat set 不继承这个频率。PoB2 假设风暴与球始终覆盖目标，参考口径关掉 `DataOrchestratorOptions::periodic_uptime` 按它比较。**没有游戏内实测**：假设敌人始终在范围内，每个符合间隔的落点都命中。

### 引导与释放周期

引导攻击（Detonating Arrow、Supercharged Slam、Perfect Strike、Gathering Storm、Snipe、Shield Charge，`calc::channel::skill_modifiers`）的一次使用近似为“释放前引导 + 释放”（`pobr-core/src/calc/channel.rs`）。引导取 `base_minimum_channel_time_ms`、`channel_start_lock_cancelling_of_attack_time_%` × 攻击时间、层数 × 攻击时间中最大的一个；完美时机只给出窗口宽度（如 `perfect_strike_timing_window_base_ms`），没有开启时间的 stat，所以完美与普通 stat set 各算自己的伤害，时间都用最短引导 / 起手锁定。

释放取 `channel_end_duration_as_%_of_attack_time` 的比例，或 `channel_skill_end_animation_duration_override_ms`（有时优先，两者不相加）；比例随攻击速度缩放，固定毫秒不缩放，只有动画倍率、没有基础时长的阶段不另计。层数同时用于伤害和时间，默认取上限（含品质），构筑配置的 multiplier `ChannelStages`（配置输入 `skillChannelStages`，Web 技能页的层数输入）可覆盖（取整并限制在 1 到上限）：Detonating Arrow 每层 `detonating_arrow_all_damage_%_to_gain_as_fire_per_stage` = 120；Supercharged Slam 每层 “40% more Damage with Hits for each stage”（只加命中），“Releases 1 Aftershock per Stage”，冲击 stat set 计冲击加全部余震、余震 stat set 只计余震，用等级行 850 毫秒不受攻击速度影响的攻击时间，余震延迟不阻止下一次引导。

Shield Charge 按 1300 毫秒最短引导，不模拟行进与碰撞。PoB2 多只把释放阶段当作命中间隔、默认一层并用武器的攻击时间。**没有游戏内实测**。

### 平均命中与每秒伤害

带 `base_skill_show_average_damage_instead_of_dps` 的技能（Grim Pillars、Lightning Warp、Vile Disruption、Skyfall、Living Bomb、Volcano、Incinerate、Rake、Parry、Raise Shield、Hand of Chayula、Stampede、Gathering Storm、Perfect Strike 等）在游戏面板上显示平均伤害；这个 stat 没有词条文本，不改变伤害或使用速率。

PoBR 的 `CombinedDPS` 仍是每秒伤害：命中 DPS 加持续伤害 DPS，再计 Culling Strike；PoB2 在这里改填平均命中伤害，也不加它已算出的持续伤害（Incinerate 的 `base_fire_damage_to_deal_per_minute`、Living Bomb / Volcano 的点燃、Rake 的流血）。这是输出口径不同，不能拿这一行当作伤害倍率去补。

Vivid Stampede（`skill_triggered_on_hit_while_have_vivid_stag_wisps`，攻击命中触发、每个 Vivid Wisp 一只鹿）同样只显示平均伤害：PoB2 的 TotalDPS 为 0、CombinedDPS 是每次触发的伤害；PoBR 暂按武器攻击速率每次攻击一只估算，精灵数量与触发来源还没有建模，两边不能比较。

### 缺少事件频率的效果

Repulsion Wave、Shield Wave、Combust 与 Caltrops 保留平均命中伤害，事件频率未建模时 `ActionRate` / `TotalDPS` 为 0。官方 `ActiveSkills.Description` 中，Repulsion 需要命中被诅咒的敌人，Fortifying Cry 需要后续盾牌攻击造成伤害，Temper Weapon 需要被强化的攻击命中；Trail of Caltrops 在闪避时布置，敌人触发时才造成伤害。

`caltrops_on_dodge_X_triggers = 3`、首次在闪避的 5%、间隔为闪避的 20% 只定义布置时机，不能从武器攻速推算闪避或敌人触发频率；Combust 的 100% 触发几率也不能替代强化次数与攻击来源。`CombinedDPS` 仍是每秒伤害，不改填平均命中；因此 sweep 的 `pobr_zero` 分类可能已有非零 `AverageHit`，不能据此判定没有伤害。

20 级 Repulsion Wave 的基础物理伤害为 128–192 × 2.04，Shield Wave 为 (85–128 + 每完整 15 点盾牌护甲的 5–7) × 2.04。Blood Explosion 为 196–294 × 4.08、基础暴击率 5%；其 sweep 差异来自前述攻击命中检查，不补伤害倍率。Blood Explosion 的现有每秒一次口径尚未按失血、重度眩晕或死亡事件建模。这些事件模型均未做游戏内实测。

### 伤害与命中精度

伤害区间与命中率保留小数：Earthquake / Stampede 的 Wooden Club 6–10 乘等级行的 `damage_multiplier` 1.25 为 7.5–12.5，Toxic Growth 的 6–9 乘 0.62 为 3.72–5.58；PoB2 先把伤害上下界取整（8–13、4–6）、命中率取整到百分数，低伤害的合成构筑上差别可超过 5%。这是计算精度的约定，游戏内部是否取整**没有游戏内实测**。

2026-10-16 的远程 sweep 也体现同一精度差异。六个 fixture 都用游戏 Makeshift Crossbow（`base/base_items.json` 的 `FourCrossbow1`）：物理 7–12、攻击时间 625 ms、基础暴击 5%。20 级伤害倍率与转换来自 `base/granted_effect_stat_sets.json` 第一个 stat set，攻速倍率来自 `base/granted_effect_levels.json`；没有附加伤害或增加伤害。表中区间为 PoBR 转换后数值 → PoB2 减免和暴击前取整值，箭头不是本次修改。

| 所选效果 | 伤害倍率／转换 | 物理区间 | 元素区间 | TotalDPS PoBR / PoB2 |
|---|---|---|---|---|
| `IceShardsPlayer` | 0.29 / 60% Cold | 0.812–1.392 → 1–1 | 1.218–2.088 → 1–2 | 5.423944799 / 4.952566667 |
| `ShockburstRoundsPlayer` | 0.36 / 40% Lightning | 1.512–2.592 → 2–3 | 1.008–1.728 → 1–2 | 4.615380330 / 5.334733228 |
| `PermafrostBoltsPlayer` | 0.49 / 80% Cold | 0.686–1.176 → 1–1 | 2.744–4.704 → 3–5 | 1.183943073 / 1.279610526 |
| `RapidShotPlayer` | 0.94 / 60% Fire | 2.632–4.512 → 3–5 | 3.948–6.768 → 4–7 | 23.651653516 / 24.984921520 |
| `ArtilleryBallistaProjectilePlayer` | 0.40 / none | 2.8–4.8 → 3–5 | — | 0.306053311 / 0.324168000 |
| `SiegeBallistaProjectilePlayer` | 0.66 / 60% Fire | 1.848–3.168 → 2–3 | 2.772–4.752 → 3–5 | 0.807980742 / 0.850941000 |

两边对这些 stat set 都按每次动作一个投射物命中、75% 物理减伤、50% 元素抗性。物理／元素中点为 P、E 时，`DPS = (0.25P + 0.5E) × h × (1 + 0.05h) × rate`。PoBR 保留 `h = 0.775345425`，PoB2 用 0.78，暴击确认也如此。

按表顺序，持续频率为 6.111111111、4.787401575、0.701754386、8.221225710、0.4、0.4/s。四种弹药的射击频率为 8.8、6.4、1.6、8.8/s，弹匣为 16、19、1、100，装填为 0.8、1、0.8、0.8 秒。代入各自区间与 h 可还原表中 DPS，因此不应补投射物数、伤害倍率或装填修正。

这只确认现有精度口径，没有测量游戏内部取整。当次 benchmark/tuned 没有这六种技能，不新增 worked 构筑豁免。

### 非武器攻击与第三击曝露

Bursting Fen Toad、Conductive Runes 的 Explosion、Blood Explosion 与 Repulsion Wave 是非武器攻击（游戏类型 `Attack` / `NonWeaponAttack`，Bursting Fen Toad 的说明是 “dealing Attack damage”），stat set 没有 `global_always_hit`，照常检查命中；PoB2 给它们的基础标志缺少攻击，不检查命中。没有游戏内实测证明这类爆炸不能被闪避。Primal Strikes 的 Exposure（`all_exposure_on_hit_magnitude` = 50，“The third attack is a large swipe that inflicts Elemental Exposure”）只在第三击的 stat set `PrimalStrikesFinalPlayer` 上，计算普通击时不假设已用过第三击；PoB2 默认计入（敌人元素抗性由 50 降到 25）。连段的覆盖率还没有建模。

### 无伤害效果

主 stat set 带 `base_deal_no_damage` 的效果（Conductive Runes 的放置、Rolling Slam、Spearfield、Tempest Bell 的主 stat set，Hollow Focus 的 Bells）PoBR 为 0，钉定的 PoB2 可能给它们算出默认武器或附加伤害（见[无伤害效果](mechanics/offence.md#命中技能类型与无伤害效果)）。

### 冷却与无弹药状态

Black Powder Blitz 按游戏等级表的冷却限制使用频率；Requiem 按 `crossbow_skill_has_no_ammo` 不计弹匣与装填时间，不能用重复装填降低它的持续射击频率。两者与钉定 PoB2 的默认周期可不同，不改伤害倍率追平。

### 图腾放置时间

图腾放置时间按召唤技能自己的 `cast_time`：Spell Totem（`SummonMetaTotemSpellTotemPlayer`）500 毫秒，Shockwave Totem 600 毫秒；PoB2 的 Spell Totem 按 600 毫秒，tuned 组 Grim Pillars 构筑的 `TotalTotemPlacementTime` 与按放置频率计的每秒消耗随之不同。

### 装备属性需求归零

The Vertex 的 “Equipment has no Attribute Requirements”（游戏 stat `global_equipment_no_attribute_requirements`）让所有装备的属性需求归零，包括武器；PoB2 未应用这行。Spirit Walker Zekoa（e071）的武器需求因此不是 180 力量 / 450 智力，基准组 Blood Mage Impending Doom（add1）的 Array Buckler 也不要求 78 敏捷；宝石和辅助宝石的需求照常计算。

### 授予技能属性需求

物品授予的技能（例如 “Grants Skill: Level N Eternal Rage”）的属性需求只决定它降到哪一级（见[授予技能](mechanics/items.md#宝石等级品质与授予技能)），不计入 `ReqStr` / `ReqDex` / `ReqInt`；构筑码另列的同等级普通组被认作同一技能，也不计入（`calc_orchestrator::requirements::requirement_snapshots`）。PoB2 按宝石计入，因此 Wolf Pack / Eternal Rage 抬高参考值的力量需求，Trinity 抬高敏捷需求，Blink / Cast on Critical 抬高智力需求；权杖的 Purity of Ice 和护身符的 Time of Need 也适用。

### Energy 触发频率

Cast on Critical 里的法术，PoBR 按宝石触发它的频率计算 DPS 和使用速率（`pobr-core/src/calc/energy.rs`）。钉定 PoB2 没有能量模型，触发配置要求同组有攻击技能（PoE1 的用法）；找不到时退回法术自身施放速率，所以构筑码自带的参考值按自身速率计算。

当前能量计算使用以下输入：

- 上限来自 meta 宝石支援部分的 `generic_ongoing_trigger_1_maximum_energy_per_Xms_total_cast_time = 10`，即每 0.1 秒基础施法时间需要 10 点。技能自带的 `total_cast_time_+_ms` 按游戏术语表 Energy 条目计两倍，Comet 的原值为 300。
- 每次暴击获得的能量 = `cast_on_crit_gain_X_centienergy_per_monster_power_on_crit = 100`（每点敌人 Power 1 点）× 敌人 Power × 暴击伤害占敌人异常阈值的比例 × 能量获取修正。独特怪 Power 为 20；修正来自 `energy_generated_+%`（“Meta Skills gain #% increased Energy”）。
- 被触发的技能不产生能量。计算从角色自己使用的其他伤害技能中，取填满最快的一个作为来源；没有来源时法术保留自身速率，并报告缺少来源。

触发频率 = 来源技能每秒暴击次数 ÷ ⌈上限 ÷ 每次暴击能量⌉。**没有游戏内实测**，以下是假设：比例按“减免前暴击伤害 ÷ 阈值”线性计算且不封顶（玩家测得的公式，游戏未公布）；每次暴击取平均暴击伤害；每次使用按命中一次算；首领的异常阈值只按等级取表（游戏 stat 表里有 `ailment_threshold_pluspercent_final_for_map_boss`，取值不在已入库数据里）；获得能量时视为“近期造成过暴击”。

自动触发按整次事件充满，多出的 Energy 丢失；半额返还按概率加权下一轮的事件数（`calc::energy::triggers_per_second`）。`DataOrchestratorOptions::energy_triggers = false` 保留参考口径的自身速率或既有触发配置路径。

### 其他 Energy 家族

Cast on Elemental Ailment、Cast on Block、Cast on Minion Death、Spellslinger、Elemental Invocation 同样按 Energy 计算（`skill_env::triggers::energy_trigger_modifiers`、`calc::energy::apply_energy_trigger`）。

格挡每次给 25，召唤物被击杀时每点 Power 给 50；Spellslinger 按自施法的基础使用时间每 10 ms 给 0.2，Total use time 的加法同样计双倍。元素事件只有点燃、感电、冻结，没有冰缓；每点敌人 Power 的感电给 1、冻结给 10，点燃再乘新实例完整减伤前伤害与异常阈值之比，不封顶。20 级前四族自带 57% increased Energy，Elemental Invocation 为 68%；兼容 meta 的增益与半额返还辅助作用于 meta 本身，不要求能辅助 payload。

Spellslinger 与 Elemental Invocation 的 500 储能和每轮法术消耗分开，后者每品质加 5 储能；自动触发的容量按同组被支持法术的总消耗计算。

### Energy 外部事件输入

Energy 的外部事件输入由 `config_resolve::resolve_config` 读取：`energyIncomingHitRate` 是可格挡来袭命中／秒，乘有效格挡率；未填才回退到显式 `enemySpeed` 的 1000／毫秒，显式 0 优先且表示零事件，`enemySpeed = 0` 同理。该回退把一次敌人动作当作一次可格挡命中，不另算敌人命中或玩家闪避。`energyMinionDeathsPerSecond` 只计被击杀，不模拟到期、生命或死亡；`energyMinionPower` 默认 1。两种速率缺失或没有格挡等必要机制时保留法术自身频率；显式零事件停止触发。`energyInvocationUsesPerSecond` 未填时假设有能量即使用，受冷却和服务器 tick 限制；显式 0 即使无来源也停止使用。Invocation 保留余能，按平均返还消耗及 ⌊容量／整轮消耗⌋ × 使用频率限制吞吐；不分别排程 payload，不模拟整轮超容量时的部分触发。上述外部速率和 Invocation 使用模型**没有游戏内实测**。

### Energy 来源近似

Energy 的来源近似是按基础频率预选最多六个玩家自用、非触发且非持续保留的技能，再取吞吐最高的一项；不叠加多来源轮转、多目标或额外投射物重叠。异常事件用单目标命中频率与施加概率；感电沿用非暴击来源近似，冻结按冷伤 buildup 向上取整所需命中数，不计冻结持续时间和再冻结间隔。混合异常按概率平均每事件 Energy，不模拟同一击的事件顺序；点燃比例、这些事件率和来源策略均**没有游戏内实测**。Barrier Invocation 与 Cast While Channelling 的持续 Energy 吞吐尚未建模，后者只展示首次充满时间。

### Energy 输出

CLI/WASM 的 `energy_trigger` 给出储能、每轮消耗、来源、事件率、每事件 Energy、返还和频率上限。六族缺来源时带 `at_own_rate: true`、`source: null` 与稳定的 `missing_source`，命中与持续伤害保留自身速率；`SkillTriggerRate` / `TriggerRateCap` 的明细给出自身速率与补充输入提示，原始触发率字段仍为未解出的 0。有来源时 CoC 的 `TriggerRateCap` 是来源暴击事件率。`ActionRate` / `TotalDPS` 的派生公式包含 Energy 调整，Full DPS 使用调整后的输出；CLI 保留 `energy_per_critical_hit` 兼容字段。四个配置项由 `data/overlay-common/energy_config_options.json` 经 `extract_config_options::append_energy_options` 合入目录，按稳定 meta id 的 `if_skill` 筛选，重新生成不重复。

### 图腾数量与层数

图腾使用的技能（图腾宝石自己的技能，或插在法术图腾这类 meta 图腾宝石里的技能，游戏类型 `UsedByTotem` / `SummonsTotem`）按“角色有图腾、最近召唤过图腾”计算，施法和攻击不算角色自己的（不置 `CastSpellRecently` / `AttackedRecently`）。“每个已召唤图腾”类词条按 1 个图腾计，配置填了“已召唤图腾数”时按配置。

PoB2 默认按图腾上限计，而它给法术图腾里的技能算出的上限不含宝石随等级增加的 `base_number_of_totems_allowed`（两个 tuned 构筑上是 2，按游戏数据 23 级以上的法术图腾是 3，再翻倍）。PoBR 的图腾上限 `ActiveTotemLimit` 只是展示（见[图腾](mechanics/minions.md#图腾与-ballista)），伤害仍不假设达到上限（`calc_orchestrator/prepare.rs`）。

异常的层数同理：插在法术图腾里的技能，PoB2 按该技能自己的图腾上限乘层数，而上限的 stat 在图腾宝石上，它取到 0，点燃为 0；PoBR 按 1 个图腾的命中计算点燃（tuned 组两个 Grim Pillars 构筑的点燃只有几十，参考值为 0）。

### Charged Staff 消耗球数

Charged Staff 的 buff 按 “Buff grants {0} to {1} Added Quarterstaff Attack Lightning damage per Power Charge Consumed, up to your maximum Power Charges”（`charged_staff_attack_minimum_added_lightning_damage_per_stack` / `charged_staff_attack_maximum_added_lightning_damage_per_stack`）计算：每消耗一个暴击球一层，最多为最大暴击球数，只加给 Quarterstaff 攻击。

PoB2 把 One with the Storm 的 “count as consuming an additional Power Charge”（`quarterstaff_skills_that_consume_power_charges_count_as_consuming_x_additional_power_charges`）加在这个上限之外，最大 6 个暴击球时按 7 层计，且不限于 Quarterstaff 攻击；PoBR 按可消耗的暴击球数（当前减最小值）计，满球时为 6 层，不读取这个额外的球，暴击球未满时它也不计入（`skill_stats::player_buff_modifiers`）。

### 无 Spell 类型的光环

Purity of Fire 这类光环的技能类型（`Buff`、`Aura` 等）里没有 `Spell`，“increased Critical Spell Damage Bonus” 这类法术词条不作用于它；PoB2 给它的 stat set 写了 `spell = true` 的基础标志，主技能是光环时暴击伤害加成按法术计（tuned 组的 Ritualist Purity of Fire，2.24 对参考值 2.49）。这一行对光环没有实际意义。

### Crystalline Phylactery

Lich 的 Crystalline Phylactery 让其中的珠宝按两倍计（见 [`Build.jewels`](build-model.md#导入与构筑结构)），The Adorned 不作用于它。PoB2 不解析 “Effect of bonuses gained from Socketed Jewel”，并把 The Adorned 用在护符匣里的珠宝上，构筑码自带的参考值也是这样算的：tuned 组四个护符匣构筑（Abyssal Lich Twister 4028 / 9d1c、Lich Spark ce24 / 0a17）的伤害、暴击与个别抗性随之与参考值不同。

### 熊形态护甲

Bear form 按游戏的形态说明 “+10 to Armour per level” 给 10 × 角色等级的护甲，并让 30% 的护甲也作用于元素伤害（`pobr-core/src/calc/bear_form.rs`，主技能带 `Shapeshift` 与 `Bear` 类型时生效）；PoB2 多给 10 点（10 + 10 × 等级），参考值因此略高（约 0.2%）。

### Predator’s Mark 与召狼

Pounce 的 Predator's Mark（“Marked target takes {0}% increased damage for each of its enemies within 6 metres of it, up to {1}%”，`skill_wolf_mark_damage_taken_+%_per_nearby_enemy` / `skill_wolf_mark_damage_taken_+%_cap`，品质加在等级值上，20 品质为 4% / 60%）按角色在 Presence 内已有的召唤物计数：计算中创建的召唤物，或配置的 “# of Minions in your Presence”（`MinionPresenceCount`，`skill_stats::curse_modifiers`）。

计算 Pounce、Predator's Mark 或其他主技能时，不计 Pounce 在被标记的敌人死亡时召唤的狼：被计算的敌人活着时它们还不存在。显式选择 Pounce 的第三个效果 `WolfPounceSummonWolfPlayer` 为主技能时，创建该效果的主召唤物（`WolfMinion`，默认使用 `MeleeAtAnimationSpeedWolfCompanion`），继承来源宝石的有效等级与辅助。

数量按一次激活的一只狼加游戏 stat `skill_cross_slash_triggered_summon_+_wolves_if_rare` / `_if_unique` 的稀有度加成（Rare +0、Unique +1），即普通／稀有敌人一只、独特敌人两只；不是假设达到 20 级 `number_of_wolves_allowed` 的六只上限。只有这个效果是所选主技能时，这些狼才计入 `MinionPresenceCount` / `SummonedMinion`；不推算击杀频率或持续累积的狼数。

所选召唤效果的最小 sweep 仍有差异：PoBR 的两只狼使同颗宝石的 Mark 增加 6% 承伤，单只狼 DPS 为 49.8971；只保留召唤效果、去掉 Mark 时为 47.0728，钉定 PoB2 为 46.9565（剩余约 0.25% 来自伤害区间与敌人护甲取整）。这里保留现有 Mark 应用规则，不按参考值补伤害倍率。tuned 组的 Witchhunter 构筑上这个数是 0：它的 Living Lightning II 所在组没有能被辅助的技能，不会出现 Living Lightning；构筑码自带的参考值按 8 只计（+32%），比 PoBR 高。辅助宝石授予的召唤物在辅助能生效时也还没有创建。

### Minion Pact 与 Presence

Presence 内的召唤物数（`MinionPresenceCount`）计入计算中创建的每个召唤物，包括没有数量上限 stat 的同伴，例如 Rhoa Mount 的 Rhoa（技能类型 `Minion` / `Companion`）；PoB2 只计入技能带数量上限 stat 的召唤物。所以 Minion Pact（“Supported Skills remove {0}% of your maximum Life from a nearby Minion on use to deal {1}% more damage”，`support_minion_pact_user_life_%_removed_from_nearby_minion` / `support_minion_pact_damage_+%_final`）在只有这类召唤物的构筑上，PoBR 计入、参考值不计：benchmark 组的 Chaos Inoculation Deadeye 最大生命为 1，每次只从 4182 生命的 Rhoa 抽 1.5 点，35% 更多伤害生效。

宝石说明里的 “Only takes effect if you have a Minion with enough life to drain the full amount” 还没有建模（PoB2 也不检查），召唤物生命不够抽取时两边都会多算。

### Wolf Pack 数量

Wolf Pack 召唤的狼数按游戏的 `skill_wolf_pack_size` 随有效宝石等级取值（1 / 18 / 20 级为 3 / 5 / 6 只），整群狼都计入召唤物数，并假设都在 Presence 内（**没有游戏内实测**）。Licking Wounds（天赋 55375，“5% increased Life and Mana Regeneration Rate for each Minion in your Presence, up to a maximum of 40%”）因此按整群计：tuned 组 Ritualist Purity of Fire（d557864ceebc）的 `LifeRegenRecovery` / `ManaRegenRecovery` 为 104.4 / 40.8，参考值 91.2 / 33.2。

侧栏的 `ActiveMinionLimit` 是主技能实际创建的召唤物上限，没有上限 stat 的召唤物为 1；PoB2 对这类主技能输出 0：tuned 组三个 Spirit Walker（Zekoa the Headcrusher）构筑 PoBR 为 1，参考值为 0。

### 敌人额外混沌伤害

Evergrasping Ring 的 “Enemies in your Presence Gain (6-12)% of Damage as Extra Chaos Damage”（游戏 stat `enemies_in_presence_non_skill_base_all_damage_%_to_gain_as_chaos`）按 stat 名是敌人全部伤害的份额：敌人的每种伤害都按这个比例额外获得混沌伤害，再按敌人对两种类型的加成缩放（`calc::ehp::assemble_enemy_damage`）。PoB2 只从物理伤害获得，所以有这枚戒指的构筑 EHP 比参考值低：tuned 组的 Ritualist Purity of Fire 三枚戒指共 39%，TotalEHP 140,103 对参考值 174,152；Stormweaver Frost Bomb 14%，12,061 对 14,185。

### 最强致盲效果

技能自己或它的辅助宝石带的 “increased Blind Effect”（`blind_effect_+%`，例如 Blind II 的 20%）只作用于这个技能施加的致盲。致盲不叠加，敌人身上只有一个，PoBR 取角色的技能里最强的那个（`skill_env::strongest_skill_blind`），转给敌人，与角色自己的 “increased Blind Effect” 相加。PoB2 把这条加成按每个被辅助的技能各算一次、全部加到角色身上：tuned 组的 Amazon（Rain of Arrows）有五个技能带 Blind II，PoB2 的致盲是 −40% 命中，PoBR 是 −24%，闪避率 72% 对参考值 76%，TotalEHP 随之较低。

### 最低元素抗性

“Enemies in your Presence Resist Elemental Damage based on their Lowest Resistance”（`enemies_in_presence_elemental_damage_resisted_by_lowest_elemental_resistance`）是敌人的属性：Presence 内的敌人承受火、冰、电伤害时都用三种元素抗性中最低的那个，穿透按所用的那种抗性取（穿透降低的是它写明的抗性），命中、持续伤害和异常都经过 `calc::offence::enemy_resist_final`。游戏另有一条 “Elemental Damage from your Hits is Resisted by the enemy's lowest Elemental Resistance” 才限于命中。PoB2 把两条都只用于命中，所以点燃这类持续伤害在敌人某一抗性被降低时比参考值高：tuned 组的 Gemling Legionnaire Twister 点燃为参考值的 1.22 倍。**没有游戏内实测**。

## 冻结语料中逐行解释的参考值差异

以下数字保留原调查的冻结语料含义，本次文档审查没有重跑数值。详细豁免仍以 `known-differences.json` 的构筑、stat 和输入 hash 为准；解释一行不能推及整个 DPS 或 EHP。

### 致盲、图腾与受击池

冻结 `amazon-02` 的 8993 闪避对 82 级敌人 2114 命中，最强 Blind II 的单份 20% 效果使敌人少 24% 命中，得到取整后 55% 闪避、16% Deflect。PoB2 从 Pounce 三个主动效果重复计辅助，得少 32% 命中、58% 闪避、18% Deflect。只将闪避／偏斜行归因为致盲；独立的 ES／恢复容量问题已通过按 Discipline 自己武器组的树检查需求修复，不能把整个 EHP 归为致盲差异。

2026-10-26 语料还显示：Spell Totem 游戏等级上限可到 3、施放时间 0.5 秒；钉定导出器隐藏辅助把上限覆盖为 2，并用 0.6 秒放置回退。Mortar Cannon 原生为 1 秒。非图腾主技能没有“当前所选图腾”容量；容量本身也不能建立召唤数量状态。

Runic Tempering 的条件减伤需要持有 Runic Plates，启用技能但未给状态不能激活。导入镶嵌词条保留当时值，例如 Zalatl 的 3% 魔力不改成现表 5%。Blood Mage03 的 oracle EHP 还追到致命伤后正生命浮点残余，其递归批量受击结果不能作正确目标。

### 仍未确认的 Storm Mage 生命

Lich02 的 Storm Mage 在识别按名 +3 等级后达宝石 38／怪物 76。基础生命 4729，213% increased 得 14801.77。导出器另扣怪物 15% ES 份额，约剩 12582。现有 `energyShield` 字段和 schema 的转换注释都来自 vendor 提取，不能独立证明游戏应扣生命还是额外获得 ES。保持未决，不改公式，也不标已知参考错误。

### 授予技能需求与装备来源

冻结 top-ten 调查有 23 条剩余属性需求差异，参考值恰等于护身符授予技能（Eternal Rage、Wolf Pack、Trinity、Cast on Critical、Cast on Elemental Ailment）的名义需求。与授予匹配的普通导出组是同一技能，不是额外装备的宝石，不进 `ReqStr/ReqDex/ReqInt`；可用等级另检查，独立真宝石仍计需求。回归覆盖十种授予／等级组合和 Gemling 8% reduced。

Witchhunter10 的 Megalomaniac 授予 Leather Bound Gauntlets（51446），1323 手套护甲给 1323 基础闪避，影响护甲／闪避、Sorcery Ward 和后续防御；导出器漏该天赋。Infernalist03 的 Runeseeker 导入行是 45% less Life，PoB2 重建为两条 MORE 再乘和取整，PoBR 保留原文。四个 Warbringer 的六条防御行使用现有选定图腾数，不假设额外图腾。

### 有限药剂回复与点燃

五个冻结 Pathfinder 已在 `Flask1RecoveryPerSecond` / `Flask1EnergyShieldRecoveryPerSecond` 报告有限药剂回复，参考值把它们加进再生。PoBR 保留单次使用的独立输出；启用药剂不证明无限充能或连续覆盖，因此不新增持续再生。

Shaman08 配置的 15 级 Flame Wall 提供 18–26 火伤，Spark 因而可按现有异常模型点燃。导出器点燃层数潜力为零，是因为只读图腾上限 BASE，而技能给 OVERRIDE；不能据此认为附加火伤不能点燃。给定多个同时图腾后的准确稳态值仍未验证，两条相关行留在诊断队列。

### 同伴与旗帜的相互抵消差异

Tactician06 的 PoB2 已计 Watch How I Do It 武器分享，却漏了所选狼的插旗伤害。导入 `bannerPlanted=true` 支持原生激活光环快照，较早数值一致来自两个错误互相抵消。仍有未完全解释的剩余差异，不能把整条 DPS 注册为 known；旗帜攻速在默认口径单独测量。
