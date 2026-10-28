# 机制约定：伤害、技能与 buff

命中与技能类型、伤害转换、敌人抗性与曝露、条件与 buff、使用速率与范围。索引与写法见[机制约定](README.md)，与 PoB2 的差异另见 [pob2-differences.md](../pob2-differences.md)。

## 命中、技能类型与无伤害效果

技能的无值 stat `global_always_hit`（“Your Hits can't be Evaded”，掌套的隐式 stat 也是它）让命中不检查闪避（`CannotBeEvaded`），暴击也不再按命中率折算。Mace Strike 的 `skill_double_hits_when_dual_wielding` 在双持时两把武器各命中一次：DPS 按两次命中计，每次命中的伤害与暴击率不变。所选 stat set 带 `base_deal_no_damage` 时该效果不造成伤害（`DealNoDamage`），武器与附加伤害也不能把 buff、放置动作或技能容器变成命中：Conductive Runes 的放置、Rolling Slam、Spearfield、Tempest Bell 的主 stat set 与 Hollow Focus 的 Bells 为 0，伤害分别在同一技能另选的 Explosion、Slam、Hazard、Shockwave、Nova stat set。Explosive Demise 引爆生命低于阈值的友方召唤物（`command_minion_target_ally_below_x_life`），没有召唤物时不造成伤害（以上见 `rules::skill_stats::skill_modifiers`）。Raise Shield 与 Parry 用盾牌的伤害攻击（`replace_off_hand_unarmed_attack_stats_with_shield_type`，盾牌每满 N 点护甲或闪避为一组，两者的 N 为 5），主手武器不提供伤害或攻击时间（`skill_env::weapon::weapon_contribution`）。Raise Shield 的说明 “Release immediately after Blocking when an enemy is close to perform a Shield Bash” 要求先格挡，它的 250 毫秒最短引导与 35 毫秒释放是举盾的过程、不是盾击频率，所以不走引导周期，按自己 550 毫秒的攻击时间计，假设每次能盾击时都有一次可格挡的命中到达；Parry 没有引导阶段的 stat，按等级行的 750 毫秒（`base_parry_duration_ms` 是敌人身上减益的持续时间）。这两条**没有游戏内实测**，不模拟敌人的攻击或格挡频率。

`chance_for_extra_damage_roll_with_lightning_damage_%` supplies
`LightningLuckyHitsChance`, not flat Lightning damage. Preserve the imported
roll and use the existing per-type hit-luck calculation; generic hit luck and
Lightning hit luck combine only for Lightning hits. This rule does not change
the separate ailment damage-roll convention.

## 伤害转换、获得与限制

“Gain #% of Physical Damage as Chaos Damage”（`PhysicalDamageAsChaos`，不带 extra）与 “as extra Chaos Damage” 一样是额外获得的伤害，进入同一张 gain 表（`calc/damage.rs::gain_names`），来源伤害不减少。Embitter（`all_damage_gained_as_cold_instead`，“causing all sources of Damage Gained to instead be treated as Damage Gained as extra Cold”）把这张表里每一行获得的额外伤害都记为冰（`skill_stats::GAINS_ONLY_COLD`）。

Blackflame Covenant (game passive 42680,
`keystone_fire_spells_become_chaos_spells`) applies only when the selected skill
has both the Spell and Fire types. Its three retained game lines convert Fire
to Chaos, include Chaos hit damage in the existing Ignite source calculation,
and make that Ignite deal Chaos damage. The last operation changes enemy
resistance and damage-taken queries, not the ailment's source, magnitude,
duration or chance; damage increases are not applied a second time and
penetration still does not affect damage over time. The type-changing passive
remains an ancestor of the Ignite trace. This is a static rule, with no new
combat state or inferred trigger history. The [game text](https://poe2db.tw/us/Blackflame_Covenant)
and retained passive agree; PoB2 is a cross-check.

Remaining-chain modifiers apply to the selected hit: available chains minus
the explicitly selected number already used, bounded to zero and capacity.
They do not multiply DPS by a hypothetical number of targets. Twisted Empyrean's
Mana-based added Cold damage uses the item's hand and the final maximum Mana;
the existing PercentStat convention rounds each endpoint up before damage
effectiveness. Its current `with Mace Skills` Fire-to-Cold conversion wording
uses the ordinary conversion matrix and normalizes totals above 100%. The
pinned exporter only recognizes the older `of ... skills` wording, so this
conversion rule is exporter-absent.

Brutality's native no-elemental/no-chaos stats admit the existing `DealNo*` flags
through the shared player/minion mapper. The existing post-conversion damage
gates remove those components while retaining physical MORE and support scope.
Damaging ailments also respect the restriction on their actual dealt type:
Physical hits cannot fund Poison when Chaos damage is prohibited. A blocked
ailment has no damage or stack terms and keeps the restricting source in its
trace. Covenant's scoped Ignite uses Chaos here; ordinary Ignite uses Fire.
The separate chance to ignore physical reduction remains diagnostic; this batch
does not infer a missing mitigation contribution or add a probabilistic model.
Second Wind's double-use capability remains unsupported, but the only selected
groups in this cohort select Convalescence's ineligible reservation effect;
the active effect is not silently substituted.

## 敌人抗性、曝露与承伤

Doryani's Prototype (`enemies_in_presence_lightning_resist_equal_to_yours`, `UniqueNearbyEnemyLightningResistanceEqual1`) sets nearby enemies' Lightning Resistance from the player's final resistance after buffs, charges and resource-derived modifiers. The shared enemy override replaces its own resistance and curse/exposure reductions, so minions, damage over time and Sacred Flame's lowest-resistance rule consume the same value. An explicit `EnemyInPresence = false` disables it. Existing explicit enemy overrides and the enemy's maximum resistance retain their priority; the positive-resistance interaction (player 80%, enemy capped at 75%) is a conservative assumption without an in-game measurement. Frozen `chronomancer-10` has −184% player Lightning Resistance, where that upper-cap question does not affect the result.

The Coming Calamity's enemy resistance overrides retain the normalized `EnemyInPresence` condition, including inside nested modifier payloads; an already prefixed name must not become `EnemyEnemyInPresence`. Legacy of Northpaw's 7% weapon base critical chance replaces the locally modified weapon base before added critical chance and global increases. It applies independently to each actual weapon source; unarmed attacks, shields, spells and attacks with their own skill-defined base critical chance retain their own bases.

元素曝露在角色的防御算出之后归并（`calc::setup_env::reduce_enemy_exposure`，由 `perform` 调用）：Refraction III 的 “Banner applies {0}% Elemental Exposure per 1000 Armour”（`support_tempered_valour_banner_applies_%_elemental_exposure_per_1000_armour`，旗帜插下时）按角色护甲的每满 1000 点计，与其他曝露一起取最强的一份，再按敌人身上的曝露效果缩放。配置的 “Is the enemy Exposed to Fire?” 是角色自己施加的减益，只在角色能施加该元素的曝露时成立（条件 `CanApply<元素>Exposure`，`setup_env::with_exposure_sources`）：某个技能或辅助它的宝石施加曝露（`skill_env::groups_inflict_exposure`），角色有 “Inflict Fire Exposure on Hit” 这类词条，旗帜按护甲施加曝露，或敌人身上已有其他来源的曝露。没有来源时配置不生效，与 PoB2 相同（它在没有来源时隐藏这个选项）。技能自己施加的减益由 `skill_stats::debuff_modifiers` 按游戏 stat 定义，先于抽取的 stat 映射，品质加在同一个数值上（两条同名曝露只取最强的一条，分开会丢掉品质）：Oil Barrage 的油地面施加曝露（`skill_base_oil_exposure_-_to_total_elemental_resistance`，20，品质每 1% 加 0.25），假设敌人站在油里。Crushed 使目标的物理伤害减免降低 15，可以降到负数，目标随之承受更多物理伤害（减益的 stat `base_additional_physical_damage_reduction_%`，[poe2db Crushed](https://poe2db.tw/us/Crushed)；`setup_env` 的敌人基础词条，条件 `EnemyCrushed`，由敌人的 `Condition:Crushed` 或配置设置）。Styrn's Ferocity 的 “Supported Skills Crush targets they Hit for 8 seconds”（`support_knights_ferocity_crush_on_hit_ms`）不论辅助哪个会命中的技能，都让敌人处于 Crushed（`skill_env::skill_crushing_on_hit`）：Raise Shield 打出的命中让 Shield Wall 打的敌人被 Crush，假设 8 秒内用过它。

Culling Strike（Reap 的 `kill_enemy_on_hit_if_under_10%_life`，游戏文本即 “Culling Strike”）在敌人生命降到按稀有度取的阈值时击杀（游戏常量 `culling_strike_<稀有度>_threshold`：普通 35%、魔法 20%、稀有 10%、独特 5%），“increased Culling Strike Threshold” 提高阈值。需要打掉的只是阈值以上的生命，所以 CombinedDPS 乘 100 ÷ (100 − 阈值)（`calc::skill_dot::cull_multiplier`），`TotalDPS` 不变，侧栏的 `CullingDPS` 是这一倍率额外贡献的 DPS。Detonate Living 的 `spells_have_culling_strike`（“Your Spells have Culling Strike”）只给法术。

Enemy `<Element>Resistance` and `<Element>Resist` modifiers are aliases in
mitigation and its source breakdown. Xoph's Presence reduction therefore
combines with Exposure; an explicit winning resistance override still
bypasses those base modifiers. Hits, damage over time, minions and Sacred
Flame use the same resolved resistance.

## 条件、buff 与光环

Mark Effect (`mark_effect_+%`, “increased Effect of your Mark Skills”) scales the enemy-side Mark effect and the damage-as-extra buff granted by Freezing/Voltaic Mark. The latter retains the existing assumption that the Mark's buff is active; no activation rate or uptime is inferred. Mark self-buffs now use `CurseBuff` specs, so duplicate copies do not stack and the existing buff truncation applies (30% becomes 33% at 12% increased effect, 36% at 21%). Party exports retain the caster's Mark Effect for shared enemy debuffs. This text rule is `exporter: "absent"`: the pinned exporter parses it as `LocalEffect` without a reader. This is based on the game stat and skill payloads, **not an in-game measurement**.

Conditional offence uses existing state: `conditionEnemyFrozen` and
`conditionEnemyPinned` pass through `config_resolve::bridge_enemy_conditions`;
enabled Herald skills set `AffectedByHeraldofAsh` / `AffectedByHeraldofThunder`
in `calc_orchestrator::inject`, and the selected triggered payload receives
`Triggered` in `prepare::stage_resolve_main_skill`. Possessing a freeze or Pin
source does not establish the target state. Coming Calamity's 40% elemental
bonuses require their matching Herald; added attack damage (including
Asphyxia's Wrath) applies only to attacks. Daze duration and attack Maim chance
feed the debuff uptime views described under [On-hit effect views](ailments.md#破甲与命中效果视图);
primed-Pin thresholds and duration feed `PinBuildupRequired` / `PinDuration`
as described in [Pin](ailments.md#晕眩钉住与-poise). Sources: game `base/stats.json` (`damage_+%_vs_frozen_enemies`,
`bleeding_magnitude_+%_against_pinned_enemies`, `triggered_spell_spell_damage_+%`,
`hit_damage_stun_multiplier_+%`, `daze_duration_+%`, `maim_on_hit_%`,
`pin_almost_pinned_enemies`) and `base/passive_tree.json` (Coming
Calamity, Seeing Stars, Maiming Strike, Constricting Command). The Daze-duration
and attack-Maim rules are exporter-absent.
`FireDamage`, `LightningDamage` and `AilmentMagnitude` have readers gated by
the selected skill's source damage; the usage ledger reports that gate when a
non-damaging skill such as Mana Remnants does not reach those readers.

内置 buff（Arcane Surge、Onslaught 等，`overlay-common/buff_definitions.json`）都是战斗中的 buff：配置项 “Do you have Arcane Surge?” 写入的条件带 “Combat” 限定，计算在战斗口径下时成立（`rules::buff_expander::expand_misc_buffs`）。词条写明拥有该 buff 时同样触发：“You have Unholy Might” 写入同名条件 `Condition:UnholyMight`；只对某类伤害成立的条件不算，“30% chance to gain Onslaught for 4 seconds on Hit” 带命中标志，是否拥有由配置决定。buff 自己写入的 Multiplier（Unholy Might 的量级 100，“Gain 30% of Damage as extra Chaos Damage” 按它计）在展开后并入 multiplier 表。武器上的 “+N to Accuracy Rating” 是游戏的本地 stat `local_accuracy_rating`，只计入用这把武器的攻击（`ingest::item::apply_weapon_hand_conditions`）：Shield Wall 用盾攻击，主手武器的命中值不计入。写明 “this Weapon” 的词条（“Attacks with this Weapon Penetrate 15% Elemental Resistances”）解析时带待定的手别条件 `{Hand}Attack`，由武器所在的栏位定为主手或副手（同一函数）。“Damage with Maces” 这类武器类型伤害和主手武器的类型标志只作用于用武器攻击的技能：技能类型有 `Attack` 而没有 `NonWeaponAttack`（`conditions::attacks_with_weapon`）。Shield Wall 与 Raise Shield 用盾自己的伤害攻击，法术不用武器，都不计入；“while wielding a Mace” 这类持有条件不受影响。Fire Spell on Hit 这类 meta 宝石触发的技能带 `no_cost`，技能本身没有消耗，meta 宝石仍保留精魄。

`buff_time_passed_+%_only_buff_category` changes elapsed buff time: “Buffs on you
expire 10% slower” divides the selected Buff skill's duration and the nominal
`ArchonDuration` by 0.9, after duration modifiers. Non-buff skill durations,
enemy debuffs, and the Archon recovery period do not use it. The existing effect
expiry slow ceiling is retained; event-driven uptime is not modelled. The rule
is exporter `differs` because the exporter treats this as increased duration.

Into the Breach 的紫色火焰（“7% of damage as extra Chaos … stacks up to 10 times”，[Into the Breach](https://poe2wiki.net/wiki/Into_the_Breach)）按 `breach_flame_chaos_addition_%` 乘配置的火焰数（`PurpleFlamesCount`，最多 10），每份的数值再乘残骸效果：技能自身与辅助宝石的 `remnant_effect_+%` 加上角色的 “increased effect of Remnants”（生成残骸的技能，`GeneratesRemnants`）。“Effect and Duration of Flames of Chayula on You is Doubled” 与只取红 / 蓝火焰的词条还没有建模。Decompose 的毒云 “Poisons enemies as though dealing Chaos damage equal to {0}% of Corpse's maximum Life” 必定中毒；它不命中，所以不能暴击，也没有暴击伤害加成（`skill_stats::corpse_damage_without_hit`，所选 stat set 没有自己的基础伤害时带 `NeverCrit`）。毒云的爆炸带基础火焰伤害，是命中，按技能的暴击率。PoB2 的毒云暴击率为 0 但保留暴击伤害加成，所以 `CritMultiplier` 与参考值不同。

Enabled ordinary buff copies must reach the existing strongest-modifier merge
before deduplication. A first-copy filter can discard a stronger later quality
bonus: the frozen Gemling08 has Cast on Critical copies with 22% and 50%
increased critical chance. Keep the larger contribution and its origin once;
disabled groups or gems do not supply a candidate. Ordinary persistent buffs
retain their existing alternate-weapon behavior; the frozen corpus has no
explicit false `enableGlobal1/2` flags and does not validate that legacy state.
This does not invent additional buff uptime or change frozen item rolls to
match an exporter.
For those same named buffs, matching `ExtraAura` lists merge their numeric
children after effect scaling instead of stacking duplicate Offering payloads.
Each child first inherits its own wrapper's source when needed. Keep surviving
children inside their original source-bearing wrappers, removing displaced
children and empty wrappers; both minion details and the outer-modifier line
ledger must retain the winning sources. Other LIST kinds retain their existing
append behavior.

Mana Remnants' permanent regeneration buff is distinct from its collected
Remnants. `RemnantEffect` does not scale that permanent buff; ordinary BuffEffect
and the existing qualifying-skill convention still apply. Clarity II's possible
use of separate minion effects to satisfy `Persistent AND Buff` remains
unverified and is not enabled merely to match the exporter. Melting Maelstrom's
finite per-use recovery remains in the existing flask utility output, without
assuming permanent flask uptime. Time of Need and Ghost Dance's repeat intervals
remain separate from activation cooldowns; no event history is inferred.

War Banner's native `base_skill_buff_banner_attack_speed_+%_to_apply` supplies
increased Attack Speed through the existing planted aura. It uses the selected
`BannerPlanted` snapshot and normal aura scaling for player and allied attacks;
it does not infer Glory generation, placement frequency or uptime. The pinned
exporter omits this speed stat, so reference-basis comparison excludes only this
contribution and retains the existing damage and accuracy contributions.

## 使用速率、弹药与范围

弩的射击效果读取同组弹药的 `base_number_of_crossbow_bolts` 和 `active_skill_reload_speed_+%_final`，装填速度的品质与等级值先相加（`skill_env::mods::crossbow_reload_modifiers`）。Requiem 的 `crossbow_skill_has_no_ammo` 使激活期间不消耗弹药，也不计装填时间。Magma Spray 的 `off_hand_weapon_*_fire_damage` 与 `off_hand_*_added_fire_damage_per_15_shield_armour/evasion` 使用盾牌自身防御的每满 15 点，技能伤害倍率只乘一次（`skill_env::weapon::shield_elemental_damage_modifiers`）。Chayula's Will 的 `archon_of_chayula_tornado_attack_interval_ms = 200` 表示单个旋风每秒攻击 5 次；The Stars Answer / Walking Calamity 的陨石读取主效果 `walking_calamity_spawn_meteor_every_ms = 300 / 150`，不受攻击／施法速度影响（`rules::skill_stats::skill_modifiers`、`calc_orchestrator::skill::mods::skill_timing_modifiers`）。假设全部落点命中，不混合每第 15 发强化形态，不计多目标频率加成或旋风叠加数量；**没有游戏内实测**。

Snap 的 `snap_damage_+%_final_if_created_from_unique`（“{0}% more damage with Hits for blasts originating from a Unique enemy”）只加命中伤害，把所计算的独特敌人当作爆炸源头。Flameblast（`pobr-core/src/calc/flameblast.rs`）每层按 `charged_blast_spell_damage_+%_final_per_stack` = 75 加更多伤害，层数取 multiplier `FlameblastStage`（默认 1，最多 `flameblast_maximum_stages` = 10）；一次使用是每层施法时间 × 层数与 `base_minimum_channel_time_ms`（490 毫秒）中较大的一个，再加 `channel_skill_end_animation_duration_override_ms`（600 毫秒），仍受冷却限制。层数由配置输入 `skillFlameblastStages` 设置（Web 技能页的层数输入，`config_resolve::resolve_config` 写入 `FlameblastStage`，留空沿用默认）；PoB2 构筑码里自己的层数选择还没有接入。

Spell Echo's `echoed_spell_area_of_effect_+%` increases the area of echoes only.
The `repeatMode` Average view weights that increase by echoes / total casts;
Final and Final DPS use the full bonus, and None uses the original cast's area.
For level-20 Frost Bomb (24 + 7 radius units), Spell Echo (one echo, 40% increased
area) and Spell Cascade (20% less area) give Average area ×0.96 and radius 3.0 m;
the original cast is 2.7 m and the echo is 3.2 m. This display convention does not
apply Light Radius modifiers to skill area by itself (`calc::skill_mechanics::calc_aoe`).
Light Radius (`light_radius_+%`) transfers only its increases/reductions when an
explicit transfer is equipped: `light_radius_additive_modifiers_apply_to_area_%_value`
uses the rolled percentage (game Mods: 25–50); the accuracy and damage transfers
use `light_radius_increases_apply_to_accuracy` and
`light_radius_additive_modifiers_apply_to_damage`. Copies retain source and
conditions. `LightRadiusMod` displays the modifier itself, so without a transfer the ledger still counts the line as read there but the shared copies stay absent.

Focused Thrust (`projectile_damage_+%_with_spears_while_there_no_enemies_surrounding_you`, passive 9227) grants 40% increased Damage to Projectile hits with Spears when `enemyDistance` is at least 30 (3 m; ten units per metre), remains inactive with an `enemyDistance` config gate when unset, and assumes the configured target is the only enemy; this single-target convention has no in-game test, and the rule is `exporter: "absent"`.

Selected warcries use `WarcrySpeed` for both action rate and its breakdown.
Generic Skill Speed already feeds that bucket and is counted once; Cast Speed
does not apply. Game cast time, action speed, cooldown recovery and explicit
Ignore Warcry Cooldowns retain their existing roles. Frozen Warbringer05 has
an 800 ms base and 868% applied increased speed, giving 12.1 uses per second
with its cooldown explicitly removed.

Flame Wall's existing `FlameWallAddedDamage` selection supplies only the named
projectile stat set's flat Fire damage. Resolve the enabled source's effective
level, then apply the recipient projectile's ordinary effectiveness and damage
pipeline. Duplicate local and party copies use the existing strongest-effect
merge. The selection does not infer passage, lingering Ignite, infusion or
uptime, and no additional configuration state is introduced.

## 评估后不建模

Thorns (retaliation damage when the character is hit) was evaluated and is deliberately not modelled. The game data
holds the damage, critical, leech and conversion stats (47 mods and 38 gap-queue templates), but no Thorns skill:
the pinned PoB2 adds a synthetic `ThornsPlayer` skill to a "Thorns" socket group whenever the modifier database
holds Thorns damage (`CalcSetup.lua`), which has no game definition, and how often it retaliates depends on how often
enemies hit the character, an input nothing defines (the reason `enemySpeed` is not an EHP multiplier). Of the 100
worked builds only two carry a Thorns line, a single "increased Thorns damage" enchant or jewel, which the ledger
reports as `inactive` ("needs Thorns"); there is no reference row and no build whose numbers would change. If a build
needs it, start from a per-hit Thorns damage view (item, body-armour, Life and Mana shares, increases, conversion)
with an explicit hit-rate input, and no inferred retaliation frequency.
