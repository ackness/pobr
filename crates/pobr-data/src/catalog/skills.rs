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
/// TODO（后续切片）：分等级 stat 缩放（GrantedEffectStatSetsPerLevel /
/// GrantedEffectsPerLevel 的 cost / cooldown / 伤害进度）尚未接入；
/// `GemEffects` FK 指向的 `GemEffects` 表当前 pipeline 未导出，
/// 故宝石→授予效果的直接连边暂缺，等该表导出后补 `granted_effects: Vec<String>`。
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
    /// 消耗类型外键索引（原始 `CostTypes` 列，如 `[0]`=法力）。与
    /// [`SkillLevelDef::cost_amounts`] 按位置配对（第 i 个类型消耗第 i 个数量）。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cost_types: Vec<u32>,
    /// 关联主动技能的类型名（`ActiveSkills.ActiveSkillTypes` → `ActiveSkillType.Id` 解析，
    /// 如 `["Attack","Projectile","Damage"]` / `["Spell","Area"]`）。用于攻击/法术判定
    /// （攻击技能的击中伤害来自武器基底）。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub skill_types: Vec<String>,
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

/// 某授予效果（技能）分等级解析出的**伤害相关 stat 值**集合
/// （来自 `GrantedEffectStatSets` + `GrantedEffectStatSetsPerLevel.dat` 外键解析）。
///
/// 每个主动技能效果关联一个 `GrantedEffectStatSets`（`BaseEffectiveness` +
/// `ImplicitStats`），其 `GrantedEffectStatSetsPerLevel` 行给出每个宝石等级上
/// `FloatStats`/`AdditionalStats` 对应的**已解析值**（`BaseResolvedValues` /
/// `AdditionalStatsValues`）。适配器把 stat 索引解析为稳定 stat id，过滤出伤害族
/// （`spell_*_base/added_<type>_damage`、`secondary_*_base_<type>_damage`、
/// `attack_*_added_<type>_damage` 等）后按 effect id 入库。
///
/// 收录于 `granted_effect_stat_sets.json`，以 [`GrantedEffectDef::id`] 为键
/// （player 技能的 stat-set Id 与 effect Id 同名，适配器已在导出时完成 join）。
/// 这是「宝石 → 技能伤害 → DPS」数据通道的最后一环。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SkillStatSetDef {
    /// 授予效果 id（如 `FireballPlayer`），与 [`GrantedEffectDef::id`] 对齐。
    pub id: String,
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
    /// 分等级 stat（按宝石等级升序；含基础伤害值 + `damage_+%[_final]` 缩放）。
    pub levels: Vec<SkillStatSetLevel>,
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

