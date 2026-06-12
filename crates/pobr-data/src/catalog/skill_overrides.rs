//! per-skill 覆盖值 overlay 域 schema（`overlay/skill_overrides.json`）。
//!
//! 数据来源：vendor PoB2 `Data/Skills/*.lua` 的人工策展层——GGG `.dat` 导出
//! （pipeline/tables）中**不存在**的分等级列（`critChance` /
//! `attackSpeedMultiplier` / `baseMultiplier`）与 statSet `baseMods` 固有
//! Speed MORE。由 `sync-pob-catalog extract-lua` 确定性抽取生成
//! （schema 标识 `skill_overrides/v1`，生成侧见该工具的 `extract_lua` 模块）。
//!
//! 消费侧：`pobr-gamedata` 在加载 `base/granted_effect_levels.json` /
//! `base/granted_effect_stat_sets.json` 时把本表 merge 到纯 base 之上
//! （merge 语义与单测见 `pobr-gamedata::domains::skill_overrides`）。
//! 本模块只定义 serde 形状，零逻辑。

use serde::{Deserialize, Serialize};

/// [`SkillOverrideEntry::stat`] 取值：技能基础暴击率（百分点，对应
/// `SkillLevelDef::crit_chance`）。
pub const OVERRIDE_STAT_CRIT_CHANCE: &str = "crit_chance";
/// [`SkillOverrideEntry::stat`] 取值：攻击速度乘数（百分点，可负，对应
/// `SkillLevelDef::attack_speed_multiplier`）。
pub const OVERRIDE_STAT_ATTACK_SPEED_MULTIPLIER: &str = "attack_speed_multiplier";
/// [`SkillOverrideEntry::stat`] 取值：技能伤害基础倍率（对应
/// `SkillLevelDef::base_multiplier`）。
pub const OVERRIDE_STAT_BASE_MULTIPLIER: &str = "base_multiplier";
/// [`SkillOverrideEntry::stat`] 取值：statSet 固有攻击速度 MORE（百分点，对应
/// `SkillStatSetDef::skill_attack_speed_more`）。
pub const OVERRIDE_STAT_SKILL_ATTACK_SPEED_MORE: &str = "skill_attack_speed_more";
/// [`SkillOverrideEntry::stat`] 取值：技能 DoT 配置布尔（M4-T4 W-D1，vendor
/// statSet `baseMods` 的 `skill("dotIs*", true)`；value 1.0 = true，对应
/// `StatSetDef::dot_flags` 的同名位）。statSet 级条目（恒带 `stat_set`）。
pub const OVERRIDE_STAT_DOT_IS_AREA: &str = "dot_is_area";
/// 同 [`OVERRIDE_STAT_DOT_IS_AREA`]（dotIsProjectile）。
pub const OVERRIDE_STAT_DOT_IS_PROJECTILE: &str = "dot_is_projectile";
/// 同 [`OVERRIDE_STAT_DOT_IS_AREA`]（dotIsSpell）。
pub const OVERRIDE_STAT_DOT_IS_SPELL: &str = "dot_is_spell";
/// 同 [`OVERRIDE_STAT_DOT_IS_AREA`]（dotIsAttack）。
pub const OVERRIDE_STAT_DOT_IS_ATTACK: &str = "dot_is_attack";
/// 同 [`OVERRIDE_STAT_DOT_IS_AREA`]（dotIsHit）。
pub const OVERRIDE_STAT_DOT_IS_HIT: &str = "dot_is_hit";
/// [`SkillOverrideEntry::stat`] 取值：尸体爆炸门控布尔（M4-G，vendor statSet
/// `baseMods` 的 `skill("explodeCorpse", true)`，CalcOffence.lua:2213 据此把
/// `monsterLife × corpseExplosionLifeMultiplier` 注入物理基伤；value 1.0 = true，
/// 对应 `StatSetDef::explode_corpse`）。statSet 级条目（恒带 `stat_set`）。
pub const OVERRIDE_STAT_EXPLODE_CORPSE: &str = "explode_corpse";

/// 全部 statSet 级 dotIs* stat 名（消费侧 merge / 生成侧抽取共用清单）。
pub const OVERRIDE_DOT_FLAG_STATS: &[&str] = &[
    OVERRIDE_STAT_DOT_IS_AREA,
    OVERRIDE_STAT_DOT_IS_PROJECTILE,
    OVERRIDE_STAT_DOT_IS_SPELL,
    OVERRIDE_STAT_DOT_IS_ATTACK,
    OVERRIDE_STAT_DOT_IS_HIT,
];

/// 单条 per-skill 覆盖值。`value` 与 `per_level` 二选一：
/// 该 stat 在 vendor **所有等级**均出现且同值时压缩为 `value`
/// （消费侧应用到该技能全部等级行），否则保留 `per_level` 明细
/// （只覆盖明细中列出的等级，缺失等级不填——忠实于 vendor）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SkillOverrideEntry {
    /// vendor 技能 id（= `GrantedEffects.Id`，如 `FlickerStrikePlayer`）。
    pub skill: String,
    /// 入库 stat 名（见本模块 `OVERRIDE_STAT_*` 常量）。
    pub stat: String,
    /// statSet 序号（仅 statSet 级覆盖值携带，如 baseMods 的 Speed MORE）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stat_set: Option<u32>,
    /// 全等级同值（或与等级无关）时的单值。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<f64>,
    /// 按等级明细：`[[level, value], ...]`，level 升序。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub per_level: Option<Vec<(u32, f64)>>,
}

/// `overlay/skill_overrides.json` 顶层（消费侧视角：`_meta` 头部为生成溯源
/// 信息，serde 默认忽略未知字段，消费侧只取 `overrides` 列表）。
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct SkillOverridesDef {
    /// 覆盖值列表，按 `(skill, stat, stat_set)` 排序。
    pub overrides: Vec<SkillOverrideEntry>,
}
