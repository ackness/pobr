//! 异常状态与 debuff DoT 计算（08-mechanics §2.4、§2.5；`agent-docs/ailments.md`）。
//!
//! 伤害类异常的 magnitude 基于 pre-mitigation 命中：流血/中毒为物理/混沌，点燃为火。
//! magnitude 再吃对应的 ailment damage inc/more 与 duration modifier。
//! Corrupted Blood 不是 bleeding，走 [`DebuffInstance`]（最多 10 层）。
//!
//! 注：异常精确系数（shock 映射、corrupted blood per-stack）依赖 PoB-PoE2 数据，
//! 标注为 `blocked_by_missing_data`，此处实现机制骨架 + agent-docs 默认值。

use pobr_data::constants::SHOCK_MIN_EFFECT;
use pobr_data::prelude::*;

use crate::{CalcConfig, ModDb};

use super::round;

/// 应用一组 ailment damage modifier（inc 累加、more 连乘）到基础 magnitude。
fn scale_magnitude(base: f64, db: &ModDb, cfg: &CalcConfig, names: &[ModName]) -> f64 {
    let inc = db.sum(ModType::Inc, cfg, names);
    let more = db.more(cfg, names);
    base * (1.0 + inc / 100.0) * more
}

/// 应用 duration modifier（inc 累加）。
fn scale_duration(base: f64, db: &ModDb, cfg: &CalcConfig, names: &[ModName]) -> f64 {
    let inc = db.sum(ModType::Inc, cfg, names);
    base * (1.0 + inc / 100.0)
}

/// 流血实例：magnitude = 15% pre-mitigation 物理命中/秒，持续 5s。
pub fn bleed_instance(
    pre_mitigation_phys_hit: f64,
    db: &ModDb,
    cfg: &CalcConfig,
) -> AilmentInstance {
    let gc = GameConstants::poe2();
    let base_dps = pre_mitigation_phys_hit * gc.bleed_base_fraction;
    let magnitude_dps = scale_magnitude(
        base_dps,
        db,
        cfg,
        &[
            ModName::from("BleedDamage"),
            ModName::from("AilmentDamage"),
            ModName::from("PhysicalDamageOverTime"),
            ModName::from("DamageOverTime"),
        ],
    );
    let duration_secs = scale_duration(
        gc.bleed_base_duration,
        db,
        cfg,
        &[
            ModName::from("BleedDuration"),
            ModName::from("AilmentDuration"),
        ],
    );
    AilmentInstance {
        ailment: AilmentType::Bleed,
        magnitude_dps: round(magnitude_dps),
        duration_secs: round(duration_secs),
        source_component: Some(DamageSource::Attack),
        bypasses_es: true,
    }
}

/// 点燃实例：magnitude = 20% pre-mitigation 火命中/秒，持续 4s。
pub fn ignite_instance(
    pre_mitigation_fire_hit: f64,
    db: &ModDb,
    cfg: &CalcConfig,
) -> AilmentInstance {
    let gc = GameConstants::poe2();
    let base_dps = pre_mitigation_fire_hit * gc.ignite_base_fraction;
    let magnitude_dps = scale_magnitude(
        base_dps,
        db,
        cfg,
        &[
            ModName::from("IgniteDamage"),
            ModName::from("BurningDamage"),
            ModName::from("AilmentDamage"),
            ModName::from("FireDamageOverTime"),
            ModName::from("DamageOverTime"),
        ],
    );
    let duration_secs = scale_duration(
        gc.ignite_base_duration,
        db,
        cfg,
        &[
            ModName::from("IgniteDuration"),
            ModName::from("AilmentDuration"),
        ],
    );
    AilmentInstance {
        ailment: AilmentType::Ignite,
        magnitude_dps: round(magnitude_dps),
        duration_secs: round(duration_secs),
        source_component: None,
        bypasses_es: false,
    }
}

/// 中毒实例：magnitude = 20% pre-mitigation 命中（物理+混沌）/秒，混沌 DoT，持续 2s。
pub fn poison_instance(pre_mitigation_hit: f64, db: &ModDb, cfg: &CalcConfig) -> AilmentInstance {
    let gc = GameConstants::poe2();
    let base_dps = pre_mitigation_hit * gc.poison_base_fraction;
    let magnitude_dps = scale_magnitude(
        base_dps,
        db,
        cfg,
        &[
            ModName::from("PoisonDamage"),
            ModName::from("AilmentDamage"),
            ModName::from("ChaosDamageOverTime"),
            ModName::from("DamageOverTime"),
        ],
    );
    let duration_secs = scale_duration(
        gc.poison_base_duration,
        db,
        cfg,
        &[
            ModName::from("PoisonDuration"),
            ModName::from("AilmentDuration"),
        ],
    );
    AilmentInstance {
        ailment: AilmentType::Poison,
        magnitude_dps: round(magnitude_dps),
        duration_secs: round(duration_secs),
        source_component: None,
        bypasses_es: true,
    }
}

/// 感电增伤幅度：`0.5 * (hit/threshold)^0.4`，clamp 到 [20%, 100%]。
///
/// **Bug#9 修正（shock-min-clamp-bug）**：
/// PoE2 0.5.0 `BaseShockMagnitude = 20`，感电最小有效值为 **20%**（非 PoE1 的 5%）。
/// 最大值为 100%（`ShockMaxEffect = 100`，远超通常可达的 50%）。
/// 出处：agent-docs/ailments.md §感电、PoB2 `nonDamagingAilmentsConfig.Shock`：
///   `Shock.effect = 50 * (damage/enemyThreshold)^0.4 * effectMod, clamp [min=20, max=100]`
pub fn shock_effect(pre_mitigation_lightning_hit: f64, target_ailment_threshold: f64) -> f64 {
    if pre_mitigation_lightning_hit <= 0.0 || target_ailment_threshold <= 0.0 {
        return 0.0;
    }
    let ratio = pre_mitigation_lightning_hit / target_ailment_threshold;
    // 50 * ratio^0.4 → 以百分点计；SHOCK_MIN_EFFECT 以整数（20）存储，转为小数比例
    let effect_pct = 50.0 * ratio.powf(0.4);
    let min_pct = SHOCK_MIN_EFFECT; // 20.0 (percent)
    let max_pct = 100.0;
    round(effect_pct.clamp(min_pct, max_pct) / 100.0)
}

/// 腐化之血 debuff（物理 DoT，最多 10 层，不属于 bleeding）。
pub fn corrupted_blood_instance(dps_per_stack: f64) -> DebuffInstance {
    DebuffInstance {
        label: "Corrupted Blood".into(),
        current_stacks: 10,
        max_stacks: 10,
        dps_per_stack: round(dps_per_stack),
        duration_secs: 8.0,
    }
}
