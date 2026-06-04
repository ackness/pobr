use serde::{Deserialize, Serialize};

/// 默认最大元素 / 混沌抗性（百分比）。超过此值的抗性记为 over-cap。
pub const DEFAULT_MAX_RESISTANCE: f64 = 75.0;
/// 抗性硬上限（百分比）；任何最大抗性提升都不能突破。
pub const HARD_MAX_RESISTANCE: f64 = 90.0;
/// 抗性下界（负抗性允许堆叠到的下限）。
pub const RESIST_FLOOR: f64 = -200.0;
/// PoE2 护甲系数（护甲减伤 `armour/(armour + ARMOUR_RATIO*raw_hit)`；PoE1 为 5）。
pub const ARMOUR_RATIO: f64 = 10.0;
/// 非吟唱动作的服务器帧时间（约 33ms → 30.3 actions/s 上限），skill-speed.md。
pub const SERVER_TICK_SECONDS: f64 = 0.033;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DamageType {
    Physical,
    Fire,
    Cold,
    Lightning,
    Chaos,
}

impl DamageType {
    /// 元素伤害三种类型（`ElementalDamage` modifier 匹配这三者，不含混沌）。
    pub const ELEMENTAL: [DamageType; 3] =
        [DamageType::Fire, DamageType::Cold, DamageType::Lightning];

    /// 是否属于元素伤害（火/冰/电）。
    pub fn is_elemental(self) -> bool {
        matches!(
            self,
            DamageType::Fire | DamageType::Cold | DamageType::Lightning
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ClassId {
    Marauder,
    Duelist,
    Ranger,
    Shadow,
    Witch,
    Templar,
    Scion,
}

/// 元素子类型，辅助 `ElementalDamage` 聚合（火/冰/电）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ElementalType {
    Fire,
    Cold,
    Lightning,
}

impl ElementalType {
    pub fn damage_type(self) -> DamageType {
        match self {
            ElementalType::Fire => DamageType::Fire,
            ElementalType::Cold => DamageType::Cold,
            ElementalType::Lightning => DamageType::Lightning,
        }
    }
}

/// 伤害来源桶：区分击中伤害与各类持续 / 次级伤害（08-mechanics §2.3）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DamageSource {
    Attack,
    Spell,
    Secondary,
    Thorns,
    Ailment,
    Debuff,
}

/// 击中 vs 持续：DoT 不能暴击，ailment magnitude 基于 pre-mitigation Hit。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DamageKind {
    Hit,
    Dot,
}

/// 异常状态类型。与 [`DamageType`] 分离以支持 Corrupted Blood 等不属于 bleeding 的物理 DoT。
///
/// 资料：`agent-docs/ailments.md` + `agent-docs/damage-types.md`（PoE2 0.5.0）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AilmentType {
    Bleed,
    Ignite,
    Shock,
    Chill,
    Freeze,
    Poison,
    Electrocute,
    CorruptedBlood,
}

/// 异常状态是否造成伤害（用于快速分组，避免上层 match 臃肿）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AilmentCategory {
    Damaging,
    NonDamaging,
}

impl AilmentType {
    /// 是否为造成伤害的异常状态。
    pub fn category(self) -> AilmentCategory {
        match self {
            AilmentType::Bleed
            | AilmentType::Ignite
            | AilmentType::Poison
            | AilmentType::CorruptedBlood => AilmentCategory::Damaging,
            AilmentType::Shock
            | AilmentType::Chill
            | AilmentType::Freeze
            | AilmentType::Electrocute => AilmentCategory::NonDamaging,
        }
    }

    /// 造成伤害的异常状态对应的伤害类型；非伤害类返回 `None`。
    ///
    /// 流血 / 腐化之血为物理，点燃为火，中毒为混沌（基于命中的物理+混沌生成 magnitude）。
    pub fn damage_type(self) -> Option<DamageType> {
        match self {
            AilmentType::Bleed | AilmentType::CorruptedBlood => Some(DamageType::Physical),
            AilmentType::Ignite => Some(DamageType::Fire),
            AilmentType::Poison => Some(DamageType::Chaos),
            AilmentType::Shock
            | AilmentType::Chill
            | AilmentType::Freeze
            | AilmentType::Electrocute => None,
        }
    }
}

/// 集中存放 PoE2 计算常量，避免在公式中散落 magic number。
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct GameConstants {
    pub resist_default_max: f64,
    pub resist_hard_cap: f64,
    pub resist_floor: f64,
    pub server_tick_seconds: f64,
    pub armour_ratio: f64,
    /// 流血基础 magnitude 占 pre-mitigation 物理命中的比例（每秒）。
    pub bleed_base_fraction: f64,
    /// 流血基础持续时间（秒）。
    pub bleed_base_duration: f64,
    /// 点燃基础 magnitude 占 pre-mitigation 火命中的比例（每秒）。
    pub ignite_base_fraction: f64,
    pub ignite_base_duration: f64,
    /// 中毒基础 magnitude 占 pre-mitigation 命中的比例（每秒）。
    pub poison_base_fraction: f64,
    pub poison_base_duration: f64,
    /// 默认感电增伤幅度（agent-docs 给定 20%）。
    pub shock_default_effect: f64,
}

impl GameConstants {
    /// PoE2 0.5.0 默认常量集。
    pub fn poe2() -> Self {
        Self {
            resist_default_max: DEFAULT_MAX_RESISTANCE,
            resist_hard_cap: HARD_MAX_RESISTANCE,
            resist_floor: RESIST_FLOOR,
            server_tick_seconds: SERVER_TICK_SECONDS,
            armour_ratio: ARMOUR_RATIO,
            bleed_base_fraction: 0.15,
            bleed_base_duration: 5.0,
            ignite_base_fraction: 0.20,
            ignite_base_duration: 4.0,
            poison_base_fraction: 0.20,
            poison_base_duration: 2.0,
            shock_default_effect: 0.20,
        }
    }

    /// 非吟唱动作的服务器帧上限速率（actions/s）。
    pub fn server_tick_rate(&self) -> f64 {
        1.0 / self.server_tick_seconds
    }
}

impl Default for GameConstants {
    fn default() -> Self {
        Self::poe2()
    }
}

/// 一段伤害区间（技能等级表 base damage 的元素）。
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct DamageRange {
    pub min: f64,
    pub max: f64,
}

impl DamageRange {
    pub fn new(min: f64, max: f64) -> Self {
        Self { min, max }
    }

    /// 区间平均值。
    pub fn avg(&self) -> f64 {
        (self.min + self.max) / 2.0
    }
}

/// 技能消耗类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SkillCostKind {
    Mana,
    Life,
    Spirit,
    EnergyShield,
    NoCost,
}

/// 技能消耗（类型 + 数值）。
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SkillCost {
    pub kind: SkillCostKind,
    pub value: f64,
}

impl Default for SkillCost {
    fn default() -> Self {
        Self {
            kind: SkillCostKind::NoCost,
            value: 0.0,
        }
    }
}
