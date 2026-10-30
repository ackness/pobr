# 机制约定：伤害、技能与 buff

命中与技能类型、伤害转换、敌人抗性与曝露、条件与 buff、使用速率与范围。索引与写法见[机制约定](README.md)，与 PoB2 的差异另见 [pob2-differences.md](../pob2-differences.md)。

## 命中、技能类型与无伤害效果

技能的无值 stat `global_always_hit`（“Your Hits can't be Evaded”，掌套的隐式 stat 也是它）让命中不检查闪避（`CannotBeEvaded`），暴击也不再按命中率折算。Mace Strike 的 `skill_double_hits_when_dual_wielding` 在双持时两把武器各命中一次：DPS 按两次命中计，每次命中的伤害与暴击率不变。所选 stat set 带 `base_deal_no_damage` 时该效果不造成伤害（`DealNoDamage`），武器与附加伤害也不能把 buff、放置动作或技能容器变成命中：Conductive Runes 的放置、Rolling Slam、Spearfield、Tempest Bell 的主 stat set 与 Hollow Focus 的 Bells 为 0，伤害分别在同一技能另选的 Explosion、Slam、Hazard、Shockwave、Nova stat set。

Explosive Demise 引爆生命低于阈值的友方召唤物（`command_minion_target_ally_below_x_life`），没有召唤物时不造成伤害（以上见 `rules::skill_stats::skill_modifiers`）。Raise Shield 与 Parry 用盾牌的伤害攻击（`replace_off_hand_unarmed_attack_stats_with_shield_type`，盾牌每满 N 点护甲或闪避为一组，两者的 N 为 5），主手武器不提供伤害或攻击时间（`skill_env::weapon::weapon_contribution`）。

Raise Shield 的说明 “Release immediately after Blocking when an enemy is close to perform a Shield Bash” 要求先格挡，它的 250 毫秒最短引导与 35 毫秒释放是举盾的过程、不是盾击频率，所以不走引导周期，按自己 550 毫秒的攻击时间计，假设每次能盾击时都有一次可格挡的命中到达；Parry 没有引导阶段的 stat，按等级行的 750 毫秒（`base_parry_duration_ms` 是敌人身上减益的持续时间）。

这两条**没有游戏内实测**，不模拟敌人的攻击或格挡频率。

`chance_for_extra_damage_roll_with_lightning_damage_%` 映射到 `LightningLuckyHitsChance`，不是附加闪电伤害。按导入值使用已有的逐类型幸运命中计算；通用幸运与闪电幸运只在闪电伤害上合并，不改异常自己的伤害取值规则。

[Parry 的天赋文本](https://poe2db.tw/us/Parry)中，命中范围与晕眩累积只属于 Parry 技能本身。当前规则按技能名匹配：`Parry Hit Area of Effect` 给 `AreaOfEffect`，`Parry has ... Stun Buildup` 给 `EnemyHeavyStunBuildup`，不会作用到其他技能。对应游戏 stat 为 `parry_hit_area_of_effect_+%`、`parry_stun_buildup_+%`，两条规则均为 `exporter: "absent"`。它们与“正在招架时”的防御、“成功招架后的下一次攻击”不同：后两类仍缺招架事件模型，台账保留 `unmodelled`。规则与回归见 `special_mods.json` 和 [passive_tree_coverage.rs](../../crates/pobr-build/tests/skills/passive_tree_coverage.rs)。

## 伤害转换、获得与限制

“Gain #% of Physical Damage as Chaos Damage”（`PhysicalDamageAsChaos`，不带 extra）与 “as extra Chaos Damage” 一样是额外获得的伤害，进入同一张 gain 表（`calc/damage.rs::gain_names`），来源伤害不减少。Embitter（`all_damage_gained_as_cold_instead`，“causing all sources of Damage Gained to instead be treated as Damage Gained as extra Cold”）把这张表里每一行获得的额外伤害都记为冰（`skill_stats::GAINS_ONLY_COLD`）。

Blackflame Covenant（42680，`keystone_fire_spells_become_chaos_spells`）只作用于同时有 Spell 和 Fire 类型的所选技能：将火转混沌、让混沌命中参与点燃来源、让该点燃实际造成混沌伤害。最后一步只改敌人抗性和承伤查询，不重算来源、幅度、时长或几率，也不二次叠加伤害增加。穿透仍不作用于 DoT，归因图保留此天赋。依据为[游戏文本](https://poe2db.tw/us/Blackflame_Covenant)及活动天赋，不需要新事件或触发历史。

剩余连锁加成只描述所选命中：可连锁次数减已用次数，再限制到零与容量之间，不用假想目标数乘 DPS。Twisted Empyrean 按最终最大魔力给该手别附加冰伤；沿用 `PercentStat` 约定，各端点先向上取整再乘伤害效用。当前 `with Mace Skills` 的火转冰用通用转换矩阵，超过 100% 时归一化。导出器只识别旧的 `of ... skills`，此规则为 `exporter: "absent"`。

Brutality 的禁止元素／混沌 stat 经玩家和召唤物共用映射进入 `DealNo*`。转换后移除被禁止的伤害，保留物理 MORE 和辅助范围。异常按最终造成的类型检查：禁止混沌时，物理命中也不能产生毒；普通点燃检查火，Covenant 点燃检查混沌。被禁止的异常不保留伤害或层数，但保留限制来源的归因。

“几率忽略物理减伤”仍报告未支持，没有概率减伤模型。Second Wind 的额外使用次数也未支持；已有调查语料选择的是 Convalescence 不兼容的保留效果，不能为了生效而自动改选主动效果。

## 敌人抗性、曝露与承伤

Doryani's Prototype（`enemies_in_presence_lightning_resist_equal_to_yours`、`UniqueNearbyEnemyLightningResistanceEqual1`）用玩家经过 buff、充能和资源派生后的最终电抗，覆盖附近敌人的电抗及诅咒／曝露减抗。召唤物、DoT、Sacred Flame 都读取同一敌人值。`EnemyInPresence = false` 关闭它；显式敌人覆盖与敌人最大抗性仍优先。玩家 +80%、敌人封顶 +75% 的正抗性交互是保守实现假设，未实测。冻结 `chronomancer-10` 的玩家电抗为 −184%，不涉及此上限。

The Coming Calamity 的敌人抗性覆盖保留 `EnemyInPresence` 条件，嵌套词条也不能变成 `EnemyEnemyInPresence`。Legacy of Northpaw 的 7% 武器基础暴击率先替换本地修正后的基础，再加额外暴击率与全局增加；每把实际武器分别处理。空手、盾牌、法术及自带技能基础暴击率的攻击保留各自基值。

元素曝露在角色的防御算出之后归并（`calc::setup_env::reduce_enemy_exposure`，由 `perform` 调用）：Refraction III 的 “Banner applies {0}% Elemental Exposure per 1000 Armour”（`support_tempered_valour_banner_applies_%_elemental_exposure_per_1000_armour`，旗帜插下时）按角色护甲的每满 1000 点计，与其他曝露一起取最强的一份，再按敌人身上的曝露效果缩放。

配置的 “Is the enemy Exposed to Fire?” 是角色自己施加的减益，只在角色能施加该元素的曝露时成立（条件 `CanApply<元素>Exposure`，`setup_env::with_exposure_sources`）：某个技能或辅助它的宝石施加曝露（`skill_env::groups_inflict_exposure`），角色有 “Inflict Fire Exposure on Hit” 这类词条，旗帜按护甲施加曝露，或敌人身上已有其他来源的曝露。

没有来源时配置不生效，与 PoB2 相同（它在没有来源时隐藏这个选项）。技能自己施加的减益由 `skill_stats::debuff_modifiers` 按游戏 stat 定义，先于抽取的 stat 映射，品质加在同一个数值上（两条同名曝露只取最强的一条，分开会丢掉品质）：Oil Barrage 的油地面施加曝露（`skill_base_oil_exposure_-_to_total_elemental_resistance`，20，品质每 1% 加 0.25），假设敌人站在油里。Crushed 使目标的物理伤害减免降低 15，可以降到负数，目标随之承受更多物理伤害（减益的 stat `base_additional_physical_damage_reduction_%`，[poe2db Crushed](https://poe2db.tw/us/Crushed)；`setup_env` 的敌人基础词条，条件 `EnemyCrushed`，由敌人的 `Condition:Crushed` 或配置设置）。

Styrn's Ferocity 的 “Supported Skills Crush targets they Hit for 8 seconds”（`support_knights_ferocity_crush_on_hit_ms`）不论辅助哪个会命中的技能，都让敌人处于 Crushed（`skill_env::skill_crushing_on_hit`）：Raise Shield 打出的命中让 Shield Wall 打的敌人被 Crush，假设 8 秒内用过它。

Culling Strike（Reap 的 `kill_enemy_on_hit_if_under_10%_life`，游戏文本即 “Culling Strike”）在敌人生命降到按稀有度取的阈值时击杀（游戏常量 `culling_strike_<稀有度>_threshold`：普通 35%、魔法 20%、稀有 10%、独特 5%），“increased Culling Strike Threshold” 提高阈值。需要打掉的只是阈值以上的生命，所以 CombinedDPS 乘 100 ÷ (100 − 阈值)（`calc::skill_dot::cull_multiplier`），`TotalDPS` 不变，侧栏的 `CullingDPS` 是这一倍率额外贡献的 DPS。Detonate Living 的 `spells_have_culling_strike`（“Your Spells have Culling Strike”）只给法术。

敌人的 `<Element>Resistance` 与 `<Element>Resist` 在减免及明细中是同义名字。因此 Xoph 的 Presence 减抗可与曝露合计；若有获胜的显式抗性覆盖，仍跳过这些基础修正。命中、DoT、召唤物和 Sacred Flame 共用结果。

## 条件、buff 与光环

Mark Effect（`mark_effect_+%`）缩放敌人身上的标记，也缩放 Freezing/Voltaic Mark 给玩家的额外伤害 buff。后者仍假设 buff 当前激活，不估计触发率或覆盖率。自身 buff 用 `CurseBuff`，同名不叠加，按已有整数截断：30% 在 12% increased 下为 33%，21% 下为 36%。队伍导出保留施法者的 Mark Effect。钉定导出器将此文本读为无消费者的 `LocalEffect`，所以规则为 `absent`；依据为游戏 stat 和技能效果，未做游戏内测量。

条件伤害按已有状态计算。`conditionEnemyFrozen` / `conditionEnemyPinned` 由 `config_resolve::bridge_enemy_conditions` 传入；拥有冻结／钉住来源不自动设置敌人状态。启用 Herald 会在 `calc_orchestrator::inject` 设置 `AffectedByHeraldofAsh` / `AffectedByHeraldofThunder`，Coming Calamity 的 40% 元素加成必须匹配 Herald。所选触发效果由 `prepare::stage_resolve_main_skill` 标为 `Triggered`。Asphyxia's Wrath 等附加攻击伤害只给攻击。

Daze 时长、攻击 Maim 几率见[命中效果视图](ailments.md#破甲与命中效果视图)，临近钉住门槛与时长见[Pin](ailments.md#晕眩钉住与-poise)。依据为 `base/stats.json` 中 `damage_+%_vs_frozen_enemies`、`bleeding_magnitude_+%_against_pinned_enemies`、`triggered_spell_spell_damage_+%`、`hit_damage_stun_multiplier_+%`、`daze_duration_+%`、`maim_on_hit_%`、`pin_almost_pinned_enemies`，及天赋 Coming Calamity、Seeing Stars、Maiming Strike、Constricting Command。Daze 时长和攻击 Maim 规则为 `absent`。

`FireDamage`、`LightningDamage`、`AilmentMagnitude` 需所选技能有对应来源伤害才被读取；Mana Remnants 等无伤害效果会在台账说明不满足的条件，不应补成无条件收益。

内置 buff（Arcane Surge、Onslaught 等，`overlay-common/buff_definitions.json`）都是战斗中的 buff：配置项 “Do you have Arcane Surge?” 写入的条件带 “Combat” 限定，计算在战斗口径下时成立（`rules::buff_expander::expand_misc_buffs`）。词条写明拥有该 buff 时同样触发：“You have Unholy Might” 写入同名条件 `Condition:UnholyMight`；只对某类伤害成立的条件不算，“30% chance to gain Onslaught for 4 seconds on Hit” 带命中标志，是否拥有由配置决定。buff 自己写入的 Multiplier（Unholy Might 的量级 100，“Gain 30% of Damage as extra Chaos Damage” 按它计）在展开后并入 multiplier 表。

武器上的 “+N to Accuracy Rating” 是游戏的本地 stat `local_accuracy_rating`，只计入用这把武器的攻击（`ingest::item::apply_weapon_hand_conditions`）：Shield Wall 用盾攻击，主手武器的命中值不计入。写明 “this Weapon” 的词条（“Attacks with this Weapon Penetrate 15% Elemental Resistances”）解析时带待定的手别条件 `{Hand}Attack`，由武器所在的栏位定为主手或副手（同一函数）。“Damage with Maces” 这类武器类型伤害和主手武器的类型标志只作用于用武器攻击的技能：技能类型有 `Attack` 而没有 `NonWeaponAttack`（`conditions::attacks_with_weapon`）。

Shield Wall 与 Raise Shield 用盾自己的伤害攻击，法术不用武器，都不计入；“while wielding a Mace” 这类持有条件不受影响。Fire Spell on Hit 这类 meta 宝石触发的技能带 `no_cost`，技能本身没有消耗，meta 宝石仍保留精魄。

`buff_time_passed_+%_only_buff_category` 改变 buff 的时间流逝。10% slower 让所选 Buff 技能时长与名义 `ArchonDuration` 在持续时间修正后再除以 0.9；不影响非 Buff 技能、敌人减益或 Archon 恢复期。沿用效果过期减慢的上限，不推算覆盖率。PoB2 将其当增加持续时间，所以标为 `differs`。

Into the Breach 的紫色火焰（“7% of damage as extra Chaos … stacks up to 10 times”，[Into the Breach](https://poe2wiki.net/wiki/Into_the_Breach)）按 `breach_flame_chaos_addition_%` 乘配置的火焰数（`PurpleFlamesCount`，最多 10），每份的数值再乘残骸效果：技能自身与辅助宝石的 `remnant_effect_+%` 加上角色的 “increased effect of Remnants”（生成残骸的技能，`GeneratesRemnants`）。

“Effect and Duration of Flames of Chayula on You is Doubled” 与只取红 / 蓝火焰的词条还没有建模。Decompose 的毒云 “Poisons enemies as though dealing Chaos damage equal to {0}% of Corpse's maximum Life” 必定中毒。它不命中，所以不能暴击，也没有暴击伤害加成（`skill_stats::corpse_damage_without_hit`，所选 stat set 没有自己的基础伤害时带 `NeverCrit`）。

毒云的爆炸带基础火焰伤害，是命中，按技能的暴击率。PoB2 的毒云暴击率为 0 但保留暴击伤害加成，所以 `CritMultiplier` 与参考值不同。

同名普通 buff 先比较所有启用副本的同类词条，再去重，不能直接保留第一份。冻结 Gemling08 的 Cast on Critical 分别有 22% 和 50% 增加暴击率，应只保留 50% 及其来源；关闭的组或宝石不参与。持续 buff 沿用既有另一武器组行为，冻结语料没有显式 false 的 `enableGlobal1/2`，不能用它证明旧状态完全正确。

同名 buff 中相同的 `ExtraAura` 列表，在效果缩放后逐个数值子项取强者，而不是叠加重复 Offering。子项先继承原包装的来源，胜出的子项仍留在该来源下，被替换子项和空包装移除。召唤物明细与外层词条台账都应保留获胜来源；其他 LIST 类型仍追加。

Mana Remnants 的常驻再生 buff 与拾取残骸是两件事，`RemnantEffect` 不放大常驻再生；普通 `BuffEffect` 和辅助资格仍适用。Clarity II 能否借另一召唤物效果满足 `Persistent AND Buff` 尚未验证，不因 PoB2 结果而放行。Melting Maelstrom 只报告有限的单次药剂回复，不假设永久覆盖。Time of Need / Ghost Dance 的重复间隔也不等于激活冷却。

War Banner 的 `base_skill_buff_banner_attack_speed_+%_to_apply` 经插旗光环提供攻击速度，要求 `BannerPlanted` 快照，玩家和友军攻击沿用正常光环缩放。不推算 Glory、插旗频率或覆盖率。钉定导出器漏掉速度 stat，所以参考口径只关闭速度，保留原有伤害与命中加成。

## 使用速率、弹药与范围

弩的射击效果读取同组弹药的 `base_number_of_crossbow_bolts` 和 `active_skill_reload_speed_+%_final`，装填速度的品质与等级值先相加（`skill_env::mods::crossbow_reload_modifiers`）。Requiem 的 `crossbow_skill_has_no_ammo` 使激活期间不消耗弹药，也不计装填时间。Magma Spray 的 `off_hand_weapon_*_fire_damage` 与 `off_hand_*_added_fire_damage_per_15_shield_armour/evasion` 使用盾牌自身防御的每满 15 点，技能伤害倍率只乘一次（`skill_env::weapon::shield_elemental_damage_modifiers`）。

Chayula's Will 的 `archon_of_chayula_tornado_attack_interval_ms = 200` 表示单个旋风每秒攻击 5 次。The Stars Answer / Walking Calamity 的陨石读取主效果 `walking_calamity_spawn_meteor_every_ms = 300 / 150`，不受攻击／施法速度影响（`rules::skill_stats::skill_modifiers`、`calc_orchestrator::skill::mods::skill_timing_modifiers`）。

假设全部落点命中，不混合每第 15 发强化形态，不计多目标频率加成或旋风叠加数量。**没有游戏内实测**。

Snap 的 `snap_damage_+%_final_if_created_from_unique`（“{0}% more damage with Hits for blasts originating from a Unique enemy”）只加命中伤害，把所计算的独特敌人当作爆炸源头。Flameblast（`pobr-core/src/calc/flameblast.rs`）每层按 `charged_blast_spell_damage_+%_final_per_stack` = 75 加更多伤害，层数取 multiplier `FlameblastStage`（默认 1，最多 `flameblast_maximum_stages` = 10）；一次使用是每层施法时间 × 层数与 `base_minimum_channel_time_ms`（490 毫秒）中较大的一个，再加 `channel_skill_end_animation_duration_override_ms`（600 毫秒），仍受冷却限制。

层数由配置输入 `skillFlameblastStages` 设置（Web 技能页的层数输入，`config_resolve::resolve_config` 写入 `FlameblastStage`，留空沿用默认）；PoB2 构筑码里自己的层数选择还没有接入。

Spell Echo 的 `echoed_spell_area_of_effect_+%` 只增大回响。`repeatMode` 的 Average 按“回响次数／总施放次数”加权，Final / Final DPS 用完整加成，None 用原始施放范围。20 级 Frost Bomb 半径为 24 + 7 单位，一次回响的 40% increased 与 Spell Cascade 的 20% less 合用，Average 面积 ×0.96、半径 3.0 米；原始施放 2.7 米，回响 3.2 米。实现见 `calc::skill_mechanics::calc_aoe`。

`light_radius_+%` 不自动增大技能范围，必须有明确转移：`light_radius_additive_modifiers_apply_to_area_%_value` 按物品 25–50% 的值复制 INC；命中与伤害分别由 `light_radius_increases_apply_to_accuracy`、`light_radius_additive_modifiers_apply_to_damage` 允许。复制保留条件和来源，只复制 increased/reduced。`LightRadiusMod` 本身也读取光照修正，所以没有转移时台账仍可显示被该视图读取。

Focused Thrust（9227，`projectile_damage_+%_with_spears_while_there_no_enemies_surrounding_you`）在 `enemyDistance >= 30`（3 米）时给长矛投射物命中 40% increased 伤害。缺少距离时保持不生效，台账提示 `enemyDistance`。这里把配置目标视为唯一敌人，未实测；规则为 `absent`。

所选战吼用 `WarcrySpeed` 计算动作率和明细；通用 Skill Speed 已并入一次，Cast Speed 不适用。游戏基础时间、行动速度、冷却恢复和明确忽略战吼冷却仍各自生效。冻结 Warbringer05 在去掉冷却后，以 800 ms 基时、868% increased 速度得到每秒 12.1 次。

`FlameWallAddedDamage` 只取已启用 Flame Wall 的投射物 stat set 固定火伤。先解来源有效等级，再走受益投射物自己的伤害效用和伤害流程；本地／队伍同名副本取最强。选择该状态不代表模拟穿墙、持续点燃、灌注或覆盖率。

### 投射物经过几次分叉、穿透或连锁

`skillForkCount` / `skillPierceCount` 分别成为 `ForkedCount` / `PiercedCount`，由 `skill_mechanics::snapshot_projectile_interactions_taken` 限制到技能实际可用次数。未填或填零表示初次命中，没有来源时填更大数字也无效。[Fork](https://poe2db.tw/us/Fork) 分叉后 30% less、[Pierce I](https://poe2db.tw/us/Pierce_I) 穿透后 20% less 通过这些状态生效；它们不是对第一次命中的惩罚，也不是额外命中数。

近距离穿透按 `enemyDistance <= 30`（3 米）提供 `PierceChance`；有效模式未填距离时通常使用 20（2 米），因此会生效。箭矢距离暴击天赋 11526 按 `40% × clamp(enemyDistance / 70, 0, 1)` 增加暴击率，到 7 米封顶，默认 2 米时为 `40% × 20/70`。箭矢加成按距离线性插值是当前模型，未做游戏内路径／时序测量。距离是单目标配置，不是投射物路径模拟；回归见 [passive_tree_coverage.rs](../../crates/pobr-build/tests/skills/passive_tree_coverage.rs)。

容量展示也不等于击中次数：`PierceCount` 将穿透几率按 `PierceChance / 100` 并入，全部穿透用 100 作为展示约定；Fork/Chain/Split/Bounce 各遵守禁止标志和对应容量。引擎不会据此假设同一敌人被额外命中，也不会把“分叉时几率多一个投射物”当成 DPS 倍率，该事件仍为 `unmodelled`。

## 评估后不建模

Thorns 是角色受击时的反击伤害，当前有意不建模。调查时游戏数据有其伤害、暴击、偷取和转换 stat（47 条 mods、38 个缺口模板），但没有独立 Thorns 技能。钉定 PoB2 在发现 Thorns 伤害时由 `CalcSetup.lua` 合成 `ThornsPlayer`，不是游戏技能定义。

反击频率需要敌人成功命中的频率，现有输入没有给出；`enemySpeed` 也不是通用 EHP／反击倍率。此前 100 个 worked 构筑中只有两个带单条 Thorns 伤害增加附魔／珠宝，没有对应参考输出，台账报 `inactive`（needs Thorns）。这些是该次调查的样本数，不代表全部玩家构筑。

以后若有实际需求，可先提供每次反击伤害，明确装备、胸甲、生命、魔力份额、增加与转换；只有明确的受击频率才能推算 DPS。当前保留未生效／未建模诊断，不把游戏中的反击机制标成全局无效果。
