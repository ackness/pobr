# 机制约定：装备、宝石与授予技能

装备条件、暗金与珠宝、宝石等级与授予技能、属性需求与消耗、药剂与护符。索引与写法见[机制约定](README.md)，与 PoB2 的差异另见 [pob2-differences.md](../pob2-differences.md)。

## 装备条件与物品读取

装备条件按当前武器组判断。Dance with Death 要求副手空、主手为单手武技武器；施法武器、空手、通过 Giant's Blood 单手握持的原双手武器都不符合。比较头盔护甲／闪避时，取本地修正后的装备数值、全局缩放前，以严格大小关系判断；相等时两个条件免疫都不给。

Warbringer 的 Renly's Training “Gain 35% Base Chance to Block from Equipped Shield instead of the Shield's value”（`ReplaceShieldBlock`）在装备盾牌时替换盾牌自身的格挡值，再乘格挡的加成（`calc::defence_panels::calc_block`）。Gemling 的 Virtuous Barrier 的属性 Mote 上限按角色从宝石获得的技能计：每个启用组里的技能宝石按属性需求计（单属性 +2，多属性各 +1），授予技能的辅助宝石（标签 `grants_active_skill`，例如 Eonyr's Thunder）也计。

物品或天赋授予的组不计，构筑里同一技能已另列一个组（`inject::virtuous_mote_counts`）。 [Virtuous Barrier 的游戏文本](https://poe2db.tw/us/Virtuous_Barrier) 按需要该属性的技能计，物品授予的技能也有自己的属性需求。tuned Gemling Twister（682f26ed1957）的权杖 Purity of Fire 因此计一次：Intelligence Mote 为 3 + Purity of Fire + Herald of Ice + Ghost Dance + Trinity = 7。

每点再生加成为 5% + floor(19 品质 × 0.125) = 7%，合计 49%。PoB2 跳过物品授予的技能，只有 6 点 / 42%。PoBR 的 LifeRegenRecovery 为 110.3391，构筑码参考值为 103.34，钉定 PoB2 为 101.76。

偶像数量按 socketable 类别计，表里没有条目时才回退到基底标签。[Carved Majesty](https://poe2db.tw/us/Carved_Majesty) 虽在 `base_items.json` 标为 `rune`，仍是 Idol，不承受 Idolatry 的 −4% 全元素抗性惩罚。因此 tuned 组 Spirit Walker `e071351f645d` 有五个非 Idol 镶嵌物，合计 −20%。它的 `ColdResistOverCap` 为 14，与钉定 PoB2 一致；构筑码参考值 2 漏掉了 Fox Idol 的 Bonded +12% Cold Resistance，胸甲的 “Idols socketed in this item gain the benefits of their Bonded modifiers” 使这条词条生效。

装备读取以物品自己的数值为准，不能把角色全局加成再算一次。Strong Chin 的 `stun_threshold_+_from_lowest_of_base_helmet_evasion_rating_and_armour` 取头盔本地护甲／闪避较低值；stat 名中的 base 指全局缩放前的装备值，见[防御门槛](defence.md#抗性减伤与门槛)。

Invoker 的 `cannot_gain_spirit_from_equipment` 只移除装备来源的 BASE 精魄，包括基底、显式 +N、镶嵌符文；天赋和任务精魄保留。诊断按 modifier 名称追踪读取，所以被抑制的装备行仍可能显示 applied，不能将这个状态当作最终有数值贡献。元素伤害吸纳 ES（`recoup_%_elemental_damage_as_energy_shield`）按类型计算，见[吸纳](defence.md#吸纳recoup)。

Giant's Blood 使用节点 32349 和 `can_wield_2h_axe_sword_mace_in_one_hand`，允许双手斧、锤、剑分别作为双持攻击来源。装备预处理先列候选来源，全部词条及物品授予基石到齐后，`GiantsBlood` 才开启第二手和 `DualWielding`。无该标志时，未使用副手的本地修正不会变成全局收益。

它只改变握持方式：基底类别、每手 `Weapon2H`、`UsingTwoHandedMelee`、双手伤害修正、三倍武技武器属性需求与力量固有生命减半都保留。盾仍是盾来源；Staff、Quarterstaff、弓、弩等不获得第二手资格。此处只选择伤害来源，不是完整装备合法性或技能武器限制校验。

导入魔法物品名称原样保留。基底查找先精确匹配目录，再找前缀与可选 ` of ...` 后缀之间最长的完整基底；Fists of Stone 等基底自身可含 of，仅出现在后缀中的基底名不算。武器与防御共用查找，已知基底时不再把导入防御显示值当基础重复加。

Grand Spectrum 在所选已镶嵌珠宝的隐式、显式、附魔中识别计数，腐化可把识别行移到隐式。按“有电抗词条的装备”计数时，当前普通装备栏位每件最多一次；全元素抗性、负值和相互抵消词条仍算，条件不生效不抹掉词条本身。嵌套友军／召唤物授予、单独存放的珠宝／药剂／护符不计。Transcendent 肢体和仅最大抗性的资格尚无客户端验证。

Loreweave 的稀有度与排他条款在同一段相邻行合并后解析，原文不改。只保留它自己的稀有度来源，不是同物品所有稀有度。原生和参考口径都保留允许的 84%（1.84 倍）；钉定 PoB2 还加 15% 护符，得到 1.99。此部分支持标为 `differs`，不能在参考比较时把已支持数字一并丢掉。

## 暗金、珠宝与天赋文本

几条暗金词条接到已有的计算入口（`overlay-common/special_mods.json`）：Blackheart 的 `base_armour_%_applies_to_chaos_damage` 让这一比例的护甲也用于混沌命中的减伤。Vigilant View 的 `maximum_chance_to_evade_is_50%` 与 Goregirdle 的 `maximum_physical_damage_reduction_is_50%` 限制闪避率与物理命中减伤。

Matsya 的 “Skills reserve 50% less Spirit”（`unique_spirit_reservations_are_halved`）乘在保留量上，不是保留效率。Plaguefinger 的 `all_damage_can_poison` 让元素命中也计入中毒的伤害，不提供中毒几率。`ignore_hexproof`（“Curses you inflict can affect Hexproof Enemies”）让诅咒越过敌人的 Hexproof。

Venopuncture / Icefang Orbit（`all_damage_to_bleeding_enemies_contributes_to_chill` / `all_damage_to_poisoned_enemies_contributes_to_chill`）只在敌人流血 / 中毒时让全部命中伤害计入冰缓强度。

Parry 的 “increased Parry Damage”（`parry_damage_+%`，Silverthorne 与天赋）和 Kaltenhalt 的 Parry 物理转冰（`parry_physical_damage_%_to_convert_to_cold`）只作用于 Parry。

Chober Chaber、Trenchtimbre、Crown of Eyes 的 “increases and reductions … also …” 只复制相应的 INC 词条：

- `minion_damage_increases_and_reductions_also_affects_you`：召唤物伤害增加／减少也给角色伤害。
- `additive_modifiers_to_minion_attack_speed_also_affect_you`：召唤物攻击速度增加／减少也给角色攻击速度。
- `additive_spell_damage_modifiers_apply_to_attack_damage`：法术伤害增加／减少也给攻击伤害。

复制保留来源和条件，不复制 MORE / LESS、点伤和只限定某类召唤物的词条。实现为 `calc::shared_modifiers::apply_shared_increases`。

Berek's Grip/Pass/Respite 和 The Taming 的 `wind_skills_count_as_empowered_by_{ignited,shocked,chilled}_ground` 合并两行文本后，设置 `Condition:TwisterFire/Lightning/Cold`。`CalculationSession::bridge_player_conditions` 将其传到条件表，供 Twister 的 `SkillDamageGainAs` 读取，与配置框相同。物品复用 `collect::combine_wrapped`，台账只记合并后的一行。

`wind_skills_can_be_empowered_by_multiple_elements`（规则 `wind_ground_multiple_surfaces`）当前标为 S0：引擎本来就逐元素独立应用，不限制 Twister 只能一种元素，所以两枚 Berek's 即使没有 The Taming 也都生效。这是已知简化，未实测，不应据此宣称游戏不需要该条款。五条来源规则为 `absent`，参考口径不变。

天赋节点 11335 / 47190 / 12183 / 36676 的额外天赋点由 `AllocationTree::edit` 计入候选预算；移除已花掉的额外点会被拒绝，`ExtraPoints` 在 ModDb 外读取。Heroic Tragedy 的 `local_unique_jewel_alternate_tree_seed` / `_version` Remembrancing 行由 `jewel_tree` 作为 `JewelData` 读取；缺种子节点数据仍报告未支持。

`spell_damage_+%_while_wielding_melee_weapon` 看当前武器类别的 `UsingMeleeWeapon`。旧的 `critical_hit_damage_bonus_+%_if_consumed_power_charge_recently` 按至少一个可移除暴击球近似，由 `usePowerCharges` / `overridePowerCharges` 控制，最小球不能激活。它不是实际近期消耗模拟；新增的耐力球消耗计数另见[配置快照](snapshots.md#敌人类型近期消耗与计数)。

`local_item_additional_skill_slots` / `num_additional_skill_slots` 和 `knockback_distance_+%` 不改变现有输出；Whispers of Doom 的 `keystone_hex_master` 则被 `CurseActivationDelay` 读取。

Undying Hate 的明确“小型天赋额外获得”与种子特定显著天赋变换分开。完整解析的内容加到征服半径内重建出的非属性小型节点，之后走分配和小型天赋效果缩放。未知内容／显著替换继续诊断。冻结 Titan06 的十二个已分配小型节点各有 6% 元素伤害，Hulking Form 每个放大为 9%，总加 108%。stat 为 `local_historic_abyss_jewel_conquered_small_passives_grant_elemental_damage_+%`，保留导入数值，不按当前范围重建。

“Conquered Attribute Passive Skills also grant ...”只加给相同半径、分配、武器组条件下的属性节点，不受 Hulking Form 小型放大。回归保留 Ritualist09 两个 +6 智力（最终 147→160）、Smith06 三个 +5 力量（271→287）；这些测试没有还原种子显著节点。

珠宝／天赋效果缩放也进入 `MinionModifier` 数值子项，不只顶层词条。The Adorned 仍检查腐化、稀有度、插槽资格，Hulking Form 仍检查已分配小型天赋；来源、作用对象不变，只加一次效果差量。Flag 和 MORE 不获得数值型天赋效果差量。

## 宝石等级、品质与授予技能

Uhtred 系辅助（“+2 to Level of Supported Skills if exactly one other Support is modifying them”）只把辅助宝石算作辅助：meta 宝石（Cast on Elemental Ailment）的触发效果是技能宝石的第二个效果，不算；只辅助 meta 宝石的辅助（Boundless Energy II 要求 `GeneratesEnergy`）也不修改被触发的技能（`skill_env::resolve::support_granted_gem_levels`）。“Skills from Corrupted Gems have …” 在主技能取等级的那颗宝石已腐化时生效（条件 `GemCorrupted`，`calc_orchestrator::skill::resolve::from_corrupted_gem`）。

宝石等级与品质加成（“+1 to Level of all Spell Skills”“+2% to Quality of all Skills”）按文本读取，不经过 ModDb：技能等级要在任何 modifier 存入之前确定。读取面是角色的全部词条来源：装备与珠宝、已分配和被物品授予的天赋、配置行（任务奖励、自定义词条），以及调用方传入的 `extra_modifier_texts`（由 `prepared_build_view` 并入构筑视图，主面板、逐技能总览和触发子计算共用）。新增一条文本通道时要同时接入 `skill::resolve::gem_property_bonuses`，否则这类词条在该通道里既不生效也不报未支持。被这样读取的行不计入未支持清单，也不再交给解析规则（规则会把“with an Intelligence requirement”误读成属性需求）。

种类有两种写法，“all Cold Skills”和“all Nova Skill Gems”，含义相同。种类按宝石标签判定（游戏表 `GemEffects.GemTags`，adapter 写进 `base/skill_gems.json`；每个标签对应一条 `<标签>_skill_gem_level_+` stat），种类里的每个词都要是宝石的标签：Rain of Arrows 带 `projectile` 标签，尽管它的技能不是 `Projectile` 类型；Shockwave Totem 带它的图腾所用 Slam 的 `slam` / `melee` / `nova` 标签。“Elemental”不是标签，指火、冰、电三种之一（`elemental_skill_gem_level_+`）。

辅助宝石的标签描述它辅助的技能和它授予的技能，它本身没有等级，所以不按标签判定；没有标签的旧快照也退回按技能类型判定，种类名与技能类型不同的（Totem → `SummonsTotem`、Curse → `AppliesCurse`、Strike → `MeleeSingleTarget`、Crossbow、Elemental）列在 `skill_env::mods::kind_skill_types`（`skill_env::mods::gem_tags_match_category` / `gem_level_category_matches`）。“increased bonuses gained from Equipped Rings and Amulets”（Ritualist 的 Mystic Attunement）放大戒指和护身符上的其他词条，也放大其中的宝石等级与品质加成，向下取整：+4 在 25% 下为 5（`skill::resolve::scale_slot_gem_property_bonuses`）。

按技能名写的等级词条（Cursecarver 的 “+4 to Level of Despair Skills”，不带 all）走同一通道，按游戏的 `ActiveSkills.DisplayedName`（`GrantedEffectDef.displayed_name`，不分大小写）匹配，Sniper's Mark 这类带标点、不能从 id 还原的名字也能匹配（`skill_env::mods::parse_gem_property_bonus`、`skill_env::resolve::gem_property_applies`）；只接受完整的 “Skills” / “Skill Gems” 写法，“Socketed”、“Supported” 和带其他条件的写法有各自的作用域，不当作全局等级加成。

物品授予的技能（“Grants Skill: Level 18 Purity of Ice”）有自己的等级和属性需求，与物品的穿戴需求无关；角色达不到时降到能满足的最高等级（[poe2wiki: Granted skill](https://www.poe2wiki.net/wiki/Granted_skill)），一级都达不到时用它最低的等级。每个宝石等级需要的角色等级取自游戏表 `ItemExperiencePerLevel`（`SkillGemDef::level_requirement`；表里没列的等级沿用前一级）；属性需求按宝石的属性占比计算，L 为该等级需要的角色等级：round((5 + (L − 3) × 1.7) × (占比 / 100)^0.9) + 4（`SkillGemDef::attribute_requirements`，与游戏显示的逐级数值一致，例如 Purity of Ice 18 级需要 42 力量、106 智力）。

被授予的组在构筑视图里记下授予等级（`SocketGroup::granted_level`，物品自己的组和上面那个被认作授予技能的普通组），`prepared_build_view` 再按角色等级和属性降级（`granted_skills::scale_granted_skills`）。属性来自不含技能的一次来源写入（`attributes_without_skills`：职业、装备、肢体、珠宝、配置、天赋和额外词条），技能自己的效果不计入；只有构筑有需要属性的授予技能时才运行，计算量约为一次计算的来源写入阶段。没有等级的授予行（“Grants Skill: Raise Shield”）不降级。已分配天赋授予的技能在构筑码里是 `<Skill source="Tree:<节点>">`，等级只是占位；它按角色等级取宝石能达到的最高等级（宝石经验表列到的最后一级，例如 98 级角色的 Sorcery Ward 为 20），同样记下授予等级、再检查属性。

游戏导出的角色还把它列成一个普通组（排在组首、等级相同、带辅助宝石），那个组就是被授予的技能，天赋的组让位给它，主技能组随之改指；节点未分配时天赋的组不授予任何技能（`granted_skills::resolve_tree_granted_skills`）。

辅助宝石辅助一颗宝石的任一效果时，它给的等级和品质作用于整颗宝石：Physical Mastery（要求 Physical）辅助 Iron Ward 的物理新星，Iron Ward 的 buff 也高一级。等级计算接收宝石每个效果的辅助判定（`skill_env::buff_specs::gem_judgements`，主效果在前），同一个辅助只算一次；Uhtred 系列“没有其他辅助时”的“其他辅助”也按整颗宝石计。

台账记录实际选中的 `GrantedEffectsPerLevel` 行，不是请求等级。Armour Explosion / Raise Shield 只有 1 级行，Pinnacle of Power 只有 20 级，普通玩家技能当前到 40 级；高请求等级不表示向表外推算。

辅助授予的效果继承首个兼容宿主宝石的等级，兼容性检查宿主全部效果，buff 也用同一宿主。[Living Lightning](https://poe2db.tw/us/Living_Lightning) 明确复制所插宝石等级；它可辅助 Repulsion Wave，Shock Siphon 可辅助 Pounce 的 Mark。按名字的等级加成同样给整颗宝石：Galvanic Shards +3 同时提高攻击和 Load Galvanic Shards，两者由 `GemEffects` 相连。

导入 1 级 Spear Throw 是默认攻击占位，基础等级跟角色走：`ItemExperiencePerLevel` 中 84 级角色为 19、90 级为 20，然后加全局等级（[Default Attack 关键词](https://poe2db.tw/us/Keywords)）。未绑定的弩、长矛、变形组在只有另一武器组满足类别时用另一组；例如 [Rend 要 Talisman](https://poe2db.tw/us/Rend)。

技能等级、品质、授予技能属性检查和镶嵌效果，均按该技能绑定武器组的装备与**天赋分配**求值，先结算 Bonded。全局等级与宝石基础等级分开：Pounce 基础 3、非武器加 5，在换掉 +5 近战武器后仍为 8，不能把中间基础值先压到 1。只有明确识别为物品授予的组免普通宝石穿戴需求；已列出的授予组保留辅助和启用状态，不另造启用副本。

全局品质也作用于[授予技能](https://www.poe2wiki.net/wiki/Granted_skill)，包括没有品质效果的 Raise Shield；导出器合成的 Thorns / Enemy Explode 没有游戏技能定义，不加品质。台账列全部启用导入组，PoB2 会合并部分物品组并补默认攻击，因而仍可能多出低等级 Mirror of Refraction / The Stars Answer、另一武器组 Spear Throw 或 PoB2 的 Spear Stab。清单不同不等于有效等级计算错误。

召唤物兼容辅助可影响召唤宝石的辅助等级加成和保留量，但仍遵守相同资格和不借其他效果类型的规则。暗金授予的技能用 `ModGrantedSkills` 附着关系匹配，不按显示名碰撞先后选择。目录保留 Coming Calamity 的三种 `UniqueHeraldOf*Player` 和 Sands of Silk 的 `BlinkSandPlayer`；只规范确认对应的计算组，原导入身份和独立宝石保留。

这些原生变体基础精魄保留为零，但不自动免除辅助的追加保留；护甲授予组保留激活武器组，效果列表改变时仍保留用户所选效果的含义。它们的原生属性占比也为零：Oracle Grim Pillars `0a7cf6c82619` 不再被 Sands 授予的普通 Blink 20 误抬到 157 智力，独立 Mana Tempest 19 / Temporal Chains 19 仍要求 147。冻结 Deadeye01 则由 16 个红辅助要求 80 力量，而不是普通 Herald of Ash 的 157；其他十六个技能记录不变。只对确切需求行登记参考差异。

## 属性需求与消耗

`ReqStr` / `ReqDex` / `ReqInt` 取全构筑装备与宝石需求的最大值，再与各颜色辅助宝石累计的需求比较（`calc_orchestrator::requirements::requirement_snapshots`）：装备先转换基底需求，再加本地固定需求，乘本地百分比修正并向下取整，最后应用全局修正（Runeforged Leaden Greathammer 的 60 力量、50 智力，转换 20% 到敏捷后再加 81 智力，得到 48 / 22 / 121；三倍武器需求后为 144 / 66 / 363）；宝石按原生等级计，腐化与装备给的技能等级不抬高需求；物品授予的技能和被认作它的普通组不新增一颗宝石的需求（见[已知的有意差异](../pob2-differences.md#已知的有意差异)）。

消耗按资源分固定、百分比与每秒三种通道；每秒消耗按使用、触发、装填或放置的频率计算，多发投射物不另付消耗（`perform::fill_resource_cost_rates`）。技能自带每秒消耗时，该资源的持续消耗直接取这一通道，不再把启动消耗按攻击频率叠加：Oil Barrage 是 Wyvern 引导技能，不是弩弹药；27 级的 69 魔力启动消耗与每分钟 4049 魔力在 110% 辅助倍率下分别为 75 与每秒 74。图腾放置用召唤技能自己的游戏 `cast_time`（`calc_orchestrator::skill::mods::skill_timing_modifiers`，陷阱、地雷与战吼同理），施法速度与图腾放置速度分开。不是攻击、也没有专属速度的技能（例如 Wildwood's Gifts，`AncientGiftsPlayer`，只有 1000 毫秒的施法时间、没有 `Spell` 类型）按施法速度加技能速度计算使用时间（`skill_use_time::speed_names_for`）；战吼、图腾、陷阱与地雷仍用各自的速度。

游戏把这类技能的基础时间标为 “Cast Time”，PoB2 与构筑码参考值也这样计，**没有找到游戏对施法速度作用范围的正式说明，也没有游戏内实测**。`ChannelTime` 只含引导阶段，不含释放与冷却。

Spell Mana Cost Efficiency（`base_spell_mana_cost_efficiency_+%`）只降低法术魔力消耗，效率 INC 与通用效率相加后，以 `1 + 效率 / 100` 除消耗；它标为 absent。The Fall of the Axe 的 “Grants Onslaught during effect”（`charm_gain_onslaught_during_effect`）同样标为 absent；按 [游戏 Onslaught 定义](https://poe2db.tw/us/Onslaught) 增加 20% Skill Speed 与 10% Movement Speed。沿用 flask/charm 约定：构筑中启用且可生效的护符视为效果激活；“Used when you are affected by a Slow” 不建模为 uptime，另一件生效的物品不会激活被关闭或超出槽位的护符。

[Derange](https://poe2db.tw/us/Derange) 的 `gain_energy_shield_cost_equal_to_intelligence` 给所选兼容技能追加等于最终智力的 ES 费用，经 `ESCostNoMult` 处理。辅助的基础费用倍率不乘这一追加量，其他适用费用修正仍生效。

Culling Strike I/II 原生的稀有／独特敌人条件，需同时处于 Effective 模式。配置可见性会在条件尚未激活时识别来源，允许显示已有稀有度控件，但不会自动勾选。

## 药剂与护符

`life_recovery_from_flasks_also_recovers_energy_shield`（Shavronne's Satchel）追加 `Flask{1,2}EnergyShieldRecovery`、`EnergyShieldInstantRecovery`、`EnergyShieldRecoveryPerSecond`。复用生命药剂总量、品质、药剂恢复和瞬间比例，缓回部分使用 ES 恢复速率；魔力药剂不复制，`CannotGainEnergyShield` 禁止复制回复。

### 启用状态与单次使用

护符／药剂的“Used when ...”（`local_flask_use_on_*`、`local_charm_trigger_when_cursed`）按启用物品约定识别，不计算触发频率。只有启用且未超槽位的护符才视为当前生效；`charm_effect_+%` 不放大关闭护符。

`charm_recover_X_mana_when_used`（`Mods.CharmGainManaOnUse1..8`）给 `Charm<N>ManaRecoveryOnUse`；生命同类 `charm_recover_X_life_when_used`（`CharmGainLifeOnUse1..8`）给 `Charm<N>LifeRecoveryOnUse`。两者是每次使用的局部量，不受品质或再生影响。魔力药剂溢出最大魔力的 `local_unique_flask_mana_flask_recovery_can_overcap_mana_during_flask_effect` 暂不改输出，因为已有恢复量未封顶，也未模拟当前魔力。

`local_%_chance_to_gain_flask_charge_on_kill` 为局部 `FlaskChargeOnKillChance`：`*ChargeGen` 增加“每秒击杀 × 几率 / 100 × 充能获取倍率”。频率必须由 `enemyKillsPerSecond`（`Multiplier:EnemyKillsPerSecond`，`append_energy_options` 合入）提供，留空则台账报缺该条件，不假设击杀率。规则 `absent`。Lavianga 的 `local_flask_always_drinking` 显示可用次数零，但保留常驻恢复率。

### 药剂吸纳与资源扣除

药效期间吸纳生命的 `local_flask_damage_taken_goes_to_life_over_4_seconds_%_during_effect`（Blood of the Warrior）映射为 `UsingFlask` 下的 `LifeRecoup`。stat 名里的 `over_4_seconds` 是旧名，当前文本未写四秒；实际按八秒基础及[吸纳速度规则](defence.md#吸纳recoup)计算。它不是固定承伤增加。旧通用解析曾给 Abyssal Lich Twister `4028d0b52984` 每种伤害 +29、Martial Artist Ice Shot `71a879f43f2f` +21；钉定 PoB2 因中段文本未解析而丢弃，规则为 `absent`。

低魔／低血使用时更多恢复（`local_flask_amount_to_recover_+%_when_on_low_mana` / `_on_low_life`，Mods `FlaskIncreasedRecoveryOnLowMana` / `OnLowLife`）是局部 `FlaskRecovery` MORE，要求 `LowMana` / `LowLife`。配置 `conditionLowMana` / `conditionLowLife` 或生命保留可提供状态；未满足时台账 inactive。恢复乘 `1 + N/100`，不猜使用时机或低资源覆盖；与其他药剂恢复的叠加未实测。旧 `FlaskLifeRecoveryLowLife` 无读取者的误解析已取代。

“从另一资源扣掉本次恢复量的一部分”使用 `local_flask_removes_%_of_life_recovery_from_mana_on_use` / `..._mana_recovery_from_life_on_use`，规则 `flask_removes_life_recovery_from_mana_on_use` / `flask_removes_mana_recovery_from_life_on_use`。局部属性是 `UtilityLifeRecoveryRemovedFromMana` / `UtilityManaRecoveryRemovedFromLife`。

生命药剂给 `Flask<N>ManaRemovedOnUse`，魔力药剂给 `Flask<N>LifeRemovedOnUse`；另一方向和没有词条的药剂为零，护符无这些字段。公式是 `UtilityRecovery × UtilityRemovedPercent / 100`（`calc::utility`），其中恢复量包含药剂自身修正后的瞬间和缓回总和，不减少药剂本来回复，也不是旧的 −N `FlaskRecovery`。这是每次量，不推算频率、当前资源或净消耗，不从 `Net*Regen` 扣除。

Forsaken Bangle 的 `charm_x%_of_chaos_damage_from_hits_prevented_recouped_as_life_and_mana_during_effect` 给 `ChaosDamagePreventedLifeRecoup` 和 `ChaosDamagePreventedManaRecoup` 各 N。它采用“转为其他类型承受之后，进入混沌减免的伤害减去抗性和混沌减伤后伤害”的防止量，最低零；负抗性不算防止。固定承伤和后续承伤倍率、格挡（含部分格挡）、偏斜不计，闪避不进入命中循环。

游戏文字没有明说抗性是否属于 prevention，因此这是**未实测假设**。EHP 的 `EhpTerms::prevented` 累计此量，`survivability::life_recoup_terms` / `mana_recoup_terms` 按各自时长、RecoupSpeed 与恢复速率回复，后者也给 `ManaRecoupRate` 明细。魔力基础八秒，`4SecondManaRecoup` / `4SecondRecoup` 可改四秒；普通 `ManaRecoup` 共用。防止物理伤害仍只回复生命。上述低资源、资源扣除、混沌防止三类规则为 S2、`absent`。

### 护符 Guard 与附身

`charm_gain_X_guard_for_duration`（`CharmGuardWhileActive1..6`）为启用且在槽位内的护符给 Guard。护符效果缩放，多个授予取最强，先于 Life/ES 吸收 100% 命中伤害，上限为最大生命的 200%（[Guard 定义](https://poe2db.tw/us/Guard)）。不模拟再次使用或持续覆盖；生命使用恢复与 Guard 两条规则均为 `absent`。

Rite of Passage 的 Bear / Boar / Cat / Owl / Ox / Primate / Serpent / Stag / Wolf 附身读取游戏 `base/mods.json` 的 `SpiritOfThe*PossessedPlayer`（Primate 对 Monkey，Serpent 对 Snake）及 `SpiritPossessedStr/Dex/IntPlayer`。独立固定 buff 给力量系 20% reduced 承伤、敏捷系 30% 技能速度和 15% 移速、智力系 80% 伤害，再加灵体本身效果（`calc::charm_spirits`）。同家族奖励一次，护符效果／品质不缩放。

游戏 Mods 优先于仍写 10% 技能速度／5% 移速的 [Wolf 提示](https://poe2db.tw/us/Spirit_Of_The_Wolf#SpiritOfTheWolfPossessedPlayerRef)。Cat 的 `critical_strike_chance_+% = 100` 是 INC：5% 基础变为 10%；Bear 的 `hit_damage_stun_multiplier_+% = 60` 进入重度晕眩，Ox 的减速效力在受减速时读取，门槛加成进 `AilmentThreshold`。

规则为 `absent`，所有附身数值名都有读取覆盖，但周期性灵体攻击、Wolf 的 Maim／命中破甲尚未模拟。Thawing Charm 的“使用时开始 ES 充能”已按启用护符约定支持，见[充能说明](defence.md#能量护盾充能)，不能再笼统写成完全不支持；真正的触发时序仍未模拟。点燃／奉献地面使用独立[事件快照](snapshots.md#地面与护符事件)。

`BleedFaster` / `IgniteFaster` / `PoisonFaster` 继续影响对应异常 DPS 和时长，台账检查所选技能能否施加。攻击致盲 `attacks_chance_to_blind_on_hit_%` 给 `BlindUptime`，实际致盲仍看配置状态。`keystone_ancestral_bond` 以 `ActiveTotemLimit` MORE 100 将图腾上限翻倍（`absent`），不是自动召唤满额。`soul_core_cannot_roll_no_outcome_with_corruption` 仅影响制作，归 S0。
