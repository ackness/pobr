//! 暴击/非暴击双 pass（M4-T2 W-B3，蓝图 m4-offence-deep.md §2、12-G3）。
//!
//! PoB2 在每个 hand pass 内按 `CriticalStrike` 条件分别聚合伤害主体
//! （`CalcOffence.lua:3978-3980`：`for pass = 1, 2 do cfg.skillCond["CriticalStrike"]
//! = (pass == 1)`，pass 1 = 暴击腿），暴击腿 `allMult ×= CritMultiplier`
//! （`:4028-4032`），两腿 pre-resist 均值分存 `Stored<Type>CritAvg/HitAvg/CombinedAvg`
//! （`:4047-4057`，ailment magnitude 输入），末端 CritBlend 合并
//! （`:4395` `AverageHit = totalHitAvg×(1−c) + totalCritAvg×c`）。
//!
//! 本模块替换 `offence.rs` 的单因子 `total_hit_avg = non_crit_hit_avg × crit.effect`。
//!
//! ## 等价性短路（I5，回退开关的正确性证明）
//!
//! 无 `CriticalStrike` 条件词条时两腿聚合输入逐位相同（分量向量 + per-type lucky
//! 几率比较），此时走旧单因子公式（数学恒等 I5：`blend(c, x×m, x) = x×(1+(m−1)c)
//! = x×crit.effect`，且**复用旧公式的取整顺序**保证逐字节等价——等价性测试钉死）。
//! 两腿有差异（暴击腿专属词条 / CritLucky 等）才走真双腿 blend。
//!
//! ## T3 乘区接线（契约 2/3，m4-t3-wiring-notes.md §2）
//!
//! - W-C1 `ScaledDamageEffect`：两腿共用（vendor `:4023-4025` allMult），由调用方
//!   传入（`scaled_damage_effect(db, enemy_db, cfg, crit.chance)`）。
//! - W-C2 lucky：min/max ×allMult **之后**按 (pass, damageType) 求 lucky 几率折
//!   avg（`:4035-4046`）。
//! - W-C3 canDeal：转换链之后、聚合之前就地清零（`:3989` 消费点）。
//!
//! ## 保守口径（登记，独立行为 commit 再修）
//!
//! 敌方护甲减伤的 `raw_hit` 沿用 **lucky 折算前**的分量均值（与替换前实现一致；
//! vendor 按 pass 内已乘 allMult 的伤害走护甲公式 `:4060+`——暴击腿 raw_hit 已含
//! ×CritMultiplier 属 per-pass 精化，本实现腿内 raw_hit 即该腿分量 avg，短路路径
//! 与旧实现完全同形）。

use pobr_data::prelude::*;

use crate::{CalcConfig, ModDb};

use super::crit::CritOutcome;
use super::damage::{DamageComponent, apply_can_deal, calculate_components, lucky_hit_chance};
use super::output::StoredDamageRange;
use super::round;
use super::scaled_damage::{AllMultExtras, ScaledDamage, all_mult};

/// 暴击双 pass 结果（蓝图 §2 W-B3 契约形态）。
#[derive(Debug, Clone, PartialEq)]
pub struct CritPassOutput {
    /// 非暴击腿分量（canDeal 门控 + allMult 缩放后；顶层 `damage_components`
    /// 字段与 ailment magnitude 的现行来源）。
    pub non_crit_components: Vec<DamageComponent>,
    /// 暴击腿分量（额外 ×CritMultiplier）。
    pub crit_components: Vec<DamageComponent>,
    /// `Stored<Type>CritAvg`（pre-resist，含 allMult 与 ×CritMultiplier，`:4049`）。
    pub stored_crit_avg: Vec<(DamageType, f64)>,
    /// `Stored<Type>HitAvg`（非暴击腿，`:4054`）。
    pub stored_hit_avg: Vec<(DamageType, f64)>,
    /// `Stored<Type>CombinedAvg`（两腿按暴击率加权累计，`:4048/:4053`）。
    pub stored_combined_avg: Vec<(DamageType, f64)>,
    /// `Stored<Type>{Hit,Crit}{Min,Max}`（`:4050-4056`；min/max **不做** lucky 折算
    /// （vendor lucky 只折 `*Avg` 族），damaging ailment 来源伤害与 RollAverage
    /// 内插的输入面，M4-G append）。
    pub stored_ranges: Vec<StoredDamageRange>,
    /// CritBlend 后玩家侧总均值（不含敌减伤；`total_hit_avg` 字段口径）。
    pub total_hit_avg: f64,
    /// CritBlend 后有效口径总均值（敌减伤后；DPS 用）。
    pub total_hit_avg_mitigated: f64,
    /// 是否走了等价性短路（两腿聚合输入逐位相同 → 旧单因子公式）。
    pub short_circuited: bool,
}

/// 单腿聚合产物。
struct Leg {
    components: Vec<DamageComponent>,
    /// (type, lucky 折算后 avg)；顺序与 `components` 一致。
    avgs: Vec<(DamageType, f64)>,
    /// lucky 折算前的分量 avg（敌方护甲减伤 raw_hit，保守口径见模块文档）。
    raw_avgs: Vec<f64>,
}

/// 暴击/非暴击双 pass 主入口（在每个 hand pass 内调用——2×2 嵌套：hand 外层、
/// crit 内层，与 PoB2 同构）。
///
/// `mitigation(pass_cfg, type, raw_hit)` = 敌方对该类型的受伤总乘子（仅
/// `mode_effective` 下被调用；由 offence 闭包提供，保持 `enemy_damage_multiplier`
/// 私有）。`scaled` = W-C1 乘区（T3 契约 2，两腿共用、暴击腿额外 ×CritMultiplier）。
#[allow(clippy::too_many_arguments)]
pub fn run_crit_passes<F>(
    db: &ModDb,
    cfg: &CalcConfig,
    base_hit_min: f64,
    base_hit_max: f64,
    crit: &CritOutcome,
    scaled: &ScaledDamage,
    mode_effective: bool,
    mitigation: F,
) -> CritPassOutput
where
    F: Fn(&CalcConfig, DamageType, f64) -> f64,
{
    // vendor `:3979`：两轮 pass 都显式置条件（绝对值，非叠加）。
    let cfg_crit = cfg.clone().with_condition("CriticalStrike", true);
    let cfg_hit = cfg.clone().with_condition("CriticalStrike", false);

    // 各腿未缩放聚合（转换链 + canDeal）——只算两次，短路判据直接复用。
    let mut hit_unscaled = calculate_components(db, &cfg_hit, base_hit_min, base_hit_max);
    apply_can_deal(&mut hit_unscaled, db, &cfg_hit);
    let mut crit_unscaled = calculate_components(db, &cfg_crit, base_hit_min, base_hit_max);
    apply_can_deal(&mut crit_unscaled, db, &cfg_crit);

    // 等价性短路判据：未缩放分量逐位相同 + per-type lucky 几率相同
    // ⇔ 无 CriticalStrike 条件词条参与伤害聚合 / canDeal / lucky。
    let short_circuit = hit_unscaled == crit_unscaled
        && hit_unscaled.iter().all(|component| {
            lucky_hit_chance(db, &cfg_hit, component.damage_type, false)
                == lucky_hit_chance(db, &cfg_crit, component.damage_type, true)
        });

    let base_mult = all_mult(scaled, &AllMultExtras::default());
    // 暴击腿额外乘区（`:4028-4032` pass==1 allMult ×= CritMultiplier）。
    let crit_leg_mult = base_mult * crit.multiplier;

    let non_crit = finish_leg(db, &cfg_hit, hit_unscaled, base_mult, false);
    let crit_leg = finish_leg(db, &cfg_crit, crit_unscaled, crit_leg_mult, true);

    let c = crit.chance;

    // Stored 族（pre-resist，`:4047-4057`）。
    let stored_crit_avg = crit_leg.avgs.clone();
    let stored_hit_avg = non_crit.avgs.clone();
    let stored_combined_avg: Vec<(DamageType, f64)> = non_crit
        .avgs
        .iter()
        .zip(crit_leg.avgs.iter())
        .map(|((ty, hit_avg), (_, crit_avg))| (*ty, crit_avg * c + hit_avg * (1.0 - c)))
        .collect();
    // Stored min/max 族（`:4050-4056`）：非暴击腿区间 + 同类型暴击腿区间（暴击腿
    // 缺该类型按 0 折入，vendor `or 0` 语义；min/max 不做 lucky 折算）。
    let stored_ranges: Vec<StoredDamageRange> = non_crit
        .components
        .iter()
        .map(|component| {
            let crit_component = crit_leg
                .components
                .iter()
                .find(|cc| cc.damage_type == component.damage_type);
            StoredDamageRange {
                damage_type: component.damage_type,
                hit_min: component.min,
                hit_max: component.max,
                crit_min: crit_component.map_or(0.0, |cc| cc.min),
                crit_max: crit_component.map_or(0.0, |cc| cc.max),
            }
        })
        .collect();

    let (total_hit_avg, total_hit_avg_mitigated) = if short_circuit {
        // 旧单因子公式（取整顺序逐字节复刻替换前 offence.rs 实现）：
        // total = round(Σavg × crit.effect)，I5 恒等保证与真 blend 数学相同。
        let player_side: f64 = leg_total(&non_crit);
        let mitigated: f64 = if mode_effective {
            // 减伤侧**不能**走「单腿减伤 × crit.effect」：敌方护甲减伤依赖
            // 单次击中量（vendor pass1 用暴击后的击中量算 DR，暴击更大 → DR
            // 更小），raw 依赖使 I5 恒等在减伤维度不成立。短路下暴击腿 =
            // 非暴击腿 × crit.multiplier，据此按 vendor `:4395` 分腿减伤后
            // blend；减伤对 raw 无依赖（纯抗性/受伤链）时数学上退化为
            // 旧公式，逐值不变。
            let hit_leg = leg_total_mitigated(&non_crit, &cfg_hit, &mitigation);
            let crit_leg_mitigated: f64 = non_crit
                .avgs
                .iter()
                .zip(non_crit.raw_avgs.iter())
                .map(|((ty, avg), raw)| {
                    avg * crit.multiplier
                        * mitigation(&cfg_crit, *ty, raw * crit.multiplier)
                })
                .sum();
            hit_leg * (1.0 - c) + crit_leg_mitigated * c
        } else {
            player_side * crit.effect
        };
        (round(player_side * crit.effect), round(mitigated))
    } else {
        // 真双腿 blend（`:4395`）：分腿过敌方减伤后按 c 加权。
        let blend = |hit: f64, crit_v: f64| hit * (1.0 - c) + crit_v * c;
        let player_side = blend(leg_total(&non_crit), leg_total(&crit_leg));
        let mitigated = if mode_effective {
            blend(
                leg_total_mitigated(&non_crit, &cfg_hit, &mitigation),
                leg_total_mitigated(&crit_leg, &cfg_crit, &mitigation),
            )
        } else {
            player_side
        };
        (round(player_side), round(mitigated))
    };

    CritPassOutput {
        non_crit_components: non_crit.components,
        crit_components: crit_leg.components,
        stored_crit_avg,
        stored_hit_avg,
        stored_combined_avg,
        stored_ranges,
        total_hit_avg,
        total_hit_avg_mitigated,
        short_circuited: short_circuit,
    }
}

/// 完成一条腿：×allMult（`:4033-4034`）→ lucky 折 avg（W-C2，`:4035-4046`）。
/// `mult == 1.0` 时跳过缩放，保持与替换前实现逐位一致。
fn finish_leg(
    db: &ModDb,
    pass_cfg: &CalcConfig,
    mut components: Vec<DamageComponent>,
    mult: f64,
    is_crit_pass: bool,
) -> Leg {
    if mult != 1.0 {
        for component in &mut components {
            component.min *= mult;
            component.max *= mult;
        }
    }
    let mut avgs = Vec::with_capacity(components.len());
    let mut raw_avgs = Vec::with_capacity(components.len());
    for component in &components {
        let lucky = lucky_hit_chance(db, pass_cfg, component.damage_type, is_crit_pass);
        avgs.push((component.damage_type, component.avg_with_lucky(lucky)));
        raw_avgs.push(component.avg());
    }
    Leg {
        components,
        avgs,
        raw_avgs,
    }
}

fn leg_total(leg: &Leg) -> f64 {
    leg.avgs.iter().map(|(_, avg)| avg).sum()
}

fn leg_total_mitigated<F>(leg: &Leg, pass_cfg: &CalcConfig, mitigation: &F) -> f64
where
    F: Fn(&CalcConfig, DamageType, f64) -> f64,
{
    leg.avgs
        .iter()
        .zip(leg.raw_avgs.iter())
        .map(|((ty, avg), raw)| avg * mitigation(pass_cfg, *ty, *raw))
        .sum()
}
