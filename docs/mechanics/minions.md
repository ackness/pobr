# 机制约定：召唤物、图腾与友军

召唤物的创建、技能与数量，光环与友军共享，图腾与 Ballista。索引与写法见[机制约定](README.md)，与 PoB2 的差异另见 [pob2-differences.md](../pob2-differences.md)。

## 召唤物

召唤数量读取主动技能自己的游戏 stat。Raise Zombie 的 `base_number_of_zombies_allowed` 基础为 10，冻结 Chronomancer 的 +5 临时上限使其为 15。Wolf Pack 在冻结 Acolyte 的 19 级有 `skill_wolf_pack_size = 6`，品质再加 1，上限为 7；PoB2 报零不能推翻游戏数据。

`temporary_minion_limit_+` / `TemporaryMinionLimit` 只增加带 `Duration` 的召唤技能上限，不影响无持续时间的常驻召唤物。可辅助召唤物技能的辅助，其费用倍率也只给召唤技能计一次。Minion Splash 同时要求召唤物来源和近战 Strike，不能绕过明确排除条件，也不会把进攻效果给玩家。

### 技能与伤害来源

召唤物（`calc/minion.rs`、`calc/perform.rs::perform_minions`、`skill_env/minions.rs`）用它怪物的一个技能攻击：宝石的 `skillMinionSkill`（从 1 起，数据里存在的技能中第几个；构筑码导入、WASM 请求的 `minion_skill` 和导出都保留）选择，默认第一个。技能类型决定攻击、法术等标志；该技能自己的 stat，以及同组里按召唤物技能类型判断能辅助它的辅助宝石，作用于召唤物。召唤宝石与其辅助宝石上关于召唤物的 stat（“Minions deal {0}% more damage”、Muster 按不同种类的复生召唤物计等）由 `skill_stats::minion_modifiers` 交给召唤物，技能本身不再报告这些 stat。

伤害基础取游戏 `DefaultMonsterStats.MinionDamage`（`pobr_data::monster::monster_ally_damage`），攻击技能还带武器的暴击率。召唤物的攻击按所选技能在召唤物等级下的 `damage_multiplier` 与 `attack_speed_multiplier` 计算，伤害倍率同时作用于武器基础与附加伤害。不是选定怪物、也不是复制武器的同伴，附加攻击伤害还乘怪物的 `MonsterVarieties.DamageMultiplier`（`calc::env`）。

召唤技能上按角色属性分组的加成（“Minions from this skill have 1% increased Damage per {0} of your Strength”，`minion_1%_damage_+%_per_X_player_strength`；`active_skill_minion_1%_attack_speed_+%_per_X_player_dexterity`、`minion_1%_area_of_effect_+%_per_X_player_dexterity`，属性 ÷ 数值向下取整）与 “Minions convert {0}% of Physical Damage to Fire Damage”（`minion_skill_physical_damage_%_to_convert_to_fire`）给该技能的召唤物，不给角色。

Supporting Fire 每次命令的齐射数（`death_from_above_number_of_volleys`）在 DPS 末端计数，不放大单次命中。Manifest Weapon（“Manifest a copy of your main hand Melee Martial Weapon”）复制主手近战武器的伤害、攻击速度与暴击率，主手不是近战武器时不创建召唤物（`skill_env::minions::spawn_minions`）。幽魂和驯服的野兽保留怪物本身的伤害（`monster_damage`），再按两条更多伤害缩放：两张表之比，和 `spectre_tamed_beast_inherent_damage_+%_final`（快照里有这个 stat 没有数值，25 取自 PoB2 的游戏内测量，`SPECTRE_TAMED_BEAST_INHERENT_DAMAGE`）。

### 召唤物接受的属性与 buff

角色的光环（技能类型 `Aura`）也作用于召唤物：按游戏的技能描述，光环作用于 “you and Allies in your Presence”（Discipline：“Emit an Aura that grants you and Allies in your Presence additional Total Energy Shield”），召唤物得到的效果按角色的光环与 buff 效果加上召唤物自身受到的 buff / 光环效果，同名光环取较强的一份（`calc::buff_pass::auras_on_minion`）。

角色另外给召唤物的东西有三种来源：`MinionModifier` 词条；只作用于友军的光环 “Allies in your Presence …”（`ExtraAura`，按召唤物自身的 buff / 光环效果缩放）；“Companions gain added Attack damage equal to #% of your main hand Weapon's damage”（Catha's Balance 等：角色主手武器本地每种伤害的这个份额，只给带 `GainMainHandDmgFromParent` 的召唤物的攻击；戒指等给角色的全局附加伤害不属于武器，`perform::apply_main_hand_share`）。

只作用于召唤物的技能 buff 也走 `ExtraAura`：Pain Offering 的 “Aura grants Minions {0}% increased damage / Attack and Cast Speed” 不给角色本人，按角色的 buff 效果加上该技能与其辅助宝石的 buff 效果（Sacrificial Offering 的 “Supported Offering Skills have {0}% increased Buff effect”，`skill_stats::OFFERING_BUFF_EFFECT`）缩放；按游戏的描述，骨刺的光环作用于其范围内的召唤物，这里假设召唤物都在范围内。“Minions / Companions gain your Strength”（Sturdy Ally）把角色的最终力量给召唤物，每点力量 +2 最大生命（`calc::minion::LIFE_PER_STRENGTH`）。

Gigantic 召唤物（“Your Minions are Gigantic”、Tangmazu's Thurible 的无值 stat `companions_are_gigantic`）按游戏的说明 “Gigantic Minions have 20% more Maximum Life, 20% more Damage” 计算（`perform::apply_gigantic`）。

### 法术等级成长与抗性

召唤物施放法术时，例如 Water Djinn 的 Navira's Fracturing（`ChilledGroundBurstWaterDjinn`），使用该法术自己的基础伤害、施法时间和暴击率。冷却与可储存次数也读技能等级行，不按召唤物武器计算。

对于不到 5 个等级行的召唤物技能 stat set，当前实现按使用者等级计算成长。适配器保留两类输入：每个 stat 的单位效能值（`SkillStatSetLevel::per_effectiveness`，游戏表 `FloatStatsValues`），以及 stat set 的成长参数（`StatSetDef::user_level_scaling`，`IncrementalEffectiveness` / `DamageIncrementalEffectiveness`）。

`calc::minion::stats_at_user_level` 使用的等级 L 效能为：`SkillDamageBaseEffectiveness` 3.885209 × `BaseEffectiveness` × (1 + 成长 × (L − 1)) × (1 + 伤害成长)^(L − 1)。`GameConstants` 表没有导出，3.885209 取自 PoB2 数据；按它算出的 1 级值与已解析的游戏等级值一致，这只能交叉检查该常数，不能替代原始游戏表。

游戏表里每行另有使用者等级和每个 stat 的插值方式，导出里没有这两列，“不到 5 行”的界线与 PoB2 相同。带无值 stat `is_commandable_skill` 的是 Command 技能，召唤物使用它时 `CommandableSkill` 条件成立（`skill_stats::COMMAND_SKILL`），“with Command Skills” 类词条和 Bidding 的 “Supported Minions deal {0}% more Damage with Command Skills” / “increased Cooldown Recovery Rate for Command Skills” 才生效。

召唤物抗性从游戏的 `MonsterVarieties` → `MonsterTypes` → `MonsterResistances` 普通稀有度列读取，叠加召唤物词条、召唤技能与辅助宝石的抗性 stat、作用于召唤物的光环；复用 `calc::offence::resolve_resistance` 的向零截断、最大值（默认 75%、通常硬上限 90%）、−200% 下限与超上限计算。

基础抗性只在召唤物 ModDb 中计一次。WASM 与 CLI 的每个召唤物 `stats` 都输出四种抗性、最大值和超上限值，参考比较支持构筑码携带的 `Minion:<stat>` 抗性行。游戏[抗性说明](https://poe2db.tw/us/Keywords)将剧情惩罚写为玩家的抗性降低，因此不转移给召唤物。[Companion](https://poe2db.tw/us/Companion) 只定义同伴是一种复生召唤物；[Tame Beast](https://poe2db.tw/us/Tame_Beast) 虽要求捕捉 Rare Beast 并保留最多四条怪物词缀，没有明确召唤后的基础抗性按 Rare 列计算，当前同伴也用普通列，不采用 PoB2 的 Rare 覆盖值；保留的怪物词缀尚未建模。

### 召唤物施加的诅咒

召唤物自己的诅咒由它怪物的 `AppliesCurse` 技能组装（`skill_env::buff_specs::minions::minion_curse_specs`），按召唤物等级取值，效果读施法召唤物自己的诅咒效果与能辅助该技能的辅助宝石，不读角色的。配置 `raiseSpectreEnableCurses` / `summonCompanionEnableCurses`（条件 `SpectreCursesEnabled` / `CompanionCursesEnabled`）决定幽魂 / 同伴是否施加。

每个施法者先按自己的诅咒上限取诅咒（怪物的 `EnemyCurseLimit` 是总数，Ribrattle 为 3），敌人身上同名诅咒只保留缩放后更强的一份，诅咒槽数取各施法者上限中的最大值（`calc::buff_pass`）。Temporal Chains 的行动速度减慢没有输出，SerpentClanCurse 的流血几率还没有建模。幽魂与同伴的 buff 开关（`raiseSpectreEnableBuffs`、`summonCompanionEnableBuffs`）没有注册：Vaal Embalmed Bearer 的旗帜回复（`ABTTProcessionBannerRegen` / `ABTTProcessionBannerRegenSpectre`）在游戏表里的 stat set 与等级行都是空的，名字和说明也为空，拿到它的游戏定义之前不建模。

### 同伴与数量快照

召唤阶段按预处理后仍存在的启用组创建召唤物。已经识别为同一个物品／天赋授予技能的普通组，先由授予技能预处理合并。若不同组仍各提供同类召唤物，则继续沿用按组创建的约定（项目所有者 2026-10-14 决定）：同一种怪物在第二个组里出现时照常创建、计入同伴生命并保留精魄（同伴和幽魂按组与怪物计保留，其他技能放在几个组里都只保留一次），但不增加角色的召唤物数量（`Multiplier:SummonedMinion`）。精魄保留效率按角色的计数读取，例如 “2% increased Reservation Efficiency of Skills per Idol in your Equipment” 按装备里的偶像数。

辅助宝石给出的 buff 是它所辅助的每个技能的 buff，以该技能命名，选了怪物的同伴 / 幽魂再按怪物区分（`buff_specs::supported_skill_buff_name`）。同名 buff 的同类词条只保留较强的一条。stat 映射自带 buff 名（`effectName`，例如 Refraction 的 “Refractive Plating”）的 payload 不论辅助哪个技能都是同一个 buff（`stat_map_engine::player_buff_effect_name`）。

所以 Loyalty 辅助 7 只不同的同伴是 7 个 buff，“10% of Damage from Hits is taken from your Damageable Companion's Life before you” 合计 70%。Refraction 辅助 3 面旗帜仍是一个 buff。这个按组处理的约定仍可能把未识别的重复来源多算，不能当作游戏允许同时保有这些副本的证明。没有游戏内实测。

“每个召唤物”类词条（Hysseg's Claw 的 `damage_+%_per_active_minion` 等，含 “per Minion, up to …”）在角色与召唤物上读同一份数量快照（`calc::minion::bridge_count_multipliers`，在 buff 与召唤物计算之前桥接，重复桥接结果不变）：默认是计算实际创建的数量（`Multiplier:SummonedMinion`，按上段的数法），配置 `multiplierSummonedMinion` 覆盖它，显式的 0 也覆盖。

Presence 内的数量（`MinionPresenceCount`）默认等于它，配置 `multiplierMinionsInPresence` 可单独覆盖。召唤物的计算沿用这份快照，不再用来源数量改写 Presence 数，桥接之后也不把 ModDb 里的来源数量再加一次。“每个已召唤的某类召唤物”（例如 `attack_and_cast_speed_+%_per_summoned_raging_spirit`）读该类自己创建的数量（`ActiveRagingSpiritLimit`、`ActiveSkeletonLimit`、`ActiveGolemLimit`、`ActiveSentinelOfPurityLimit`），没有创建该类时为 0，其他种类的召唤物不计入。

图腾的数量另按[已知的有意差异](../pob2-differences.md#已知的有意差异)里的图腾口径。

所选召唤物效果若自身有命中伤害，即使原始游戏类型漏了 `Damage`，也可满足辅助需要的伤害能力，例如 Fire Djinn 的 Brand。判断只看选定效果，不借同技能其他效果的伤害，不改原始类型；无伤害或其他条件不合的辅助仍不兼容。

Storm Mage 的品质 `minion_maximum_life_%_to_gain_as_maximum_energy_shield` 按该召唤物 buff 后最终生命获得额外 ES。现有处理器用 `EnergyShield BASE` 加 `PercentStat(Life)`，不是基础资源 gain-as 矩阵。沿用 PercentStat 向上取整后再乘 ES 修正的约定；取整未实测。这个生命快照在进攻／防御前解出，来源查询和 ES 公式共用；不借玩家或其他召唤物生命，也不扣生命。这没有解决怪物固有 15% ES 是否扣生命的问题，见 [Lich02 未决差异](../pob2-differences.md#冻结语料中逐行解释的参考值差异)。

部分怪物通用攻击同时带 Melee 和 Projectile，表示可用能力。实际投射物命中标志根据所保留武器上下文判断能否发箭／魔杖投射物，明确 `base_is_projectile` 优先。已知空手怪物不吃武器投射物加成，缺上下文仍保留未知。原始类型继续用于辅助资格，不全局重写。

## 光环、友军共享与同伴

Presence 装备光环通过 `ExtraAura` 给召唤物；写明“你和友军”时玩家另得一份。武器上的友军攻速不是武器本地攻速。

Offering 效果查询使用 Offering 自己的技能类型，即使主技能另选，通用 Offering 天赋仍生效。冻结 `chronomancer-07` 的两条 15% 使 Pain Offering 的伤害／速度 83/35 经整数取整变为 107/45。Danse Macabre 的额外骷髅消耗和玩家 `HitRecently` 仍需适用的明确状态，不能为贴合 PoB2 自动开启。

“你和 Presence 内的友军 …” 类词条（例如 Infernalist 的 Bringer of Flame：“All Damage from you and Allies in your Presence contributes to Flammability and Ignite Magnitudes”）角色自己也有。PoBR 的 `ExtraAura` 只给友军，所以抽取器把 PoB2 规则里不带 `onlyAllies` 的 `ExtraAura` 展开为角色自己的词条（带同样的条件）加给友军的 `ExtraAura`（`sync-pob-catalog` 的 `character_share_of_aura`）；带 `onlyAllies` 的规则抽取器不收。

Blackened Heart 按施法者最终最大魔力，每满 100 点缩放其授予的 Unholy Might。Necromantic Conduit 将该命名 buff 交给友军后再展开，不能按接收召唤物的魔力决定来源幅度。接收方 buff 效果另算，重复标志只建立一份 buff，玩家拥有 buff 的条件不会自动给所有召唤物。低魔、关闭来源、Presence 数零或阻止友军光环仍保留限制，明细保留施法者天赋来源。

同伴 Presence 符文的 `companions_in_presence_non_skill_base_all_damage_%_to_gain_as_random_element` 与 `companions_in_presence_base_resist_all_elements_%` 走友军光环，只给召唤技能带 Companion 类型的对象。明确 Presence 为零、隐藏接收者、阻止友军光环同时禁掉收益和抗性惩罚。随机元素沿用三元素等权平均；正数 Presence 总量不代表逐个确定位置，效果仍按现有 ExtraAura 缩放，未实测范围／效果。

Watch How I Do It 经友军光环给接收者 `MainHandDamageToAllies`，只分享主手武器自身伤害给会攻击的接收者；玩家／戒指附加伤害不属武器。同伴专属分享另算，仍检查光环、隐藏对象及 Presence。

“每种不同同伴”的伤害计数，按启用且当前武器组生效的组中、实际创建数量为正的不同同伴身份计，不按容量或重复副本。明确 false／零 Presence 可关闭，正总量不确定每个位置。Manifest Weapon 是 Companion，也受 Presence 排除；但它不能受伤，不增加 `TotalCompanionLife`。Watch How I Do It 和 Catha 的武器分享各保留来源及原文，在召唤物明细中不合成匿名修正。

## 图腾与 Ballista

所选技能带 `SummonsTotem` / `UsedByTotem` 时，`calc::totem` 输出图腾防御。这里展示的是一只图腾和容量，不自动按容量乘 DPS。

游戏关系 `ActiveSkills.SkillTotemId → SkillTotemVariations → MonsterVarieties` 存于 `overlay-common/totem_varieties.json`。Ancestral Warrior 的 SlamTotem 原始 `LifeMultiplier` 为 120，其余为 100；加载时除以 100，分别转换为 1.2／1.0，再代入下表公式。等级由 `MinionGemLevelScaling` 取有效宝石等级的两倍，20 级宝石为 40 级图腾，等级映射尚未游戏内实测。

| 输出 | 计算与来源 |
|---|---|
| `TotemLife` | `TotemLifeBase = floor(DefaultMonsterStats.AllyLife[level] × LifeMultiplier)`，再用 `totem_life_+%` INC、`totem_life_+%_final` / `support_totem_life_+%_final` MORE 并取整；基础注入在 `skill::mods::totem_base_modifiers` |
| `TotemDurationMod` | `(1 + totem_duration_+% / 100) × MORE` |
| `Totem<T>Resist`、`Totem<T>ResistTotal`、`MissingTotem<T>Resist` | T 为 Fire/Cold/Lightning/Chaos。怪物基础均 0%，加宝石 `totem_elemental_resistance_%` / `totem_chaos_resistance_%` 及辅助。宝石 13 级前为零，20 级为 75/35。最大基础 75%，再加 `totem_maximum_all_elemental_resistances_%`，沿用玩家截断、上下限 |
| `TotemArmour` / `TotemEvasion` | 基础 `floor(DefaultMonsterStats.Armour/Evasion[level] × MonsterTypes.Armour/Evasion / 100)`；Spell、Ballista、Ancestral Warrior、Mortar Cannon 护甲系数 100，Shockwave/Dark Effigy 为零。全部闪避系数为零 |
| `TotemEnergyShield` | `EnergyShieldFromLife`% × TotemLife + `totem_maximum_energy_shield`，再乘 `totem_energy_shield_+%`。当前怪物的前项及已有 stat 来源均为零 |

各项有派生明细；20 级 Spell Totem 基础护甲为 418、闪避零。没有游戏表或 stat 给图腾格挡率，因此不输出 `TotemBlockChance`。这些字段不改变 DPS 或已召唤数量。

PoB2 固定给每种图腾 40% 元素／20% 混沌抗性，并忽略宝石等级 stat，游戏表没有这些固定基值。已有 tuned 对照中，Shockwave Totem `8468096e8b99` 为 PoBR 65/30 对 PoB2 40/41，Grim Pillars `19a4efbb971d` 为 75/35 对 40（火 75）/20；三份检查的生命 2285、2490、5019 一致。PoB2 护甲为零。

图腾被视为在玩家 Presence 内的友军，角色光环与 `ExtraAura` 可加抗性，例如 20 级 Purity of Fire +41% 火抗。效果按无自身效果修正的召唤物处理（`buff_pass::auras_on_minion`）；站位是假设，未实测。

`ActiveTotemLimit` 是宝石 `base_number_of_totems_allowed`（20 级 Spell Totem 为 2）加 `ActiveTotemLimit` BASE，攻击／法术／近战／Ballista 专属加成按技能标签筛选；再乘 Ancestral Bond MORE，或用 `non_modifiable_totem_limit` 的 OVERRIDE。无图腾为零。它只报告容量；实际数量默认与配置见[PoB2 差异](../pob2-differences.md#已知的有意差异)。

`base_skill_is_ballistified` / `is_ranged_attack_totem` 标识当前 Ballista，Mortar Cannon 内的攻击因此吃 Ballista 伤害和速度。容量查询含 `ActiveBallistaLimit` 及宿主 `TotemsAreBallistae`，不改原始辅助资格类型。冻结 Warbringer08 容量为 1 基础 +1 升华 +2 弩 +1 天赋 =5，没有覆盖、MORE 为 1；PoB2 因攻击缺宿主类型漏掉专属加成，只报 2。只解释这一个容量行，不能据此解释剩余伤害差异。
