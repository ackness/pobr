/// 默认最大元素 / 混沌抗性（百分比）。超过此值的抗性记为 over-cap。
pub const DEFAULT_MAX_RESISTANCE: f64 = 75.0;
/// 抗性硬上限（百分比）；任何最大抗性提升都不能突破。
pub const HARD_MAX_RESISTANCE: f64 = 90.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DamageType {
    Physical,
    Fire,
    Cold,
    Lightning,
    Chaos,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ClassId {
    Marauder,
    Duelist,
    Ranger,
    Shadow,
    Witch,
    Templar,
    Scion,
}
