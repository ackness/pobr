//! EHP 与 max hit（09-player-facing §3.2、§4.3）。
//!
//! 每种伤害类型按其有效血池与减伤算出可承受的单次最大命中；EHP 取最低 max hit
//! （短板决定生存）。物理走护甲 + 固定 PDR，元素/混沌走抗性；ES 对混沌按半效计入。
//!
//! 注：armour reduction 需要 incoming hit 估计值（`reference_hit`），无真实敌人伤害时
//! 用 display 基准；EHP 加权口径（lowest vs 类型加权）取 lowest，标注待 product 决策。

use pobr_data::prelude::*;

use crate::TraceGraph;
use crate::TraceOperation;
use crate::calc::defence::armour_reduction;
use crate::rules::DefenceKeystones;
use crate::{CalcConfig, ModDb};

use super::env::Env;
use super::output::OutputTable;
use super::pool_damage::{
    PoolCtx, PoolState, TypedDamage, extend_total_hit_pool, reduce_pools, total_hit_pool_base,
};
use super::pool_setup::{
    MomHitPools, PoolBaseStats, build_pool_ctx, build_pool_state, mom_hit_pools,
};
use super::round;
use super::taken::{MitigationCtx, MitigationInputs, build_mitigation_ctx, taken_hit_from_damage};

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

// EHP PoB2 口径：vendor `CalcDefence.lua` 的 numberOfHitsToDie × 单击进伤。
//
// `total_ehp` 仍是旧 lowest-max-hit 口径；本节的值挂在 `total_ehp_pob2` /
// `*_max_hit_pob2` / `number_of_damaging_hits` / `number_of_mitigated_hits`。
// 本管线不挂 trace——归因只沿用 calc_ehp_traced 的 Mitigate/Clamp 节点。

/// per-type 数组下标序（= `DamageType as usize`，与 pool_damage/taken 同约定）。
const DAMAGE_TYPE_BY_INDEX: [DamageType; 5] = [
    DamageType::Physical,
    DamageType::Fire,
    DamageType::Cold,
    DamageType::Lightning,
    DamageType::Chaos,
];

/// vendor `round`（Modules/Common.lua：`m_floor(val + 0.5)`）。
fn vendor_round(value: f64) -> f64 {
    (value + 0.5).floor()
}

/// 伤害向量数乘（vendor `Damage[t] = DamageIn[t] × k` 形）。
fn scale_damage(damage: &TypedDamage, factor: f64) -> TypedDamage {
    TypedDamage {
        physical: damage.physical * factor,
        fire: damage.fire * factor,
        cold: damage.cold * factor,
        lightning: damage.lightning * factor,
        chaos: damage.chaos * factor,
    }
}

/// 敌人单击进伤 placeholder（vendor ConfigOptions.lua:1982-1996）：
/// `default = round(monsterDamageTable[lv] × ehp_base_damage_mult × DPSMult)`，
/// 物理/火/冰/电四类同值，chaos 再 `round(default / chaos_damage_div)`（除数
/// per-tier，None/Boss/Pinnacle = 2.5、Uber = 4，L1987/L2028/L2070/L2111）。
///
/// 数据全部入库：`monster_scaling.json::damage` + `enemy_presets.json`
/// （`ehp_base_damage_mult` / per-tier `dps_mult`/`chaos_damage_div`）。
/// per-type config 覆盖（`enemy<X>Damage` configInput）留config_interpreter。
/// 预设缺档（损坏数据）→ 全 0（消费侧 0 进伤 → 致死击数 ∞ 中性短路）。
pub fn enemy_damage_placeholder(
    constants: &RuntimeConstants,
    level: u32,
    tier: EnemyTier,
) -> TypedDamage {
    let presets = &constants.enemy_presets;
    let Some(preset) = presets.tier_for(tier) else {
        return TypedDamage::default();
    };
    let base = vendor_round(
        constants.monster_scaling.damage_at(level)
            * presets.ehp_base_damage_mult
            * preset.dps_mult.value(),
    );
    TypedDamage {
        physical: base,
        fire: base,
        cold: base,
        lightning: base,
        chaos: vendor_round(base / preset.chaos_damage_div),
    }
}

/// 敌人进伤装配结果（vendor CalcDefence.lua:2040-2168 敌伤段）。
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct EnemyDamageIn {
    /// per-type 终值 `<X>EnemyDamage = enemyDamage × enemyDamageMult × EnemyCritEffect`
    /// （:2137；敌方 Conversion :2098-2128 无词条来源、留）。
    pub damage: TypedDamage,
    /// `totalEnemyDamageIn = Σ enemyDamage`（mult/crit **之前**，:2136；
    /// TotalEHP 末端乘数 :3322）。
    pub total_in: f64,
    /// per-type `<X>EnemyDamageMult`（max-hit 末端除数，:3660/:3696）。
    pub mult_by_type: [f64; 5],
}

/// 敌人进伤装配（vendor CalcDefence.lua:2065-2137）：placeholder（setup_enemy 注入的
/// `Enemy<X>Damage` BASE）+ enemyDB `<X>Min`/`<X>Max` 词条均值（:2098）→ per-type ×
/// `enemyDamageMult`（`calcLib.mod(enemyDB, "Damage", "<X>Damage"[, "ElementalDamage"])`，
/// :2133）× `EnemyCritEffect`。
pub fn assemble_enemy_damage(
    enemy_db: &ModDb,
    cfg: &CalcConfig,
    crit_effect: f64,
) -> EnemyDamageIn {
    let mut result = EnemyDamageIn::default();
    let mut per_type = [0.0_f64; 5];
    for dtype in DAMAGE_TYPE_BY_INDEX {
        let i = dtype as usize;
        let name = type_prefix(dtype);
        // placeholder（setup_enemy 注入；无敌人配置的裸 Env 下为 0 → 中性）。
        let placeholder = enemy_db.sum(
            ModType::Base,
            cfg,
            &[ModName::from(format!("Enemy{name}Damage"))],
        );
        // :2098 敌方 Min/Max 词条均值。
        let min_max = (enemy_db.sum(ModType::Base, cfg, &[ModName::from(format!("{name}Min"))])
            + enemy_db.sum(ModType::Base, cfg, &[ModName::from(format!("{name}Max"))]))
            / 2.0;
        let enemy_damage = placeholder + min_max;
        // :2133 enemyDamageMult。
        let mut names = vec![
            ModName::from("Damage"),
            ModName::from(format!("{name}Damage")),
        ];
        if dtype.is_elemental() {
            names.push(ModName::from("ElementalDamage"));
        }
        let mult =
            (1.0 + enemy_db.sum(ModType::Inc, cfg, &names) / 100.0) * enemy_db.more(cfg, &names);
        result.mult_by_type[i] = mult;
        result.total_in += enemy_damage;
        per_type[i] = enemy_damage * mult * crit_effect;
    }
    result.damage = TypedDamage {
        physical: per_type[0],
        fire: per_type[1],
        cold: per_type[2],
        lightning: per_type[3],
        chaos: per_type[4],
    };
    result
}

/// DamageType → 词条名前缀。
fn type_prefix(dtype: DamageType) -> &'static str {
    match dtype {
        DamageType::Physical => "Physical",
        DamageType::Fire => "Fire",
        DamageType::Cold => "Cold",
        DamageType::Lightning => "Lightning",
        DamageType::Chaos => "Chaos",
    }
}

/// 敌人暴击效果（EHP 口径，vendor CalcDefence.lua:2065-2071）：
///
/// ```text
/// chance = clamp(placeholder(5) × (1 + (玩家 EnemyCritChance INC + 敌 CritChance INC)/100)
///                × (1 − ConfiguredEvadeChance/100), 0, 100)   （NeverCrit→0 / AlwaysCrit→100）
/// damage = max((placeholder(30) + 敌 CritMultiplier BASE) × (1 + 敌 CritMultiplier INC/100), 0)
/// effect = 1 + chance/100 × damage/100 × (1 − CritExtraDamageReduction/100)
/// ```
///
/// placeholder 走 `enemy_presets.json`（`default_enemy_crit_chance` /
/// `default_enemy_crit_damage_bonus`；configInput 覆盖留）。
pub fn enemy_crit_effect_ehp(
    player_db: &ModDb,
    enemy_db: &ModDb,
    cfg: &CalcConfig,
    configured_evade_pct: f64,
    crit_extra_reduction_pct: f64,
) -> f64 {
    let presets = &cfg.constants.enemy_presets;
    let mut chance = if enemy_db.flag(cfg, ModName::from("NeverCrit")) {
        0.0
    } else if enemy_db.flag(cfg, ModName::from("AlwaysCrit")) {
        100.0
    } else {
        (presets.default_enemy_crit_chance
            * (1.0
                + player_db.sum(ModType::Inc, cfg, &[ModName::from("EnemyCritChance")]) / 100.0
                + enemy_db.sum(ModType::Inc, cfg, &[ModName::from("CritChance")]) / 100.0)
            * (1.0 - configured_evade_pct / 100.0))
            .clamp(0.0, 100.0)
    };
    // :2066 EnemyUnluckyCrit → 两次取劣幂。
    if player_db.flag(cfg, ModName::from("EnemyUnluckyCrit")) {
        chance = chance / 100.0 * chance;
    }
    let crit_damage = ((presets.default_enemy_crit_damage_bonus
        + enemy_db.sum(ModType::Base, cfg, &[ModName::from("CritMultiplier")]))
        * (1.0 + enemy_db.sum(ModType::Inc, cfg, &[ModName::from("CritMultiplier")]) / 100.0))
        .max(0.0);
    1.0 + chance / 100.0 * (crit_damage / 100.0) * (1.0 - crit_extra_reduction_pct / 100.0)
}

/// per-type 承受命中（vendor「panel 路径」：taken-as shift 聚合 :2214-2227 +
/// per-type 减伤 :2326-2444）。返回 `(per-type TakenHit, per-type 面板 DR%)`。
///
/// 与 [`taken_hit_from_damage`]（单源 raw 入口，:422-455）的差异：本函数对**全部
/// 源类型**的进伤先做 shift 聚合再 per-dest 减伤（takenFlat 只加一次、armour DR
/// 以聚合后伤害求值）——与 vendor `<X>TakenDamage → <X>TakenHit` 链一致；
/// `:2442` 不取整（takenHitFromDamage 的 round 是另一入口）。
pub fn taken_hit_per_type(
    enemy_damage: &TypedDamage,
    mit: &MitigationCtx,
) -> (TypedDamage, [f64; 5]) {
    // 1. taken-as shift 聚合（:2214-2227）：taken[dst] = Σ_src enemyDamage[src] × shift[src][dst]。
    let mut taken = [0.0_f64; 5];
    for src in DAMAGE_TYPE_BY_INDEX {
        let s = src as usize;
        let raw = enemy_damage.get(src);
        if raw <= 0.0 {
            continue;
        }
        for (d, taken_slot) in taken.iter_mut().enumerate() {
            *taken_slot += raw * mit.shift[s][d];
        }
    }
    // 2. per-type 减伤（:2371-2383 + :2442）。
    let mut hits = [0.0_f64; 5];
    let mut dr_pct = [0.0_f64; 5];
    for i in 0..5 {
        let damage = taken[i];
        // :2402 armourReduct = min(drMax, armourReduction(effArmour, damage))——
        // 注意这是**取整**变体（Common.lua round=floor(x+0.5)；armourReductionF 才是
        // 小数变体，仅 takenHitFromDamage/:437 用）。golden PhysicalDamageReduction
        // 恒为整数即由此来。
        let armour_dr =
            vendor_round(armour_reduction(mit.effective_applied_armour[i], damage) * 100.0)
                .min(mit.dr_max_pct[i]);
        // :2382 totalReduct = min(drMax, armourReduct + flatDR)。
        let total_dr = (armour_dr + mit.flat_dr_pct[i]).min(mit.dr_max_pct[i]);
        // :2383 reductMult = 1 − clamp(totalReduct − overwhelm, 0, drMax)/100。
        let reduct_mult =
            1.0 - (total_dr - mit.overwhelm_pct[i]).clamp(0.0, mit.dr_max_pct[i]) / 100.0;
        dr_pct[i] = 100.0 - reduct_mult * 100.0;
        // :2442 TakenHit = max(damage × resMult × reductMult + takenFlat, 0) × afterReductionMulti。
        let base_mult = mit.resist_taken_multi[i] * reduct_mult;
        hits[i] = (damage * base_mult + mit.taken_flat[i]).max(0.0) * mit.after_reduction_multi[i];
    }
    (
        TypedDamage {
            physical: hits[0],
            fire: hits[1],
            cold: hits[2],
            lightning: hits[3],
            chaos: hits[4],
        },
        dr_pct,
    )
}

/// EHP 循环参数（vendor Data.lua:235-239 + :3094 的 LimitEHPSpeedup 收紧）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EhpLoopParams {
    /// 单击伤害上限（超限仍存活 → 致死击数 ∞）。`ehp_calc_max_damage`。
    pub max_damage: f64,
    /// 全递归共享的迭代预算。`ehp_calc_max_iterations`。
    pub max_iterations: f64,
    /// 递归加速因子。`ehp_calc_speed_up`（=8）；loss-prevention/recoup/gain 跟踪时
    /// 收紧为 [`LIMITED_EHP_SPEED_UP`]（vendor :3094 字面量 4——MoM/损失防止会把
    /// 多击坍缩成一击导致 eHP 跳变，故放慢加速）。
    pub speed_up: f64,
}

/// vendor CalcDefence.lua:3094：`speedUp = DamageIn["LimitEHPSpeedup"] and 4 or ehpCalcSpeedUp`。
const LIMITED_EHP_SPEED_UP: f64 = 4.0;

impl EhpLoopParams {
    /// 从注入常量包构造（`limit_speedup` = vendor `LimitEHPSpeedup`）。
    pub fn from_constants(constants: &RuntimeConstants, limit_speedup: bool) -> Self {
        let game = constants.game();
        Self {
            max_damage: game.ehp_calc_max_damage,
            max_iterations: game.ehp_calc_max_iterations,
            speed_up: if limit_speedup {
                LIMITED_EHP_SPEED_UP
            } else {
                game.ehp_calc_speed_up
            },
        }
    }
}

/// 致死所需命中数（vendor CalcDefence.lua:2979-3145 `numberOfHitsToDie` 逐行）。
///
/// 循环调 Track A [`reduce_pools`]；递归加速（`speed_up` 倍伤害从**满池**重算一次
/// 估出跳跃步长 :3105-3119）；overkill 小数折算（:3133-3135，仅顶层 cycles=1）；
/// `max_damage` / `max_iterations` 上限保证终止；`WardNotBreak && Σ伤害 < Ward` → ∞
/// （:2990）。GainWhenHit（格挡/压制回复钩子）本阶段置 0，
/// 接入后在本函数加恢复步（:3074-3090）。
pub fn number_of_hits_to_die(
    damage_in: &TypedDamage,
    pools_full: &PoolState,
    ctx: &PoolCtx,
    params: &EhpLoopParams,
) -> f64 {
    number_of_hits_to_die_tracked(damage_in, pools_full, ctx, params).0
}

/// 致死击数 + per-type recoupable 累计（13-G15 部分）。
///
/// vendor `DamageIn.TrackRecoupable` 路径（CalcDefence.lua:3232-3236 置位、
/// :3119-3123 把 `poolTable.damageTakenThatCanBeRecouped`（reducePoolsByDamage
/// :489/:537 在 allies 层之后、aegis/guard/ward/ES 之前记录的 per-type
/// damageRemainder）累计进 `<X>RecoupableDamageTaken`）。与 vendor 同口径：
/// **仅顶层（cycles==1）循环累计**——加速递归（cycles>1）置 TrackRecoupable=false
/// （:3046-3049）；顶层放大击（iterationMultiplier 缩放后的 hit）按缩放后伤害
/// 记入，与 vendor 行为一致。
pub fn number_of_hits_to_die_tracked(
    damage_in: &TypedDamage,
    pools_full: &PoolState,
    ctx: &PoolCtx,
    params: &EhpLoopParams,
) -> (f64, [f64; 5]) {
    let mut iterations = 0.0;
    let mut recoupable = [0.0_f64; 5];
    let hits = hits_to_die_inner(
        damage_in,
        pools_full,
        ctx,
        params,
        1.0,
        &mut iterations,
        &mut recoupable,
    );
    (hits, recoupable)
}

/// `numberOfHitsToDie` 递归本体（`cycles`/`iterations` 对应 vendor DamageIn 同名键，
/// iterations 跨递归共享预算）。
#[allow(clippy::too_many_arguments)]
fn hits_to_die_inner(
    damage_in: &TypedDamage,
    pools_full: &PoolState,
    ctx: &PoolCtx,
    params: &EhpLoopParams,
    cycles: f64,
    iterations: &mut f64,
    recoupable: &mut [f64; 5],
) -> f64 {
    // :2984-2994 进伤为 0 → ∞；WardNotBreak 且单击总伤低于 Ward → ∞。
    let per_hit_total = damage_in.total();
    if per_hit_total <= 0.0 {
        return f64::INFINITY;
    }
    if ctx.ward_not_break && pools_full.ward > 0.0 && per_hit_total < pools_full.ward {
        return f64::INFINITY;
    }
    // :2996-3000 非永续 ward 在加速递归（cycles>1）中清零（每击归零无法正确建模）。
    let mut pool = pools_full.clone();
    if !ctx.ward_not_break && cycles > 1.0 {
        pool.ward = 0.0;
    }

    let mut num_hits = 0.0_f64;
    let mut iteration_multiplier = 1.0_f64;
    let mut cycles_ran = false;
    let mut last_overkill = 0.0_f64;
    // :3063 while 主循环。
    while pool.life > 0.0 && *iterations < params.max_iterations {
        *iterations += 1.0;
        let hit = scale_damage(damage_in, iteration_multiplier);
        let after = reduce_pools(&pool, &hit, ctx);
        last_overkill = after.overkill;
        // F-4：recoupable 累计（仅顶层 cycles==1，vendor :3046-3049/:3119-3123）。
        if cycles <= 1.0 {
            for (acc, v) in recoupable.iter_mut().zip(after.recoupable_by_type) {
                *acc += v;
            }
        }
        pool = after.pools;
        // :3084 存活且单击伤害已超上限 → 视为可承受无限击。
        if pool.life > 0.0 && per_hit_total >= params.max_damage {
            return f64::INFINITY;
        }
        iteration_multiplier = 1.0;
        // :3095-3119 递归加速：speed_up 倍伤害从满池重算，估出可安全跳过的击数。
        if !cycles_ran && pool.life > 0.0 && *iterations < params.max_iterations {
            let accelerated = scale_damage(damage_in, params.speed_up);
            let recursive_hits = hits_to_die_inner(
                &accelerated,
                pools_full,
                ctx,
                params,
                cycles * params.speed_up,
                iterations,
                recoupable,
            );
            // :3112 递归已知无限存活 → 直接 ∞。
            if recursive_hits.is_infinite() {
                return f64::INFINITY;
            }
            iteration_multiplier = ((recursive_hits - 1.0) * params.speed_up - 1.0).max(1.0);
            cycles_ran = true;
        }
        num_hits += iteration_multiplier;
    }
    // :3133-3135 overkill 小数折算（仅顶层 cycles=1，避免破坏加速估计）。
    if pool.life <= 0.0 && cycles <= 1.0 {
        num_hits -= last_overkill / per_hit_total;
    }
    // :3137-3140 终检：总承受伤害超上限仍存活 → ∞。
    let damage_total = per_hit_total * num_hits;
    if pool.life >= 0.0 && damage_total >= params.max_damage {
        return f64::INFINITY;
    }
    // :3141-3143 NaN → 0。
    if num_hits.is_nan() {
        return 0.0;
    }
    num_hits.max(0.0)
}

/// per-type TotalHitPool（vendor :2942-2960 MoM/ES 基底 + :3540-3596 ward/aegis/
/// guard/allies 扩展层；Track A 原语 [`total_hit_pool_base`] / [`extend_total_hit_pool`]）。
pub fn total_hit_pools(
    mom: &MomHitPools,
    energy_shield_recovery_cap: f64,
    pools: &PoolState,
    ctx: &PoolCtx,
) -> [f64; 5] {
    let mut out = [0.0_f64; 5];
    for dtype in DAMAGE_TYPE_BY_INDEX {
        let i = dtype as usize;
        let base = total_hit_pool_base(
            dtype,
            mom.hit_pool_by_type[i],
            energy_shield_recovery_cap,
            ctx,
        );
        out[i] = extend_total_hit_pool(base, dtype, pools, ctx);
    }
    out
}

/// 新口径 max-hit 求解输入（per-actor 一次整备，逐类型复用）。
#[derive(Debug, Clone, Copy)]
pub struct MaxHitInputs<'a> {
    pub mit: &'a MitigationCtx,
    /// 满池快照（conversion smoothing 的 reduce_pools 基准，vendor :3670 传 nil = 满池）。
    pub pools_full: &'a PoolState,
    pub ctx: &'a PoolCtx,
    /// per-type TotalHitPool（[`total_hit_pools`] 产出）。
    pub total_hit_pool: [f64; 5],
    /// 护甲系数（`armour_ratio`，Data.lua:193）。
    pub armour_ratio: f64,
    /// 多转换平滑迭代上限（`max_hit_smoothing_passes`，Data.lua:241）。
    pub smoothing_passes: u32,
}

/// 新口径 per-type 最大承受命中（vendor CalcDefence.lua:3601-3697）。
///
/// 对 `shift[dtype]` 的每个转换目标独立求「恰好打穿该目标 TotalHitPool 的 RAW」：
/// - `convert ≤ 0`：takenFlat 单独判定（:3611-3613）；
/// - 无护甲且全额转换：闭式解（:3614-3617）；
/// - 其余：vendor 二次方程解（:3608-3641——armour DR 随击中大小变化的自洽条件
///   `takenHit(RAW) = TotalHitPool` 的代数解，与定点迭代数学等价），再以
///   noDR/maxDR 上下界 clamp + floor；
/// - `partMin = min(各目标)`；存在部分转换时走平滑迭代（:3663-3692，
///   [`taken_hit_from_damage`] + [`reduce_pools`] 实测 overkill 收敛）；
/// - 末端 `round(partMin / enemyDamageMult)`（:3660/:3696）。
pub fn max_hit_pob2(dtype: DamageType, inputs: &MaxHitInputs, enemy_damage_mult: f64) -> f64 {
    let mit = inputs.mit;
    let src = dtype as usize;
    let mut part_min = f64::INFINITY;
    let mut use_smoothing = false;
    for conv in DAMAGE_TYPE_BY_INDEX {
        let c = conv as usize;
        let convert = mit.shift[src][c];
        let taken_flat = mit.taken_flat[c];
        if convert <= 0.0 && taken_flat == 0.0 {
            continue;
        }
        let eff_armour = mit.effective_applied_armour[c];
        let total_pool = inputs.total_hit_pool[c];
        // :3607 totalTakenMulti = AfterReductionTakenHitMulti ×(1−VAA)（VAA 无来源 → 1）。
        let total_taken_multi = mit.after_reduction_multi[c];
        let resist_mult = mit.resist_taken_multi[c];
        let hit_taken = if convert <= 0.0 {
            // :3611-3613 仅 takenFlat：flat 即打穿池 → 0，否则该目标不约束（∞）。
            let taken_without_incoming = taken_flat.max(0.0) * total_taken_multi;
            if taken_without_incoming >= total_pool {
                0.0
            } else {
                f64::INFINITY
            }
        } else if total_taken_multi <= 0.0 {
            // 承受乘数 0（如 CI 的 ChaosDamageTaken MORE −100）→ 免疫：vendor 在
            // Lua 中 x/0 = inf 自然得 ∞；此处显式短路避免 0×∞ 的 NaN 分支。
            f64::INFINITY
        } else if eff_armour == 0.0 && convert >= 1.0 {
            // :3614-3617 无护甲 DR 的简化闭式（此时面板 DR = clamp(min(drMax, flat) − ow)）。
            let dr_pct = (mit.flat_dr_pct[c].min(mit.dr_max_pct[c]) - mit.overwhelm_pct[c])
                .clamp(0.0, mit.dr_max_pct[c]);
            let dr_multi = resist_mult * (1.0 - dr_pct / 100.0);
            (total_pool / convert / dr_multi - taken_flat).max(0.0) / total_taken_multi
        } else {
            // :3620-3641 二次方程解 + noDR/maxDR 边界 + floor。
            let flat_dr = mit.flat_dr_pct[c] / 100.0;
            let overwhelm = mit.overwhelm_pct[c];
            let one_minus_flat_plus_ow = 1.0 - flat_dr + overwhelm / 100.0;
            let hp_term = (total_pool / total_taken_multi - taken_flat) / (convert * resist_mult);
            let a = inputs.armour_ratio * convert * one_minus_flat_plus_ow;
            let b = eff_armour * one_minus_flat_plus_ow
                - eff_armour
                - hp_term * inputs.armour_ratio * convert;
            let c_term = -hp_term * eff_armour;
            let raw = if a != 0.0 {
                ((b * b - 4.0 * a * c_term).max(0.0).sqrt() - b) / (2.0 * a)
            } else {
                f64::INFINITY
            };
            // :3637-3639 上下界（无 DR / 满 DR）；上限按**源类型** drMax（:3638 vendor 同）。
            let no_dr_max_hit = total_pool / convert / resist_mult / total_taken_multi
                * (1.0 - taken_flat * total_taken_multi / total_pool);
            let max_dr_max_hit = no_dr_max_hit / (1.0 - (mit.dr_max_pct[src] - overwhelm) / 100.0);
            // :3641 部分转换时启用平滑（仅二次方程分支置位，vendor 同）。
            use_smoothing = use_smoothing || (convert - 1.0).abs() > f64::EPSILON;
            raw.min(max_dr_max_hit).max(no_dr_max_hit).floor()
        };
        part_min = part_min.min(hit_taken);
    }

    if part_min.is_infinite() {
        return f64::INFINITY;
    }
    if !use_smoothing {
        // :3696 无转换：直接折算敌伤乘数。
        return vendor_round(part_min / enemy_damage_mult);
    }
    // :3663-3692 conversion smoothing：以 partMin 起步，实测 takenHit → reducePools
    // 的 overkill，按 overkill 比例逐步逼近（|overkill| < 1 收敛）。
    let mut pass_incoming = part_min;
    let mut previous_overkill = f64::NAN;
    for n in 1..=inputs.smoothing_passes {
        let (_, parts) = taken_hit_from_damage(pass_incoming, dtype, mit);
        let pass_damage = TypedDamage {
            physical: parts[0],
            fire: parts[1],
            cold: parts[2],
            lightning: parts[3],
            chaos: parts[4],
        };
        let pass_pools = reduce_pools(inputs.pools_full, &pass_damage, inputs.ctx);
        let pass_overkill = pass_pools.overkill - pass_pools.hit_pool_remaining;
        // :3672-3679 passRatio：各受击池的 (overkill+pool)/pool 取最大（≤0 → 1）。
        let mut pass_ratio = 0.0_f64;
        for conv in DAMAGE_TYPE_BY_INDEX {
            let c = conv as usize;
            if mit.shift[src][c] > 0.0 || mit.taken_flat[c] != 0.0 {
                let part_pool = inputs.total_hit_pool[c];
                if part_pool > 0.0 {
                    pass_ratio = pass_ratio.max((pass_overkill + part_pool) / part_pool);
                }
            }
        }
        if pass_ratio <= 0.0 {
            pass_ratio = 1.0;
        }
        // :3682-3686 步长：相邻两轮 overkill 比值（cap 2）决定调整方向与幅度。
        let mut step_size = 1.0_f64;
        if n > 1 && previous_overkill != 0.0 && !previous_overkill.is_nan() {
            step_size = ((pass_overkill - previous_overkill) / previous_overkill)
                .abs()
                .min(2.0);
        }
        let step_adjust = if step_size > 1.0 {
            -pass_overkill / step_size
        } else if n > 1 {
            -pass_overkill * step_size
        } else {
            0.0
        };
        previous_overkill = pass_overkill;
        pass_incoming = (pass_incoming + step_adjust) / pass_ratio.sqrt();
        if pass_overkill < 1.0 && pass_overkill > -1.0 {
            break;
        }
    }
    vendor_round(pass_incoming / enemy_damage_mult)
}

/// 四分型「未被命中」几率（vendor CalcDefence.lua:2018-2026）。
///
/// PoE2 无 dodge（vendor 该乘项恒 1），specificTypeAvoidance 无来源 → AvoidProjectiles
/// 计入 Projectile/SpellProjectile 分型。`average_evade` 为 :2025 原式
/// （melee/proj 取 Evade、spell/spellProj 取 NotHit——vendor 字面语义照抄）。
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct NotHitSuite {
    pub melee: f64,
    pub projectile: f64,
    pub spell: f64,
    pub spell_projectile: f64,
    /// `AverageNotHitChance`（四分型均值，damageCategory=Average 的 Configured 值）。
    pub average: f64,
    /// `AverageEvadeChance`（:2025；敌暴击几率的闪避折减项）。
    pub average_evade: f64,
}

/// 由防御面板输出（Track D/E 经 OutputTable 解耦的字段）合成 [`NotHitSuite`]。
/// D/E 未接线时各输入为 0 → 全部 NotHit = 0（中性）。
pub fn not_hit_suite(out: &OutputTable) -> NotHitSuite {
    let avoid_all = out.avoid_all_damage_from_hits / 100.0;
    let avoid_proj = out.avoid_projectile_damage / 100.0;
    let not_hit = |evade_pct: f64, with_proj: bool| -> f64 {
        let proj_factor = if with_proj { 1.0 - avoid_proj } else { 1.0 };
        100.0 - (1.0 - evade_pct / 100.0) * (1.0 - avoid_all) * proj_factor * 100.0
    };
    let melee = not_hit(out.melee_evade_chance, false);
    let projectile = not_hit(out.projectile_evade_chance, true);
    let spell = not_hit(out.spell_evade_chance, false);
    let spell_projectile = not_hit(out.spell_projectile_evade_chance, true);
    NotHitSuite {
        melee,
        projectile,
        spell,
        spell_projectile,
        average: (melee + projectile + spell + spell_projectile) / 4.0,
        average_evade: (out.melee_evade_chance
            + out.projectile_evade_chance
            + spell
            + spell_projectile)
            / 4.0,
    }
}

/// EHP PoB2 口径 fill（产出 / F-3 口径切换；perform `fill_mechanics` 末尾
/// 一行调用，须在 `fill_evade_stun` / `fill_defence_panels` **之后**——
/// not-hit/block/deflect 层读它们写入的 OutputTable 字段，未接线字段默认 0 → 中性 1.0）。
///
/// **F-3 口径切换**：
/// - `total_ehp` = 新口径（`mitigatedHits / (1−notHit) × totalEnemyDamageIn`，
///   CalcDefence.lua:3271/:3322）；旧 lowest-max-hit 值保留在
///   `total_ehp_lowest_max_hit`（perform 旧管线照常写入，不删码——revert 本函数
///   末尾的切换段即回旧口径）。
/// - `*_max_hit` = 新口径（TotalHitPool 池扩展层 + taken-as，:3540-3697）；
///   `*_max_hit_pob2` 保留为同值别名。
/// - `avoid_stun` / Stun 体系换**真值** totalTakenHit（per-hit taken 伤害，
///   :2444 聚合 → :2554-2557 ES 减半条件 / :2525-2643 阈值几率）——替换
///   Track E 接线期的 reference_hit 近似。
///
/// 返回值（13-G15 部分）：mitigated EHP 循环累计的 recoupable 伤害总量
/// （vendor `Σ <X>RecoupableDamageTaken`，:3347-3357）——perform 以此作 recoup
/// 面板速率的承伤基数（替换旧 life×10% 估算）。无 recoup 词条 / 未重算
/// mitigated 循环时为 0（消费侧 recoup pct 同为 0 → 速率 0，语义一致）。
pub fn fill_ehp_pob2(
    env: &mut Env,
    keystones: &DefenceKeystones,
    resistances: &ResistanceSuite,
) -> f64 {
    struct Computed {
        life_recoverable: f64,
        es_recovery_cap: f64,
        physical_dr: f64,
        n_hits: f64,
        n_mitigated: f64,
        total_in: f64,
        total_ehp_pob2: f64,
        max_hits: [f64; 5],
        avoid_stun: f64,
        stun_threshold: f64,
        self_stun_chance: f64,
        stun_duration: f64,
        recoupable_total: f64,
    }
    let computed = {
        let db = &env.player.mod_db;
        let enemy_db = &env.enemy.mod_db;
        let cfg = &env.cfg;
        let out = &env.player.output;

        // 池口径（:1411 ES 恢复上限 / :2644-2657 可恢复生命）
        // CappingES（ArmourESRecoveryCap 等 flag）与 lowLife/lowES config 留。
        let life_recoverable = out.life_unreserved.max(1.0);
        let es_recovery_cap = out.energy_shield;
        let base = PoolBaseStats {
            max_life: out.life,
            life_recoverable,
            mana_unreserved: out.mana_unreserved,
            energy_shield_recovery_cap: es_recovery_cap,
            ward: out.ward,
        };
        let ctx = build_pool_ctx(db, cfg, keystones, &base);
        let pools = build_pool_state(db, cfg, &base);
        let mom = mom_hit_pools(&ctx, &base);

        // 减伤快照（Track B 契约；deflect 折入 Track D 输出，:2433）
        // 敌方元素穿透（vendor CalcDefence.lua:2328/:2363）：`resMult = 1 −
        // max(resist − enemyPen, 0)/100`（仅 resist > 0 时扣减；负抗不受 pen 影响）。
        // pen 来源 = setup_enemy 注入的 `Enemy<X>Pen` placeholder（Pinnacle 3 / Uber 8，
        // ConfigOptions.lua:2072-2074、Modules/Data.lua:231）；物理/混沌无 pen。
        let enemy_pen = |name: &str| enemy_db.sum(ModType::Base, cfg, &[ModName::from(name)]);
        let resist_after_pen = |resist: f64, pen: f64| -> f64 {
            if resist > 0.0 {
                (resist - pen).max(0.0)
            } else {
                resist
            }
        };
        let deflect_effect_pct = (cfg.constants.game().deflect_effect
            + db.sum(ModType::Base, cfg, &[ModName::from("DeflectEffect")]))
        .clamp(0.0, 100.0);
        let mut mit = build_mitigation_ctx(
            db,
            cfg,
            &MitigationInputs {
                armour: out.armour,
                evasion: out.evasion,
                energy_shield: out.energy_shield,
                resist_pct: [
                    0.0,
                    resist_after_pen(resistances.fire, enemy_pen("EnemyFirePen")),
                    resist_after_pen(resistances.cold, enemy_pen("EnemyColdPen")),
                    resist_after_pen(resistances.lightning, enemy_pen("EnemyLightningPen")),
                    resistances.chaos,
                ],
                deflect_chance_pct: out.deflect_chance,
                deflect_effect_pct,
            },
        );
        // CI 混沌免疫：vendor keystone 词组含 `ChaosDamageTaken MORE -100`
        // （ModParser.lua:2356/2360）→ ChaosTakenHitMult = 0。pobr 解析侧暂未发该
        // 数值词条（避免扰动旧口径输出），新管线经 C-1 keystone 快照等价施加。
        if keystones.chaos_inoculation {
            mit.after_reduction_multi[DamageType::Chaos as usize] = 0.0;
        }

        // not-hit 层（:2018-2026，读 Track E evade 四分型 + avoid 输出）
        let nh = not_hit_suite(out);

        // 敌人进伤（:2040-2137；placeholder 经 setup_enemy 注入 enemy modDB）
        let crit_effect = enemy_crit_effect_ehp(
            db,
            enemy_db,
            cfg,
            nh.average_evade,
            out.crit_extra_damage_reduction,
        );
        let enemy_in = assemble_enemy_damage(enemy_db, cfg, crit_effect);

        // per-type TakenHit + 面板 DR（:2171-2444）
        let (taken_hit, dr_pct) = taken_hit_per_type(&enemy_in.damage, &mit);

        // avoid_stun / Stun 真值接线
        // vendor totalTakenHit = Σ <X>TakenHit（:2444）；ES 减半条件
        // `ES > totalTakenHit && !EnergyShieldProtectsMana`（:2554-2557）；
        // SelfStunChance 的有效伤用 totalTakenHit/PhysicalTakenHit（:2525-2643）。
        let total_taken_hit = taken_hit.total();
        let avoidance = crate::calc::defence::calc_avoidance(
            db,
            cfg,
            out.energy_shield,
            total_taken_hit,
            keystones.energy_shield_protects_mana,
        );
        let stun = crate::calc::stun::calc_stun(
            db,
            cfg,
            &crate::calc::stun::StunInputs {
                life: out.life,
                life_base_flat: env.player.base.life,
                energy_shield: out.energy_shield,
                mana: out.mana,
                total_taken_hit,
                physical_taken_hit: taken_hit.physical,
                avoid_stun: avoidance.avoid_stun,
                chaos_inoculation: keystones.chaos_inoculation,
            },
        );

        // 致死击数（:3148-3153）
        // preventedLifeLossTotal > 0 → LimitEHPSpeedup（:3151）。
        let below_half_effective =
            (1.0 - ctx.prevented_life_loss / 100.0) * ctx.life_loss_below_half_prevented;
        let prevented_total = ctx.prevented_life_loss > 0.0 || below_half_effective > 0.0;
        let params = EhpLoopParams::from_constants(&cfg.constants, prevented_total);
        let n_hits = number_of_hits_to_die(&taken_hit, &pools, &ctx, &params);

        // mitigation 概率层（:3155-3247）
        // 平均格挡 = 四分型均值（vendor :1067 EffectiveAverageBlockChance）。旧
        // 二分型均值把 SpellProjectileBlock = max(spellBlock, projBlock)（:1013）
        // 漏掉——盾 build（spellBlock 0、projBlock = block）被低估：smith 13.65
        // vs vendor 20.475（TotalEHP 0.92x 根因）。
        let avg_block_frac = (out.effective_block_chance
            + out.effective_projectile_block_chance
            + out.effective_spell_block_chance
            + out.effective_spell_projectile_block_chance)
            / 4.0
            / 100.0;
        // vendor BlockEffect（防住份额%）= 100 − ΣBASE = 100 − out.block_effect（承伤份额）。
        let block_effect_mult = 1.0 - avg_block_frac * (100.0 - out.block_effect) / 100.0;
        // :3195 deflect 乘数（chance<100 口径；=100 时已折入 afterReductionMulti）。
        let deflect_mult = if out.deflect_chance < 100.0 {
            1.0 - out.deflect_chance * deflect_effect_pct / 10_000.0
        } else {
            1.0
        };
        // 分类型击中规避（vendor CalcDefence.lua:3262/:3277-3300）：有 `Avoid<Type>
        // DamageChance` 来源时，averageAvoidChance = 五类型规避均值，折入
        // configured_damage_chance；每类型另享 "Average" damageCategory 的
        // ExtraAvoidChance = 投射物规避/2（:3262），再 clamp 75。无分类型规避时
        // averageAvoidChance = 0，与旧值逐位一致（其余 build 零行为）。
        let specific_type_avoidance = avoidance.avoid_typed_damage.iter().any(|&a| a > 0.0);
        let extra_avoid = avoidance.avoid_projectile_damage / 2.0;
        let avoid_cap = crate::calc::defence::AVOID_HIT_CAP;
        let avoid: [f64; 5] = avoidance.avoid_typed_damage.map(|base| {
            if specific_type_avoidance {
                (base + extra_avoid).min(avoid_cap)
            } else {
                0.0
            }
        });
        let average_avoid_chance = avoid.iter().sum::<f64>() / 5.0;
        let configured_damage_chance =
            100.0 * block_effect_mult * deflect_mult * (1.0 - average_avoid_chance / 100.0);
        // F-4：anyRecoup 改读词条本体（vendor :1795-1812 `Σ <Resource>Recoup` BASE；
        // recoup 速率字段此时尚未写入——其基数正来自本段的 mitigated 循环累计）。
        let any_recoup = ["LifeRecoup", "ManaRecoup", "EnergyShieldRecoup"]
            .iter()
            .any(|name| db.sum(ModType::Base, cfg, &[ModName::from(*name)]) > 0.0);
        let (n_mitigated, recoupable_total) = if (configured_damage_chance - 100.0).abs()
            > f64::EPSILON
            || any_recoup
            || prevented_total
        {
            // 逐类型缩减（vendor :3277-3300 DamageIn[type] × (1 - avoid_type/100)）——
            // 分类型规避不能走 scale_damage 的统一因子（那条在 hits_to_die 迭代里另有
            // 复用，须保持 uniform）。无分类型规避时 avoid 全 0，与旧 uniform 缩逐位一致。
            let base_mult = block_effect_mult * deflect_mult;
            let mitigated_in = super::pool_damage::TypedDamage {
                physical: taken_hit.physical * base_mult * (1.0 - avoid[0] / 100.0),
                fire: taken_hit.fire * base_mult * (1.0 - avoid[1] / 100.0),
                cold: taken_hit.cold * base_mult * (1.0 - avoid[2] / 100.0),
                lightning: taken_hit.lightning * base_mult * (1.0 - avoid[3] / 100.0),
                chaos: taken_hit.chaos * base_mult * (1.0 - avoid[4] / 100.0),
            };
            let m_params =
                EhpLoopParams::from_constants(&cfg.constants, any_recoup || prevented_total);
            // F-4（13-G15 部分）：mitigated 循环同步累计 recoupable（vendor
            // :3232-3236 TrackRecoupable 置位于 NumberOfMitigatedDamagingHits 重算前；
            // :3347-3361 totalDamage = Σ <X>RecoupableDamageTaken 作 recoup 基数）。
            let (hits, recoupable) =
                number_of_hits_to_die_tracked(&mitigated_in, &pools, &ctx, &m_params);
            (hits, recoupable.iter().sum::<f64>())
        } else {
            (n_hits, 0.0)
        };

        // TotalEHP（:3271 TotalNumberOfHits + :3322）
        let not_hit_frac = (nh.average / 100.0).clamp(0.0, 1.0);
        let total_hits = if not_hit_frac >= 1.0 {
            f64::INFINITY
        } else {
            n_mitigated / (1.0 - not_hit_frac)
        };
        // 裸 Env（无敌人进伤）下 total_in = 0、total_hits = ∞ → 0 中性（避免 ∞×0 NaN）。
        let total_ehp_pob2 = if enemy_in.total_in > 0.0 {
            round(total_hits * enemy_in.total_in)
        } else {
            0.0
        };

        // 新口径 max hit（:3540-3697）
        let pool_by_type = total_hit_pools(&mom, es_recovery_cap, &pools, &ctx);
        // 诊断：POBR_DBG_EHPPOOL=1 时 dump 池分解（与 oracle <Type>TotalHitPool /
        // MoMHitPool / Ward 对照）。
        if dbg_env!("POBR_DBG_EHPPOOL").is_some() {
            eprintln!(
                "[POBR_EHPPOOL] pools={pool_by_type:?} mom={mom:?} es_cap={es_recovery_cap:.2} ward={:.2} guard=({:.2},{:.2}) aegis_shared={:.2}",
                pools.ward, pools.guard_shared, pools.guard_shared_rate, pools.aegis_shared,
            );
        }
        let inputs = MaxHitInputs {
            mit: &mit,
            pools_full: &pools,
            ctx: &ctx,
            total_hit_pool: pool_by_type,
            armour_ratio: cfg.constants.game().armour_ratio,
            smoothing_passes: cfg.constants.game().max_hit_smoothing_passes as u32,
        };
        let mut max_hits = [0.0_f64; 5];
        for dtype in DAMAGE_TYPE_BY_INDEX {
            let i = dtype as usize;
            max_hits[i] = max_hit_pob2(dtype, &inputs, enemy_in.mult_by_type[i]);
        }

        Computed {
            life_recoverable,
            es_recovery_cap,
            physical_dr: dr_pct[DamageType::Physical as usize],
            n_hits,
            n_mitigated,
            total_in: enemy_in.total_in,
            total_ehp_pob2,
            max_hits,
            avoid_stun: avoidance.avoid_stun,
            stun_threshold: stun.threshold,
            self_stun_chance: stun.self_stun_chance,
            stun_duration: stun.stun_duration,
            recoupable_total,
        }
    };

    let out = &mut env.player.output;
    out.life_recoverable = computed.life_recoverable;
    out.energy_shield_recovery_cap = computed.es_recovery_cap;
    out.physical_damage_reduction = computed.physical_dr;
    out.number_of_damaging_hits = computed.n_hits;
    out.number_of_mitigated_hits = computed.n_mitigated;
    out.total_enemy_damage_in = computed.total_in;
    out.total_ehp_pob2 = computed.total_ehp_pob2;
    out.physical_max_hit_pob2 = computed.max_hits[DamageType::Physical as usize];
    out.fire_max_hit_pob2 = computed.max_hits[DamageType::Fire as usize];
    out.cold_max_hit_pob2 = computed.max_hits[DamageType::Cold as usize];
    out.lightning_max_hit_pob2 = computed.max_hits[DamageType::Lightning as usize];
    out.chaos_max_hit_pob2 = computed.max_hits[DamageType::Chaos as usize];

    // F-3 口径切换段
    // canonical 字段改挂新口径值；旧 lowest-max-hit 口径已由 perform 旧管线写入
    // `total_ehp_lowest_max_hit` 保留（不删码）。
    out.total_ehp = computed.total_ehp_pob2;
    out.physical_max_hit = computed.max_hits[DamageType::Physical as usize];
    out.fire_max_hit = computed.max_hits[DamageType::Fire as usize];
    out.cold_max_hit = computed.max_hits[DamageType::Cold as usize];
    out.lightning_max_hit = computed.max_hits[DamageType::Lightning as usize];
    out.chaos_max_hit = computed.max_hits[DamageType::Chaos as usize];
    // avoid_stun / Stun 体系真值（覆盖 fill_evade_stun 的 reference_hit 近似产出）。
    out.avoid_stun = computed.avoid_stun;
    out.stun_threshold = computed.stun_threshold;
    out.self_stun_chance = computed.self_stun_chance;
    out.stun_duration = computed.stun_duration;

    computed.recoupable_total
}
