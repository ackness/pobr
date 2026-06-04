//! 异常状态 / debuff 的跨层结果结构。
//!
//! 这些类型是 `pobr-core` 计算结果的数据契约，放在 `pobr-data` 使 `pobr-build`
//! 等上层可直接持有结果而无需依赖 `pobr-core`。
//!
//! 注意：按伤害分量分桶的 `DamageComponent` 由 `pobr-core::calc` 提供，本模块
//! 不重复定义，避免同名冲突。

use serde::{Deserialize, Serialize};

use crate::constants::{AilmentType, DamageSource};

/// 一个已施加的异常状态实例。
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct AilmentInstance {
    pub ailment: AilmentType,
    /// 每秒伤害（非伤害类异常为 0）。
    pub magnitude_dps: f64,
    /// 持续时间（秒）。
    pub duration_secs: f64,
    /// 生成该异常的命中分量来源。
    #[serde(default)]
    pub source_component: Option<DamageSource>,
    /// 该 DoT 是否无视能量护盾。
    #[serde(default)]
    pub bypasses_es: bool,
}

impl AilmentInstance {
    /// 异常状态总伤害（dps * 持续时间）。
    pub fn total_damage(&self) -> f64 {
        self.magnitude_dps * self.duration_secs
    }
}

/// 可堆叠的 debuff 实例（如腐化之血，不属于 bleeding）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DebuffInstance {
    pub label: String,
    pub current_stacks: u8,
    pub max_stacks: u8,
    pub dps_per_stack: f64,
    pub duration_secs: f64,
}

impl DebuffInstance {
    /// 当前总 DPS（受 max_stacks 限制）。
    pub fn total_dps(&self) -> f64 {
        self.dps_per_stack * f64::from(self.current_stacks.min(self.max_stacks))
    }
}
