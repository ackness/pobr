# 与 PoB2 的关系：数据来源与已知差异

原则见[维护者指南的“与 PoB2 的关系”](maintainer-guide.md#与-pob2-的关系逐步独立)。本文列出 PoBR 仍依赖 PoB2 的数据、按游戏规则有意与 PoB2 不同的地方，以及冻结语料中逐行解释过的参考值差异。

登记方式：差异的规则与依据写在这里（规则本身属于某个机制时写在对应的[机制约定](mechanics/README.md)并在这里链接）；逐行的构筑与 stat 写进 `devs/scripts/known-differences.json`，`reason` 指向本文或对应的机制文档。导出工具没有实现的词条规则只标 `exporter: "absent"`，由参考口径自动排除，不在这里逐条登记（见[参考口径](diagnostics.md#参考口径导出工具规则与语料)）。

## 数据来源现状与退出顺序

现状清单（`data/<版本>/` 各文件的 `_meta.generator` 可核对）：

| 来源 | 内容 |
|------|------|
| 游戏数据（GGG `.dat` → `pobr-data-adapter`） | `base/` 下的物品基底、词缀、stat、技能与等级（含技能的显示名 `ActiveSkills.DisplayedName`：buff 名、`AffectedBy<名字>` 条件和诅咒优先级都按它，见 `skill_env::buff_skill_name`）、stat set（含无值 stat，如 `base_is_projectile`；召唤物技能另有按使用者等级取值的效能成长）、天赋树、宝石（含授予的效果与宝石标签，游戏表 `GemEffects` / `GemTags`；每个宝石等级需要的角色等级，游戏表 `ItemExperiencePerLevel`）；`overlay/gem_quality_stats.json`；`base/monster_resistances.json`（`MonsterVarieties` → `MonsterTypes` → `MonsterResistances`，普通稀有度的 `Fire1/Cold1/Lightning1/Chaos1`，合并全部链接并保留负数弱点，用于召唤物基础抗性） |
| 从 PoB2 Lua 抽取（`sync-pob-catalog`） | 词条解析规则 `mod_parser_rules.json`、特殊词条 `generated/special_vendor.json`、`mod_scalability.json`、技能 stat 映射 `skill_stat_map.json` / `skill_overrides.json` / `stat_set_labels.json`、配置项 `config_options.json`（含显示条件 `if*` 与静态说明 `tooltip`，只决定配置页显示哪些选项，不决定计算效果）、暗金 `uniques.json`、基底修正 `base_item_overrides.json`、符文 `runes.json`、`gem_effects.json`（宝石与效果的链接已由游戏表写进 `base/skill_gems.json`；此文件留给旧快照和几颗宝石共用的效果，以及效果的显示顺序 `GemEffects.GrantedEffectDisplayOrder`，`pathofexile-dat@15.2.0` 使用的社区 schema 尚未命名这一列，不能按列名导出，显示顺序继续由此文件提供）、召唤物与幽魂（`overlay/minions.json` 的 `monster_variety_id` 从 `Export/Minions/Minions.txt` 的 `#monster` 映射抽取；已入库的召唤 stat 没有可反查的 variety 链接（只有 monster hash 等数值）；幽魂与驯服野兽的 id 已是游戏 variety id。只从 PoB2 获取这一映射，抗性数值读游戏数据）、触发与幻影配置、`curse_priority.json`、`catalysts.json`，以及 stat 描述 `stat_descriptions.json`（来自 PoB2 的导出，不是直接来自游戏客户端）和由它派生的 `stat_id_map.json` |
| 按 PoB2 语义手工整理 | `overlay-common/` 的 `buff_definitions.json`、`special_mods.json`、`high_precision_mods.json`、`local_mods.json` |
| 按游戏数据手工整理 | `overlay-common/item_granted_skills.json`（游戏表 `ItemInherentSkills` 中 `NoReservation` 的基底；adapter 读到这张表后改为生成）、`energy_config_options.json`（Energy 外部事件输入，由配置目录抽取器合入） |
| 回归基准 | `examples/demo-bd-test/builds/*/meta.json` 的对照值由 `tools/pob2-oracle` 重录 |

所以目前机制覆盖不会超过 PoB2，版本更新也要等 PoB2 的导出。建议的退出顺序（每一项单独立项）：stat 描述改为直接取自游戏数据；词条从“正则规则”改为“游戏 stat id → 引擎 modifier”的自有映射（可用现有 `stat_id_map.json` 一次性引导，之后新 stat 以“未映射”出现在审计里而不是等 PoB2）；技能 stat 映射和配置项冻结为自有数据后按游戏数据差异维护；回归基准增加游戏内实测值。

## 已知的有意差异

- Boss poise uses the current game table rather than vendor presets. In patch 0.5.5.4.2, `MonsterVarieties` row 1946 (Xesht) has `PoiseThreshold = 1700` (17×), while the pinned PoB2 Xesht preset uses 838% MORE (9.38×). The curated game-source rows and export hash are in `data/overlay-common/boss_poise_thresholds.json`; map-boss mode 313 agrees with the older 213% MORE preset. On worked Shaman Spark `5dcf9ecdd375`, `HeavyStunBuildupAvg` is 3.627930% versus the oracle's 6.578395%; the dominant oracle/engine factor is 17/9.38 = 1.812367 (remaining hit/rounding difference about 0.05%). Pinnacle and Uber use Xesht as the representative monster; no distinct Uber Xesht variety exists in this export. This threshold also affects Freeze/Electrocute poise buildup; it is separate from unique rarity's 6×.
- 偷取每种资源只取恢复速率最高的有效实例（[0.5 补丁说明](https://www.pathofexile.com/forum/view-thread/3932540)：“Only a single Leech instance per resource (Life, Mana, or Energy Shield) can apply at a time.”）；[游戏关键词](https://poe2db.tw/us/Keywords)给出一秒基础恢复时间，偷取量和偷取速度分别修正。钉定的 PoB2 仍叠加实例，保留按资源池计算的单实例偷取量上限，并把“increased amount … Leeched”当作最大速率加成。tuned 组的 Deadeye（Twister，`bed1edc6d8b2`）`ManaLeechGainRate` 为 PoBR 80.72 对参考值 166.17；Pathfinder（Ice Shot，`728cbde29279`）同一字段为 162.96 对 190.12；Blood Mage（Arc，`5bc8be0fa967`）`LifeLeechGainRate` 为 513.68 对 575.23。去掉单实例偷取量上限后，原来吻合的三行 `LifeLeechGainRate` 也出现有意差异：Warbringer（Shield Wall，`1e4e329e5e5c`）为 582.978 对 552.40416，Blood Mage（Comet，`bc750d615159`）为 641.43144 对 207.01274，Gemling Legionnaire（Frostbolt，`42f790c0f9ba`）为 0.2 对 0.0635；这些行已登记在 `devs/scripts/known-differences.json`。前述默认速率上限仍是兼容约定，平均命中频率也只是覆盖时间估计；这些数值不是游戏内实测结果。
- PoB2 用自己的符文表覆盖物品原文；PoBR 以物品原文为准。同一物品上同一 stat 的符文在原文里合成一行：tuned 组的 Infernalist（Comet）武器上是一行 “45% less maximum Life”，PoBR 按 0.55 计，PoB2 按两块符文的 15% 与 30% 分别连乘（0.595），所以它的生命比参考值低。
- 同一个诅咒由两个技能组施加时，敌人身上只有最强的那一份（“Debuffs of the same type do not stack, and only the copy with the strongest effect applies”，[Curse](https://www.poe2wiki.net/wiki/Curse)），比较的是各份自身与其辅助宝石的诅咒效果（`skill_env::buff_specs::curse_strength`）。PoB2 按自己的优先级表挑，光环来源优先：tuned 组的 Acolyte of Chayula（Decompose）有 Doedre's Undoing 组和 Blasphemy 组两份 Despair，PoB2 用后者（敌人混沌抗性 −31），PoBR 用更强的前者（−58）。
- 尸体爆炸的伤害类型取自游戏 stat：`corpse_explosion_monster_life_permillage_<类型>` 与 `corpse_explosion_monster_life_%_<类型>` 写明类型；不带类型的 `corpse_explosion_monster_life_%`（Fiery Death）按它的文本 “Explosion deals base Fire Damage equal to {0}% of the Corpse's Maximum Life” 是火焰伤害（`skill_stats::corpse_explosion_damage`），PoB2 按物理计。
- Doedre's Undoing 的 Witchtoad 每 0.25 秒生成一只，PoB2 把这个间隔乘以施法时间，施法越快爆炸越多且没有上限；PoBR 按“区域数 × 1000 / 间隔”计算，与施法速度无关（见 `skill_stats::granted_skill_modifiers`）。Dark Consequences 自带魔力消耗为 0，但 Archmage 的“Non-Channelling Spells cost an additional … of your maximum Mana”仍给它本身增加消耗；这不是继承诅咒的消耗。tuned 组 Blood Mage 的 5.7% 最大魔力经 Atalui 转成生命、Efficiency II 减少 40% 后为每次爆炸 199 生命，两区域每秒 8 次为 1592。PoB2 的每秒生命消耗却用它的施法速率 2.14/s，得到 425.86；PoBR 保留按爆炸频率支付，伤害计算不变。
- The frozen Forbidden Rites 2026-10-26 rank files `blood-mage-03/04/06/08/10` and `chronomancer-04` select Dark Consequences, despite their Impending Doom titles. The same Witchtoad interval gives 8/s; after dividing out the oracle's cast-speed-dependent rate, the five Blood Mage TotalDPS differences are below 1.5%. Chronomancer also differs in Hindered state, so only its rate rows are classified as explained. New known-difference entries bind the decoded XML SHA-256 as well as the short rank name, preventing reuse against another snapshot.
- Instruments of Power 的 “50% reduced bonuses gained from Equipped Focus” 让 focus 上的 stat 按一半给予，“+# to Level of all X Skills” 不低于 1（[Instruments of Power](https://www.poe2wiki.net/wiki/Instruments_of_Power)）：focus 的 +2 法术等级计 +1，+1 仍为 +1（`skill::resolve::scale_slot_gem_property_bonuses`）。PoB2 减半 focus 的其他词条，宝石等级不减半：demo 语料的 Disciple of Varashta（Comet）的法术比参考值低一级，TotalDPS 0.86×，`ninja_parity` 的进攻与持续伤害基线各降 1 / 2 格。
- Bonestorm（“Channel to conjure a swarm of bone spikes in the air, then release to fire them at enemies and explode”，“Can fire up to 20 Projectiles”，[Bonestorm](https://poe2db.tw/us/Bonestorm)）引导时每次施法召出一根骨刺，释放时全部射出：PoBR 按每根骨刺命中一次计，每秒命中数即施法速率；一次引导最多 20 根（`base_number_of_allowed_bone_storm_projectiles`），不改变每秒的骨刺数。PoB2 的引导层数（`Multiplier:BonestormStage`）没有配置时为 0，按最短引导时间 0.3 秒（`base_minimum_channel_time_ms`）命中一次：tuned 组的 Abyssal Lich（Bonestorm）PoB2 每秒 3.33 次，PoBR 11.96 次。**没有游戏内实测**：假设每根骨刺都命中目标。
- “Minions have 10% chance to inflict Withered on Hit” 在没有召唤物的构筑上，PoB2 把这条词条当作玩家自己能施加 Withered，并按配置的层数给敌人加混沌承伤；PoBR 把它交给召唤物，角色自己的技能、Presence 或已有的召唤物能施加 Withered 时才计入配置的层数（`calc/withered.rs`），所以这类构筑会比参考值低。每层 5%、最多 10 层两边一致。
- Lament / Portent / Absent Amulet 授予的技能不保留精魄（游戏表 `ItemInherentSkills` 的 `NoReservation`，三个基底列在 `overlay-common/item_granted_skills.json`）。PoB2 导入角色时把这个技能再列成一个普通宝石组并按宝石保留 30 / 60 / 100 精魄，所以这类构筑码自带的 `SpiritUnreserved` 常为负数，而游戏里保留量不可能超过精魄总量。PoBR 把“排在组首、等级与授予等级相同、无品质、未腐化”的那个普通组认作被授予的技能本身，不计保留，也按授予技能的需求降级（见[授予技能](mechanics/items.md#宝石等级品质与授予技能)）；其余同名宝石照常保留（`calc_orchestrator/granted_skills.rs`）。其他物品授予、需要保留精魄的技能也是这样认出的同一个技能，只保留一次；PoB2 的物品组和宝石组各保留一次（tuned 组 Gemling Legionnaire Frostbolt 与 Oracle Grim Pillars 胸甲授予的 Blink：PoB2 120，PoBR 60）。PoB2 只对仅由物品授予的技能（`fromItem`，如 Purity of Ice）这样合并，所以 Absent Amulet 的 Eternal Rage 在 PoB2 里是两个组：降级的物品组和未降级的宝石组。该技能上的辅助宝石追加的精魄也一并免除，PoB2 仍计入：依据是权杖授予的技能在游戏数据里以 `reservation_multiplier = -100` 表达“不保留”，辅助的追加量随之归零；这一点没有游戏内实测。
- Rain of Arrows 的命中频率按游戏的两条规则计算（`pobr-core/src/calc/rain.rs`）：“One arrow lands every 0.08 seconds”（`rain_of_arrows_delay_per_arrow`，“Modifiers to Projectile speed apply to arrow landing frequency”）和 “Can Hit each enemy only once every 0.12 seconds”（`rain_hit_delay_ms`）。命中只能来自落下的箭：间隔过后落下的第一支箭命中，所以每 ⌈0.12 ÷ 落箭间隔⌉ 支箭命中一次；一次使用射出 N 支箭（技能的投射物数量），一场箭雨最多命中 ⌈N ÷ 该数⌉ 次，使用得慢、箭雨在下一次使用前已经落完时按每次使用的命中数封顶（“Maximum 1 arrow rain”）。PoB2 把命中间隔记为 max(0.08, 0.12) 秒，等于假设敌人每次能被命中时恰好有箭落下，也不计投射物速度：没有投射物速度时游戏规则给出每秒 6.25 次，PoB2 为 8.33 次；tuned 组的 Rain of Arrows 构筑（169% 增加的投射物速度和 Projectile Acceleration III 的 40% 更多投射物速度，每 0.0212 秒落一支）是每 6 支箭命中一次，每秒 7.85 次。以下是假设，**没有游戏内实测**：每支落下的箭都打到敌人（箭雨半径 3.6 米、单支箭的冲击半径 0.7 米，对单个敌人实际更少，取决于体型）；落箭节奏均匀，按游戏的数值、不按服务器帧取整；角色持续使用该技能；品质的 “Arrows fall #% faster” 是箭下落的速度，不改落箭频率。同样带 `rain_hit_delay_ms` 的 Firestorm、Icestorm 见下一条；Hailstorm Rounds（`rain_hit_delay_ms` = 200）的每次发射提供一次落点，连续发射时每 ⌈0.2 × 发射速率⌉ 次落点命中一次，再按弹匣与装填周期折算（`perform::fill_crossbow_reload`），一次攻击的多个投射物共用这个命中间隔，命中限制不减慢弹药消耗；`siege_cascade_fall_time_ms`（300）是飞行时间，不是落点间隔。Hailstorm Rounds 没有模拟散布和弹匣交界处的命中锁定相位，**没有游戏内实测**。参考口径（`on_reference_basis()`）按 PoB2 的计法比较：箭雨每当敌人能再被命中时就算一次，Hailstorm Rounds 取发射速率与每秒 5 次中的较小值。
- 球与风暴的命中不随施法速率（`pobr-core/src/calc/periodic.rs`，stat 由 `rules::spell_hits::modifiers` 映射）：Ball Lightning “Hits enemies in range every {0} seconds”（`ball_lightning_base_hit_frequency_ms` = 200），Solar Orb “Pulses every {0} seconds”（`solar_orb_base_pulse_frequency_ms` = 1400）；Firestorm / Icestorm 每 `fire_storm_fireball_delay_ms`（200 / 150）落一次，按上一条箭雨的落点规则受 `rain_hit_delay_ms`（150）限制；Thunderstorm “Hits one target every {0} seconds”（`lightning_storm_hit_frequency_ms` = 350），同一敌人 `lightning_storm_hit_prevention_duration_ms`（500）内只被命中一次，取两者约束下的潜在频率，不模拟 `lightning_storm_hit_frequency_variance_ms` 的随机间隔。角色维持一场风暴或一个球，多个不叠加；有持续时间的按 min(持续时间 × 每秒使用次数, 实例上限) 计覆盖率（最多 1），Ball Lightning 没有这类持续时间 stat，假设目标一直在范围内、不估算球的行进。Ball Lightning、Solar Orb 的其他 stat set 不继承这个频率。PoB2 假设风暴与球始终覆盖目标，参考口径关掉 `DataOrchestratorOptions::periodic_uptime` 按它比较。**没有游戏内实测**：假设敌人始终在范围内，每个符合间隔的落点都命中。
- 引导攻击（Detonating Arrow、Supercharged Slam、Perfect Strike、Gathering Storm、Snipe、Shield Charge，`calc::channel::skill_modifiers`）的一次使用近似为“释放前引导 + 释放”（`pobr-core/src/calc/channel.rs`）。引导取 `base_minimum_channel_time_ms`、`channel_start_lock_cancelling_of_attack_time_%` × 攻击时间、层数 × 攻击时间中最大的一个；完美时机只给出窗口宽度（如 `perfect_strike_timing_window_base_ms`），没有开启时间的 stat，所以完美与普通 stat set 各算自己的伤害，时间都用最短引导 / 起手锁定。释放取 `channel_end_duration_as_%_of_attack_time` 的比例，或 `channel_skill_end_animation_duration_override_ms`（有时优先，两者不相加）；比例随攻击速度缩放，固定毫秒不缩放，只有动画倍率、没有基础时长的阶段不另计。层数同时用于伤害和时间，默认取上限（含品质），构筑配置的 multiplier `ChannelStages`（配置输入 `skillChannelStages`，Web 技能页的层数输入）可覆盖（取整并限制在 1 到上限）：Detonating Arrow 每层 `detonating_arrow_all_damage_%_to_gain_as_fire_per_stage` = 120；Supercharged Slam 每层 “40% more Damage with Hits for each stage”（只加命中），“Releases 1 Aftershock per Stage”，冲击 stat set 计冲击加全部余震、余震 stat set 只计余震，用等级行 850 毫秒不受攻击速度影响的攻击时间，余震延迟不阻止下一次引导。Shield Charge 按 1300 毫秒最短引导，不模拟行进与碰撞。PoB2 多只把释放阶段当作命中间隔、默认一层并用武器的攻击时间。**没有游戏内实测**。
- 带 `base_skill_show_average_damage_instead_of_dps` 的技能（Grim Pillars、Lightning Warp、Vile Disruption、Skyfall、Living Bomb、Volcano、Incinerate、Rake、Parry、Raise Shield、Hand of Chayula、Stampede、Gathering Storm、Perfect Strike 等）在游戏面板上显示平均伤害；这个 stat 没有词条文本，不改变伤害或使用速率。PoBR 的 `CombinedDPS` 仍是每秒伤害：命中 DPS 加持续伤害 DPS，再计 Culling Strike；PoB2 在这里改填平均命中伤害，也不加它已算出的持续伤害（Incinerate 的 `base_fire_damage_to_deal_per_minute`、Living Bomb / Volcano 的点燃、Rake 的流血）。这是输出口径不同，不能拿这一行当作伤害倍率去补。Vivid Stampede（`skill_triggered_on_hit_while_have_vivid_stag_wisps`，攻击命中触发、每个 Vivid Wisp 一只鹿）同样只显示平均伤害：PoB2 的 TotalDPS 为 0、CombinedDPS 是每次触发的伤害；PoBR 暂按武器攻击速率每次攻击一只估算，精灵数量与触发来源还没有建模，两边不能比较。
- Repulsion Wave、Shield Wave、Combust 与 Caltrops 保留平均命中伤害，事件频率未建模时 `ActionRate` / `TotalDPS` 为 0。官方 `ActiveSkills.Description` 中，Repulsion 需要命中被诅咒的敌人，Fortifying Cry 需要后续盾牌攻击造成伤害，Temper Weapon 需要被强化的攻击命中；Trail of Caltrops 在闪避时布置，敌人触发时才造成伤害。`caltrops_on_dodge_X_triggers = 3`、首次在闪避的 5%、间隔为闪避的 20% 只定义布置时机，不能从武器攻速推算闪避或敌人触发频率；Combust 的 100% 触发几率也不能替代强化次数与攻击来源。`CombinedDPS` 仍是每秒伤害，不改填平均命中；因此 sweep 的 `pobr_zero` 分类可能已有非零 `AverageHit`，不能据此判定没有伤害。20 级 Repulsion Wave 的基础物理伤害为 128–192 × 2.04，Shield Wave 为 (85–128 + 每完整 15 点盾牌护甲的 5–7) × 2.04。Blood Explosion 为 196–294 × 4.08、基础暴击率 5%；其 sweep 差异来自前述攻击命中检查，不补伤害倍率。Blood Explosion 的现有每秒一次口径尚未按失血、重度眩晕或死亡事件建模。这些事件模型均未做游戏内实测。
- 伤害区间与命中率保留小数：Earthquake / Stampede 的 Wooden Club 6–10 乘等级行的 `damage_multiplier` 1.25 为 7.5–12.5，Toxic Growth 的 6–9 乘 0.62 为 3.72–5.58；PoB2 先把伤害上下界取整（8–13、4–6）、命中率取整到百分数，低伤害的合成构筑上差别可超过 5%。这是计算精度的约定，游戏内部是否取整**没有游戏内实测**。

  The 2026-10-16 ranged sweep has the same precision difference. All six fixtures use
  the game's Makeshift Crossbow (`base/base_items.json`, `FourCrossbow1`):
  7–12 Physical damage, 625 ms attack time and 5% base crit. Level-20 damage
  multipliers and conversion percentages below come from the first stat set
  in `base/granted_effect_stat_sets.json`; attack-speed multipliers come from
  `base/granted_effect_levels.json`. No added damage or damage increases apply.
  Each bounds cell shows PoBR's result after conversion → PoB2's integer bounds
  before mitigation and crit; the arrows do not indicate an engine change.

  | Selected effect | Damage multiplier / conversion | Physical bounds | Elemental bounds | TotalDPS PoBR / PoB2 |
  |---|---|---|---|---|
  | `IceShardsPlayer` | 0.29 / 60% Cold | 0.812–1.392 → 1–1 | 1.218–2.088 → 1–2 | 5.423944799 / 4.952566667 |
  | `ShockburstRoundsPlayer` | 0.36 / 40% Lightning | 1.512–2.592 → 2–3 | 1.008–1.728 → 1–2 | 4.615380330 / 5.334733228 |
  | `PermafrostBoltsPlayer` | 0.49 / 80% Cold | 0.686–1.176 → 1–1 | 2.744–4.704 → 3–5 | 1.183943073 / 1.279610526 |
  | `RapidShotPlayer` | 0.94 / 60% Fire | 2.632–4.512 → 3–5 | 3.948–6.768 → 4–7 | 23.651653516 / 24.984921520 |
  | `ArtilleryBallistaProjectilePlayer` | 0.40 / none | 2.8–4.8 → 3–5 | — | 0.306053311 / 0.324168000 |
  | `SiegeBallistaProjectilePlayer` | 0.66 / 60% Fire | 1.848–3.168 → 2–3 | 2.772–4.752 → 3–5 | 0.807980742 / 0.850941000 |

  Both sides use one projectile hit per action in these selected stat sets,
  75% Physical reduction and 50% elemental resistance. With physical and
  elemental midpoints P and E, DPS is `(0.25P + 0.5E) × h × (1 + 0.05h) × rate`:
  PoBR retains hit chance h = 0.775345425; PoB2 uses 0.78 (also in crit
  confirmation). In table order, both sides' sustained rates are
  6.111111111, 4.787401575, 0.701754386, 8.221225710, 0.4 and 0.4/s.
  The four ammo skills' firing rates are 8.8, 6.4, 1.6 and 8.8/s; magazine
  sizes are 16, 19, 1 and 100, with reload times 0.8, 1, 0.8 and 0.8 seconds.
  Substituting the respective bounds and h reproduces every TotalDPS above;
  no projectile-count, damage-multiplier or reload correction is justified.
  This confirms the existing precision convention, not the game's internal
  rounding. No benchmark or tuned build carries these six skills at this
  audit, so no worked-build row is added to `known-differences.json`.

- Bursting Fen Toad、Conductive Runes 的 Explosion、Blood Explosion 与 Repulsion Wave 是非武器攻击（游戏类型 `Attack` / `NonWeaponAttack`，Bursting Fen Toad 的说明是 “dealing Attack damage”），stat set 没有 `global_always_hit`，照常检查命中；PoB2 给它们的基础标志缺少攻击，不检查命中。没有游戏内实测证明这类爆炸不能被闪避。Primal Strikes 的 Exposure（`all_exposure_on_hit_magnitude` = 50，“The third attack is a large swipe that inflicts Elemental Exposure”）只在第三击的 stat set `PrimalStrikesFinalPlayer` 上，计算普通击时不假设已用过第三击；PoB2 默认计入（敌人元素抗性由 50 降到 25）。连段的覆盖率还没有建模。
- 主 stat set 带 `base_deal_no_damage` 的效果（Conductive Runes 的放置、Rolling Slam、Spearfield、Tempest Bell 的主 stat set，Hollow Focus 的 Bells）PoBR 为 0，钉定的 PoB2 可能给它们算出默认武器或附加伤害（见[无伤害效果](mechanics/offence.md#命中技能类型与无伤害效果)）。
- Black Powder Blitz 按游戏等级表的冷却限制使用频率；Requiem 按 `crossbow_skill_has_no_ammo` 不计弹匣与装填时间，不能用重复装填降低它的持续射击频率。两者与钉定 PoB2 的默认周期可不同，不改伤害倍率追平。
- 图腾放置时间按召唤技能自己的 `cast_time`：Spell Totem（`SummonMetaTotemSpellTotemPlayer`）500 毫秒，Shockwave Totem 600 毫秒；PoB2 的 Spell Totem 按 600 毫秒，tuned 组 Grim Pillars 构筑的 `TotalTotemPlacementTime` 与按放置频率计的每秒消耗随之不同。
- The Vertex 的 “Equipment has no Attribute Requirements”（游戏 stat `global_equipment_no_attribute_requirements`）让所有装备的属性需求归零，包括武器；PoB2 未应用这行。Spirit Walker Zekoa（e071）的武器需求因此不是 180 力量 / 450 智力，基准组 Blood Mage Impending Doom（add1）的 Array Buckler 也不要求 78 敏捷；宝石和辅助宝石的需求照常计算。
- 物品授予的技能（例如 “Grants Skill: Level N Eternal Rage”）的属性需求只决定它降到哪一级（见[授予技能](mechanics/items.md#宝石等级品质与授予技能)），不计入 `ReqStr` / `ReqDex` / `ReqInt`；构筑码另列的同等级普通组被认作同一技能，也不计入（`calc_orchestrator::requirements::requirement_snapshots`）。PoB2 按宝石计入，因此 Wolf Pack / Eternal Rage 抬高参考值的力量需求，Trinity 抬高敏捷需求，Blink / Cast on Critical 抬高智力需求；权杖的 Purity of Ice 和护身符的 Time of Need 也适用。
- Cast on Critical 里的法术，PoBR 按宝石触发它的频率计算 DPS 和使用速率（`pobr-core/src/calc/energy.rs`）；PoB2 没有能量模型，它的触发配置要求同组有攻击技能（PoE1 的用法），找不到就退回法术自身施放速率，构筑码自带的参考值因此是自身速率口径。规则取自游戏数据和文本：能量上限是支援半的 `generic_ongoing_trigger_1_maximum_energy_per_Xms_total_cast_time = 10`（每 0.1 秒基础施法时间 10 点），技能自带的 `total_cast_time_+_ms` 按术语表 Energy 条目计两倍（Comet 为 300）；每次暴击获得 `cast_on_crit_gain_X_centienergy_per_monster_power_on_crit = 100`（每点敌人 Power 1 点）× 暴击伤害占敌人异常阈值的比例 × 能量获取修正（`energy_generated_+%`，即“Meta Skills gain #% increased Energy”）；独特怪 Power 恒为 20；被触发的技能不产生能量，所以充能来源是角色自己使用的其他伤害技能，取填满最快的一个，没有来源时法术保持自身速率。触发频率 = 来源技能每秒暴击次数 ÷ ⌈上限 ÷ 每次暴击能量⌉。**没有游戏内实测**，以下是假设：比例按“减免前暴击伤害 ÷ 阈值”线性计算且不封顶（玩家测得的公式，游戏未公布）；每次暴击取平均暴击伤害；每次使用按命中一次算；首领的异常阈值只按等级取表（游戏 stat 表里有 `ailment_threshold_pluspercent_final_for_map_boss`，取值不在已入库数据里）；获得能量时视为“近期造成过暴击”。自动触发按整次事件充满，多出的 Energy 丢失；半额返还按概率加权下一轮的事件数（`calc::energy::triggers_per_second`）。`DataOrchestratorOptions::energy_triggers = false` 保留参考口径的自身速率或既有触发配置路径。
- Cast on Elemental Ailment、Cast on Block、Cast on Minion Death、Spellslinger、Elemental Invocation 同样按 Energy 计算（`skill_env::triggers::energy_trigger_modifiers`、`calc::energy::apply_energy_trigger`）。格挡每次给 25，召唤物被击杀时每点 Power 给 50；Spellslinger 按自施法的基础使用时间每 10 ms 给 0.2，Total use time 的加法同样计双倍。元素事件只有点燃、感电、冻结，没有冰缓；每点敌人 Power 的感电给 1、冻结给 10，点燃再乘新实例完整减伤前伤害与异常阈值之比，不封顶。20 级前四族自带 57% increased Energy，Elemental Invocation 为 68%；兼容 meta 的增益与半额返还辅助作用于 meta 本身，不要求能辅助 payload。Spellslinger 与 Elemental Invocation 的 500 储能和每轮法术消耗分开，后者每品质加 5 储能；自动触发的容量按同组被支持法术的总消耗计算。
- Energy 的外部事件输入由 `config_resolve::resolve_config` 读取：`energyIncomingHitRate` 是可格挡来袭命中／秒，乘有效格挡率；未填才回退到显式 `enemySpeed` 的 1000／毫秒，显式 0 优先且表示零事件，`enemySpeed = 0` 同理。该回退把一次敌人动作当作一次可格挡命中，不另算敌人命中或玩家闪避。`energyMinionDeathsPerSecond` 只计被击杀，不模拟到期、生命或死亡；`energyMinionPower` 默认 1。两种速率缺失或没有格挡等必要机制时保留法术自身频率；显式零事件停止触发。`energyInvocationUsesPerSecond` 未填时假设有能量即使用，受冷却和服务器 tick 限制；显式 0 即使无来源也停止使用。Invocation 保留余能，按平均返还消耗及 ⌊容量／整轮消耗⌋ × 使用频率限制吞吐；不分别排程 payload，不模拟整轮超容量时的部分触发。上述外部速率和 Invocation 使用模型**没有游戏内实测**。
- Energy 的来源近似是按基础频率预选最多六个玩家自用、非触发且非持续保留的技能，再取吞吐最高的一项；不叠加多来源轮转、多目标或额外投射物重叠。异常事件用单目标命中频率与施加概率；感电沿用非暴击来源近似，冻结按冷伤 buildup 向上取整所需命中数，不计冻结持续时间和再冻结间隔。混合异常按概率平均每事件 Energy，不模拟同一击的事件顺序；点燃比例、这些事件率和来源策略均**没有游戏内实测**。Barrier Invocation 与 Cast While Channelling 的持续 Energy 吞吐尚未建模，后者只展示首次充满时间。
- CLI/WASM 的 `energy_trigger` 给出储能、每轮消耗、来源、事件率、每事件 Energy、返还和频率上限。六族缺来源时带 `at_own_rate: true`、`source: null` 与稳定的 `missing_source`，命中与持续伤害保留自身速率；`SkillTriggerRate` / `TriggerRateCap` 的明细给出自身速率与补充输入提示，原始触发率字段仍为未解出的 0。有来源时 CoC 的 `TriggerRateCap` 是来源暴击事件率。`ActionRate` / `TotalDPS` 的派生公式包含 Energy 调整，Full DPS 使用调整后的输出；CLI 保留 `energy_per_critical_hit` 兼容字段。四个配置项由 `data/overlay-common/energy_config_options.json` 经 `extract_config_options::append_energy_options` 合入目录，按稳定 meta id 的 `if_skill` 筛选，重新生成不重复。
- 图腾使用的技能（图腾宝石自己的技能，或插在法术图腾这类 meta 图腾宝石里的技能，游戏类型 `UsedByTotem` / `SummonsTotem`）按“角色有图腾、最近召唤过图腾”计算，施法和攻击不算角色自己的（不置 `CastSpellRecently` / `AttackedRecently`）。“每个已召唤图腾”类词条按 1 个图腾计，配置填了“已召唤图腾数”时按配置。PoB2 默认按图腾上限计，而它给法术图腾里的技能算出的上限不含宝石随等级增加的 `base_number_of_totems_allowed`（两个 tuned 构筑上是 2，按游戏数据 23 级以上的法术图腾是 3，再翻倍）。PoBR 的图腾上限 `ActiveTotemLimit` 只是展示（见[图腾](mechanics/minions.md#图腾与-ballista)），伤害仍不假设达到上限（`calc_orchestrator/prepare.rs`）。异常的层数同理：插在法术图腾里的技能，PoB2 按该技能自己的图腾上限乘层数，而上限的 stat 在图腾宝石上，它取到 0，点燃为 0；PoBR 按 1 个图腾的命中计算点燃（tuned 组两个 Grim Pillars 构筑的点燃只有几十，参考值为 0）。
- Charged Staff 的 buff 按 “Buff grants {0} to {1} Added Quarterstaff Attack Lightning damage per Power Charge Consumed, up to your maximum Power Charges”（`charged_staff_attack_minimum_added_lightning_damage_per_stack` / `charged_staff_attack_maximum_added_lightning_damage_per_stack`）计算：每消耗一个暴击球一层，最多为最大暴击球数，只加给 Quarterstaff 攻击。PoB2 把 One with the Storm 的 “count as consuming an additional Power Charge”（`quarterstaff_skills_that_consume_power_charges_count_as_consuming_x_additional_power_charges`）加在这个上限之外，最大 6 个暴击球时按 7 层计，且不限于 Quarterstaff 攻击；PoBR 按可消耗的暴击球数（当前减最小值）计，满球时为 6 层，不读取这个额外的球，暴击球未满时它也不计入（`skill_stats::player_buff_modifiers`）。
- Purity of Fire 这类光环的技能类型（`Buff`、`Aura` 等）里没有 `Spell`，“increased Critical Spell Damage Bonus” 这类法术词条不作用于它；PoB2 给它的 stat set 写了 `spell = true` 的基础标志，主技能是光环时暴击伤害加成按法术计（tuned 组的 Ritualist Purity of Fire，2.24 对参考值 2.49）。这一行对光环没有实际意义。
- Lich 的 Crystalline Phylactery 让其中的珠宝按两倍计（见 [`Build.jewels`](build-model.md#导入与构筑结构)），The Adorned 不作用于它。PoB2 不解析 “Effect of bonuses gained from Socketed Jewel”，并把 The Adorned 用在护符匣里的珠宝上，构筑码自带的参考值也是这样算的：tuned 组四个护符匣构筑（Abyssal Lich Twister 4028 / 9d1c、Lich Spark ce24 / 0a17）的伤害、暴击与个别抗性随之与参考值不同。
- Bear form 按游戏的形态说明 “+10 to Armour per level” 给 10 × 角色等级的护甲，并让 30% 的护甲也作用于元素伤害（`pobr-core/src/calc/bear_form.rs`，主技能带 `Shapeshift` 与 `Bear` 类型时生效）；PoB2 多给 10 点（10 + 10 × 等级），参考值因此略高（约 0.2%）。
- Pounce 的 Predator's Mark（“Marked target takes {0}% increased damage for each of its enemies within 6 metres of it, up to {1}%”，`skill_wolf_mark_damage_taken_+%_per_nearby_enemy` / `skill_wolf_mark_damage_taken_+%_cap`，品质加在等级值上，20 品质为 4% / 60%）按角色在 Presence 内已有的召唤物计数：计算中创建的召唤物，或配置的 “# of Minions in your Presence”（`MinionPresenceCount`，`skill_stats::curse_modifiers`）。计算 Pounce、Predator's Mark 或其他主技能时，不计 Pounce 在被标记的敌人死亡时召唤的狼：被计算的敌人活着时它们还不存在。显式选择 Pounce 的第三个效果 `WolfPounceSummonWolfPlayer` 为主技能时，创建该效果的主召唤物（`WolfMinion`，默认使用 `MeleeAtAnimationSpeedWolfCompanion`），继承来源宝石的有效等级与辅助。数量按一次激活的一只狼加游戏 stat `skill_cross_slash_triggered_summon_+_wolves_if_rare` / `_if_unique` 的稀有度加成（Rare +0、Unique +1），即普通／稀有敌人一只、独特敌人两只；不是假设达到 20 级 `number_of_wolves_allowed` 的六只上限。只有这个效果是所选主技能时，这些狼才计入 `MinionPresenceCount` / `SummonedMinion`；不推算击杀频率或持续累积的狼数。所选召唤效果的最小 sweep 仍有差异：PoBR 的两只狼使同颗宝石的 Mark 增加 6% 承伤，单只狼 DPS 为 49.8971；只保留召唤效果、去掉 Mark 时为 47.0728，钉定 PoB2 为 46.9565（剩余约 0.25% 来自伤害区间与敌人护甲取整）。这里保留现有 Mark 应用规则，不按参考值补伤害倍率。tuned 组的 Witchhunter 构筑上这个数是 0：它的 Living Lightning II 所在组没有能被辅助的技能，不会出现 Living Lightning；构筑码自带的参考值按 8 只计（+32%），比 PoBR 高。辅助宝石授予的召唤物在辅助能生效时也还没有创建。
- Presence 内的召唤物数（`MinionPresenceCount`）计入计算中创建的每个召唤物，包括没有数量上限 stat 的同伴，例如 Rhoa Mount 的 Rhoa（技能类型 `Minion` / `Companion`）；PoB2 只计入技能带数量上限 stat 的召唤物。所以 Minion Pact（“Supported Skills remove {0}% of your maximum Life from a nearby Minion on use to deal {1}% more damage”，`support_minion_pact_user_life_%_removed_from_nearby_minion` / `support_minion_pact_damage_+%_final`）在只有这类召唤物的构筑上，PoBR 计入、参考值不计：benchmark 组的 Chaos Inoculation Deadeye 最大生命为 1，每次只从 4182 生命的 Rhoa 抽 1.5 点，35% 更多伤害生效。宝石说明里的 “Only takes effect if you have a Minion with enough life to drain the full amount” 还没有建模（PoB2 也不检查），召唤物生命不够抽取时两边都会多算。
- Wolf Pack 召唤的狼数按游戏的 `skill_wolf_pack_size` 随有效宝石等级取值（1 / 18 / 20 级为 3 / 5 / 6 只），整群狼都计入召唤物数，并假设都在 Presence 内（**没有游戏内实测**）。Licking Wounds（天赋 55375，“5% increased Life and Mana Regeneration Rate for each Minion in your Presence, up to a maximum of 40%”）因此按整群计：tuned 组 Ritualist Purity of Fire（d557864ceebc）的 `LifeRegenRecovery` / `ManaRegenRecovery` 为 104.4 / 40.8，参考值 91.2 / 33.2。侧栏的 `ActiveMinionLimit` 是主技能实际创建的召唤物上限，没有上限 stat 的召唤物为 1；PoB2 对这类主技能输出 0：tuned 组三个 Spirit Walker（Zekoa the Headcrusher）构筑 PoBR 为 1，参考值为 0。
- Evergrasping Ring 的 “Enemies in your Presence Gain (6-12)% of Damage as Extra Chaos Damage”（游戏 stat `enemies_in_presence_non_skill_base_all_damage_%_to_gain_as_chaos`）按 stat 名是敌人全部伤害的份额：敌人的每种伤害都按这个比例额外获得混沌伤害，再按敌人对两种类型的加成缩放（`calc::ehp::assemble_enemy_damage`）。PoB2 只从物理伤害获得，所以有这枚戒指的构筑 EHP 比参考值低：tuned 组的 Ritualist Purity of Fire 三枚戒指共 39%，TotalEHP 140,103 对参考值 174,152；Stormweaver Frost Bomb 14%，12,061 对 14,185。
- 技能自己或它的辅助宝石带的 “increased Blind Effect”（`blind_effect_+%`，例如 Blind II 的 20%）只作用于这个技能施加的致盲。致盲不叠加，敌人身上只有一个，PoBR 取角色的技能里最强的那个（`skill_env::strongest_skill_blind`），转给敌人，与角色自己的 “increased Blind Effect” 相加。PoB2 把这条加成按每个被辅助的技能各算一次、全部加到角色身上：tuned 组的 Amazon（Rain of Arrows）有五个技能带 Blind II，PoB2 的致盲是 −40% 命中，PoBR 是 −24%，闪避率 72% 对参考值 76%，TotalEHP 随之较低。
- “Enemies in your Presence Resist Elemental Damage based on their Lowest Resistance”（`enemies_in_presence_elemental_damage_resisted_by_lowest_elemental_resistance`）是敌人的属性：Presence 内的敌人承受火、冰、电伤害时都用三种元素抗性中最低的那个，穿透按所用的那种抗性取（穿透降低的是它写明的抗性），命中、持续伤害和异常都经过 `calc::offence::enemy_resist_final`。游戏另有一条 “Elemental Damage from your Hits is Resisted by the enemy's lowest Elemental Resistance” 才限于命中。PoB2 把两条都只用于命中，所以点燃这类持续伤害在敌人某一抗性被降低时比参考值高：tuned 组的 Gemling Legionnaire Twister 点燃为参考值的 1.22 倍。**没有游戏内实测**。

## 冻结语料中逐行解释的参考值差异

Frozen `amazon-02` illustrates the existing Blind-effect difference: 8993 Evasion against level-82 enemy Accuracy 2114, with the strongest Blind II's single 20% increased effect, gives 24% less enemy accuracy and 55% rounded evade chance (16% Deflect). The pinned PoB2 counts three copies from Pounce's three active effects, giving 32% less accuracy and 58% evade (18% Deflect). Only those evade/deflect rows are classified as explained by Blind. The separate Energy Shield/Recovery Cap error was fixed by resolving Discipline's requirements against its own weapon-set tree; explaining Blind alone does not establish TotalEHP correctness.

The frozen 2026-10-26 corpus also exposes reference differences that are not
engine targets. Spell Totem's retained game-level cap reaches three (and its
cast time is 0.5 seconds); the pinned exporter's hidden support overrides the
cap to two and uses a 0.6-second placement fallback. Mortar Cannon's retained
cast time is one second. A selected non-totem has no selected-totem capacity,
and capacity alone does not establish a summoned-count snapshot. Runic
Tempering's conditional reduction requires held Runic Plates; an enabled skill
without that snapshot does not activate it. Frozen socket modifier text remains
an input even when the current rune definition changes (for example Zalatl's
3% versus 5% Mana). Blind comparisons must distinguish the strongest applicable
effect from duplicate copies; explaining Evade/Deflect does not by itself
explain TotalEHP. The Blood Mage03 pinned-oracle EHP has a separately traced
positive-Life floating residue after lethal damage; its recursive batched-hit
result is not a valid numerical target.

Frozen Lich02's Storm Mage reaches gem level 38 / monster level 76 after the
named +3 level bonus is recognized. Its base Life 4729 and 213% increased Life
give 14801.77. The pinned exporter additionally deducts the monster's 15%
Energy Shield share, producing approximately 12582 Life. The retained
`energyShield` field and the native schema's conversion comment come from the
vendor extraction path; they do not independently establish whether the game
deducts Life or grants additional Energy Shield. Keep this row unresolved,
without changing the formula or classifying it as a known reference error.

The frozen top-ten corpus has 23 remaining attribute-requirement differences
whose reference is exactly the nominal requirement of an exported amulet grant
(Eternal Rage, Wolf Pack, Trinity, Cast on Critical or Cast on Elemental
Ailment). An ordinary exported listing matched to that grant is the same skill,
not a second equipped gem. Keep its nominal requirement out of `ReqStr`,
`ReqDex` and `ReqInt`; the granted skill's usable level is checked separately.
An independent genuine gem still contributes its requirement. The bounded
regression covers ten grant/level combinations and the Gemling's 8% reduction;
the evidence is in `fix-round-3/requirements-evidence.json`.

Frozen Witchhunter10's Megalomaniac grants Leather Bound Gauntlets (51446):
1323 equipped-glove Armour adds 1323 base Evasion. This affects Armour/Evasion,
Sorcery Ward and the downstream defence panels; the pinned exporter omits the
granted passive. Frozen Infernalist03 instead differs at its Runeseeker rune:
the imported line is 45% less Life, whereas PoB2 rebuilds two MORE modifiers
and rounds their product. Preserve the frozen line. Six defence rows across
four Warbringer builds use the existing selected-totem count instead of assuming
an additional totem. Exact affected rows and arithmetic are in
`fix-round-3/static-defence-attribution.json`.

The five frozen Pathfinder examples already report their finite flask recovery
in `Flask1RecoveryPerSecond` and `Flask1EnergyShieldRecoveryPerSecond`. The
reference adds those rates to regeneration. PoBR keeps the per-use output
separate; selecting an active flask does not establish continuous charge supply
or repeated-use uptime. No extra state or sustained-regeneration contribution
is introduced for these comparisons.

Frozen Shaman08's configured Flame Wall supplies 18–26 Fire damage at level 15;
its recipient Spark can therefore ignite under the existing ailment model.
The pinned exporter's zero stack potential reads only the BASE totem limit
while the skill supplies an OVERRIDE. This zero is not evidence that added
Fire damage cannot ignite. The exact steady-state value with a chosen number
of simultaneous totems remains unverified, so the two affected rows stay in
the diagnostic queue rather than being declared fully explained.

The frozen Tactician06 comparison illustrates why agreement alone is insufficient:
pinned PoB2 already includes Watch How I Do It's weapon share, but omits the
planted Banner damage on the selected wolf. The imported `bannerPlanted=true`
supports the native active-aura snapshot. Earlier agreement came from cancelling
differences. The [isolated evidence](../examples/reference-builds/ninja-top10-2026-10-26/reports/fix-round-5/evidence/tactician-06-s04/evidence.json)
retains a qualified residual rather than registering the whole DPS row as known;
the subsequent Banner speed repair is measured separately in default mode.
