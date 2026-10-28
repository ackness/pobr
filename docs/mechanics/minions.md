# 机制约定：召唤物、图腾与友军

召唤物的创建、技能与数量，光环与友军共享，图腾与 Ballista。索引与写法见[机制约定](README.md)，与 PoB2 的差异另见 [pob2-differences.md](../pob2-differences.md)。

## 召唤物

Summon counts include the active skill's native game stat: Raise Zombie's `base_number_of_zombies_allowed` is 10 before temporary-minion bonuses (15 with the frozen Chronomancer's +5). Wolf Pack's `skill_wolf_pack_size` is 6 at the frozen Acolyte's level 19, plus one from quality; its limit stays 7 even though the pinned PoB2 reports 0. Supports compatible with a summoned minion's skills also contribute their cost multiplier once to the summoning skill. Minion Splash requires both a minion source and a melee strike; this combined eligibility does not bypass explicit minion exclusions or give the support's offensive payload to the player.

召唤物（`calc/minion.rs`、`calc/perform.rs::perform_minions`、`skill_env/minions.rs`）用它怪物的一个技能攻击：宝石的 `skillMinionSkill`（从 1 起，数据里存在的技能中第几个；构筑码导入、WASM 请求的 `minion_skill` 和导出都保留）选择，默认第一个。技能类型决定攻击、法术等标志；该技能自己的 stat，以及同组里按召唤物技能类型判断能辅助它的辅助宝石，作用于召唤物。召唤宝石与其辅助宝石上关于召唤物的 stat（“Minions deal {0}% more damage”、Muster 按不同种类的复生召唤物计等）由 `skill_stats::minion_modifiers` 交给召唤物，技能本身不再报告这些 stat。伤害基础取游戏 `DefaultMonsterStats.MinionDamage`（`pobr_data::monster::monster_ally_damage`），攻击技能还带武器的暴击率。召唤物的攻击按所选技能在召唤物等级下的 `damage_multiplier` 与 `attack_speed_multiplier` 计算，伤害倍率同时作用于武器基础与附加伤害；不是选定怪物、也不是复制武器的同伴，附加攻击伤害还乘怪物的 `MonsterVarieties.DamageMultiplier`（`calc::env`）。召唤技能上按角色属性分组的加成（“Minions from this skill have 1% increased Damage per {0} of your Strength”，`minion_1%_damage_+%_per_X_player_strength`；`active_skill_minion_1%_attack_speed_+%_per_X_player_dexterity`、`minion_1%_area_of_effect_+%_per_X_player_dexterity`，属性 ÷ 数值向下取整）与 “Minions convert {0}% of Physical Damage to Fire Damage”（`minion_skill_physical_damage_%_to_convert_to_fire`）给该技能的召唤物，不给角色。Supporting Fire 每次命令的齐射数（`death_from_above_number_of_volleys`）在 DPS 末端计数，不放大单次命中。Manifest Weapon（“Manifest a copy of your main hand Melee Martial Weapon”）复制主手近战武器的伤害、攻击速度与暴击率，主手不是近战武器时不创建召唤物（`skill_env::minions::spawn_minions`）。幽魂和驯服的野兽保留怪物本身的伤害（`monster_damage`），再按两条更多伤害缩放：两张表之比，和 `spectre_tamed_beast_inherent_damage_+%_final`（快照里有这个 stat 没有数值，25 取自 PoB2 的游戏内测量，`SPECTRE_TAMED_BEAST_INHERENT_DAMAGE`）。角色的光环（技能类型 `Aura`）也作用于召唤物：按游戏的技能描述，光环作用于 “you and Allies in your Presence”（Discipline：“Emit an Aura that grants you and Allies in your Presence additional Total Energy Shield”），召唤物得到的效果按角色的光环与 buff 效果加上召唤物自身受到的 buff / 光环效果，同名光环取较强的一份（`calc::buff_pass::auras_on_minion`）。角色另外给召唤物的东西有三种来源：`MinionModifier` 词条；只作用于友军的光环 “Allies in your Presence …”（`ExtraAura`，按召唤物自身的 buff / 光环效果缩放）；“Companions gain added Attack damage equal to #% of your main hand Weapon's damage”（Catha's Balance 等：角色主手武器本地每种伤害的这个份额，只给带 `GainMainHandDmgFromParent` 的召唤物的攻击；戒指等给角色的全局附加伤害不属于武器，`perform::apply_main_hand_share`）。只作用于召唤物的技能 buff 也走 `ExtraAura`：Pain Offering 的 “Aura grants Minions {0}% increased damage / Attack and Cast Speed” 不给角色本人，按角色的 buff 效果加上该技能与其辅助宝石的 buff 效果（Sacrificial Offering 的 “Supported Offering Skills have {0}% increased Buff effect”，`skill_stats::OFFERING_BUFF_EFFECT`）缩放；按游戏的描述，骨刺的光环作用于其范围内的召唤物，这里假设召唤物都在范围内。“Minions / Companions gain your Strength”（Sturdy Ally）把角色的最终力量给召唤物，每点力量 +2 最大生命（`calc::minion::LIFE_PER_STRENGTH`）。Gigantic 召唤物（“Your Minions are Gigantic”、Tangmazu's Thurible 的无值 stat `companions_are_gigantic`）按游戏的说明 “Gigantic Minions have 20% more Maximum Life, 20% more Damage” 计算（`perform::apply_gigantic`）。召唤物施放法术时（例如 Water Djinn 的 Navira's Fracturing，`ChilledGroundBurstWaterDjinn`），伤害是该法术自己的基础伤害，每个施法时间使用一次，受技能冷却限制（等级行的冷却、可储存次数，法术还有等级行的暴击率）。召唤物技能的 stat set 只有不到 5 个等级行，游戏按使用者的等级取值：适配器写出每个随效能成长的 stat 每点效能的值（`SkillStatSetLevel::per_effectiveness`，游戏表 `FloatStatsValues`）和 stat set 的效能成长（`StatSetDef::user_level_scaling`，`IncrementalEffectiveness` / `DamageIncrementalEffectiveness`），等级 L 的效能 = `SkillDamageBaseEffectiveness` 3.885209 × `BaseEffectiveness` × (1 + 成长 × (L − 1)) × (1 + 伤害成长)^(L − 1)（`calc::minion::stats_at_user_level`；`GameConstants` 表没有导出，常数取自 PoB2 的数据，按它算出的 1 级值与游戏表里已解析的值一致）。游戏表里每行另有使用者等级和每个 stat 的插值方式，导出里没有这两列，“不到 5 行”的界线与 PoB2 相同。带无值 stat `is_commandable_skill` 的是 Command 技能，召唤物使用它时 `CommandableSkill` 条件成立（`skill_stats::COMMAND_SKILL`），“with Command Skills” 类词条和 Bidding 的 “Supported Minions deal {0}% more Damage with Command Skills” / “increased Cooldown Recovery Rate for Command Skills” 才生效。召唤物抗性从游戏的 `MonsterVarieties` → `MonsterTypes` → `MonsterResistances` 普通稀有度列读取，叠加召唤物词条、召唤技能与辅助宝石的抗性 stat、作用于召唤物的光环；复用 `calc::offence::resolve_resistance` 的向零截断、最大值（默认 75%、通常硬上限 90%）、−200% 下限与超上限计算。基础抗性只在召唤物 ModDb 中计一次。WASM 与 CLI 的每个召唤物 `stats` 都输出四种抗性、最大值和超上限值，参考比较支持构筑码携带的 `Minion:<stat>` 抗性行。游戏[抗性说明](https://poe2db.tw/us/Keywords)将剧情惩罚写为玩家的抗性降低，因此不转移给召唤物。[Companion](https://poe2db.tw/us/Companion) 只定义同伴是一种复生召唤物；[Tame Beast](https://poe2db.tw/us/Tame_Beast) 虽要求捕捉 Rare Beast 并保留最多四条怪物词缀，没有明确召唤后的基础抗性按 Rare 列计算，当前同伴也用普通列，不采用 PoB2 的 Rare 覆盖值；保留的怪物词缀尚未建模。

召唤物自己的诅咒由它怪物的 `AppliesCurse` 技能组装（`skill_env::buff_specs::minions::minion_curse_specs`），按召唤物等级取值，效果读施法召唤物自己的诅咒效果与能辅助该技能的辅助宝石，不读角色的；配置 `raiseSpectreEnableCurses` / `summonCompanionEnableCurses`（条件 `SpectreCursesEnabled` / `CompanionCursesEnabled`）决定幽魂 / 同伴是否施加。每个施法者先按自己的诅咒上限取诅咒（怪物的 `EnemyCurseLimit` 是总数，Ribrattle 为 3），敌人身上同名诅咒只保留缩放后更强的一份，诅咒槽数取各施法者上限中的最大值（`calc::buff_pass`）。Temporal Chains 的行动速度减慢没有输出，SerpentClanCurse 的流血几率还没有建模。幽魂与同伴的 buff 开关（`raiseSpectreEnableBuffs`、`summonCompanionEnableBuffs`）没有注册：Vaal Embalmed Bearer 的旗帜回复（`ABTTProcessionBannerRegen` / `ABTTProcessionBannerRegenSpectre`）在游戏表里的 stat set 与等级行都是空的，名字和说明也为空，拿到它的游戏定义之前不建模。

同伴的数法按 PoB2（项目所有者 2026-10-14 的决定）：每个启用组里的召唤技能各自创建召唤物，物品或天赋授予的技能与放辅助宝石的同名技能组各算一个；同一种怪物在第二个组里出现时照常创建、计入同伴生命并保留精魄（同伴和幽魂按组与怪物计保留，其他技能放在几个组里都只保留一次），但不增加角色的召唤物数量（`Multiplier:SummonedMinion`）。精魄保留效率按角色的计数读取，例如 “2% increased Reservation Efficiency of Skills per Idol in your Equipment” 按装备里的偶像数。辅助宝石给出的 buff 是它所辅助的每个技能的 buff，以该技能命名，选了怪物的同伴 / 幽魂再按怪物区分（`buff_specs::supported_skill_buff_name`）；同名 buff 的同类词条只保留较强的一条。stat 映射自带 buff 名（`effectName`，例如 Refraction 的 “Refractive Plating”）的 payload 不论辅助哪个技能都是同一个 buff（`stat_map_engine::player_buff_effect_name`）。所以 Loyalty 辅助 7 只不同的同伴是 7 个 buff，“10% of Damage from Hits is taken from your Damageable Companion's Life before you” 合计 70%；Refraction 辅助 3 面旗帜仍是一个 buff。游戏里一个技能只有一份，授予的技能和放辅助的组是同一个技能，这样数会多算，没有游戏内实测。

“每个召唤物”类词条（Hysseg's Claw 的 `damage_+%_per_active_minion` 等，含 “per Minion, up to …”）在角色与召唤物上读同一份数量快照（`calc::minion::bridge_count_multipliers`，在 buff 与召唤物计算之前桥接，重复桥接结果不变）：默认是计算实际创建的数量（`Multiplier:SummonedMinion`，按上段的数法），配置 `multiplierSummonedMinion` 覆盖它，显式的 0 也覆盖；Presence 内的数量（`MinionPresenceCount`）默认等于它，配置 `multiplierMinionsInPresence` 可单独覆盖。召唤物的计算沿用这份快照，不再用来源数量改写 Presence 数，桥接之后也不把 ModDb 里的来源数量再加一次。“每个已召唤的某类召唤物”（例如 `attack_and_cast_speed_+%_per_summoned_raging_spirit`）读该类自己创建的数量（`ActiveRagingSpiritLimit`、`ActiveSkeletonLimit`、`ActiveGolemLimit`、`ActiveSentinelOfPurityLimit`），没有创建该类时为 0，其他种类的召唤物不计入。图腾的数量另按[已知的有意差异](../pob2-differences.md#已知的有意差异)里的图腾口径。

Selected minion hits can establish the `Damage` capability required by a
support even when their raw game skill types omit it, as with Fire Djinn's
Brand. This uses that selected effect's intrinsic damage payload; it does not
add raw types or borrow damage from a sibling skill. Non-damaging effects and
otherwise incompatible supports remain ineligible.

Storm Mage's separate quality stat
`minion_maximum_life_%_to_gain_as_maximum_energy_shield` grants extra ES from
that minion's completed Life. Its retained handler uses `EnergyShield BASE`
with `PercentStat(Life)`, not the base-resource gain-as matrix channel. The
existing PercentStat convention rounds this contribution upward before normal
ES scaling; this rounding has no in-game measurement. Resolve the recipient's
Life after its buffs, before offence and defence, and use the same snapshot
for its source queries and ES formula. Player Life and unrelated minions do
not supply this value. This quality grant does not deduct Life or resolve the
intrinsic 15% question ([Lich02](../pob2-differences.md#冻结语料中逐行解释的参考值差异)).

Some monster generic attacks retain both Melee and Projectile skill types as
capabilities. Their actual projectile hit flag uses retained weapon context
when the selected attack can fire arrows or wand projectiles; an explicit
`base_is_projectile` takes precedence. Known unarmed recipients do not receive
weapon-projectile bonuses. Missing context remains unknown, and raw types stay
available for support compatibility rather than being rewritten globally.

## 光环、友军共享与同伴

Presence item auras reach minions through `ExtraAura`; “You and Allies” additionally gives the player one copy. Ally modifiers on a weapon are not its numeric local attack-speed lines. Offering buff-effect queries use the Offering's own skill types, so generic Offering-effect passives apply even when another skill is selected. In frozen `chronomancer-07`, two 15% passives scale Pain Offering's damage/speed bonuses from 83/35 to 107/45 with the existing integer rounding. Danse Macabre's extra-skeleton-consumption condition and the player's `HitRecently` remain inactive without an explicit applicable snapshot; they are not enabled to match PoB2.

“你和 Presence 内的友军 …” 类词条（例如 Infernalist 的 Bringer of Flame：“All Damage from you and Allies in your Presence contributes to Flammability and Ignite Magnitudes”）角色自己也有。PoBR 的 `ExtraAura` 只给友军，所以抽取器把 PoB2 规则里不带 `onlyAllies` 的 `ExtraAura` 展开为角色自己的词条（带同样的条件）加给友军的 `ExtraAura`（`sync-pob-catalog` 的 `character_share_of_aura`）；带 `onlyAllies` 的规则抽取器不收。

Blackened Heart scales Unholy Might buffs the character grants from the
character's completed maximum Mana, once per full 100 Mana. Necromantic
Conduit's ally grant expands that named buff after it reaches the minion;
the minion's Mana does not determine the caster's granted magnitude. Recipient
buff effect remains separate, duplicate flags establish one buff, and the
player's own buff condition is not inherited by every minion. Low Mana,
inactive grants, zero Presence count and blocked ally auras preserve their
gates. The derived ally magnitude retains the caster's passive origin.

The companion Presence rune pair uses its retained game stats
`companions_in_presence_non_skill_base_all_damage_%_to_gain_as_random_element`
and `companions_in_presence_base_resist_all_elements_%`. Both effects follow
the existing ally aura path, restricted by the summoning skill's Companion
type. Explicit zero Presence, hidden recipients and blocked ally auras suppress
both the benefit and the resistance penalty. Random elemental gain uses the
existing equal three-element average; positive Presence counts do not assign
individual actor positions. Recipient aura/buff scaling follows the current
ExtraAura convention, without an in-game effect/range measurement.

Watch How I Do It supplies recipient-local `MainHandDamageToAllies` through the
existing ally aura path. Only the main-hand weapon's own damage components feed
attacking recipients; player or ring-added damage does not. Companion-only
sharing remains separate, and existing aura, hidden-recipient and Presence gates
still apply. Damage per different Companion counts distinct resolved companion
identities with positive populations in enabled, active-set groups, not capacity
or duplicate copies. Existing explicit false/zero Presence snapshots suppress it;
a positive aggregate does not locate individual companions.
Companion membership is independent of damageability: Manifest Weapon counts
as a Companion and obeys Presence exclusions, but its undamageable body does
not add to `TotalCompanionLife`. Shared weapon-damage contributions retain each
grant's original source and raw text; Watch How I Do It and Catha remain distinct
in minion detail rows instead of becoming one anonymous derived modifier.

## 图腾与 Ballista

When the selected skill places or is used by a totem (types `SummonsTotem` /
`UsedByTotem`), `calc::totem` reports the totem's defences: `TotemLife`,
`TotemDurationMod`, `Totem<T>Resist`, `Totem<T>ResistTotal` and
`MissingTotem<T>Resist` for Fire, Cold, Lightning and Chaos, each with a derived
detail. The totem monster comes from game `ActiveSkills.SkillTotemId` →
`SkillTotemVariations` → `MonsterVarieties` (`overlay-common/totem_varieties.json`,
with its `LifeMultiplier`: 120 for Ancestral Warrior's SlamTotem, 100 for the
others). Its level is game `MinionGemLevelScaling` (2 × the totem gem's effective
level, 40 for a level-20 gem); `TotemLifeBase` is
`floor(DefaultMonsterStats.AllyLife[level] × LifeMultiplier)`, scaled by
`totem_life_+%` (INC) and `totem_life_+%_final` / `support_totem_life_+%_final`
(MORE) and rounded (`calc_orchestrator::skill::mods::totem_base_modifiers`).
Resistances start from the totem monsters' 0% (`base/monster_resistances.json`)
and add the totem gem's own level stats `totem_elemental_resistance_%` /
`totem_chaos_resistance_%` (0 below gem level 13, 75 / 35 from level 20) and
supports such as Reinforced Totems; the maximum is the monster base 75% plus
`totem_maximum_all_elemental_resistances_%`, truncated like the player's.
`TotemDurationMod` is `(1 + totem_duration_+% / 100) × more`. PoB2 instead gives
every totem 40% elemental and 20% chaos resistance, absent from the game tables,
and ignores the gem's level stats: tuned Shockwave Totem `8468096e8b99` reports
65 / 30 in PoBR against PoB2's 40 / 41, and Grim Pillars `19a4efbb971d` 75 / 35
against 40 (Fire 75) / 20. `TotemLife` agrees with PoB2 on the three tuned totem
builds checked (2285, 2490, 5019). A totem is an Ally in the character's Presence, so the character's auras
(`calc::buff_pass::auras_on_minion`, as on a minion with no effect-on-self
modifiers of its own) and ally-only `ExtraAura` lists add their resistances
(level-20 Purity of Fire: +41% Fire) to the totem's; this assumes the totem
stands in the Presence, with no in-game test. `TotemArmour` / `TotemEvasion` are
`floor(DefaultMonsterStats.Armour/Evasion[level] × MonsterTypes.Armour/Evasion / 100)`
of the totem's monster type (game `MonsterTypes`: Armour 100 for the Spell,
Ballista, Ancestral Warrior and Mortar Cannon totem types, 0 for Shockwave and
Dark Effigy; Evasion and `EnergyShieldFromLife` 0 for all), so a level-20 Spell
Totem has 418 Armour and no Evasion; `TotemEnergyShield` is
`EnergyShieldFromLife`% of TotemLife plus `totem_maximum_energy_shield`, scaled by
`totem_energy_shield_+%` (no current source carries either stat). PoB2 reports
0 Armour. No game table or stat defines a totem Block chance, so
`TotemBlockChance` is not output. `ActiveTotemLimit` is the totem gem's
`base_number_of_totems_allowed` (Spell Totem: 2 at level 20) plus BASE
`ActiveTotemLimit` (“+1 to maximum number of Summoned Totems”, the attack /
spell / melee / ballista variants through their skill tags), times its MORE
(Ancestral Bond), or its OVERRIDE (`non_modifiable_totem_limit`); 0 without a
totem. It is only reported: damage still counts one totem. The level mapping has
no in-game measurement. These
fields change no DPS or totem count.

Native `base_skill_is_ballistified` / `is_ranged_attack_totem` establish the
selected Ballista identity. A Mortar Cannon's socketed attack receives its
Ballista damage and speed modifiers. Capacity queries include
`ActiveBallistaLimit` and the host's `TotemsAreBallistae` category alongside
generic totem additions; raw support-compatibility types remain unchanged.
Capacity is not evidence of how many totems are present.
Frozen Warbringer08's selected Ballista capacity is 1 base + 1 ascendancy +
2 crossbow + 1 passive = 5, with no override and a neutral MORE factor. Pinned
PoB2 returns 2 because its capacity query misses the Ballista additions when
the selected attack lacks its host's type. Only this capacity row is explained;
the remaining damage difference is separate.
