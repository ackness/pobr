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

/// 减伤上限（fraction，默认 0.9）。对齐 PoB2
/// `output.DamageReductionMax = Max('DamageReductionMax') or DamageReductionCap(=90)`
/// （CalcDefence.lua:1862）。`+Maximum Damage Reduction` 词条可提升此值。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DamageReductionCaps {
    /// 全局减伤上限 fraction（默认 0.9 = 90%）。
    pub global: f64,
}

impl Default for DamageReductionCaps {
    fn default() -> Self {
        Self { global: 0.9 }
    }
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
    physical_taken_fraction_overwhelm(pdr_flat, armour, reference_hit, 0.0)
}

/// 物理承受比例，含敌人**压制**（overwhelm，fraction）：先按 90% 上限算总减伤，再被
/// overwhelm 削减（提高承受）。PoB2：armour 1e9（90% DR）+ 15% overwhelm → 75% DR → 承受 0.25。
pub fn physical_taken_fraction_overwhelm(
    pdr_flat: f64,
    armour: f64,
    reference_hit: f64,
    overwhelm: f64,
) -> f64 {
    physical_taken_fraction_overwhelm_cap(pdr_flat, armour, reference_hit, overwhelm, 0.9)
}

/// 同 [`physical_taken_fraction_overwhelm`]，减伤上限改为可变 `dr_max`（fraction）。
/// 对齐 PoB2：armour+flat 求和后 clamp 到 `DamageReductionMax`（CalcDefence.lua:396）。
pub fn physical_taken_fraction_overwhelm_cap(
    pdr_flat: f64,
    armour: f64,
    reference_hit: f64,
    overwhelm: f64,
    dr_max: f64,
) -> f64 {
    let reduction = (pdr_flat + armour_reduction(armour, reference_hit)).clamp(0.0, dr_max);
    (1.0 - (reduction - overwhelm)).clamp(0.0, 1.0)
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

/// 物理最大可承受命中。
///
/// 护甲减伤随击中大小变化（`armour/(armour+10*hit)`），故最大承受击中 `H` 须**自洽**：
/// `H * taken(H) = pool`（被该击中打中、过减伤后恰好等于血池）。PoB2 同此口径
/// （`takenHitFromDamage(MaxHit) == pool`）。用定点迭代求解（`taken` 随 `H` 单调，收敛快）；
/// 无护甲时 `taken` 与 `H` 无关，一步收敛 → 退化为 `pool/taken`。`reference_hit` 作初值。
pub fn physical_max_hit(pool: f64, pdr_flat: f64, armour: f64, reference_hit: f64) -> f64 {
    physical_max_hit_overwhelm(pool, pdr_flat, armour, reference_hit, 0.0)
}

/// 物理最大承受击中（含敌人 overwhelm）。同 [`physical_max_hit`] 的自洽迭代，承受比例改用
/// [`physical_taken_fraction_overwhelm`]。
pub fn physical_max_hit_overwhelm(
    pool: f64,
    pdr_flat: f64,
    armour: f64,
    reference_hit: f64,
    overwhelm: f64,
) -> f64 {
    physical_max_hit_overwhelm_cap(pool, pdr_flat, armour, reference_hit, overwhelm, 0.9)
}

/// 同 [`physical_max_hit_overwhelm`]，减伤上限改为可变 `dr_max`（fraction）。
pub fn physical_max_hit_overwhelm_cap(
    pool: f64,
    pdr_flat: f64,
    armour: f64,
    reference_hit: f64,
    overwhelm: f64,
    dr_max: f64,
) -> f64 {
    let mut hit = reference_hit.max(pool).max(1.0);
    for _ in 0..50 {
        let taken = physical_taken_fraction_overwhelm_cap(pdr_flat, armour, hit, overwhelm, dr_max);
        if taken <= 0.0 {
            return f64::INFINITY;
        }
        let next = pool / taken;
        if (next - hit).abs() < 1e-3 {
            hit = next;
            break;
        }
        hit = next;
    }
    round(hit)
}

/// 元素类型走护甲的最大承受击中（「Armour applies to <Element> instead of Physical」）：
/// 护甲减伤作用于**抗性前（raw）**伤害（PoB2 `armourReductionF(armour, RAW)`，
/// CalcDefence.lua:56/393/427/3626），与抗性层独立相乘。故 `taken = res_taken × (1 - armour_dr(H))`，
/// armour_dr 上限 90%。同样自洽迭代求 `H × taken(H) = pool`。
fn element_max_hit_with_armour(
    pool: f64,
    resist_pct: f64,
    armour: f64,
    reference_hit: f64,
    dr_max: f64,
) -> f64 {
    let res_taken = resist_taken_fraction(resist_pct);
    if res_taken <= 0.0 {
        return f64::INFINITY;
    }
    let mut hit = reference_hit.max(pool).max(1.0);
    for _ in 0..50 {
        // PoB2：armour DR 基于 RAW（抗性前）伤害，即迭代当前 hit H，而非 post-resist。
        // 减伤上限可变（默认 0.9），承受下限 = 1 - dr_max。
        let armour_part = (1.0 - armour_reduction(armour, hit)).clamp(1.0 - dr_max, 1.0);
        let taken = res_taken * armour_part;
        let next = pool / taken;
        if (next - hit).abs() < 1e-3 {
            hit = next;
            break;
        }
        hit = next;
    }
    round(hit)
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
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EhpOptions {
    /// Chaos Inoculation：最大生命变 1，ES 作生命池（`es` 用于所有伤害池），混沌伤害免疫。
    pub chaos_inoculation: bool,
    /// 敌人物理压制（overwhelm，fraction）：削减玩家物理总减伤（提高承受）。
    pub physical_overwhelm: f64,
    /// 「Armour applies to <Element> instead of Physical」：火/冰/电是否改走护甲减伤；
    /// 任一为真时物理不再吃护甲（仅 PDR）。对应 PoB2 同名词条。
    pub armour_applies_to_element: [bool; 3],
    /// 减伤上限（可被 `+Maximum Damage Reduction` 词条提升）。默认 90%。
    pub damage_reduction_caps: DamageReductionCaps,
}

impl Default for EhpOptions {
    fn default() -> Self {
        Self {
            chaos_inoculation: false,
            physical_overwhelm: 0.0,
            armour_applies_to_element: [false; 3],
            damage_reduction_caps: DamageReductionCaps::default(),
        }
    }
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

    // 「Armour applies to <Element> instead of Physical」：物理改吃护甲与否取决于是否有重定向。
    let any_redirect = opts.armour_applies_to_element.iter().any(|&x| x);
    let phys_armour = if any_redirect { 0.0 } else { armour };
    let dr_max = opts.damage_reduction_caps.global;
    let physical_max_hit = physical_max_hit_overwhelm_cap(
        ele_pool,
        resistances.physical_pdr,
        phys_armour,
        ref_hit,
        opts.physical_overwhelm,
        dr_max,
    );
    // 各元素：重定向时走护甲（抗性后），否则纯抗性。
    let elem_max_hit = |resist_pct: f64, idx: usize| -> f64 {
        if opts.armour_applies_to_element[idx] {
            element_max_hit_with_armour(ele_pool, resist_pct, armour, ref_hit, dr_max)
        } else {
            max_hit_for_type(ele_pool, resist_pct)
        }
    };
    let fire_max_hit = elem_max_hit(resistances.fire, 0);
    let cold_max_hit = elem_max_hit(resistances.cold, 1);
    let lightning_max_hit = elem_max_hit(resistances.lightning, 2);
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
