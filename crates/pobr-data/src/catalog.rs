//! 适配后入库的游戏数据目录（catalog）schema。
//!
//! 这是 PoBR **自有的最小 JSON schema**，由 `pobr-data-adapter` 从 GGG `.dat`
//! 原始导出（pathofexile-dat 产物）解析外键、反范式化后生成，落在仓库
//! `data/<poe_version>/`。运行时由 loader（`pobr-gamedata`）以 serde 加载。
//!
//! 设计目标：与 GGG 原始列名 / PoB 生成 Lua 解耦；只保留计算/显示需要的字段；
//! 稳定字符串 ID；版本可钉、diff 友好（数组按 id 排序）。

use serde::{Deserialize, Serialize};

/// 当前 catalog schema 版本。结构不兼容变更时 +1。
pub const CATALOG_SCHEMA_VERSION: u32 = 1;

/// 数据包信封：描述某个 PoE2 版本下入库了哪些域与语言。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DataManifest {
    pub schema_version: u32,
    /// CDN 补丁版本号，如 `4.5.0.3.4`（公开版 0.5.0）。
    pub poe_version: String,
    /// 已生成 i18n 边车的语言标签，如 `["zh-TW"]`（英文为 canonical，不计入）。
    pub languages: Vec<String>,
    /// 已生成的数据域文件名（不含扩展名），如 `["base_items"]`。
    pub domains: Vec<String>,
}

/// 物品基底定义（来自 `BaseItemTypes.dat` + 外键解析）。
///
/// `name` 为英文 canonical；其它语言的名称走 `i18n/<lang>/base_items.json` 边车
/// （`id -> 本地化名称`）。武器/护甲数值（如 PhysicalMin/Max）来自独立的
/// `WeaponTypes` / `ArmourTypes` 表，后续切片接入。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BaseItemDef {
    /// 稳定 ID，即 `.dat` 的 `Id`（如 `Metadata/Items/Weapons/.../FourOneHandAxe1`）。
    pub id: String,
    /// 英文 canonical 名称。
    pub name: String,
    /// 物品类别（解析 `ItemClasses.Id`，如 `One Hand Axe`）。
    pub item_class: String,
    /// 掉落等级。
    pub drop_level: u32,
    /// 物品栏宽 / 高。
    pub width: u8,
    pub height: u8,
    /// 标签（解析 `Tags.Id`，如 `ezomyte_basetype`）。
    pub tags: Vec<String>,
    /// 固有词缀（implicit）的 mod 稳定 ID（解析 `Mods.Id`）。
    pub implicits: Vec<String>,
    /// mod domain（GGG 原始枚举值，用于词缀适用域判定）。
    pub mod_domain: u32,
    /// 武器基底数值（来自 `WeaponTypes.dat`；非武器为 `None`）——攻击伤害的基底。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub weapon: Option<WeaponBaseStats>,
    /// 护甲基底数值（来自 `ArmourTypes.dat`；非护甲为 `None`）——armour/evasion/ES 局部基底。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub armour: Option<ArmourBaseStats>,
}

/// 武器基底数值（`WeaponTypes.dat` 外键解析；攻击技能伤害的基底，对照 PoB2
/// `CalcSetup.lua` weaponData 装配）。数值均为原始 `.dat` 整型，计算侧按单位换算。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WeaponBaseStats {
    /// 基底物理伤害下/上限（`DamageMin`/`DamageMax`）。
    pub physical_min: u32,
    pub physical_max: u32,
    /// 攻击间隔（`Speed`，毫秒）；攻击速率 = `1000 / speed_ms`。
    pub speed_ms: u32,
    /// 基底暴击率（`CritChance` 原始值；暴击% = `crit_chance / 100`，如 `500` = 5%）。
    pub crit_chance: u32,
    /// 攻击射程（`RangeMax`）。
    #[serde(default, skip_serializing_if = "is_zero_u32")]
    pub range: u32,
}

/// 护甲基底数值（`ArmourTypes.dat` 外键解析；armour/evasion/ES/ward 局部基底）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArmourBaseStats {
    pub armour: u32,
    pub evasion: u32,
    pub energy_shield: u32,
    #[serde(default, skip_serializing_if = "is_zero_u32")]
    pub ward: u32,
}

/// serde 跳过零值 u32（diff 友好）。
fn is_zero_u32(v: &u32) -> bool {
    *v == 0
}

/// Stat 注册表条目（来自 `Stats.dat`）。
///
/// `id` 是 GGG 稳定字符串 stat key（如 `additional_strength`），是 Mods 里
/// `Stat1..4` 整型外键解析后的目标，也是未来 i18n stat 描述的主键。
/// `semantic` / `category` 是 GGG 原始整型枚举（无独立解析表，保留原值）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StatDef {
    /// 稳定 stat ID，即 `Stats.dat` 的 `Id`（如 `additional_strength`）。
    pub id: String,
    /// 是否为本地词缀（local，仅作用于所在装备）。
    pub is_local: bool,
    /// GGG 原始 `Semantic` 枚举值（数值正负 / 百分比 / 时长等显示语义）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub semantic: Option<u32>,
    /// GGG 原始 `Category` 枚举值（stat 归类，可空）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<u32>,
}

/// 词缀（mod）某个 stat 槽位的掷值区间（来自 `Mods.StatNValue`，形如 `[min, max]`）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModStat {
    /// 该槽位作用的 stat 稳定 ID（解析 `StatN` 外键 → `Stats.Id`）。
    pub stat_id: String,
    /// 掷值下界。
    pub min: i64,
    /// 掷值上界。
    pub max: i64,
}

/// 词缀池定义（来自 `Mods.dat` + 外键解析）。
///
/// `name` 为英文 canonical 词缀名（前后缀名，如 `of the Brute`）；其它语言走
/// `i18n/<lang>/mods.json` 边车（`id -> 本地化名称`）。`Stat1..4` + `Stat1Value..4Value`
/// 被合并成 `stats` 数组（解析 stat 外键、跳过空槽）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModDef {
    /// 稳定 ID，即 `Mods.dat` 的 `Id`（如 `Strength1`）。
    pub id: String,
    /// 英文 canonical 词缀名（可空：大量内部 mod 无显示名）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// GGG 原始 `ModType` 枚举值（无独立解析表，保留原值；可空）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mod_type: Option<u32>,
    /// mod domain（GGG 原始枚举值，用于词缀适用域判定）。
    pub domain: u32,
    /// GGG 原始 `GenerationType` 枚举值（前缀 / 后缀 / 固有等生成类型；可空）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generation_type: Option<u32>,
    /// 词缀生成等级。
    pub level: u32,
    /// 该词缀作用的 stat 槽位（已合并 Stat 外键 + 掷值区间，跳过空槽）。
    pub stats: Vec<ModStat>,
    /// 标签（解析 `Tags.Id`）。
    pub tags: Vec<String>,
}

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
/// 施放时间 + 允许的主动技能类型枚举 + StatSet/CostTypes 外键索引。
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
    /// 允许作用的主动技能类型（GGG 原始枚举值；当前无导出的类型名查表）。
    pub allowed_active_skill_types: Vec<u32>,
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
    /// 当 stat-set 的 `BaseMultiplier` 缺失时作为 [`SkillStatSetLevelDef::damage_multiplier`]
    /// 的回退源（二者同义，PoB 在两表均存）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_multiplier: Option<f64>,
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

/// 被动天赋节点的种类。
///
/// 源自 GGG 官方树导出（`poe2-skilltree-export/data.json`）的节点布尔标志：
/// `isKeystone` / `isNotable` / `isMastery` / `isJewelSocket` / `isAscendancyStart`，
/// 否则为 [`PassiveNodeKind::Normal`]（小天赋）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PassiveNodeKind {
    /// 小天赋（普通属性节点）。
    Normal,
    /// 大天赋（notable）。
    Notable,
    /// 基石（keystone）。
    Keystone,
    /// 精通节点（mastery）。
    Mastery,
    /// 珠宝插槽。
    JewelSocket,
    /// 飞升起始节点。
    AscendancyStart,
}

/// 被动天赋树节点定义（来自 GGG 官方树导出 `data.json` 的 `nodes`）。
///
/// 计算内部只用稳定 ID：`id` 为 GGG 的字符串 slug（如
/// `passive_keystone_avatar_of_fire`），`skill` 为数值 skill id（树连线 / Build Code
/// 引用的稳定数值键）。`stats` 是节点授予的英文词条文本行（PoB 兼容解析的输入）。
/// `connections` 为该节点的出边目标 `skill` id（无向树用出边即可重建邻接）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PassiveNodeDef {
    /// 数值 skill id（GGG `nodes` 的 map key / `skill` 字段）。树连线、Build Code 用此引用。
    pub skill: u32,
    /// 字符串 slug（GGG `id` 字段，如 `passive_keystone_avatar_of_fire`）。
    pub id: String,
    /// 节点名称（英文 canonical）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// 节点种类。
    pub kind: PassiveNodeKind,
    /// 节点授予的词条文本行（英文 canonical；i18n 边车后续切片）。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub stats: Vec<String>,
    /// 所属节点组（GGG `group`，用于坐标/布局；计算无关，保留以便和 PoB2 对比）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<u32>,
    /// 在 orbit 上的环号（GGG `orbit`）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub orbit: Option<u32>,
    /// 在 orbit 上的角度槽位（GGG `orbitIndex`）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub orbit_index: Option<u32>,
    /// 出边目标节点的 `skill` id（GGG `out`，已从字符串 key 转为数值）。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub connections: Vec<u32>,
    /// 所属飞升（GGG `ascendancyId`，如 `Warrior3`）；主树节点为 None。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ascendancy_id: Option<String>,
}

/// 某个职业的飞升摘要。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PassiveAscendancy {
    /// 飞升稳定 ID（与节点 `ascendancy_id` 对应，如 `Warrior3`）。
    pub id: String,
    /// 飞升名称（英文 canonical，如 `Smith of Kitava`）。
    pub name: String,
}

/// 某个职业的摘要（基础属性 + 飞升列表）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PassiveClass {
    /// 职业名称（英文 canonical，如 `Warrior`）。
    pub name: String,
    /// 基础力量 / 敏捷 / 智慧。
    pub base_str: i32,
    pub base_dex: i32,
    pub base_int: i32,
    /// 该职业的飞升摘要（无名占位项已过滤）。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ascendancies: Vec<PassiveAscendancy>,
}

/// 被动天赋树的元数据摘要（职业 / 飞升 / 树名）。
///
/// orbit 半径 / 每环槽位数（PoB 的 `constants`）在当前 GGG PoE2 导出中**未以独立
/// `constants` 块给出**（坐标直接落在节点 `x`/`y`），故此切片不收录——见 TODO。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PassiveTreeMeta {
    /// 树标识（GGG `tree`，如 `Default`）。
    pub tree: String,
    /// 职业 + 飞升摘要（按职业名排序）。
    pub classes: Vec<PassiveClass>,
}
