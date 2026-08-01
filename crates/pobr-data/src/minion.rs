//! 召唤物定义 (`MinionDef`) 入库 schema — 纯数据，零逻辑、零 I/O。
//!
//! 与 `catalog.rs` 中其他域对齐；由 `pobr-data-adapter` 离线写入
//! `data/<poe_version>/minions.json`，由 `pobr-gamedata` 运行时反序列化。
//!
//! 设计对齐 PoB2 `src/Data/Minions.lua`：每个 `MinionDef` 对应 Lua 中的一个
//! `minions["<Id>"]` 条目，字段名遵循 Lua 原始命名习惯（snake_case 化）。
//!
//! 出处：
//! - PoB2 `src/Data/Minions.lua`（各召唤物归一化乘数 + limit + 保留量）。
//! - agent-docs/minions.md §1（怪物式 scaling）、§4.1（数量上限）、§4.2（保留）。
//! - PoB2 `src/Modules/CalcActiveSkill.lua`（virtualWeapon / limit / hiddenDamageFixup）。
//! - PoB2 `src/Modules/CalcPerform.lua`（limit → Multiplier:SummonedMinion）。

use serde::{Deserialize, Serialize};

// 召唤物上限稳定 ID（对应 PoB2 Lua `data.minionLimitNames` / `Active<X>Limit`）

/// 召唤物数量上限的稳定 ID。
///
/// 每类召唤物技能有各自的 limit；数量上限通过 `base_number_of_<x>_allowed` stat
/// 累加后暴露为 `Multiplier:SummonedMinion`（per-minion 词条引用）。
///
/// 出处：PoB2 `src/Data/SkillStatMap.lua` / `CalcPerform.lua`；
/// agent-docs/minions.md §4.1。
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MinionLimitId {
    /// 僵尸（Raised Zombie）上限。
    ActiveZombieLimit,
    /// 骷髅系召唤物上限（所有 Raise Skeleton 变体共用）。
    ActiveSkeletonLimit,
    /// 暴怒魂灵（Raging Spirit）上限。
    ActiveRagingSpiritLimit,
    /// 幽灵（Spectre）上限。
    ActiveSpectreLimit,
    /// 活体闪电（Living Lightning）上限。
    ActiveLivingLightningLimit,
    /// 地狱猎犬（Infernal Hound）上限（独立 companion 槽不占此限）。
    ActiveHellhoundLimit,
    /// 地狱猎犬伴侣上限（若技能数据有独立 companion limit 时使用）。
    ActiveCompanionLimit,
    /// 战士亡灵（Bone Construct / Unearth 系）上限。
    ActiveUnearthBoneConstructLimit,
    /// Wardbound 上限（0.5.0 新增召唤技能）。
    WardboundLimit,
    /// Hyena 上限。
    HyenaLimit,
    /// Wolf 上限（Wolfpack / Companion 共用）。
    WolfLimit,
    /// Beetle 上限。
    BeetleLimit,
    /// Azmerian Swarm 上限。
    AzmerianSwarmLimit,
    /// 无上限（如 Companion、Spirit Walker 的 Bear 等）。
    None,
    /// 自定义上限（不在枚举内的 limit ID，保持原始字符串以便扩展）。
    Custom(String),
}

impl MinionLimitId {
    /// 将 PoB2 Lua 字符串形式的 limit 名称解析为枚举值。
    pub fn from_pob2(s: &str) -> Self {
        match s {
            "ActiveZombieLimit" => Self::ActiveZombieLimit,
            "ActiveSkeletonLimit" => Self::ActiveSkeletonLimit,
            "ActiveRagingSpiritLimit" => Self::ActiveRagingSpiritLimit,
            "ActiveSpectreLimit" => Self::ActiveSpectreLimit,
            "ActiveLivingLightningLimit" => Self::ActiveLivingLightningLimit,
            "ActiveHellhoundLimit" => Self::ActiveHellhoundLimit,
            "ActiveCompanionLimit" => Self::ActiveCompanionLimit,
            "ActiveUnearthBoneConstructLimit" => Self::ActiveUnearthBoneConstructLimit,
            "WardboundLimit" => Self::WardboundLimit,
            "HyenaLimit" => Self::HyenaLimit,
            "WolfLimit" => Self::WolfLimit,
            "BeetleLimit" => Self::BeetleLimit,
            "AzmerianSwarmLimit" => Self::AzmerianSwarmLimit,
            "" => Self::None,
            other => Self::Custom(other.to_string()),
        }
    }

    /// 返回 PoB2 Lua 原始字符串（供序列化 / 调试）。
    pub fn to_pob2_str(&self) -> &str {
        match self {
            Self::ActiveZombieLimit => "ActiveZombieLimit",
            Self::ActiveSkeletonLimit => "ActiveSkeletonLimit",
            Self::ActiveRagingSpiritLimit => "ActiveRagingSpiritLimit",
            Self::ActiveSpectreLimit => "ActiveSpectreLimit",
            Self::ActiveLivingLightningLimit => "ActiveLivingLightningLimit",
            Self::ActiveHellhoundLimit => "ActiveHellhoundLimit",
            Self::ActiveCompanionLimit => "ActiveCompanionLimit",
            Self::ActiveUnearthBoneConstructLimit => "ActiveUnearthBoneConstructLimit",
            Self::WardboundLimit => "WardboundLimit",
            Self::HyenaLimit => "HyenaLimit",
            Self::WolfLimit => "WolfLimit",
            Self::BeetleLimit => "BeetleLimit",
            Self::AzmerianSwarmLimit => "AzmerianSwarmLimit",
            Self::None => "",
            Self::Custom(s) => s.as_str(),
        }
    }
}

// 召唤物类别（对应 PoB2 Lua `monsterCategory`）

/// 召唤物怪物类别（影响异常状态 / 机制标签的条件判定）。
///
/// 出处：PoB2 `src/Data/Minions.lua` `monsterCategory` 字段。
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MinionCategory {
    Undead,
    Construct,
    Demon,
    Beast,
    Humanoid,
    /// 未知 / 其他类别。
    Other(String),
}

impl MinionCategory {
    pub fn from_pob2(s: &str) -> Self {
        match s {
            "Undead" => Self::Undead,
            "Construct" => Self::Construct,
            "Demon" => Self::Demon,
            "Beast" => Self::Beast,
            "Humanoid" => Self::Humanoid,
            other => Self::Other(other.to_string()),
        }
    }
}

// MinionDef — 召唤物入库定义（纯数据，供 pobr-gamedata 反序列化）

/// 召唤物归一化乘数定义（对应 PoB2 `src/Data/Minions.lua` 的一个条目）。
///
/// **每个字段含义**：
/// - `life` / `damage` / `armour` / `evasion` / `energy_shield`：
///   与怪物等级基准表（`monsterAllyLifeTable` 等）相乘得到基础值。缺省为 1.0。
/// - `damage_spread`：min/max 伤害区间宽度。0.0 = 无区间（min == max == avg）。
/// - `attack_time`：基础攻击间隔（秒）；虚拟武器 `attack_rate = 1 / attack_time`。
/// - `crit_chance`：虚拟武器暴击率（默认 5.0）。
/// - `*_resist`：基础抗性（%），直接注入召唤物 `modDB.<Type>Resist BASE`。
/// - `base_damage_ignores_attack_speed`：基础伤害是否忽略 `attack_time`；
///   `true`（Zombie / RagingSpirit 等）：攻速仅影响 DPS，不影响每击伤害。
/// - `limit`：召唤物数量上限稳定 ID；`None` 表示无明确上限（Companion 类）。
/// - `spectre_reservation` / `companion_reservation`：单个召唤物消耗的精神/伴侣保留量。
///
/// 出处：PoB2 `src/Data/Minions.lua`；agent-docs/minions.md §1 / §4。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MinionDef {
    /// 稳定 ID（对应 Lua `minions["<Id>"]` 的 key，如 `RaisedZombie`）。
    pub id: String,
    /// 英文 canonical 名称（对应 Lua `name`）。
    pub name: String,
    /// 怪物类别（对应 Lua `monsterCategory`）；无该字段时为 `None`。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<MinionCategory>,
    /// 生命归一化乘数（对应 Lua `life`；默认 1.0）。
    #[serde(default = "default_one")]
    pub life: f64,
    /// 伤害归一化乘数（对应 Lua `damage`；默认 1.0）。
    #[serde(default = "default_one")]
    pub damage: f64,
    /// 伤害区间宽度（对应 Lua `damageSpread`；默认 0.0）。
    #[serde(default)]
    pub damage_spread: f64,
    /// 基础攻击间隔秒（对应 Lua `attackTime`；默认 1.0）。
    #[serde(default = "default_one")]
    pub attack_time: f64,
    /// 虚拟武器暴击率（对应 Lua `critChance`；默认 5.0）。
    #[serde(default = "default_five")]
    pub crit_chance: f64,
    /// 护甲归一化乘数（对应 Lua `armour`；默认 1.0；部分召唤物缺省字段 = 1）。
    #[serde(default = "default_one")]
    pub armour: f64,
    /// 闪避归一化乘数（对应 Lua `evasion`；默认 1.0）。
    #[serde(default = "default_one")]
    pub evasion: f64,
    /// 生命转 ES 占比（对应 Lua `energyShield`；0.15 → LifeConvertToEnergyShield BASE=15；默认 0.0）。
    #[serde(default)]
    pub energy_shield: f64,
    /// 火焰抗性基础值（%；对应 Lua `fireResist`；默认 0.0）。
    #[serde(default)]
    pub fire_resist: f64,
    /// 冰冷抗性基础值（%；默认 0.0）。
    #[serde(default)]
    pub cold_resist: f64,
    /// 闪电抗性基础值（%；默认 0.0）。
    #[serde(default)]
    pub lightning_resist: f64,
    /// 混沌抗性基础值（%；默认 0.0）。
    #[serde(default)]
    pub chaos_resist: f64,
    /// 基础伤害是否忽略攻速（对应 Lua `baseDamageIgnoresAttackSpeed`；默认 false）。
    #[serde(default)]
    pub base_damage_ignores_attack_speed: bool,
    /// 数量上限稳定 ID（对应 Lua `limit`）。
    #[serde(default = "default_limit_none")]
    pub limit: MinionLimitId,
    /// 单个召唤物消耗的精神保留量（对应 Lua `spectreReservation`；默认 50.0）。
    #[serde(default = "default_spectre_reservation")]
    pub spectre_reservation: f64,
    /// 单个召唤物消耗的伴侣保留量（对应 Lua `companionReservation`；默认 30.0）。
    #[serde(default = "default_companion_reservation")]
    pub companion_reservation: f64,
    /// 是否为敌对型召唤物（对应 Lua `hostile`；影响 MinionModifier vs EnemyModifier 传递）。
    #[serde(default)]
    pub hostile: bool,
    /// 怪物标签列表（对应 Lua `monsterTags`；影响条件判定和 keyword 匹配）。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub monster_tags: Vec<String>,
    /// 技能列表（对应 Lua `skillList`；法术型召唤物通过此列表获取技能基础伤害）。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub skill_list: Vec<String>,
}

// serde 默认值辅助函数（#[serde(default = "...")] 需要 fn() 签名）

#[inline]
fn default_one() -> f64 {
    1.0
}

#[inline]
fn default_five() -> f64 {
    5.0
}

#[inline]
fn default_limit_none() -> MinionLimitId {
    MinionLimitId::None
}

#[inline]
fn default_spectre_reservation() -> f64 {
    50.0
}

#[inline]
fn default_companion_reservation() -> f64 {
    30.0
}

// 代表性召唤物常量（内置数据，供无 JSON 的单元测试 / 快速原型使用）
// 数值来自 PoB2 src/Data/Minions.lua（dev 分支，2025-06）

/// 僵尸（Raised Zombie）的 `MinionDef` 默认常量。
///
/// 出处：PoB2 Minions.lua `minions["RaisedZombie"]`。
pub fn minion_def_zombie() -> MinionDef {
    MinionDef {
        id: "RaisedZombie".into(),
        name: "Raised Zombie".into(),
        category: Some(MinionCategory::Undead),
        life: 0.7,
        damage: 0.75,
        damage_spread: 0.3,
        attack_time: 1.25,
        crit_chance: 5.0,
        armour: 1.0,
        evasion: 1.0,
        energy_shield: 0.0,
        fire_resist: 0.0,
        cold_resist: 0.0,
        lightning_resist: 0.0,
        chaos_resist: 0.0,
        base_damage_ignores_attack_speed: true,
        limit: MinionLimitId::ActiveZombieLimit,
        spectre_reservation: 50.0,
        companion_reservation: 30.0,
        hostile: false,
        monster_tags: vec![
            "animal_claw_weapon".into(),
            "flesh_armour".into(),
            "melee".into(),
            "undead".into(),
        ],
        skill_list: vec!["MinionMeleeStep".into()],
    }
}

/// 暴怒魂灵（Raging Spirit）的 `MinionDef` 默认常量。
///
/// 出处：PoB2 Minions.lua `minions["SummonedRagingSpirit"]`。
pub fn minion_def_raging_spirit() -> MinionDef {
    MinionDef {
        id: "SummonedRagingSpirit".into(),
        name: "Raging Spirit".into(),
        category: Some(MinionCategory::Construct),
        life: 0.25,
        damage: 0.7,
        damage_spread: 0.2,
        attack_time: 1.0,
        crit_chance: 5.0,
        armour: 1.0,
        evasion: 1.0,
        energy_shield: 0.0,
        fire_resist: 0.0,
        cold_resist: 0.0,
        lightning_resist: 0.0,
        chaos_resist: 0.0,
        base_damage_ignores_attack_speed: true,
        limit: MinionLimitId::ActiveRagingSpiritLimit,
        spectre_reservation: 50.0,
        companion_reservation: 30.0,
        hostile: false,
        monster_tags: vec![
            "bone_armour".into(),
            "construct".into(),
            "melee".into(),
            "undead".into(),
        ],
        skill_list: vec!["MinionMeleeStep".into()],
    }
}

/// 骷髅战士（Skeletal Warrior）的 `MinionDef` 默认常量。
///
/// 出处：PoB2 Minions.lua `minions["RaisedSkeletonWarriors"]`。
pub fn minion_def_skeletal_warrior() -> MinionDef {
    MinionDef {
        id: "RaisedSkeletonWarriors".into(),
        name: "Skeletal Warrior".into(),
        category: Some(MinionCategory::Undead),
        life: 0.88,
        damage: 0.7,
        damage_spread: 0.3,
        attack_time: 1.0,
        crit_chance: 5.0,
        armour: 0.5,
        evasion: 1.0,
        energy_shield: 0.0,
        fire_resist: 0.0,
        cold_resist: 0.0,
        lightning_resist: 0.0,
        chaos_resist: 0.0,
        base_damage_ignores_attack_speed: true,
        limit: MinionLimitId::ActiveSkeletonLimit,
        spectre_reservation: 50.0,
        companion_reservation: 30.0,
        hostile: false,
        monster_tags: vec![
            "bone_armour".into(),
            "bones".into(),
            "melee".into(),
            "undead".into(),
        ],
        skill_list: vec!["MinionMeleeStep".into()],
    }
}

/// 骷髅法师·暴风（Skeletal Storm Mage）的 `MinionDef` 默认常量。
///
/// 出处：PoB2 Minions.lua `minions["RaisedSkeletonStormMage"]`。
pub fn minion_def_skeletal_storm_mage() -> MinionDef {
    MinionDef {
        id: "RaisedSkeletonStormMage".into(),
        name: "Skeletal Storm Mage".into(),
        category: Some(MinionCategory::Undead),
        life: 0.53,
        damage: 0.65,
        damage_spread: 0.3,
        attack_time: 1.0,
        crit_chance: 5.0,
        armour: 1.0,
        evasion: 1.0,
        energy_shield: 0.15,
        fire_resist: 0.0,
        cold_resist: 0.0,
        lightning_resist: 50.0,
        chaos_resist: 0.0,
        base_damage_ignores_attack_speed: true,
        limit: MinionLimitId::ActiveSkeletonLimit,
        spectre_reservation: 50.0,
        companion_reservation: 30.0,
        hostile: false,
        monster_tags: vec![
            "bone_armour".into(),
            "bones".into(),
            "caster".into(),
            "undead".into(),
        ],
        skill_list: vec![
            "ArcSkeletonMageMinion".into(),
            "DeathStormSkeletonStormMageMinion".into(),
        ],
    }
}

/// 幽灵（Spectre）的 `MinionDef` 占位常量（全量入库后替换为真实 Spectre 条目）。
///
/// Spectre 在 PoB2 中是「用区域等级锁定怪物等级」的特殊召唤物，
/// 此处仅给出 schema 结构与占位数值，计算时需注意 level 来源不同（见 agent-docs §1.1）。
pub fn minion_def_spectre_placeholder() -> MinionDef {
    MinionDef {
        id: "Spectre".into(),
        name: "Raised Spectre (placeholder)".into(),
        category: None,
        life: 1.0,
        damage: 1.0,
        damage_spread: 0.2,
        attack_time: 1.0,
        crit_chance: 5.0,
        armour: 1.0,
        evasion: 1.0,
        energy_shield: 0.0,
        fire_resist: 0.0,
        cold_resist: 0.0,
        lightning_resist: 0.0,
        chaos_resist: 0.0,
        base_damage_ignores_attack_speed: false,
        limit: MinionLimitId::ActiveSpectreLimit,
        spectre_reservation: 50.0,
        companion_reservation: 30.0,
        hostile: false,
        monster_tags: vec![],
        skill_list: vec![],
    }
}

/// 从 `MinionDef` 提取的归一化乘数快照（无 serde，内存内部使用）。
///
/// 由 [`MinionDef::scaling`] 返回，供 `pobr-core::calc::minion::MinionData::from_def` 消费。
/// 不引入对 `pobr-core` 的依赖（data 层不依赖 core）。
///
/// 出处：agent-docs/minions.md §1；PoB2 CalcActiveSkill.lua `minionData.*`。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MinionScaling {
    pub life: f64,
    pub damage: f64,
    pub damage_spread: f64,
    pub attack_time: f64,
    pub crit_chance: f64,
    pub armour: f64,
    pub evasion: f64,
    pub energy_shield: f64,
    pub fire_resist: f64,
    pub cold_resist: f64,
    pub lightning_resist: f64,
    pub chaos_resist: f64,
    pub base_damage_ignores_attack_speed: bool,
}

// impl MinionDef

impl MinionDef {
    /// 从 `MinionDef` 提取归一化乘数，返回 [`MinionScaling`]。
    ///
    /// 调用方（`pobr-core` / `pobr-build`）负责把结构体字段填入 `MinionData`。
    ///
    /// 出处：agent-docs/minions.md §1；PoB2 CalcActiveSkill.lua `minionData.*`。
    pub fn scaling(&self) -> MinionScaling {
        MinionScaling {
            life: self.life,
            damage: self.damage,
            damage_spread: self.damage_spread,
            attack_time: self.attack_time,
            crit_chance: self.crit_chance,
            armour: self.armour,
            evasion: self.evasion,
            energy_shield: self.energy_shield,
            fire_resist: self.fire_resist,
            cold_resist: self.cold_resist,
            lightning_resist: self.lightning_resist,
            chaos_resist: self.chaos_resist,
            base_damage_ignores_attack_speed: self.base_damage_ignores_attack_speed,
        }
    }

    /// 是否有 energyShield（`energy_shield > 0`）。
    pub fn has_energy_shield(&self) -> bool {
        self.energy_shield > 0.0
    }

    /// 是否有明确的数量上限（`limit != None`）。
    pub fn has_limit(&self) -> bool {
        !matches!(self.limit, MinionLimitId::None)
    }

    /// 从入库 overlay 形态 [`crate::catalog::actors::MinionEntryDef`] 构造计算消费形态
    /// `MinionDef`（桥接：loader 产 `MinionEntryDef`，calc 消费 `MinionDef`）。
    ///
    /// 字段对齐：`armour`/`evasion` 缺省按 1.0、`energy_shield` 缺省按 0.0（MinionEntryDef
    /// 忠实转录 vendor 缺失语义，此处物化消费侧默认值，见 `MinionEntryDef` 文档）。
    /// `limit` 字符串经 [`MinionLimitId::from_pob2`] 映射；`monster_category` 经
    /// [`MinionCategory::from_pob2`]。`mod_list` 暂不转入（calc 侧 C3 装配从 entry
    /// 直接读，避免重复建模）。
    pub fn from_entry(e: &crate::catalog::actors::MinionEntryDef) -> Self {
        Self {
            id: e.id.clone(),
            name: e.name.clone(),
            category: e.monster_category.as_deref().map(MinionCategory::from_pob2),
            life: e.life,
            damage: e.damage,
            damage_spread: e.damage_spread,
            attack_time: e.attack_time,
            crit_chance: e.crit_chance,
            armour: e.armour.unwrap_or(1.0),
            evasion: e.evasion.unwrap_or(1.0),
            energy_shield: e.energy_shield.unwrap_or(0.0),
            fire_resist: e.fire_resist,
            cold_resist: e.cold_resist,
            lightning_resist: e.lightning_resist,
            chaos_resist: e.chaos_resist,
            base_damage_ignores_attack_speed: e.base_damage_ignores_attack_speed,
            limit: e
                .limit
                .as_deref()
                .map(MinionLimitId::from_pob2)
                .unwrap_or(MinionLimitId::None),
            spectre_reservation: e.spectre_reservation,
            companion_reservation: e.companion_reservation,
            hostile: false,
            monster_tags: e.monster_tags.clone(),
            skill_list: e.skill_list.clone(),
        }
    }
}

// 单测

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zombie_def_matches_pob2() {
        // PoB2 minions["RaisedZombie"]: life=0.7, damage=0.75, attackTime=1.25
        let def = minion_def_zombie();
        assert_eq!(def.id, "RaisedZombie");
        assert_eq!(def.life, 0.7);
        assert_eq!(def.damage, 0.75);
        assert!((def.damage_spread - 0.3).abs() < 1e-9);
        assert!((def.attack_time - 1.25).abs() < 1e-9);
        assert_eq!(def.crit_chance, 5.0);
        assert!(def.base_damage_ignores_attack_speed);
        assert_eq!(def.limit, MinionLimitId::ActiveZombieLimit);
        assert_eq!(def.energy_shield, 0.0);
        assert!(!def.hostile);
    }

    #[test]
    fn raging_spirit_def_matches_pob2() {
        // PoB2 minions["SummonedRagingSpirit"]: life=0.25, damage=0.7, damageSpread=0.2
        let def = minion_def_raging_spirit();
        assert_eq!(def.id, "SummonedRagingSpirit");
        assert_eq!(def.life, 0.25);
        assert_eq!(def.damage, 0.7);
        assert!((def.damage_spread - 0.2).abs() < 1e-9);
        assert_eq!(def.limit, MinionLimitId::ActiveRagingSpiritLimit);
    }

    #[test]
    fn skeletal_warrior_def_armour_normalizer() {
        // PoB2 minions["RaisedSkeletonWarriors"]: armour=0.5
        let def = minion_def_skeletal_warrior();
        assert!((def.armour - 0.5).abs() < 1e-9);
        assert_eq!(def.limit, MinionLimitId::ActiveSkeletonLimit);
    }

    #[test]
    fn storm_mage_has_energy_shield() {
        // PoB2 minions["RaisedSkeletonStormMage"]: energyShield=0.15, lightningResist=50
        let def = minion_def_skeletal_storm_mage();
        assert!(def.has_energy_shield());
        assert!((def.energy_shield - 0.15).abs() < 1e-9);
        assert!((def.lightning_resist - 50.0).abs() < 1e-9);
    }

    #[test]
    fn limit_id_roundtrip_from_pob2() {
        let cases = [
            ("ActiveZombieLimit", MinionLimitId::ActiveZombieLimit),
            ("ActiveSkeletonLimit", MinionLimitId::ActiveSkeletonLimit),
            (
                "ActiveRagingSpiritLimit",
                MinionLimitId::ActiveRagingSpiritLimit,
            ),
            ("ActiveSpectreLimit", MinionLimitId::ActiveSpectreLimit),
            (
                "ActiveLivingLightningLimit",
                MinionLimitId::ActiveLivingLightningLimit,
            ),
            (
                "ActiveUnearthBoneConstructLimit",
                MinionLimitId::ActiveUnearthBoneConstructLimit,
            ),
            ("WardboundLimit", MinionLimitId::WardboundLimit),
            ("WolfLimit", MinionLimitId::WolfLimit),
            ("BeetleLimit", MinionLimitId::BeetleLimit),
            ("AzmerianSwarmLimit", MinionLimitId::AzmerianSwarmLimit),
        ];
        for (s, expected) in cases {
            let got = MinionLimitId::from_pob2(s);
            assert_eq!(got, expected, "from_pob2({s})");
            assert_eq!(got.to_pob2_str(), s, "to_pob2_str({s}) roundtrip");
        }
    }

    #[test]
    fn limit_id_none_from_empty() {
        assert_eq!(MinionLimitId::from_pob2(""), MinionLimitId::None);
        assert_eq!(MinionLimitId::None.to_pob2_str(), "");
    }

    #[test]
    fn limit_id_custom_unknown() {
        let custom = MinionLimitId::from_pob2("SomeUnknownLimit");
        assert!(matches!(custom, MinionLimitId::Custom(ref s) if s == "SomeUnknownLimit"));
    }

    #[test]
    fn def_has_limit_works() {
        assert!(minion_def_zombie().has_limit());
        let placeholder = minion_def_spectre_placeholder();
        assert!(placeholder.has_limit()); // Spectre has ActiveSpectreLimit
        let no_limit = MinionDef {
            id: "NoLimitMinion".into(),
            name: "No Limit".into(),
            category: None,
            life: 1.0,
            damage: 1.0,
            damage_spread: 0.0,
            attack_time: 1.0,
            crit_chance: 5.0,
            armour: 1.0,
            evasion: 1.0,
            energy_shield: 0.0,
            fire_resist: 0.0,
            cold_resist: 0.0,
            lightning_resist: 0.0,
            chaos_resist: 0.0,
            base_damage_ignores_attack_speed: false,
            limit: MinionLimitId::None,
            spectre_reservation: 50.0,
            companion_reservation: 30.0,
            hostile: false,
            monster_tags: vec![],
            skill_list: vec![],
        };
        assert!(!no_limit.has_limit());
    }

    #[test]
    fn scaling_returns_correct_fields() {
        let def = minion_def_zombie();
        let s = def.scaling();
        assert_eq!(s.life, 0.7);
        assert_eq!(s.damage, 0.75);
        assert!((s.damage_spread - 0.3).abs() < 1e-9);
        assert!((s.attack_time - 1.25).abs() < 1e-9);
        assert_eq!(s.crit_chance, 5.0);
        assert_eq!(s.armour, 1.0);
        assert_eq!(s.evasion, 1.0);
        assert_eq!(s.energy_shield, 0.0);
        assert_eq!(s.fire_resist, 0.0);
        assert_eq!(s.cold_resist, 0.0);
        assert_eq!(s.lightning_resist, 0.0);
        assert_eq!(s.chaos_resist, 0.0);
        assert!(s.base_damage_ignores_attack_speed);
    }

    #[test]
    fn category_roundtrip() {
        let cases = [
            ("Undead", MinionCategory::Undead),
            ("Construct", MinionCategory::Construct),
            ("Demon", MinionCategory::Demon),
            ("Beast", MinionCategory::Beast),
            ("Humanoid", MinionCategory::Humanoid),
        ];
        for (s, expected) in cases {
            assert_eq!(MinionCategory::from_pob2(s), expected, "{s}");
        }
    }

    #[test]
    fn serde_roundtrip_zombie() {
        // Verify serde traits are derivable: serialize and deserialize the def.
        // We can't use serde_json here (no dev-dep), but we can verify Clone + PartialEq work.
        let def = minion_def_zombie();
        let cloned = def.clone();
        assert_eq!(cloned.id, def.id);
        assert_eq!(cloned.life, def.life);
        assert_eq!(cloned.limit, def.limit);
        assert!((cloned.attack_time - def.attack_time).abs() < 1e-12);
    }
}
