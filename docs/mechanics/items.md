# 机制约定：装备、宝石与授予技能

装备条件、暗金与珠宝、宝石等级与授予技能、属性需求与消耗、药剂与护符。索引与写法见[机制约定](README.md)，与 PoB2 的差异另见 [pob2-differences.md](../pob2-differences.md)。

## 装备条件与物品读取

Equipment conditions use the selected weapon set. Dance with Death requires an empty off hand and a one-handed martial weapon in the main hand; caster weapons, unarmed, and originally two-handed weapons held through Giant's Blood do not qualify. Helmet Armour/Evasion comparisons use the item's final local ratings with strict inequality, before character-wide scaling; ties grant neither conditional immunity.

Warbringer 的 Renly's Training “Gain 35% Base Chance to Block from Equipped Shield instead of the Shield's value”（`ReplaceShieldBlock`）在装备盾牌时替换盾牌自身的格挡值，再乘格挡的加成（`calc::defence_panels::calc_block`）。Gemling 的 Virtuous Barrier 的属性 Mote 上限按角色从宝石获得的技能计：每个启用组里的技能宝石按属性需求计（单属性 +2，多属性各 +1），授予技能的辅助宝石（标签 `grants_active_skill`，例如 Eonyr's Thunder）也计；物品或天赋授予的组不计，构筑里同一技能已另列一个组（`inject::virtuous_mote_counts`）。 [Virtuous Barrier 的游戏文本](https://poe2db.tw/us/Virtuous_Barrier) 按需要该属性的技能计，物品授予的技能也有自己的属性需求。tuned Gemling Twister（682f26ed1957）的权杖 Purity of Fire 因此计一次：Intelligence Mote 为 3 + Purity of Fire + Herald of Ice + Ghost Dance + Trinity = 7；每点再生加成为 5% + floor(19 品质 × 0.125) = 7%，合计 49%。PoB2 跳过物品授予的技能，只有 6 点 / 42%；PoBR 的 LifeRegenRecovery 为 110.3391，构筑码参考值为 103.34，钉定 PoB2 为 101.76。

偶像数量按 socketable 类别计，表里没有条目时才回退到基底标签。[Carved Majesty](https://poe2db.tw/us/Carved_Majesty) 虽在 `base_items.json` 标为 `rune`，仍是 Idol，不承受 Idolatry 的 −4% 全元素抗性惩罚。因此 tuned 组 Spirit Walker `e071351f645d` 有五个非 Idol 镶嵌物，合计 −20%。它的 `ColdResistOverCap` 为 14，与钉定 PoB2 一致；构筑码参考值 2 漏掉了 Fox Idol 的 Bonded +12% Cold Resistance，胸甲的 “Idols socketed in this item gain the benefits of their Bonded modifiers” 使这条词条生效。

Three equipment-adjacent readings share one convention: the value is the item's own displayed number, and suppression is tracked per modifier name. Strong Chin's `stun_threshold_+_from_lowest_of_base_helmet_evasion_rating_and_armour` grants Stun Threshold equal to the lower of the helmet's displayed Armour and Evasion (pre-global scaling; the stat name's "base" is the pre-global item base). The Invoker's `cannot_gain_spirit_from_equipment` drops only BASE Spirit with an equipment origin (per-item base Spirit, explicit +N lines, socketed runes); passive and quest Spirit still count, and the suppressed equipment lines still show as applied in the usage ledger because modifier reads are tracked per name. Elemental recoup (`recoup_%_elemental_damage_as_energy_shield`, parsed as `Fire/Cold/LightningEnergyShieldRecoup`) adds its percentage to its own damage type's recoupable only, sharing the Energy Shield recoup duration and recovery rate (`calc::survivability::es_recoup_terms`, which also feeds the EsRecoupRate derived detail).

Giant's Blood uses the current game's passive 32349 and stat
`can_wield_2h_axe_sword_mace_in_one_hand`: two-handed Axes, Maces and Swords may
supply either hand of a dual-wield weapon attack. The prepared equipment view
provides possible weapon sources; after all modifiers and item-granted keystones
arrive, the `GiantsBlood` flag enables the second hand and `DualWielding`.
Without it, the second-hand candidate contributes no attack. Local modifiers of
an unused off-hand weapon never become global modifiers. This changes permitted
grip only: base classes, per-hand `Weapon2H` flags, `UsingTwoHandedMelee`,
two-handed damage modifiers, the existing triple martial-weapon attribute
requirements and halved inherent Strength Life are retained. Shields remain
shield sources; staves, quarterstaves, bows, crossbows and other excluded weapon
types gain no second-hand eligibility. This is static weapon-source selection,
not a general equipment legality or skill weapon-restriction validator.

Imported magic item names remain unchanged. Base lookup first accepts an exact
catalog name, then the longest complete base between a prefix and optional
` of ...` suffix. The base itself may contain `of` (Fists of Stone); a base
mentioned only inside a suffix is not a match. Local defence and weapon readers
share this lookup, so an imported defence display is not counted again as a
base when the game base is known. Grand Spectrum counts selected socketed jewels
across implicit, explicit and enchant text; corruption can move its identifying
line into the implicit section.

Resistance per equipped item with a Lightning resistance modifier counts each
current ordinary equipment slot once. Direct all-element and negative modifiers
qualify; cancelling modifiers or an inactive condition do not erase the item's
modifier. Nested ally/minion grants and separately stored jewels/flasks/charms
do not count. This follows the existing equipment-count boundary; eligibility of
Transcendent limbs and maximum-resistance-only modifiers is not client-verified.

Loreweave's combined rarity/exclusion stat retains only its own rarity source,
not every rarity modifier on the same item. The exact adjacent pair is joined
within its section before parsing and diagnostic collection; imported text stays
unchanged. Native and reference-basis modes retain the allowed 84% (factor 1.84),
whereas pinned PoB2 also applies a 15% charm (1.99). This partial support is
`exporter: "differs"`, so comparison does not discard the supported numeric part.

## 暗金、珠宝与天赋文本

几条暗金词条接到已有的计算入口（`overlay-common/special_mods.json`）：Blackheart 的 `base_armour_%_applies_to_chaos_damage` 让这一比例的护甲也用于混沌命中的减伤；Vigilant View 的 `maximum_chance_to_evade_is_50%` 与 Goregirdle 的 `maximum_physical_damage_reduction_is_50%` 限制闪避率与物理命中减伤；Matsya 的 “Skills reserve 50% less Spirit”（`unique_spirit_reservations_are_halved`）乘在保留量上，不是保留效率；Plaguefinger 的 `all_damage_can_poison` 让元素命中也计入中毒的伤害，不提供中毒几率；`ignore_hexproof`（“Curses you inflict can affect Hexproof Enemies”）让诅咒越过敌人的 Hexproof；Venopuncture / Icefang Orbit（`all_damage_to_bleeding_enemies_contributes_to_chill` / `all_damage_to_poisoned_enemies_contributes_to_chill`）只在敌人流血 / 中毒时让全部命中伤害计入冰缓强度。Parry 的 “increased Parry Damage”（`parry_damage_+%`，Silverthorne 与天赋）和 Kaltenhalt 的 Parry 物理转冰（`parry_physical_damage_%_to_convert_to_cold`）只作用于 Parry。Chober Chaber、Trenchtimbre、Crown of Eyes 的 “increases and reductions … also …”（`minion_damage_increases_and_reductions_also_affects_you`、`additive_modifiers_to_minion_attack_speed_also_affect_you`、`additive_spell_damage_modifiers_apply_to_attack_damage`）只复制相应的 INC 词条：召唤物伤害、召唤物攻击速度、法术伤害分别给角色伤害、角色攻击速度、攻击伤害，保留来源和条件，不复制 MORE / LESS、点伤和只限定某类召唤物的词条（`calc::shared_modifiers::apply_shared_increases`）。

Berek's Grip/Pass/Respite and The Taming carry the game stats `wind_skills_count_as_empowered_by_{ignited,shocked,chilled}_ground`: their joined item lines set `Condition:TwisterFire/Lightning/Cold` exactly like the Twister config checkboxes, bridged into the conditions table by `CalculationSession::bridge_player_conditions`, so Twister's `SkillDamageGainAs` tags apply without any config (`special_mods.json`: `wind_ground_ignited/shocked/chilled/all_elements`). The game text wraps across two item lines, so equipment reuses the tree wrap-merge (`filter_item_parseable` via `collect::combine_wrapped`) and the pair is ledgered once as its joined line. `wind_skills_can_be_empowered_by_multiple_elements` is S0 (`wind_ground_multiple_surfaces`): PoBR applies each Twister element condition independently and never limits a Twister to one element, so two Berek's rings without The Taming both apply — a known simplification with no in-game test. All five rules are `exporter: absent`, so the reference basis is unchanged and the game-mode DPS rise shows only in the beyond-PoB2 report.

PassiveSkills nodes 11335 / 47190 / 12183 / 36676 grant ordinary passive points
through their “Grants 1 Passive Skill Point” text. `AllocationTree::edit` includes
shared allocated grants in the candidate budget and rejects refunds that remove
spent grants; `ExtraPoints` is consumed outside ModDb. Heroic Tragedy's
`local_unique_jewel_alternate_tree_seed` / `_version` Remembrancing line is
`JewelData`, read by `jewel_tree`; missing seeded node data remains unsupported.
`spell_damage_+%_while_wielding_melee_weapon` uses `UsingMeleeWeapon` from the
currently equipped weapon classes. The existing
`critical_hit_damage_bonus_+%_if_consumed_power_charge_recently` convention uses
at least one removable Power Charge, controlled by `usePowerCharges` and
`overridePowerCharges`; minimum charges cannot activate it. This is a charge
snapshot convention, not a simulated recent-consumption event.

Additional skill slots (`local_item_additional_skill_slots` /
`num_additional_skill_slots`) and knockback distance (`knockback_distance_+%`)
change no reported output. Whispers of Doom's curse delay (`keystone_hex_master`)
feeds `CurseActivationDelay`.

Undying Hate's explicit "Conquered Small Passive Skills also grant ..." lines
are independent of its seed-specific notable transformations. Attach a fully
parsed payload to each reconstructed non-attribute small passive in the
conquered radius; allocation and small-passive effect scaling then follow the
ordinary tree path. Attribute passives have their own category and do not
receive this addition. Unknown payloads and unknown notable replacements
remain diagnostic. Frozen Titan06 has twelve allocated small passives with
6% Elemental Damage each; Hulking Form raises each to 9%, adding 108% in total.
The retained game stat is
`local_historic_abyss_jewel_conquered_small_passives_grant_elemental_damage_+%`;
its imported roll is preserved instead of rebuilding it from a current range.
The separate "Conquered Attribute Passive Skills also grant ..." directive
attaches only to attribute nodes under the same radius, allocation and weapon-set
gates. It retains the imported roll and source, and does not receive Hulking
Form's small-passive amplification. Reduced frozen regressions cover Ritualist09's
two +6 Intelligence grants (147 to 160 final Intelligence) and Smith06's three
+5 Strength grants (271 to 287 final Strength); these checks do not reconstruct
seed-specific notable replacements.

Jewel and passive-effect scaling reaches numeric payloads inside
`MinionModifier` lists as well as top-level modifiers. The Adorned still uses
the jewel's corruption, rarity and socket eligibility; Hulking Form still uses
the allocated small passive. Preserve the wrapper's scope and source, and add
only the effect delta once. Flags and MORE modifiers do not acquire a numeric
passive-effect delta.

## 宝石等级、品质与授予技能

Uhtred 系辅助（“+2 to Level of Supported Skills if exactly one other Support is modifying them”）只把辅助宝石算作辅助：meta 宝石（Cast on Elemental Ailment）的触发效果是技能宝石的第二个效果，不算；只辅助 meta 宝石的辅助（Boundless Energy II 要求 `GeneratesEnergy`）也不修改被触发的技能（`skill_env::resolve::support_granted_gem_levels`）。“Skills from Corrupted Gems have …” 在主技能取等级的那颗宝石已腐化时生效（条件 `GemCorrupted`，`calc_orchestrator::skill::resolve::from_corrupted_gem`）。

宝石等级与品质加成（“+1 to Level of all Spell Skills”“+2% to Quality of all Skills”）按文本读取，不经过 ModDb：技能等级要在任何 modifier 存入之前确定。读取面是角色的全部词条来源：装备与珠宝、已分配和被物品授予的天赋、配置行（任务奖励、自定义词条），以及调用方传入的 `extra_modifier_texts`（由 `prepared_build_view` 并入构筑视图，主面板、逐技能总览和触发子计算共用）。新增一条文本通道时要同时接入 `skill::resolve::gem_property_bonuses`，否则这类词条在该通道里既不生效也不报未支持。被这样读取的行不计入未支持清单，也不再交给解析规则（规则会把“with an Intelligence requirement”误读成属性需求）。种类有两种写法，“all Cold Skills”和“all Nova Skill Gems”，含义相同。种类按宝石标签判定（游戏表 `GemEffects.GemTags`，adapter 写进 `base/skill_gems.json`；每个标签对应一条 `<标签>_skill_gem_level_+` stat），种类里的每个词都要是宝石的标签：Rain of Arrows 带 `projectile` 标签，尽管它的技能不是 `Projectile` 类型；Shockwave Totem 带它的图腾所用 Slam 的 `slam` / `melee` / `nova` 标签。“Elemental”不是标签，指火、冰、电三种之一（`elemental_skill_gem_level_+`）。辅助宝石的标签描述它辅助的技能和它授予的技能，它本身没有等级，所以不按标签判定；没有标签的旧快照也退回按技能类型判定，种类名与技能类型不同的（Totem → `SummonsTotem`、Curse → `AppliesCurse`、Strike → `MeleeSingleTarget`、Crossbow、Elemental）列在 `skill_env::mods::kind_skill_types`（`skill_env::mods::gem_tags_match_category` / `gem_level_category_matches`）。“increased bonuses gained from Equipped Rings and Amulets”（Ritualist 的 Mystic Attunement）放大戒指和护身符上的其他词条，也放大其中的宝石等级与品质加成，向下取整：+4 在 25% 下为 5（`skill::resolve::scale_slot_gem_property_bonuses`）。按技能名写的等级词条（Cursecarver 的 “+4 to Level of Despair Skills”，不带 all）走同一通道，按游戏的 `ActiveSkills.DisplayedName`（`GrantedEffectDef.displayed_name`，不分大小写）匹配，Sniper's Mark 这类带标点、不能从 id 还原的名字也能匹配（`skill_env::mods::parse_gem_property_bonus`、`skill_env::resolve::gem_property_applies`）；只接受完整的 “Skills” / “Skill Gems” 写法，“Socketed”、“Supported” 和带其他条件的写法有各自的作用域，不当作全局等级加成。

物品授予的技能（“Grants Skill: Level 18 Purity of Ice”）有自己的等级和属性需求，与物品的穿戴需求无关；角色达不到时降到能满足的最高等级（[poe2wiki: Granted skill](https://www.poe2wiki.net/wiki/Granted_skill)），一级都达不到时用它最低的等级。每个宝石等级需要的角色等级取自游戏表 `ItemExperiencePerLevel`（`SkillGemDef::level_requirement`；表里没列的等级沿用前一级）；属性需求按宝石的属性占比计算，L 为该等级需要的角色等级：round((5 + (L − 3) × 1.7) × (占比 / 100)^0.9) + 4（`SkillGemDef::attribute_requirements`，与游戏显示的逐级数值一致，例如 Purity of Ice 18 级需要 42 力量、106 智力）。被授予的组在构筑视图里记下授予等级（`SocketGroup::granted_level`，物品自己的组和上面那个被认作授予技能的普通组），`prepared_build_view` 再按角色等级和属性降级（`granted_skills::scale_granted_skills`）。属性来自不含技能的一次来源写入（`attributes_without_skills`：职业、装备、肢体、珠宝、配置、天赋和额外词条），技能自己的效果不计入；只有构筑有需要属性的授予技能时才运行，计算量约为一次计算的来源写入阶段。没有等级的授予行（“Grants Skill: Raise Shield”）不降级。已分配天赋授予的技能在构筑码里是 `<Skill source="Tree:<节点>">`，等级只是占位；它按角色等级取宝石能达到的最高等级（宝石经验表列到的最后一级，例如 98 级角色的 Sorcery Ward 为 20），同样记下授予等级、再检查属性。游戏导出的角色还把它列成一个普通组（排在组首、等级相同、带辅助宝石），那个组就是被授予的技能，天赋的组让位给它，主技能组随之改指；节点未分配时天赋的组不授予任何技能（`granted_skills::resolve_tree_granted_skills`）。

辅助宝石辅助一颗宝石的任一效果时，它给的等级和品质作用于整颗宝石：Physical Mastery（要求 Physical）辅助 Iron Ward 的物理新星，Iron Ward 的 buff 也高一级。等级计算接收宝石每个效果的辅助判定（`skill_env::buff_specs::gem_judgements`，主效果在前），同一个辅助只算一次；Uhtred 系列“没有其他辅助时”的“其他辅助”也按整颗宝石计。

The usage ledger reports the selected `GrantedEffectsPerLevel` row, rather than
the requested gem level before row selection: Armour Explosion and Raise Shield
have only level 1, Pinnacle of Power only level 20, and ordinary player skills
end at level 40 in the current game tables. Their former higher ledger levels
did not mean the calculation extrapolated beyond those rows. Support-granted
effects inherit the first compatible host gem's level, checking all that gem's
effects ([Living Lightning's game text](https://poe2db.tw/us/Living_Lightning)
explicitly says it copies its socketing gem's level): Living Lightning can
support Repulsion Wave, and Shock Siphon can support Pounce's mark. Buff assembly
uses the same host. Named skill bonuses
also apply to the whole gem: “+3 to Level of all Galvanic Shards Skills” raises
both the attack and Load Galvanic Shards (`GemEffects` links the two effects).

Spear Throw's imported level-1 entries are default-attack placeholders. Its
base level follows the character (`ItemExperiencePerLevel`: level 19 at character
84, level 20 at 90), then receives global skill bonuses, per the game's
[Default Attack keyword](https://poe2db.tw/us/Keywords). Unbound crossbow, spear
and shapeshift groups use the other weapon set when only that set has the
required weapon class ([Rend requires Talismans](https://poe2db.tw/us/Rend)).
Granted-skill attribute checks likewise use the granting weapon set's equipment;
its Bonded modifiers are settled before attributes and level bonuses are read.
The prepared global level bonus stays separate from the base gem level, so a
level-3 Pounce plus five non-weapon levels stays level 8 after swapping away
from a +5 melee weapon; an intermediate base level must not clamp to 1.
The alternate set's passive allocation is not retained in the `Build` view, so
these checks still share the imported passive tree. Global quality applies to
[granted skills](https://www.poe2wiki.net/wiki/Granted_skill), including Raise
Shield even though it has no quality effect; the exporter's synthetic Thorns and
Enemy Explode entries have no game skill definition and receive no bonus quality.
The ledger lists all enabled imported groups. PoB2 consolidates some item grants
with their supported ordinary group and adds some default attacks automatically;
therefore extra lower Mirror of Refraction / The Stars Answer entries, another
weapon set's Spear Throw, or an extra PoB2 Spear Stab can remain inventory
differences even when the supported skill's effective level agrees. They do not
justify changing the imported gem or the game's level bonuses.

Skills bound to another weapon set resolve attributes, granted levels, quality
and socket effects against that set's passive allocation as well as its weapons.
Only the group explicitly identified as an item grant is exempt from ordinary
gem requirements. A listed grant keeps its supports and enabled state instead
of acquiring a second enabled source copy. Minion-compatible supports can count
for the summoning gem's support-based levels and reservation, using the same
eligibility and no-type-borrowing restrictions as summon costs.

Unique item grants use the game's `ModGrantedSkills` attachment relationship,
not display-name collision order. The current catalog records Coming Calamity's
three `UniqueHeraldOf*Player` variants and Sands of Silk's `BlinkSandPlayer`.
Only proven attached calculation groups are canonicalized; imported identities
and independent gems remain intact. Zero base Spirit reservation does not exempt
additive support reservation. Armour grants retain their group's activation set,
and changing an effect list must preserve the selected effect's meaning.
The native variants also have zero attribute shares. In the worked Oracle Grim
Pillars `0a7cf6c82619`, Sands' grant no longer supplies ordinary Blink 20's
157 Intelligence requirement; independent Mana Tempest 19 and Temporal Chains 19
retain the correct maximum of 147. This deliberately loses one saved-reference
match. Frozen Deadeye01 similarly needs 80 Strength from sixteen red supports,
above its equipment requirements, after the attached native Heralds replace
ordinary Herald of Ash's nominal 157. The other sixteen skill records are
unchanged; only this exact requirement row is registered as explained.

## 属性需求与消耗

`ReqStr` / `ReqDex` / `ReqInt` 取全构筑装备与宝石需求的最大值，再与各颜色辅助宝石累计的需求比较（`calc_orchestrator::requirements::requirement_snapshots`）：装备先转换基底需求，再加本地固定需求，乘本地百分比修正并向下取整，最后应用全局修正（Runeforged Leaden Greathammer 的 60 力量、50 智力，转换 20% 到敏捷后再加 81 智力，得到 48 / 22 / 121；三倍武器需求后为 144 / 66 / 363）；宝石按原生等级计，腐化与装备给的技能等级不抬高需求；物品授予的技能和被认作它的普通组不新增一颗宝石的需求（见[已知的有意差异](../pob2-differences.md#已知的有意差异)）。消耗按资源分固定、百分比与每秒三种通道；每秒消耗按使用、触发、装填或放置的频率计算，多发投射物不另付消耗（`perform::fill_resource_cost_rates`）。技能自带每秒消耗时，该资源的持续消耗直接取这一通道，不再把启动消耗按攻击频率叠加：Oil Barrage 是 Wyvern 引导技能，不是弩弹药；27 级的 69 魔力启动消耗与每分钟 4049 魔力在 110% 辅助倍率下分别为 75 与每秒 74。图腾放置用召唤技能自己的游戏 `cast_time`（`calc_orchestrator::skill::mods::skill_timing_modifiers`，陷阱、地雷与战吼同理），施法速度与图腾放置速度分开。不是攻击、也没有专属速度的技能（例如 Wildwood's Gifts，`AncientGiftsPlayer`，只有 1000 毫秒的施法时间、没有 `Spell` 类型）按施法速度加技能速度计算使用时间（`skill_use_time::speed_names_for`）；战吼、图腾、陷阱与地雷仍用各自的速度。游戏把这类技能的基础时间标为 “Cast Time”，PoB2 与构筑码参考值也这样计，**没有找到游戏对施法速度作用范围的正式说明，也没有游戏内实测**。`ChannelTime` 只含引导阶段，不含释放与冷却。

Spell Mana Cost Efficiency（`base_spell_mana_cost_efficiency_+%`）只降低法术魔力消耗，效率 INC 与通用效率相加后，以 `1 + 效率 / 100` 除消耗；它标为 absent。The Fall of the Axe 的 “Grants Onslaught during effect”（`charm_gain_onslaught_during_effect`）同样标为 absent；按 [游戏 Onslaught 定义](https://poe2db.tw/us/Onslaught) 增加 20% Skill Speed 与 10% Movement Speed。沿用 flask/charm 约定：构筑中启用且可生效的护符视为效果激活；“Used when you are affected by a Slow” 不建模为 uptime，另一件生效的物品不会激活被关闭或超出槽位的护符。

[Derange](https://poe2db.tw/us/Derange) adds an ES cost equal to final
Intelligence (`gain_energy_shield_cost_equal_to_intelligence`). Its compatible,
selected support supplies `ESCostNoMult` through the existing cost path.
Support multipliers of a skill's base cost do not multiply this additional
cost; ordinary applicable cost modifiers still do. Culling Strike I/II's native
rare/unique grant requires the Effective context as well as the selected enemy
rarity. Config visibility recognizes the conditional grant before it is active,
so users can expose the existing rarity checkbox without enabling it implicitly.

## 药剂与护符

`life_recovery_from_flasks_also_recovers_energy_shield` (Shavronne's Satchel)
adds `Flask{1,2}EnergyShieldRecovery`, `EnergyShieldInstantRecovery`, and
`EnergyShieldRecoveryPerSecond`. Life flask amounts, quality, flask recovery,
and the instant fraction are reused; gradual recovery uses the destination's
Energy Shield recovery rate, not Life recovery rate. Mana flasks do not copy,
and `CannotGainEnergyShield` blocks the copied recovery.

Charm/flask “Used when …” triggers (game `local_flask_use_on_*` and
`local_charm_trigger_when_cursed`) are recognized with no numeric effect under
the enabled-item convention. “Recover # Mana when Used”
(`charm_recover_X_mana_when_used`, game `Mods.CharmGainManaOnUse1..8`) is the
local per-use `Charm<N>ManaRecoveryOnUse`, independent of regeneration and
quality. “Mana Recovery from Flasks can Overflow maximum Mana during Effect”
(`local_unique_flask_mana_flask_recovery_can_overcap_mana_during_flask_effect`)
changes no output: recovery amounts are uncapped and current mana is not
simulated. “#% Chance to gain a Charge when you kill an enemy”
(`local_%_chance_to_gain_flask_charge_on_kill`) is the local
`FlaskChargeOnKillChance`: one charge per kill at that chance, so the slot's
`*ChargeGen` adds kills per second × chance / 100, times its charges-gained
factor. The kill rate is the PoBR config input `enemyKillsPerSecond`
(`Multiplier:EnemyKillsPerSecond`, merged with the Energy inputs by
`append_energy_options`); left empty, the line is inactive with that config
gate and no kill rate is assumed. The rule is exporter-absent. Lavianga's “This Flask cannot be Used but
applies its Effect constantly” (`local_flask_always_drinking`) reports zero
available uses while keeping its recovery rate. The flask line “N% of Damage taken
during effect Recouped as Life” (`local_flask_damage_taken_goes_to_life_over_4_seconds_%_during_effect`,
for example Blood of the Warrior) is `LifeRecoup` under `UsingFlask`, so it follows
the enabled-flask convention and shares the Life recoup duration (8 seconds by
default; the stat id's `over_4_seconds` is legacy naming, and the line states no
duration). It is not flat Damage Taken: the generic `N% of` rule used to read it
that way, adding 29 (Abyssal Lich Twister `4028d0b52984`) or 21 (Martial Artist Ice
Shot `71a879f43f2f`) to every damage type, and the pinned PoB2 drops the line
because its mid-line form leaves an unparsed remainder. The rule is
`exporter: "absent"`.

Three flask/charm lines that the generic phrase table read in part now have rules
(`special_mods.json`, all `exporter: "absent"`). “N% more Recovery if used while on Low
Mana / Low Life” (game stats `local_flask_amount_to_recover_+%_when_on_low_mana` /
`_on_low_life`, mods `FlaskIncreasedRecoveryOnLowMana` / `OnLowLife`) is a local
`FlaskRecovery` MORE tagged with the existing `LowMana` / `LowLife` condition: the
ledger reports it `inactive` until that condition holds (config `conditionLowMana` /
`conditionLowLife`; `LowLife` is also derived from Life reservation), then the flask's
recovery is multiplied by `1 + N / 100`. The generic rule had made it a
`FlaskLifeRecoveryLowLife` bonus that nothing read, even for the Low Mana wording. No
use timing or uptime of the Low pool is inferred, and there is no in-game test of the
stacking with other flask recovery modifiers. “Removes N% of Life Recovered from Mana
when used” and its Mana-from-Life twin (game stats
`local_flask_removes_%_of_life_recovery_from_mana_on_use` /
`..._mana_recovery_from_life_on_use`; rules `flask_removes_life_recovery_from_mana_on_use` /
`flask_removes_mana_recovery_from_life_on_use`, local utility properties
`UtilityLifeRecoveryRemovedFromMana` / `UtilityManaRecoveryRemovedFromLife`) make the
other resource pay a share of the flask's recovery each use. Each Flask slot reports
`Flask<N>ManaRemovedOnUse` (on a Life flask) or `Flask<N>LifeRemovedOnUse` (on a Mana
flask) as N% of that flask's total `Flask<N>Recovery` (instant plus gradual, after the
flask's own recovery modifiers such as the Low Life MORE); the opposite direction and
every flask without the line report 0, and the flask's own recovery is not reduced (the
generic rule used to make it a −N `FlaskRecovery`). Charms do not have these fields.
The derived detail is `UtilityRecovery * UtilityRemovedPercent / 100`
(`calc::utility`); it is a per-use amount, so no use frequency, current-pool state or net
resource model is assumed, and nothing is subtracted from `Net*Regen`. The Forsaken Bangle
charm line “N% of Chaos damage you prevent when Hit Recouped as Life and Mana during
effect” (game stat
`charm_x%_of_chaos_damage_from_hits_prevented_recouped_as_life_and_mana_during_effect`,
rule `charm_chaos_damage_prevented_recouped_as_life_and_mana`) stores
`ChaosDamagePreventedLifeRecoup` and `ChaosDamagePreventedManaRecoup`, each N, under the
enabled-charm convention above (the charm must be enabled and within the charm limit; no
uptime is inferred). The Chaos damage a hit “prevents” follows the physical definition: Chaos
damage entering the Chaos mitigation stage after taken-as shifts, minus what leaves it
after Chaos resistance and Chaos damage reduction, never below zero (negative resistance
prevents nothing). Flat damage taken and later damage-taken multipliers do not count, and
blocked and deflected hits contribute nothing; evaded hits never enter the landed-hit
loop. The game text does not call resistance “prevention”, so this is an assumption with
**no in-game test**. The mitigated EHP loop accumulates the per-type prevented damage
(`EhpTerms::prevented`); `survivability::life_recoup_terms` adds Chaos to the Life rate and
`survivability::mana_recoup_terms` returns the same amount as Mana (`ManaRecoupRate`,
with its own derived detail), each over its own recoup duration (8 seconds, 4 seconds
with `4SecondManaRecoup` / `4SecondRecoup` for Mana) and recovery rate
(`ManaRecoveryRate`). Ordinary `ManaRecoup` shares the Mana timing. Physical prevented
recoup still returns Life only. All three rules are batch `S2` and `exporter: "absent"`.

Rite of Passage's “Possessed by Spirit Of The Bear / Boar / Cat / Owl / Ox /
Primate / Serpent / Stag / Wolf for # seconds on use” uses game
`base/mods.json`: `SpiritOfThe*PossessedPlayer` (Monkey for Primate, Snake for
Serpent) and `SpiritPossessedStr/Dex/IntPlayer`. Separate fixed buffs grant
20% reduced damage taken (Strength), 30% skill speed / 15% movement speed
(Dexterity), or 80% damage (Intelligence), plus the spirit's own stats
(`calc::charm_spirits`). These buffs do not scale with charm effect/quality;
shared family bonuses apply once. Game Mods values take priority over the
[Wolf tooltip](https://poe2db.tw/us/Spirit_Of_The_Wolf#SpiritOfTheWolfPossessedPlayerRef),
which still says 10% skill speed / 5% movement speed. All new rules declare
`exporter: "absent"` and a game source. Periodic spirit attacks, Wolf's
maim/on-hit armour break remain outside reported calculations. Ox's slow
potency is
read when a slow applies, and its threshold bonus feeds `AilmentThreshold`. “Energy Shield Recharge starts on use” remains unsupported pending an event model;
Ignited and Consecrated Ground use the explicit [snapshots](snapshots.md#地面与护符事件).

Cat possession's game `critical_strike_chance_+% = 100` is an INC modifier to
`CriticalStrikeChance` (5% base becomes 10%); all possession names have reader
coverage, including Bear's `hit_damage_stun_multiplier_+% = 60` in enemy Heavy
Stun buildup. Charm “Recover # Life when Used”
(`charm_recover_X_life_when_used`, `CharmGainLifeOnUse1..8`) reports local
`Charm<N>LifeRecoveryOnUse`, independent of quality and regeneration, matching
the Mana sibling. “Also grants # Guard” (`charm_gain_X_guard_for_duration`,
`CharmGuardWhileActive1..6`) uses the existing hit-absorption pool for enabled,
in-budget charms: charm effect scales the grant; the strongest grant wins,
absorbs 100% of hit damage before Life/ES, and is capped at 200% maximum Life
([game Guard definition](https://poe2db.tw/us/Guard)); no reuse or duration
uptime is inferred. Both charm rules are exporter-absent. `BleedFaster`,
`IgniteFaster` and `PoisonFaster` retain their existing DPS/duration calculation
and the ledger identifies the selected skill's ability to inflict that ailment
as their read gate. Attack Blind chance (`attacks_chance_to_blind_on_hit_%`)
feeds `BlindUptime`; Blind's effects still use the configured enemy state.
“Your Totem Limit is doubled”
(`keystone_ancestral_bond`) is an `ActiveTotemLimit` MORE 100 (exporter-absent);

“Corrupting will always result in change”
(`soul_core_cannot_roll_no_outcome_with_corruption`) is crafting-only `S0`.
