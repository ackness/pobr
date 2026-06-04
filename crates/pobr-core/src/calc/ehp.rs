//! EHP 与 max hit（09-player-facing §3.2、§4.3）。
//!
//! 每种伤害类型按其有效血池与减伤算出可承受的单次最大命中；EHP 取最低 max hit
//! （短板决定生存）。物理走护甲 + 固定 PDR，元素/混沌走抗性；ES 对混沌按半效计入。
//!
//! 注：armour reduction 需要 incoming hit 估计值（`reference_hit`），无真实敌人伤害时
//! 用 display 基准；EHP 加权口径（lowest vs 类型加权）取 lowest，标注待 product 决策。

use crate::TraceGraph;
use crate::TraceOperation;
use crate::calc::defence::armour_reduction;

use super::round;

/// 各伤害类型的最终减伤参数。
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ResistanceSuite {
    /// 物理减伤固定加成（来自护甲以外的来源），fraction [0,1)。
    pub physical_pdr: f64,
    pub fire: f64,
    pub cold: f64,
    pub lightning: f64,
    pub chaos: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EhpResult {
    pub life: f64,
    pub es: f64,
    pub mana: f64,
    pub physical_max_hit: f64,
    pub fire_max_hit: f64,
    pub cold_max_hit: f64,
    pub lightning_max_hit: f64,
    pub chaos_max_hit: f64,
    pub total_ehp: f64,
}

/// 元素/混沌承受比例：`1 - resist%/100`，下限 0。
fn resist_taken_fraction(resist_pct: f64) -> f64 {
    (1.0 - resist_pct / 100.0).max(0.0)
}

/// 物理承受比例：`1 - (pdr_flat + 护甲减伤)`，clamp 到 [0.1, 1.0]（PoE 物理减伤上限 90%）。
pub fn physical_taken_fraction(pdr_flat: f64, armour: f64, reference_hit: f64) -> f64 {
    let total_reduction = pdr_flat + armour_reduction(armour, reference_hit);
    (1.0 - total_reduction).clamp(0.1, 1.0)
}

/// 元素/混沌类型的最大可承受命中：`pool / (1 - resist%/100)`。
pub fn max_hit_for_type(pool: f64, resist_pct: f64) -> f64 {
    let taken = resist_taken_fraction(resist_pct);
    if taken <= 0.0 {
        f64::INFINITY
    } else {
        round(pool / taken)
    }
}

/// 物理最大可承受命中：`pool / physical_taken_fraction`。
pub fn physical_max_hit(pool: f64, pdr_flat: f64, armour: f64, reference_hit: f64) -> f64 {
    let taken = physical_taken_fraction(pdr_flat, armour, reference_hit);
    round(pool / taken)
}

/// 元素血池（life + es）。
fn elemental_pool(life: f64, es: f64) -> f64 {
    life + es
}

/// 混沌血池（ES 对混沌半效：life + es*0.5）。
fn chaos_pool(life: f64, es: f64) -> f64 {
    life + es * 0.5
}

/// Chaos Inoculation (CI) keystone 选项。
/// 出处：agent-docs/active-defences.md §五 Keystone 表；
///       PoB2 CalcDefence.lua：CI → maxLife=1，ES 作为生命池，混沌伤害免疫（chaos_resist = 100%）。
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct EhpOptions {
    /// Chaos Inoculation：最大生命变 1，ES 作生命池（`es` 用于所有伤害池），混沌伤害免疫。
    pub chaos_inoculation: bool,
}

/// 计算 EHP 与各类型 max hit。`reference_hit` 为物理护甲减伤的 incoming hit 估计基准。
pub fn calc_ehp(
    life: f64,
    es: f64,
    mana: f64,
    resistances: &ResistanceSuite,
    armour: f64,
    reference_hit: f64,
) -> EhpResult {
    calc_ehp_with_opts(
        life,
        es,
        mana,
        resistances,
        armour,
        reference_hit,
        EhpOptions::default(),
    )
}

/// `calc_ehp` 的完整版本，支持 Chaos Inoculation 等 keystone 选项。
///
/// **Bug#10 修正（ehp-chaos-inoculation-wrong）**：
/// CI build 中 ES 成为生命池（`life_pool = es`），混沌伤害免疫（`chaos_max_hit = ∞`）。
/// 出处：agent-docs/active-defences.md §五 Keystone：
///   `Chaos Inoculation: 最大生命变 1；免疫混沌伤害与流血`。
pub fn calc_ehp_with_opts(
    life: f64,
    es: f64,
    mana: f64,
    resistances: &ResistanceSuite,
    armour: f64,
    reference_hit: f64,
    opts: EhpOptions,
) -> EhpResult {
    let (effective_life, effective_es) = if opts.chaos_inoculation {
        // CI：life = 1（已在 actor 层处理），ES 用作所有伤害池
        // 这里把 es 放入 effective_life 以复用 elemental_pool/chaos_pool 函数
        (es, 0.0)
    } else {
        (life, es)
    };
    let ele_pool = elemental_pool(effective_life, effective_es);
    let ref_hit = if reference_hit > 0.0 {
        reference_hit
    } else {
        ele_pool.max(1.0)
    };

    let physical_max_hit = physical_max_hit(ele_pool, resistances.physical_pdr, armour, ref_hit);
    let fire_max_hit = max_hit_for_type(ele_pool, resistances.fire);
    let cold_max_hit = max_hit_for_type(ele_pool, resistances.cold);
    let lightning_max_hit = max_hit_for_type(ele_pool, resistances.lightning);
    // CI：混沌伤害免疫 → 无限大 max hit
    let chaos_max_hit = if opts.chaos_inoculation {
        f64::INFINITY
    } else {
        max_hit_for_type(chaos_pool(effective_life, effective_es), resistances.chaos)
    };

    let total_ehp = [
        physical_max_hit,
        fire_max_hit,
        cold_max_hit,
        lightning_max_hit,
        chaos_max_hit,
    ]
    .into_iter()
    .filter(|v| v.is_finite())
    .fold(f64::INFINITY, f64::min);
    let total_ehp = if total_ehp.is_finite() {
        round(total_ehp)
    } else {
        ele_pool
    };

    EhpResult {
        life,
        es,
        mana,
        physical_max_hit,
        fire_max_hit,
        cold_max_hit,
        lightning_max_hit,
        chaos_max_hit,
        total_ehp,
    }
}

/// `calc_ehp` 的追踪版本：为火与物理 max hit 各记录一个 Mitigate 节点，total 记录 Clamp。
pub fn calc_ehp_traced(
    life: f64,
    es: f64,
    mana: f64,
    resistances: &ResistanceSuite,
    armour: f64,
    reference_hit: f64,
    trace: &mut TraceGraph,
) -> EhpResult {
    let result = calc_ehp(life, es, mana, resistances, armour, reference_hit);

    let fire_node = trace.add_node(
        "fire max hit",
        result.fire_max_hit,
        TraceOperation::Mitigate,
    );
    let phys_node = trace.add_node(
        "physical max hit",
        result.physical_max_hit,
        TraceOperation::Mitigate,
    );
    let total_node = trace.add_node(
        "total EHP (lowest)",
        result.total_ehp,
        TraceOperation::Clamp,
    );
    trace.add_edge(fire_node, total_node);
    trace.add_edge(phys_node, total_node);

    result
}
