//! 技能宝石 / 授予效果域 schema（`base/skill_gems.json` / `base/granted_effects.json` /
//! `base/granted_effect_levels.json` / `base/granted_effect_stat_sets.json` /
//! `base/cost_types.json`，来自 `SkillGems.dat` / `GrantedEffects*` / `CostTypes.dat`）。

use serde::{Deserialize, Serialize};

/// 技能宝石定义（来自 `SkillGems.dat` + `BaseItemTypes` 外键解析）。
///
/// 宝石**自身无 Id 列**，其身份取自 `BaseItemType` 指向的基底 Id
/// （形如 `Metadata/Items/Gems/SkillGemFireball`）。`name` 走 base_items 域，
/// 此处只存与宝石机制相关的字段。
///
/// 宝石→授予效果的连边（M1-T5.1，契约 C5）：`granted_effect_id` /
/// `additional_granted_effect_ids` 不在 base 产物中（`GemEffects` 表所在 bundle
/// 在钉定补丁不可下载，见 `pipeline/config.json` `_tablesUnavailableForPinnedPatch`），
/// 由 `pobr-gamedata` 在加载时从 `overlay/gem_effects.json`（[`GemEffectDef`]，
/// extract-lua 自 vendor `Data/Gems.lua` 抽取）merge 填充；adapter 恒写空。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillGemDef {
    /// 稳定 ID，取自 `BaseItemType` 基底的 `Id`。
    pub id: String,
    /// 宝石类型（GGG 原始枚举：0=主动技能，1=辅助），保留原值便于排查。
    pub gem_type: Option<u32>,
    /// 宝石颜色（GGG 原始枚举：1=红/力，2=绿/敏，3=蓝/智，4=白等）。
    pub gem_colour: Option<u32>,
    /// 使用所需最小角色等级。
    pub min_level_req: u32,
    /// 力量需求百分比（属性需求权重）。
    pub str_pct: u32,
    /// 敏捷需求百分比。
    pub dex_pct: u32,
    /// 智慧需求百分比。
    pub int_pct: u32,
    /// 是否为辅助宝石（由 `GemType == 1` 判定）。
    pub is_support: bool,
    /// 宝石授予的主效果 id（`GemEffects.GrantedEffect` → `GrantedEffects.Id`，
    /// 如 `IceNovaPlayer`）。来源 = `overlay/gem_effects.json` 加载期 merge；
    /// base 产物 / 旧数据包恒为 `None`。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub granted_effect_id: Option<String>,
    /// 宝石**附加**授予的效果 id（`GemEffects.AdditionalGrantedEffects` 列序，
    /// 如 InfernalCry → `InfernalCryCorpseExplosionPlayer`）。对应 PoB2
    /// `additionalGrantedEffectId1..N`（`Export/Scripts/skills.lua:919-923`），
    /// meta/复合宝石展开（T5.6）的外键。来源同 `granted_effect_id`。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub additional_granted_effect_ids: Vec<String>,
}

/// 授予效果定义（来自 `GrantedEffects.dat` + 外键解析）。
///
/// 每个宝石/物品最终授予一个或多个 `GrantedEffect`；主动技能效果会关联到一条
/// `ActiveSkills` 记录（显示名 / 技能类型）。本切片取身份 + 主动技能链接 +
/// 施放时间 + support 适用性裁决列（require/add/exclude 类型表达式 +
/// cannot_be_supported/support_gems_only 布尔）+ StatSet/CostTypes 外键索引。
///
/// 分等级参数（cost / cooldown / attack time）在独立域
/// [`SkillLevelDef`]（`granted_effect_levels.json`），以本 `id` 为键。
///
/// 分等级**伤害 stat 值**在独立域 [`SkillStatSetDef`]
/// （`granted_effect_stat_sets.json`），同样以本 `id` 为键（适配器已按 `stat_set`
/// 外键 join 解析）。`stat_set` 字段保留原始索引备查。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GrantedEffectDef {
    /// 稳定 ID，即 `GrantedEffects.Id`（如 `FireballPlayer`）。
    pub id: String,
    /// 是否为辅助效果。
    pub is_support: bool,
    /// 关联的主动技能 Id（解析 `ActiveSkills.Id`；辅助效果为 None）。
    pub active_skill: Option<String>,
    /// 施放/吟唱时间（毫秒）。原始值为 0（瞬发/辅助）时归一化为 None。
    pub cast_time: Option<u32>,
    /// support 适用性 **require 后缀表达式** token 流（`AllowedActiveSkillTypes` 列，
    /// FK → `ActiveSkillType.Id` 名称；`"AND"/"OR"/"NOT"` 是该表的特殊行，保序保留）。
    /// 空 = 不限制（接受任何主动技能）。求值语义见 PoB2
    /// `CalcTools.lua::doesTypeExpressionMatch`（后缀栈机，栈内任一真即匹配）。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub require_skill_types: Vec<String>,
    /// 兼容 support 给主动技能**并入**的类型 token（`AddedActiveSkillTypes` 列；
    /// 普通名单，非表达式）。对应 PoB2 `addSkillTypes`，参与 support 裁决不动点
    /// （`CalcActiveSkill.lua:179-210`）。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub add_skill_types: Vec<String>,
    /// support 适用性 **exclude 后缀表达式** token 流（`ExcludedActiveSkillTypes` 列，
    /// 同 require 的 token 编码）。表达式命中即拒绝支援。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub exclude_skill_types: Vec<String>,
    /// 主动效果**不可被任何 support 支援**（`CannotBeSupported` 列）。对应 PoB2
    /// `grantedEffect.cannotBeSupported`（裁决第一段）。
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub cannot_be_supported: bool,
    /// 该 support **仅能支援宝石授予**的技能（`SupportsGemsOnly` 列）。对应 PoB2
    /// `grantedEffect.supportGemsOnly`（无 gemData 的技能拒收，裁决第二段）。
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub support_gems_only: bool,
    /// `GrantedEffectStatSets` 表的外键索引（原始 `StatSet` 列）。分等级伤害 stat 值
    /// 经此解析（待 stat-set 表下载后接入）；当前仅保留索引备查。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stat_set: Option<u32>,
    /// **附加** statSet 的稳定 id（`AdditionalStatSets` 列，FK → `GrantedEffectStatSets.Id`
    /// ——W0 核验修正：FK 目标是 statSet 表而非另一行 GrantedEffects；列序保留）。
    /// 技能形态变体（如 IceNova → `IceNovaPlayerOnFrostbolt` / `IceNovaColdInfusedPlayer`），
    /// 与 [`SkillStatSetDef::sets`] 的 `sets[1..]` 一一对应（M1-T5.2）。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub additional_stat_set_ids: Vec<String>,
    /// 消耗类型外键索引（原始 `CostTypes` 列，如 `[0]`=法力）。与
    /// [`SkillLevelDef::cost_amounts`] 按位置配对（第 i 个类型消耗第 i 个数量）。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cost_types: Vec<u32>,
    /// 关联主动技能的类型名（`ActiveSkills.ActiveSkillTypes` → `ActiveSkillType.Id` 解析，
    /// 如 `["Attack","Projectile","Damage"]` / `["Spell","Area"]`）。用于攻击/法术判定
    /// （攻击技能的击中伤害来自武器基底）。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub skill_types: Vec<String>,
    /// 该技能召唤的 minion id 列表（PoB2 Export 模板手工指令 `#minionList`，
    /// 不在 `.dat`——M5a-A3）。**merge 后内存形态**：base `granted_effects.json`
    /// 不含此字段，由 `pobr-gamedata` 在加载期从 `overlay/granted_effect_minions.json`
    /// （[`crate::catalog::actors::GrantedEffectMinionDef`]）按 `id` 拼入。
    /// 空 = 非召唤技能（向后兼容）。出处：vendor `Export/Scripts/skills.lua:771-776`。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub minion_list: Vec<String>,
    /// support 追加的 minion id 列表（`addMinionList`，merge 同 `minion_list`）。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub add_minion_list: Vec<String>,
    /// 召唤物借用玩家装备槽位列表（`minionUses` 中值为 true 的键，merge 同上）。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub minion_uses: Vec<String>,
    /// 召唤物使用独立 item set（`minionHasItemSet`，merge 同上）。
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub minion_has_item_set: bool,
}

impl GrantedEffectDef {
    /// 是否为攻击技能（类型名含 `Attack`）——攻击技能的击中伤害来自武器基底。
    pub fn is_attack(&self) -> bool {
        self.skill_types.iter().any(|t| t == "Attack")
    }

    /// 是否为法术技能（类型名含 `Spell`）——法术不使用武器伤害。
    pub fn is_spell(&self) -> bool {
        self.skill_types.iter().any(|t| t == "Spell")
    }

    /// 是否为**非武器攻击**（类型名含 `NonWeaponAttack`，如 Shield Wall）——攻击的击中
    /// 基础伤害由技能自身（off-hand stat-set）提供，而非主手武器基底。对应 PoB2
    /// `skillFlags.shieldAttack`/`NonWeaponAttack`：source 不取 weaponData1，而由
    /// `setOffHand*` 技能 stat 决定。
    pub fn is_non_weapon_attack(&self) -> bool {
        self.skill_types.iter().any(|t| t == "NonWeaponAttack")
    }
}

/// 某个授予效果在某一等级上的参数（来自 `GrantedEffectsPerLevel.dat`）。
///
/// 与 [`GrantedEffectDef`] 解耦为独立域（一个效果有几十个等级行，避免主表膨胀）。
/// 收录于 `granted_effect_levels.json`，以 `GrantedEffect` id 为键聚合为升序数组。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SkillLevelDef {
    /// 宝石/技能等级（1-based）。
    pub level: u32,
    /// 冷却时间（毫秒）。原始 0（无冷却）归一化为 None。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cooldown_ms: Option<u32>,
    /// 攻击时间（毫秒，攻击型技能）。原始 0（非攻击/由武器决定）归一化为 None。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attack_time_ms: Option<u32>,
    /// 各消耗类型的消耗量（与 [`GrantedEffectDef::cost_types`] 按位置配对）。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cost_amounts: Vec<u32>,
    /// 攻击速度乘数（PoB `GrantedEffectsPerLevel.attackSpeedMultiplier`，百分点，可负）。
    /// 作用于武器攻击速率：`AttackRate × (1 + attackSpeedMultiplier/100)`（如 Flicker -50）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attack_speed_multiplier: Option<f64>,
    /// 技能伤害基础倍率（PoB `GrantedEffectsPerLevel.baseMultiplier`，如 Flicker L13 = 1.99）。
    /// 当 stat-set 的 `BaseMultiplier` 缺失时作为 [`SkillStatSetLevel::damage_multiplier`]
    /// 的回退源（二者同义，PoB 在两表均存）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_multiplier: Option<f64>,
    /// 技能基础暴击率（PoB `critChance`，百分点；如 Comet = 13.0=13%）。来源 =
    /// `GrantedEffectStatSetsPerLevel`（社区 schema 列 `SpellCritChance` = vendor
    /// `AttackCritChance` 主列 `/100`，社区 `AttackCritChance` = vendor
    /// `OffhandCritChance`，≠0 时覆盖；vendor `Export/Scripts/skills.lua:281-286`）。
    /// 法系/攻击技能的固有暴击率来源——攻击技能若 `None` 则回退武器基底暴击。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub crit_chance: Option<f64>,
    /// 辅助宝石 cost 倍率（PoB `manaMultiplier` = `CostMultiplier - 100`，百分点，可负，
    /// 如 Heightened Curse +30）。`None` = 原始 `CostMultiplier == 100`（无倍率，对齐
    /// vendor `Export/Scripts/skills.lua:262-264` 的省略条件）。消费侧对被支援技能注入
    /// `SupportManaMultiplier` MORE（PoB2 `CalcActiveSkill.lua:689-691`）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mana_multiplier: Option<f64>,
    /// Spirit 扁平保留量（PoB `spiritReservationFlat`，`.dat` 社区列名 `Reservation`，
    /// 原值；vendor `skills.lua:244-246`）。持续型效果（光环/常驻 buff）的 Spirit
    /// 预留来源。`None` = 0（无保留）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spirit_reservation_flat: Option<f64>,
    /// 保留倍率（PoB `reservationMultiplier` = 原值 `- 100`，百分点，可负；`.dat` 社区
    /// 列名 `EffectOnPlayer`；vendor `skills.lua:247-249`）。`None` = 原始值 == 100。
    /// 消费侧注入 `ReservationMultiplier` MORE（PoB2 `CalcActiveSkill.lua:692-694`
    /// support 侧 / `:754-756` active 侧）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reservation_multiplier: Option<f64>,
    /// 可储存使用次数（PoB `storedUses`，原值；vendor `skills.lua:277-279`）。
    /// `None` = 0（无储存）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stored_uses: Option<u32>,
    /// 等级需求（PoB `levelRequirement`）。**PoE2 `.dat` 无 `PlayerLevelReq` 列**——真源
    /// 是 `SkillGems.ItemExperienceType → ItemExperiencePerLevel.PlayerLevel`（vendor
    /// `skills.lua:239-240`），该表在钉定补丁 4.5.0.3.4 的 bundle 不可下载（见
    /// `pipeline/config.json` `_tablesUnavailableForPinnedPatch`）。M1 仅留 schema 席位
    /// （adapter 恒写 `None`、不消费）；数据由 M5a（createMinionSkills 选级）经
    /// extract-lua 兜底通道落库。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub level_requirement: Option<u32>,
}

/// 某授予效果（技能）的**全部 statSet**（主 set + 附加 set，M1-T5.2 多 set 建模）。
///
/// 主 set = `GrantedEffects.StatSet` 指向的 `GrantedEffectStatSets` 行；附加 set =
/// `GrantedEffects.AdditionalStatSets` 列序指向的各行（如 IceNova →
/// `IceNovaPlayerOnFrostbolt` / `IceNovaColdInfusedPlayer`，技能形态变体）。
/// 附加 set 在入库时已按 vendor 导出语义与主 set **合并**（constant/分等级 stat
/// 拼接、效力回退，见 [`StatSetDef`]），消费侧选中即用、无需再 merge。
///
/// 收录于 `granted_effect_stat_sets.json`，以 [`GrantedEffectDef::id`] 为键
/// （适配器已在导出时完成 join）。这是「宝石 → 技能伤害 → DPS」数据通道的最后一环。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SkillStatSetDef {
    /// 授予效果 id（如 `FireballPlayer`），与 [`GrantedEffectDef::id`] 对齐。
    pub effect_id: String,
    /// statSet 列表：`sets[0]` 恒为主 set，其后为附加 set（`AdditionalStatSets` 列序）。
    /// 消费缺省取主 set（PoB2 `<Gem statSetIndex>` 缺省 1 = 第一个 set）。
    pub sets: Vec<StatSetDef>,
}

/// 单个 statSet（来自 `GrantedEffectStatSets` + `GrantedEffectStatSetsPerLevel`）。
///
/// 附加 set（`sets[1..]`）的内容已按 PoB2 导出脚本的 base-merge 语义入库
/// （vendor `Export/Scripts/skills.lua:498-553`）：
/// - `constant_stats` = 主 set 常量 ++ 本 set 常量（`:502-504` tableConcat）；
/// - `base_effectiveness`：本 set 原始值为默认 1 时回退主 set（`:506-508`）；
/// - 分等级行与主 set 行**按数组位置配对**，stat = 主行 ++ 本行（`:541-549`）；
/// - `damage_multiplier`：本行 `BaseMultiplier ≠ 0` 取本行，否则取主配对行
///   （`:533-541`；`UseSetAttackMulti` 列未下载，两分支终值同为本行 `/10000+1`，
///   缺失时回退主行为保守近似）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StatSetDef {
    /// statSet 稳定 id（`GrantedEffectStatSets.Id`，如 `IceNovaColdInfusedPlayer`；
    /// 主 set 通常与 effect id 同名）。
    pub set_id: String,
    /// 形态 label 文本（如 `Cold-Infused`）。来源 = `overlay/stat_set_labels.json`
    /// （vendor 抽取——`.dat` `Label` 列的 FK 目标表 `GrantedEffectLabels` 不可下载），
    /// 加载期 merge；base 产物 / 无 vendor 对应时为 `None`。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// PoB2 导出 statSets 列表中的 1-based 序号（vendor 模板 `#set` 顺序，=
    /// `<Gem statSetIndex>` 的索引语义）。vendor 模板会策展跳过个别 set（如
    /// IceNovaPlayerOnFrostbolt），故与 `sets` 数组下标**不必然一致**。来源同
    /// `label`（overlay merge）；`None` = vendor 未导出该 set（不可被 statSetIndex 选中）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vendor_set_index: Option<u32>,
    /// stat-set 的基础效力（`BaseEffectiveness`，备查；分等级值已是解析后的最终量）。
    #[serde(default, skip_serializing_if = "is_zero_f64")]
    pub base_effectiveness: f64,
    /// 等级无关的常量 stat（`ConstantStats` + 值；如 support 宝石的 `damage_+%_final` 倍率）。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub constant_stats: Vec<SkillDamageStat>,
    /// statSet `baseMods` 中的固有**攻击速度 MORE**（PoB2 `mod("Speed", "MORE", N, ModFlag.Attack)`，
    /// 百分点；如 Flicker Strike = 285）。这是 PoB2 自带的常量 baseMod，不在 GGG `.dat` 表中——
    /// 由 vendor Lua 抽取合并。作为 `AttackSpeed` MORE 注入（仅攻击技能链路消费）。`None`=无。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skill_attack_speed_more: Option<f64>,
    /// 技能 DoT 配置布尔族（M4-T4 W-D1）：vendor skillData `dotIs*`（statSet
    /// `baseMods` 的 `skill("dotIsArea", true)` 类条目，PoB2 据此从 dotCfg 剥除
    /// 对应 ModFlag 位，`CalcOffence.lua:5831-5860`）。GGG `.dat` 无对应列——
    /// 经 extract-lua skill_overrides 通道在加载期 merge。缺省全 false =
    /// 保守不剥 flag（蓝图 m4-offence-deep §5 风险行）。
    #[serde(default, skip_serializing_if = "DotFlags::is_default")]
    pub dot_flags: DotFlags,
    /// 尸体爆炸门控（M4-G）：vendor statSet `baseMods` 的
    /// `skill("explodeCorpse", true)`（如 DetonateDeadPlayer，act_int.lua:5287）。
    /// PoB2 据此把 `monsterLife × skillData.corpseExplosionLifeMultiplier` 注入
    /// 物理基伤（`CalcOffence.lua:2211-2217`）。GGG `.dat` 无对应列——经
    /// extract-lua skill_overrides 通道在加载期 merge。缺省 false = 不注入。
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub explode_corpse: bool,
    /// 隐式（无每级数值）stat（M4-H）：vendor statSet `stats` 列表中**任何等级行
    /// 都没有数值**的条目（= GGG `.dat` `GrantedEffectStatSets.ImplicitStats` 列，
    /// 适配器未下载该列）。vendor 消费值恒 1（`CalcTools.lua:152`
    /// `statSetLevel[index] or 1`），多为行为 flag stat（如 Garukhan's Resolve 的
    /// `attacks_roll_crits_twice` → statmap flag BifurcateCrit）。
    ///
    /// 数据来源 = extract-lua skill_overrides 通道（`implicit_stat` 条目，**策展
    /// 白名单**——vendor 全量 4394 条多为显示/行为噪声，仅抽取 PoBR calc 有消费方
    /// 的 stat），加载期 merge。消费 = `effect_stats` 以值 1 并入 base 段。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub implicit_stats: Vec<String>,
    /// 分等级 stat（按宝石等级升序；含基础伤害值 + `damage_+%[_final]` 缩放）。
    pub levels: Vec<SkillStatSetLevel>,
}

/// 技能 DoT 配置布尔族（vendor skillData `dotIs*`）+ 抽取核验标记。
///
/// 注意 vendor 中 `dotIsSpell`/`dotIsProjectile`/`doubleHitsWhenDualWielding`
/// 是 **stat 驱动**（`SkillStatMap.lua` 的 `skill(...)` 条目，已入库
/// `overlay/skill_stat_map.json` 的 `skill_data` kind），不经本结构；本结构只
/// 承载 statSet `baseMods` 直挂的布尔（4.5.0.3.4 vendor 全量仅 `dotIsArea`
/// 一处：TornadoShotPlayer statSet "Tornado"）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct DotFlags {
    /// dotIsArea：DoT 保留 Area 位（false = dotCfg 剥除 Area 位）。
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub area: bool,
    /// dotIsProjectile。
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub projectile: bool,
    /// dotIsSpell。
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub spell: bool,
    /// dotIsAttack。
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub attack: bool,
    /// dotIsHit。
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub hit: bool,
    /// 是否经 vendor 抽取核验（true = overlay 有该 statSet 的 dotIs* 条目；
    /// false = 未核验的保守默认——蓝图 §5 `verified:false` 元数据，
    /// parity 报告单列）。
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub verified: bool,
}

impl DotFlags {
    /// 是否为全默认值（serde skip 谓词：全 false 时整段不落盘，diff 友好）。
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

/// 某授予效果在某宝石等级上的伤害 stat 列表。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SkillStatSetLevel {
    /// 宝石等级（1-based，对齐 [`SkillLevelDef::level`]）。
    pub gem_level: u32,
    /// 技能伤害倍率（PoB `baseMultiplier` = `1 + GrantedEffectStatSetsPerLevel.BaseMultiplier/10000`）。
    /// 攻击技能据此把武器+附加伤害放大（如 grenade L18 = 7.57 → 757% 武器伤害）；
    /// `1.0` = 无倍率（多数法术）。
    #[serde(default = "one_f64", skip_serializing_if = "is_one_f64")]
    pub damage_multiplier: f64,
    /// 该等级上已解析的伤害 stat（stat id → 值）。
    pub stats: Vec<SkillDamageStat>,
}

fn one_f64() -> f64 {
    1.0
}

fn is_one_f64(v: &f64) -> bool {
    *v == 1.0
}

/// 单条已解析的伤害 stat（稳定 stat id + 解析后的数值）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SkillDamageStat {
    /// 稳定 stat id（如 `spell_minimum_base_fire_damage`）。
    pub stat: String,
    /// 该宝石等级上的已解析值（`BaseResolvedValues` / `AdditionalStatsValues`）。
    pub value: f64,
}

/// serde 跳过零值 f64（diff 友好）。
fn is_zero_f64(v: &f64) -> bool {
    *v == 0.0
}

// ---- M1-T1 宝石品质 stat 域（`overlay/gem_quality_stats.json`）----

/// 单条宝石品质 stat 斜率。
///
/// 数据来源：PoB2 导出 `Data/Skills/*.lua` 的 `qualityStats` 字段（原始 `.dat` 为
/// `GrantedEffectQualityStats.StatValues / 1000`，见 vendor
/// `Export/Scripts/skills.lua:304-313`；当前钉定补丁该表 bundle 不可下载，
/// 走 extract-lua 通道，见 `pipeline/config.json` 的 `_tablesUnavailableForPinnedPatch`）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QualityStat {
    /// 稳定 stat id（如 `base_spell_%_chance_to_echo`）。
    pub stat: String,
    /// 每 1 点品质的 stat 斜率。消费侧按 `trunc(rate × quality)` 叠加进该技能的
    /// stat 集——**截断取整**（toward zero），对齐 PoB2 `CalcTools.lua:142`
    /// `math.modf(stat[2] * skillInstance.quality)`。
    pub per_quality_rate: f64,
}

/// 某授予效果的品质 stat 表（`overlay/gem_quality_stats.json` 的单条）。
///
/// 辅助宝石效果不在表中（PoB2 导出条件 `not (skillGem and granted.IsSupport)`
/// 已在 vendor 数据侧生效，抽取忠实转录）。
///
/// TODO（待 `.dat` 表通道恢复）：`GrantedEffectQualityStats` 的 Alt 列
/// （`AltStats`/`AltStatValuesPermille`/`AltApplyToStatSets`/`ApplyToStatSets`）
/// 原样入库不消费——PoB2 导出同样只读主列，行为对齐；语义实现 defer。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GemQualityStatDef {
    /// 授予效果 id（与 [`GrantedEffectDef::id`] 对齐，如 `CometPlayer`）。
    pub effect_id: String,
    /// 品质 stat 斜率列表（保持 vendor 导出顺序；同 stat 多条按加法叠加）。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub stats: Vec<QualityStat>,
}

/// `overlay/gem_quality_stats.json` 顶层（消费侧视角：`_meta` 溯源头部由 serde
/// 默认忽略，消费侧只取 `effects` 列表，按 `effect_id` 升序）。
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct GemQualityStatsDef {
    /// 品质 stat 表，按 `effect_id` 升序。
    pub effects: Vec<GemQualityStatDef>,
}

// ---- M1-T5.1 宝石↔效果外键域（`overlay/gem_effects.json`）----

/// 单个宝石变体的授予效果连边（`overlay/gem_effects.json` 的单条）。
///
/// 数据来源：vendor PoB2 `Data/Gems.lua`（由 `Export/Scripts/skills.lua:898-925`
/// 自 `.dat` `SkillGems.GemEffects` → `GemEffects` 表导出；该表所在 bundle 在钉定
/// 补丁 4.5.0.3.4 不可下载，按 owner 裁决「生产工具定层」走 extract-lua → overlay/，
/// 后续 `.dat` 通道恢复则迁 base/，迁移 commit byte 等价）。
/// `sync-pob-catalog extract-lua --what gem-effects` 确定性抽取。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GemEffectDef {
    /// 宝石基底 id（vendor `gameId` = `BaseItemTypes.Id`，与 [`SkillGemDef::id`] 对齐）。
    pub gem_id: String,
    /// 宝石效果变体 id（vendor `variantId` = `GemEffects.Id`，如 `IceNova`）。
    /// 当前数据 1 gem ↔ 1 变体（vendor Gems.lua 无重复 gameId，抽取时已校验）。
    pub variant_id: String,
    /// 授予的主效果 id（`GemEffects.GrantedEffect` → `GrantedEffects.Id`）。
    pub granted_effect_id: String,
    /// 附加授予的效果 id（`GemEffects.AdditionalGrantedEffects` 列序 =
    /// vendor `additionalGrantedEffectId1..N`）。meta/复合宝石展开外键（18-G5）。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub additional_granted_effect_ids: Vec<String>,
    /// 主效果的附加 statSet id（`GrantedEffects.AdditionalStatSets` →
    /// `GrantedEffectStatSets.Id`，vendor `additionalStatSet1..N`）。与
    /// [`GrantedEffectDef::additional_stat_set_ids`]（`.dat` 直读）互为对照边车。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub additional_stat_set_ids: Vec<String>,
}

/// `overlay/gem_effects.json` 顶层（消费侧视角：`_meta` 溯源头部由 serde 默认
/// 忽略，消费侧只取 `gems` 列表，按 `gem_id` 升序）。
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct GemEffectsDef {
    /// 宝石→效果连边表，按 `gem_id` 升序。
    pub gems: Vec<GemEffectDef>,
}

// ---- M1-T5.2 statSet label 边车（`overlay/stat_set_labels.json`）----

/// 单条 statSet 的 vendor 导出序号 + label 文本（`overlay/stat_set_labels.json` 的单条）。
///
/// 数据来源：vendor `Export/Skills/*.txt` 模板（`#skill`/`#set` 行 → 导出顺序与
/// set id）join vendor `Data/Skills/*.lua`（`statSets[i].label` 文本，原始 `.dat`
/// `GrantedEffectStatSets.Label` 的 FK 目标表 `GrantedEffectLabels` 不可下载）。
/// 由 `sync-pob-catalog extract-lua --what stat-set-labels` 确定性抽取。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StatSetLabelDef {
    /// 所属授予效果 id（模板 `#skill` 行）。
    pub skill: String,
    /// statSet 稳定 id（模板 `#set` 行）。
    pub set_id: String,
    /// PoB2 导出 statSets 列表中的 1-based 序号（= `<Gem statSetIndex>` 索引语义）。
    pub set_index: u32,
    /// label 文本（vendor `LabelType.Label`，缺省回退技能显示名——见
    /// `Export/Scripts/skills.lua:478`，抽取忠实转录导出产物）。
    pub label: String,
}

/// `overlay/stat_set_labels.json` 顶层（消费侧视角：`_meta` 忽略，按
/// `(skill, set_index)` 升序）。
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct StatSetLabelsDef {
    /// statSet label 表。
    pub labels: Vec<StatSetLabelDef>,
}

#[cfg(test)]
mod m4_t4_dot_flags_tests {
    use super::{DotFlags, StatSetDef};

    fn bare_set() -> StatSetDef {
        StatSetDef {
            set_id: "S".into(),
            label: None,
            vendor_set_index: None,
            base_effectiveness: 0.0,
            constant_stats: Vec::new(),
            skill_attack_speed_more: None,
            dot_flags: DotFlags::default(),
            explode_corpse: false,
            implicit_stats: Vec::new(),
            levels: Vec::new(),
        }
    }

    /// 全默认 dot_flags 不落盘（既有 base JSON 零 diff），且旧 JSON
    /// （无 `dot_flags` 键）可反序列化为保守默认。
    #[test]
    fn default_dot_flags_are_skipped_and_backward_compatible() {
        let json = serde_json::to_string(&bare_set()).unwrap();
        assert!(!json.contains("dot_flags"), "全 false 不得落盘：{json}");
        let parsed: StatSetDef = serde_json::from_str(&json).unwrap();
        assert!(parsed.dot_flags.is_default(), "缺键 = 保守默认（全 false）");
        assert!(!parsed.dot_flags.verified, "缺键 = 未核验");
    }

    /// 非默认 dot_flags serde 往返无损，且只序列化为 true 的位。
    #[test]
    fn dot_flags_round_trip() {
        let mut set = bare_set();
        set.dot_flags = DotFlags {
            area: true,
            verified: true,
            ..Default::default()
        };
        let json = serde_json::to_string(&set).unwrap();
        assert!(json.contains(r#""area":true"#));
        assert!(json.contains(r#""verified":true"#));
        assert!(!json.contains(r#""spell""#), "false 位不落盘：{json}");
        let parsed: StatSetDef = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, set, "serde 往返必须无损");
    }
}

/// 技能消耗资源类型定义（来自 `CostTypes.dat`）。
///
/// [`GrantedEffectDef::cost_types`] 是本表的整型外键索引；[`SkillLevelDef::cost_amounts`]
/// 按位置给出每种资源的消耗量。`per_minute` 资源（如 `ManaPerMinute`，按秒持续消耗）
/// 的 `divisor` 为 60（原始值是「每分钟」，÷60 得每秒）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CostTypeDef {
    /// 稳定资源 id（如 `Mana` / `Life` / `ES` / `LifePercent` / `ManaPerMinute`）。
    pub id: String,
    /// 数值除数（瞬时消耗为 1；per-minute 资源为 60，÷得每秒量）。
    pub divisor: u32,
    /// 是否为按时间持续消耗（per-second/per-minute）的资源。
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub per_minute: bool,
}
