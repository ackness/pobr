//! 召唤物 / 魂灵（spectre）overlay 域 schema（`overlay/minions.json` /
//! `overlay/spectres.json` / `overlay/granted_effect_minions.json`）。
//!
//! 数据来源（M5a 蓝图 §1 路线 B 裁决）：vendor PoB2 `Data/Minions.lua`（32 条）/
//! `Data/Spectres.lua`（593 条）——两文件头部自注 "automatically generated"，本身
//! 即 `.dat` 的确定性投影、PoB2 计算引擎的实际输入；由
//! `sync-pob-catalog extract-lua --what minions|spectres` 在最小 stub 环境用
//! luajit 执行后逐字段忠实抽取（schema 标识 `minions/v1` / `spectres/v1`）。
//! 物理层归 `overlay/`（00-index §4.2 裁决 1：生产工具定层——extract-lua 产物
//! 落 overlay，后续 pipeline 补齐 `.dat` 表则迁 base，迁移 commit byte 等价）。
//!
//! 与既有 `pobr_data::minion::MinionDef`（手抄常量 + 计算消费形态）的关系：
//! 本模块是**入库 serde 形状**（v2 字段全集，含 `mod_list` 结构化 modList）；
//! M5a 主波 A1 将把消费侧统一迁到本类型并删除手抄常量（A6），当前两者并存、
//! 由加载单测锁定逐值一致（搬迁不变式）。本模块零逻辑、零 I/O。
//!
//! `overlay/granted_effect_minions.json` 是宝石→召唤物外键**边车**（00-index
//! 裁决 §4-10 归 M5a）：`minionList` 不在 `.dat`，来自 PoB2 Export 模板手工指令
//! `#minionList`（`Export/Scripts/skills.lua:771-776`），随 `Data/Skills/*.lua`
//! 一起被 extract-lua 抽取；merge 进 `GrantedEffectDef` 属 M5a 主波接线，
//! 此处只定义边车形状。

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// Lua 标量值的忠实转录（mod 构造参数 / tag 字段值可为布尔、数字或字符串；
/// R3 纪律：stub 把入参原样记录，消费侧不识别的形态显式 Unsupported）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum LuaValueDef {
    /// 布尔（如 `flag(...)` 构造的 value=true）。
    Bool(bool),
    /// 数字（Lua number，统一 f64）。
    Number(f64),
    /// 字符串（如 stub 环境下经 `__index` 自映射的 ModFlag 名）。
    Text(String),
}

/// `modList` 中一条 `mod(...)` / `flag(...)` 构造的忠实序列化
/// （对应 vendor `Modules/Data.lua:56 makeSkillMod` 的入参；蓝图 R3 警告：
/// 构造参数必须完整序列化，丢参即静默错数据）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MinionModDef {
    /// mod 名（如 `StunDuration`）。
    pub name: String,
    /// mod 类型字面量（`BASE`/`INC`/`MORE`/`OVERRIDE`/`FLAG`）。
    #[serde(rename = "type")]
    pub mod_type: String,
    /// 数值/布尔值（`flag(...)` 为 true）。
    pub value: LuaValueDef,
    /// flags 位（vendor 数据文件内为字面数字；stub 自映射环境下可能为名字
    /// 字符串——忠实转录，位语义解释属消费接线）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub flags: Option<LuaValueDef>,
    /// keywordFlags 位（同上）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keyword_flags: Option<LuaValueDef>,
    /// 尾随 tag 表（如 `{ effectName = "ArmourBreak", effectType = "Buff" }`），
    /// 每个 tag = 扁平 `键 → 标量` 映射，键序固定（BTreeMap）。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<BTreeMap<String, LuaValueDef>>,
}

/// 一条召唤物 / 魂灵条目（对应 Lua `minions["<Id>"]`；字段名 = vendor 键
/// snake_case 化）。可选字段缺省 = vendor 中该键缺失（忠实转录，不物化默认值；
/// 消费侧默认语义见 `pobr_data::minion::MinionDef` 文档：armour/evasion 缺省按
/// 1.0、energyShield 缺省按 0.0 处理）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MinionEntryDef {
    /// 稳定 ID（Lua key；spectre 为完整 metadata 路径如
    /// `Metadata/Monsters/LeagueAbyss/Lightless/Cocoon3Spectre`）。
    pub id: String,
    /// 英文 canonical 名称（`name`）。
    pub name: String,
    /// 怪物标签（`monsterTags`）。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub monster_tags: Vec<String>,
    /// 生命归一化乘数（`life`）。
    pub life: f64,
    /// 伤害归一化乘数（`damage`）。
    pub damage: f64,
    /// 伤害区间宽度（`damageSpread`）。
    pub damage_spread: f64,
    /// 基础攻击间隔秒（`attackTime`）。
    pub attack_time: f64,
    /// 攻击距离（`attackRange`）。
    pub attack_range: f64,
    /// 命中归一化乘数（`accuracy`）。
    pub accuracy: f64,
    /// 暴击率（`critChance`，百分点）。
    pub crit_chance: f64,
    /// 基础移动速度（`baseMovementSpeed`）。
    pub base_movement_speed: f64,
    /// 火抗（`fireResist`，百分点）。
    pub fire_resist: f64,
    /// 冰抗（`coldResist`）。
    pub cold_resist: f64,
    /// 电抗（`lightningResist`）。
    pub lightning_resist: f64,
    /// 混沌抗（`chaosResist`）。
    pub chaos_resist: f64,
    /// 基础伤害忽略攻速（`baseDamageIgnoresAttackSpeed`；vendor 缺失 = false）。
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub base_damage_ignores_attack_speed: bool,
    /// 数量上限 stat 名原始字符串（`limit`，如 `ActiveZombieLimit`；枚举映射
    /// 属消费接线，见 `pobr_data::minion::MinionLimitId::from_pob2`）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<String>,
    /// 精神保留量（`spectreReservation`）。
    pub spectre_reservation: f64,
    /// 伴侣保留量（`companionReservation`）。
    pub companion_reservation: f64,
    /// 伴侣形态火抗覆盖（`companionFireResist`，PoB2 0.5.4 起；缺失 = 沿用本体）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub companion_fire_resist: Option<f64>,
    /// 伴侣形态冰抗覆盖（`companionColdResist`）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub companion_cold_resist: Option<f64>,
    /// 伴侣形态电抗覆盖（`companionLightningResist`）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub companion_lightning_resist: Option<f64>,
    /// 伴侣形态混沌抗覆盖（`companionChaosResist`）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub companion_chaos_resist: Option<f64>,
    /// 怪物类别（`monsterCategory`，如 `Undead`/`Demon`）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub monster_category: Option<String>,
    /// 护甲归一化乘数（`armour`；vendor 缺失 = 消费侧按 1.0）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub armour: Option<f64>,
    /// 闪避归一化乘数（`evasion`；vendor 缺失 = 消费侧按 1.0）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evasion: Option<f64>,
    /// 生命转 ES 占比（`energyShield`；vendor 缺失 = 0.0）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub energy_shield: Option<f64>,
    /// 虚拟武器类型 1（`weaponType1`，ModFlags 派生用）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub weapon_type1: Option<String>,
    /// 虚拟武器类型 2（`weaponType2`）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub weapon_type2: Option<String>,
    /// 出现区域（`spawnLocation`，spectre 专用；召唤物为空）。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub spawn_location: Vec<String>,
    /// 技能列表（`skillList`，createMinionSkills 输入）。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub skill_list: Vec<String>,
    /// 附加 flag（`extraFlags` 中值为 true 的键，升序；如 `recommendedSpectre`）。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub extra_flags: Vec<String>,
    /// 结构化 modList（`mod(...)`/`flag(...)` 构造的完整转录）。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mod_list: Vec<MinionModDef>,
}

/// `overlay/minions.json` / `overlay/spectres.json` 顶层（消费侧视角：
/// `_meta` 头部为生成溯源信息，serde 忽略未知字段，只取 `minions` 列表）。
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct MinionsDef {
    /// 条目列表，按 `id` 升序。
    pub minions: Vec<MinionEntryDef>,
}

/// 宝石授予效果 → 召唤物外键边车的一条记录（按 `effect_id` 键控）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GrantedEffectMinionDef {
    /// 授予效果 id（= `GrantedEffects.Id`，如 `RagingSpiritsPlayer`）。
    pub effect_id: String,
    /// 该技能召唤的 minion id 列表（`minionList`）。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub minion_list: Vec<String>,
    /// support 追加的 minion id 列表（`addMinionList`；当前 vendor 数据无
    /// 此键，字段预留，R7 纪律 `#[serde(default)]`）。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub add_minion_list: Vec<String>,
    /// 召唤物借用玩家装备槽位列表（`minionUses` 中值为 true 的键，升序；
    /// 如 Manifest Weapon 的 `Weapon 1`）。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub minion_uses: Vec<String>,
    /// 召唤物使用独立 item set（`minionHasItemSet`）。
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub minion_has_item_set: bool,
}

/// `overlay/granted_effect_minions.json` 顶层。
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct GrantedEffectMinionsDef {
    /// 边车记录，按 `effect_id` 升序。
    pub entries: Vec<GrantedEffectMinionDef>,
}
