//! EHP and max hit (09-player-facing §3.2, §4.3).
//!
//! For each damage type, computes the maximum single hit that can be
//! survived based on its effective health pool and mitigation; EHP takes the
//! lowest max hit (the weakest link determines survival). Physical goes
//! through armour + flat PDR, elemental/chaos through resistances; ES counts
//! for chaos at half effectiveness.
//!
//! Note: armour reduction needs an incoming hit estimate (`reference_hit`);
//! without a real enemy damage value, the display baseline is used; EHP's
//! weighting scheme (lowest vs type-weighted) takes lowest, flagged as pending a product decision.

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
use super::taken::{
    MitigationCtx, MitigationInputs, build_mitigation_ctx_for_category, taken_hit_from_damage,
};

/// Final mitigation parameters per damage type.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ResistanceSuite {
    /// Physical damage reduction flat bonus (from sources other than armour), fraction [0,1).
    pub physical_pdr: f64,
    pub fire: f64,
    pub cold: f64,
    pub lightning: f64,
    pub chaos: f64,
}

/// Damage reduction ceiling (fraction, default 0.9). Mirrors PoB2's
/// `output.DamageReductionMax = Max('DamageReductionMax') or DamageReductionCap(=90)`
/// (CalcDefence.lua:1862). `+Maximum Damage Reduction` mods can raise this.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DamageReductionCaps {
    /// Global damage reduction ceiling as a fraction (default 0.9 = 90%).
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

/// Elemental/chaos damage-taken fraction: `1 - resist%/100`, floored at 0.
fn resist_taken_fraction(resist_pct: f64) -> f64 {
    (1.0 - resist_pct / 100.0).max(0.0)
}

/// Physical damage-taken fraction: `1 - (pdr_flat + armour mitigation)`, clamped to [0.1, 1.0] (PoE's physical mitigation ceiling is 90%).
pub fn physical_taken_fraction(pdr_flat: f64, armour: f64, reference_hit: f64) -> f64 {
    physical_taken_fraction_overwhelm(pdr_flat, armour, reference_hit, 0.0)
}

/// Physical damage-taken fraction, including enemy **overwhelm** (fraction):
/// first computes total mitigation against the 90% ceiling, then reduces it
/// by overwhelm (raising the taken fraction). PoB2: armour 1e9 (90% DR) + 15% overwhelm → 75% DR → 0.25 taken.
pub fn physical_taken_fraction_overwhelm(
    pdr_flat: f64,
    armour: f64,
    reference_hit: f64,
    overwhelm: f64,
) -> f64 {
    physical_taken_fraction_overwhelm_cap(pdr_flat, armour, reference_hit, overwhelm, 0.9)
}

/// Same as [`physical_taken_fraction_overwhelm`], but with the mitigation
/// ceiling switched to a variable `dr_max` (fraction). Mirrors PoB2: armour+flat
/// is summed then clamped to `DamageReductionMax` (CalcDefence.lua:396).
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

/// Maximum survivable hit for an elemental/chaos type: `pool / (1 - resist%/100)`.
pub fn max_hit_for_type(pool: f64, resist_pct: f64) -> f64 {
    let taken = resist_taken_fraction(resist_pct);
    if taken <= 0.0 {
        f64::INFINITY
    } else {
        round(pool / taken)
    }
}

/// Physical maximum survivable hit.
///
/// Armour mitigation varies with hit size (`armour/(armour+10*hit)`), so the
/// maximum survivable hit `H` must be **self-consistent**:
/// `H * taken(H) = pool` (getting hit by exactly this much, after
/// mitigation, exactly equals the health pool). PoB2 uses the same
/// semantics (`takenHitFromDamage(MaxHit) == pool`). Solved via fixed-point
/// iteration (`taken` is monotonic in `H`, converges fast); without armour,
/// `taken` is independent of `H` and converges in one step → degenerates to
/// `pool/taken`. `reference_hit` seeds the initial value.
pub fn physical_max_hit(pool: f64, pdr_flat: f64, armour: f64, reference_hit: f64) -> f64 {
    physical_max_hit_overwhelm(pool, pdr_flat, armour, reference_hit, 0.0)
}

/// Physical maximum survivable hit (including enemy overwhelm). The same
/// self-consistent iteration as [`physical_max_hit`], with the taken
/// fraction switched to [`physical_taken_fraction_overwhelm`].
pub fn physical_max_hit_overwhelm(
    pool: f64,
    pdr_flat: f64,
    armour: f64,
    reference_hit: f64,
    overwhelm: f64,
) -> f64 {
    physical_max_hit_overwhelm_cap(pool, pdr_flat, armour, reference_hit, overwhelm, 0.9)
}

/// Same as [`physical_max_hit_overwhelm`], but with the mitigation ceiling switched to a variable `dr_max` (fraction).
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

/// Maximum survivable hit for an elemental type routed through armour
/// ("Armour applies to <Element> instead of Physical"): armour mitigation
/// applies to the **pre-resistance (raw)** damage (PoB2's
/// `armourReductionF(armour, RAW)`, CalcDefence.lua:56/393/427/3626),
/// multiplied independently with the resistance layer. So
/// `taken = res_taken × (1 - armour_dr(H))`, with armour_dr capped at 90%.
/// Likewise solved via self-consistent iteration for `H × taken(H) = pool`.
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
        // PoB2: armour DR is based on RAW (pre-resistance) damage, i.e. the
        // currently-iterated hit H, not post-resist. Mitigation ceiling is
        // variable (default 0.9), taken-fraction floor = 1 - dr_max.
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

/// Elemental health pool (life + es).
fn elemental_pool(life: f64, es: f64) -> f64 {
    life + es
}

/// Chaos health pool (ES at half effectiveness against chaos: life + es*0.5).
fn chaos_pool(life: f64, es: f64) -> f64 {
    life + es * 0.5
}

/// Chaos Inoculation (CI) keystone options.
/// Source: agent-docs/active-defences.md §5's Keystone table;
///       PoB2 CalcDefence.lua: CI → maxLife=1, ES acts as the life pool, chaos damage immunity (chaos_resist = 100%).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EhpOptions {
    /// Chaos Inoculation: max life becomes 1, ES acts as the life pool
    /// (`es` used for every damage pool), chaos damage immunity.
    pub chaos_inoculation: bool,
    /// Enemy physical overwhelm (fraction): reduces the player's total physical mitigation (raising taken damage).
    pub physical_overwhelm: f64,
    /// "Armour applies to <Element> instead of Physical": whether fire/cold/
    /// lightning is routed through armour mitigation instead; when any is
    /// true, physical no longer benefits from armour (PDR only).
    /// Corresponds to PoB2's same-named mod.
    pub armour_applies_to_element: [bool; 3],
    /// Damage reduction ceiling (can be raised by `+Maximum Damage Reduction` mods). Default 90%.
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

/// Calculates EHP and each type's max hit. `reference_hit` is the incoming hit estimate baseline for physical armour mitigation.
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

/// The full version of `calc_ehp`, supporting keystone options like Chaos Inoculation.
///
/// **Bug#10 fix (ehp-chaos-inoculation-wrong)**:
/// in a CI build, ES becomes the life pool (`life_pool = es`), with chaos
/// damage immunity (`chaos_max_hit = ∞`). Source: agent-docs/active-defences.md
///   §5 Keystone: `Chaos Inoculation: max life becomes 1; immune to chaos damage and bleed`.
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
        // CI: life = 1 (already handled at the actor layer), ES acts as every damage pool
        // Here es is placed into effective_life to reuse the elemental_pool/chaos_pool functions
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

    // "Armour applies to <Element> instead of Physical": whether physical
    // still benefits from armour depends on whether any redirect is active.
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
    // Each element: routed through armour (post-resistance) when redirected, otherwise pure resistance.
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
    // CI: chaos damage immunity → infinite max hit
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

/// Traced version of `calc_ehp`: records a Mitigate node each for fire and physical max hit, and a Clamp node for the total.
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

// The PoB2-view EHP: vendor `CalcDefence.lua`'s numberOfHitsToDie × per-hit incoming damage.
//
// `total_ehp` is still the old lowest-max-hit view; this section's values
// live on `total_ehp_pob2` / `*_max_hit_pob2` / `number_of_damaging_hits` /
// `number_of_mitigated_hits`. This pipeline attaches no trace -- attribution
// only reuses calc_ehp_traced's Mitigate/Clamp nodes.

/// Per-type array index order (= `DamageType as usize`, the same convention as pool_damage/taken).
const DAMAGE_TYPE_BY_INDEX: [DamageType; 5] = [
    DamageType::Physical,
    DamageType::Fire,
    DamageType::Cold,
    DamageType::Lightning,
    DamageType::Chaos,
];

/// Vendor's `round` (Modules/Common.lua: `m_floor(val + 0.5)`).
fn vendor_round(value: f64) -> f64 {
    (value + 0.5).floor()
}

/// Scalar multiplication of a damage vector (vendor's `Damage[t] = DamageIn[t] × k` shape).
fn scale_damage(damage: &TypedDamage, factor: f64) -> TypedDamage {
    TypedDamage {
        physical: damage.physical * factor,
        fire: damage.fire * factor,
        cold: damage.cold * factor,
        lightning: damage.lightning * factor,
        chaos: damage.chaos * factor,
    }
}

/// Enemy single-hit incoming damage placeholder (vendor ConfigOptions.lua:1982-1996):
/// `default = round(monsterDamageTable[lv] × ehp_base_damage_mult × DPSMult)`,
/// same value for physical/fire/cold/lightning, with chaos additionally
/// `round(default / chaos_damage_div)` (a per-tier divisor,
/// None/Boss/Pinnacle = 2.5, Uber = 4, L1987/L2028/L2070/L2111).
///
/// All data is catalogued: `monster_scaling.json::damage` + `enemy_presets.json`
/// (`ehp_base_damage_mult` / per-tier `dps_mult`/`chaos_damage_div`).
/// Per-type config overrides (the `enemy<X>Damage` configInput) are left for config_interpreter.
/// A missing preset (corrupted data) → all 0 (a 0 incoming-damage consumer → an infinite, neutral short-circuit for the lethal hit count).
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

/// What the EHP stage computed with, per damage type in the order Physical,
/// Fire, Cold, Lightning, Chaos: the enemy's hit, the part of it taken, and
/// the pool that type of damage has to deplete.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct EhpTerms {
    /// The enemy's damage of each type in one hit, after its modifiers,
    /// critical hits and gained damage (`<X>EnemyDamage`).
    pub enemy_damage: [f64; 5],
    /// The damage of each type taken from that hit (`<X>TakenHit`).
    pub taken_hit: [f64; 5],
    /// Incoming damage after taken-as conversion, before mitigation.
    pub taken_damage: [f64; 5],
    /// Resolved taken-as fractions, indexed by source and destination type.
    pub damage_shift: [[f64; 5]; 5],
    pub damage_reduction: [DamageReductionTerms; 5],
    pub resist_taken_multi: [f64; 5],
    pub taken_flat: [f64; 5],
    pub after_reduction_multi: [f64; 5],
    /// Resolved ES bypass percentages used by the pool calculation.
    pub energy_shield_bypass: [f64; 5],
    /// Factors used by the configured hit probability layer.
    pub block_effect_multiplier: f64,
    pub deflect_multiplier: f64,
    pub average_avoid_chance: f64,
    pub not_hit_before_unlucky: f64,
    pub unlucky_worst_of: f64,
    /// The factors of the enemy's chance to crit (`EnemyCritChance`).
    pub enemy_crit_chance: EnemyCritChanceTerms,
    /// The pool each type depletes (`<X>TotalHitPool`).
    pub total_hit_pool: [f64; 5],
    /// Hits like that to die (`NumberOfDamagingHits`).
    pub damaging_hits: f64,
    /// The percentage of a hit left after block, deflection and avoidance
    /// (`ConfiguredDamageChance`).
    pub damage_chance: f64,
    /// Hits to die with that percentage (`NumberOfMitigatedDamagingHits`).
    pub mitigated_hits: f64,
    /// The configured chance not to be hit, after unlucky rolls (`ConfiguredNotHitChance`).
    pub not_hit_chance: f64,
    /// The enemy's damage before modifiers, which EHP is in units of
    /// (`totalEnemyDamageIn`).
    pub enemy_damage_in: f64,
    pub max_hit_solutions: [f64; 5],
    pub enemy_damage_multipliers: [f64; 5],
    pub physical_reduction: DamageReductionTerms,
    pub recoupable_total: f64,
    /// Physical damage prevented by Armour plus additional physical reduction,
    /// accumulated alongside damage-taken recoup in the mitigated hit loop.
    pub physical_prevented: f64,
    /// The recoupable damage of each type (Physical/Fire/Cold/Lightning/Chaos
    /// order), feeding per-type recoup (elemental Energy Shield recoup) and
    /// the EsRecoupRate derived detail.
    pub recoupable_by_type: [f64; 5],
    /// Enemy penetration of each type (`<X>EnemyPen`; Physical and Chaos 0).
    pub enemy_pen: [f64; 5],
    /// The player's final resistance before penetration (Physical 0).
    pub resist_before_pen: [f64; 5],
    /// The mitigation snapshot every typed hit above was taken through.
    pub mitigation: MitigationCtx,
    /// Life and Mind over Matter hit pools (`LifeHitPool`, `<X>MoMHitPool`,
    /// `<X>ManaEffectiveLife`).
    pub mom: MomHitPools,
    pub life_hit_pool_terms: LifeHitPoolTerms,
    /// The Mind over Matter pool with Energy Shield folded in, before the
    /// Runic Ward/Aegis/Guard/ally layers of `total_hit_pool`.
    pub base_hit_pool: [f64; 5],
}

/// Inputs of the life hit pool: recoverable Life with loss prevention.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct LifeHitPoolTerms {
    pub life: f64,
    pub max_life: f64,
    pub loss_prevented: f64,
    pub below_half_loss_prevented: f64,
}

impl EhpTerms {
    /// Resistance, reduction and post-reduction multipliers together
    /// (`<X>BaseTakenHitMult`); flat damage taken is outside it.
    pub(crate) fn base_taken_hit_multiplier(&self, i: usize) -> f64 {
        self.resist_taken_multi[i]
            * self.damage_reduction[i].taken_multiplier()
            * self.after_reduction_multi[i]
    }

    /// Ratio view of the mitigated hit to its converted incoming damage.
    /// An absent incoming type has a zero multiplier, even with flat damage.
    pub(crate) fn taken_hit_multiplier(&self, i: usize) -> f64 {
        if self.taken_damage[i] > 0.0 {
            self.taken_hit[i] / self.taken_damage[i]
        } else {
            0.0
        }
    }
}

/// Enemy incoming-damage assembly result (vendor CalcDefence.lua:2040-2168's enemy-damage section).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct EnemyDamageIn {
    /// Per-type final value `<X>EnemyDamage = enemyDamage × enemyDamageMult ×
    /// EnemyCritEffect`, plus the damage the enemy gains as that type from
    /// the others.
    pub damage: TypedDamage,
    /// `totalEnemyDamageIn = Σ enemyDamage` (**before** mult/crit, `:2136`;
    /// the end-of-pipeline TotalEHP multiplier `:3322`).
    pub total_in: f64,
    /// Per-type `<X>EnemyDamageMult` (the max-hit end-of-pipeline divisor, `:3660`/`:3696`).
    pub mult_by_type: [f64; 5],
}

/// Explicit inputs replace preset base values, leaving added enemy damage intact.
fn enemy_base(db: &ModDb, cfg: &CalcConfig, name: &str) -> f64 {
    db.override_(cfg, name)
        .unwrap_or_else(|| db.sum(ModType::Base, cfg, &[ModName::from(name)]))
}

/// Enemy incoming-damage assembly (vendor CalcDefence.lua:2065-2137): the
/// placeholder (`Enemy<X>Damage` BASE injected by setup_enemy) + the enemy
/// db's `<X>Min`/`<X>Max` mod average (`:2098`) → per-type ×
/// `enemyDamageMult` (`calcLib.mod(enemyDB, "Damage", "<X>Damage"[, "ElementalDamage"])`,
/// `:2133`) × `EnemyCritEffect`.
///
/// The enemy also gains damage as other types, read with the names the
/// player's gains are (`damage::gain_names`): Evergrasping Ring's "Enemies in
/// your Presence Gain {0}% of Damage as Extra Chaos Damage"
/// (`enemies_in_presence_non_skill_base_all_damage_%_to_gain_as_chaos`) is
/// a share of all of its damage, each type gaining from the others. Gained
/// damage is modified by the modifiers of both types.
pub fn assemble_enemy_damage(
    enemy_db: &ModDb,
    cfg: &CalcConfig,
    crit_effect: f64,
) -> EnemyDamageIn {
    let mut result = EnemyDamageIn::default();
    let mut per_type = [0.0_f64; 5];
    let mut base_by_type = [0.0_f64; 5];
    for dtype in DAMAGE_TYPE_BY_INDEX {
        let i = dtype as usize;
        let name = type_prefix(dtype);
        // Placeholder (injected by setup_enemy; 0 under a bare Env with no enemy config → neutral).
        let placeholder = enemy_base(enemy_db, cfg, &format!("Enemy{name}Damage"));
        // :2098 the enemy Min/Max mod average.
        let min_max = (enemy_db.sum(ModType::Base, cfg, &[ModName::from(format!("{name}Min"))])
            + enemy_db.sum(ModType::Base, cfg, &[ModName::from(format!("{name}Max"))]))
            / 2.0;
        let enemy_damage = placeholder + min_max;
        // :2133 enemyDamageMult.
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
        base_by_type[i] = enemy_damage;
        per_type[i] = enemy_damage * mult * crit_effect;
    }
    // What the enemy gains as another type: Evergrasping Ring's "Enemies in
    // your Presence Gain (6-12)% of Damage as Extra Chaos Damage" is
    // `enemies_in_presence_non_skill_base_all_damage_%_to_gain_as_chaos`,
    // from all of its damage, scaled by its increases to either type.
    for from in DAMAGE_TYPE_BY_INDEX {
        for to in DAMAGE_TYPE_BY_INDEX {
            if from == to || base_by_type[from as usize] == 0.0 {
                continue;
            }
            let share =
                enemy_db.sum(ModType::Base, cfg, &super::damage::gain_names(from, to)) / 100.0;
            if share <= 0.0 {
                continue;
            }
            let mut names = vec![
                ModName::from("Damage"),
                ModName::from(format!("{}Damage", type_prefix(to))),
                ModName::from(format!("{}Damage", type_prefix(from))),
            ];
            if to.is_elemental() || from.is_elemental() {
                names.push(ModName::from("ElementalDamage"));
            }
            let mult = (1.0 + enemy_db.sum(ModType::Inc, cfg, &names) / 100.0)
                * enemy_db.more(cfg, &names);
            per_type[to as usize] += base_by_type[from as usize] * share * mult * crit_effect;
        }
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

/// A typed damage as an array in `DAMAGE_TYPE_BY_INDEX` order.
fn by_type(damage: &TypedDamage) -> [f64; 5] {
    DAMAGE_TYPE_BY_INDEX.map(|dtype| damage.get(dtype))
}

/// DamageType → mod name prefix.
fn type_prefix(dtype: DamageType) -> &'static str {
    match dtype {
        DamageType::Physical => "Physical",
        DamageType::Fire => "Fire",
        DamageType::Cold => "Cold",
        DamageType::Lightning => "Lightning",
        DamageType::Chaos => "Chaos",
    }
}

/// The second smallest of the five typed maximum hits; equal values count
/// separately, so two equal minima give that value.
pub fn second_minimal_maximum_hit(max_hits: [f64; 5]) -> f64 {
    let mut sorted = max_hits;
    sorted.sort_by(f64::total_cmp);
    sorted[1]
}

/// How the enemy's chance to deal a critical hit is decided.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum EnemyCritMode {
    /// The rolled chance from the base, increases and evasion.
    #[default]
    Rolled,
    /// `NeverCrit`: the enemy never crits.
    Never,
    /// `AlwaysCrit`: every hit is a critical hit.
    Always,
}

/// The factors of the enemy's chance to crit the player:
///
/// ```text
/// chance = clamp(base × (1 + increased/100) × (1 − ConfiguredEvadeChance/100), 0, 100)
/// ```
///
/// `base` is the configured value or the preset `default_enemy_crit_chance`
/// (5), `increased` adds the player's `EnemyCritChance` and the enemy's
/// `CritChance` increases. `NeverCrit` gives 0 and `AlwaysCrit` 100;
/// `EnemyUnluckyCrit` takes the worse of two rolls, `chance² / 100`.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct EnemyCritChanceTerms {
    pub mode: EnemyCritMode,
    pub base: f64,
    pub increased: f64,
    pub evade: f64,
    pub unlucky: bool,
}

impl EnemyCritChanceTerms {
    /// The chance before the unlucky roll (%).
    pub fn rolled(self) -> f64 {
        match self.mode {
            EnemyCritMode::Never => 0.0,
            EnemyCritMode::Always => 100.0,
            EnemyCritMode::Rolled => {
                (self.base * (1.0 + self.increased / 100.0) * (1.0 - self.evade / 100.0))
                    .clamp(0.0, 100.0)
            }
        }
    }

    /// The chance a hit is critical (%, `EnemyCritChance`).
    pub fn chance(self) -> f64 {
        let chance = self.rolled();
        if self.unlucky {
            chance / 100.0 * chance
        } else {
            chance
        }
    }
}

pub(crate) fn enemy_crit_chance_terms(
    player_db: &ModDb,
    enemy_db: &ModDb,
    cfg: &CalcConfig,
    configured_evade_pct: f64,
) -> EnemyCritChanceTerms {
    let presets = &cfg.constants.enemy_presets;
    EnemyCritChanceTerms {
        mode: if enemy_db.flag(cfg, "NeverCrit") {
            EnemyCritMode::Never
        } else if enemy_db.flag(cfg, "AlwaysCrit") {
            EnemyCritMode::Always
        } else {
            EnemyCritMode::Rolled
        },
        base: enemy_db
            .override_(cfg, "EnemyCritChance")
            .unwrap_or(presets.default_enemy_crit_chance),
        increased: player_db.sum(ModType::Inc, cfg, &[ModName::from("EnemyCritChance")])
            + enemy_db.sum(ModType::Inc, cfg, &[ModName::from("CritChance")]),
        evade: configured_evade_pct,
        unlucky: player_db.flag(cfg, "EnemyUnluckyCrit"),
    }
}

/// Enemy crit effect (the EHP view):
///
/// ```text
/// damage = max((placeholder(30) + enemy's CritMultiplier BASE) × (1 + enemy's CritMultiplier INC/100), 0)
/// effect = 1 + chance/100 × damage/100 × (1 − CritExtraDamageReduction/100)
/// ```
///
/// The placeholder comes from `enemy_presets.json` (`default_enemy_crit_damage_bonus`),
/// unless explicitly configured; the chance is [`EnemyCritChanceTerms::chance`].
pub fn enemy_crit_effect_ehp(
    player_db: &ModDb,
    enemy_db: &ModDb,
    cfg: &CalcConfig,
    configured_evade_pct: f64,
    crit_extra_reduction_pct: f64,
) -> f64 {
    let terms = enemy_crit_chance_terms(player_db, enemy_db, cfg, configured_evade_pct);
    enemy_crit_effect_with(terms, enemy_db, cfg, crit_extra_reduction_pct)
}

fn enemy_crit_effect_with(
    terms: EnemyCritChanceTerms,
    enemy_db: &ModDb,
    cfg: &CalcConfig,
    crit_extra_reduction_pct: f64,
) -> f64 {
    let presets = &cfg.constants.enemy_presets;
    let chance = terms.chance();
    let crit_damage = ((enemy_db
        .override_(cfg, "EnemyCritDamage")
        .unwrap_or(presets.default_enemy_crit_damage_bonus)
        + enemy_db.sum(ModType::Base, cfg, &[ModName::from("CritMultiplier")]))
        * (1.0 + enemy_db.sum(ModType::Inc, cfg, &[ModName::from("CritMultiplier")]) / 100.0))
        .max(0.0);
    1.0 + chance / 100.0 * (crit_damage / 100.0) * (1.0 - crit_extra_reduction_pct / 100.0)
}

/// Aggregate every incoming damage type through taken-as conversion before
/// applying mitigation once per destination. Flat damage is added once and
/// armour reduction uses the combined incoming damage of that destination.
/// Returns the mitigated hit and the per-type damage reduction percentages.
pub fn taken_hit_per_type(
    enemy_damage: &TypedDamage,
    mit: &MitigationCtx,
) -> (TypedDamage, [f64; 5]) {
    let terms = taken_hit_terms(enemy_damage, mit);
    (
        terms.hit,
        terms
            .reduction
            .map(|r| 100.0 - r.taken_multiplier() * 100.0),
    )
}

struct TakenHitTerms {
    damage: [f64; 5],
    hit: TypedDamage,
    reduction: [DamageReductionTerms; 5],
}

fn taken_hit_terms(enemy_damage: &TypedDamage, mit: &MitigationCtx) -> TakenHitTerms {
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
    let mut hits = [0.0_f64; 5];
    let mut reduction = [DamageReductionTerms::default(); 5];
    for i in 0..5 {
        let damage = taken[i];
        reduction[i] = damage_reduction_terms(damage, mit, i);
        let base_mult = mit.resist_taken_multi[i] * reduction[i].taken_multiplier();
        hits[i] = (damage * base_mult + mit.taken_flat[i]).max(0.0) * mit.after_reduction_multi[i];
    }
    TakenHitTerms {
        damage: taken,
        hit: TypedDamage {
            physical: hits[0],
            fire: hits[1],
            cold: hits[2],
            lightning: hits[3],
            chaos: hits[4],
        },
        reduction,
    }
}

/// EHP loop parameters (vendor Data.lua:235-239 + `:3094`'s LimitEHPSpeedup tightening).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EhpLoopParams {
    /// Single-hit damage ceiling (still alive above this → infinite lethal
    /// hit count). `ehp_calc_max_damage`.
    pub max_damage: f64,
    /// The iteration budget shared across all recursion. `ehp_calc_max_iterations`.
    pub max_iterations: f64,
    /// Recursion acceleration factor. `ehp_calc_speed_up` (=8); tightened to
    /// [`LIMITED_EHP_SPEED_UP`] when tracking loss-prevention/recoup/gain
    /// (vendor `:3094`'s literal 4 -- MoM/loss-prevention would collapse
    /// multiple hits into one, causing EHP to jump discontinuously, so acceleration is slowed).
    pub speed_up: f64,
    /// Resources recovered per hit (vendor `DamageIn.<X>WhenHit`); zero = no `GainWhenHit`.
    pub gain: GainWhenHit,
}

/// Average recovery per hit taken, currently recovery on block × block chance
/// (vendor CalcDefence.lua:3261-3268 `DamageIn.<X>WhenHit`).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct GainWhenHit {
    pub life: f64,
    pub mana: f64,
    pub energy_shield: f64,
    pub ward: f64,
}

impl GainWhenHit {
    /// Vendor `DamageIn.GainWhenHit` (:3297-3299).
    pub fn any(&self) -> bool {
        self.life != 0.0 || self.mana != 0.0 || self.energy_shield != 0.0 || self.ward != 0.0
    }

    /// Adds `gain × count` to each pool, capped at `cap_mult ×` its full value
    /// (LifeRecoverable / ManaUnreserved / EnergyShieldRecoveryCap / Ward,
    /// vendor :2146-2151 and :2159-2164).
    fn apply(&self, pool: &mut PoolState, full: &PoolState, count: f64, cap_mult: f64) {
        pool.life = (pool.life + self.life * count).min(cap_mult * full.life);
        pool.mana = (pool.mana + self.mana * count).min(cap_mult * full.mana);
        pool.energy_shield =
            (pool.energy_shield + self.energy_shield * count).min(cap_mult * full.energy_shield);
        pool.ward = (pool.ward + self.ward * count).min(cap_mult * full.ward);
    }
}

/// Vendor CalcDefence.lua:3094: `speedUp = DamageIn["LimitEHPSpeedup"] and 4 or ehpCalcSpeedUp`.
const LIMITED_EHP_SPEED_UP: f64 = 4.0;

impl EhpLoopParams {
    /// Builds from the injected constants pack (`limit_speedup` = vendor's `LimitEHPSpeedup`).
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
            gain: GainWhenHit::default(),
        }
    }
}

/// Number of hits needed to be lethal (vendor CalcDefence.lua:2979-3145's
/// `numberOfHitsToDie`, mirrored line by line).
///
/// Loops calling Track A's [`reduce_pools`]; recursion acceleration
/// (recomputes once at `speed_up`× damage from the **full pool** to estimate
/// a jump step size, `:3105-3119`); fractional overkill folding
/// (`:3133-3135`, top level only, cycles=1); `max_damage` / `max_iterations`
/// ceilings guarantee termination. `params.gain` (vendor `GainWhenHit`) is
/// recovered after every survived hit (`:2144-2164`).
pub fn number_of_hits_to_die(
    damage_in: &TypedDamage,
    pools_full: &PoolState,
    ctx: &PoolCtx,
    params: &EhpLoopParams,
) -> f64 {
    number_of_hits_to_die_tracked(damage_in, pools_full, ctx, params).0
}

/// Lethal hit count and recoupable damage, accumulated only at the top level.
/// Accelerated recursive probes do not count; amplified top-level hits count
/// their scaled damage, including the lethal hit's full damage.
pub fn number_of_hits_to_die_tracked(
    damage_in: &TypedDamage,
    pools_full: &PoolState,
    ctx: &PoolCtx,
    params: &EhpLoopParams,
) -> (f64, [f64; 5]) {
    let (hits, recoup) = hits_to_die_with_recoup(damage_in, pools_full, ctx, params, 0.0);
    (hits, recoup.by_type)
}

#[derive(Default)]
struct RecoupAccumulation {
    by_type: [f64; 5],
    physical_prevented: f64,
    physical_prevented_per_hit: f64,
}

fn hits_to_die_with_recoup(
    damage_in: &TypedDamage,
    pools_full: &PoolState,
    ctx: &PoolCtx,
    params: &EhpLoopParams,
    physical_prevented_per_hit: f64,
) -> (f64, RecoupAccumulation) {
    let mut iterations = 0.0;
    let mut recoup = RecoupAccumulation {
        physical_prevented_per_hit,
        ..Default::default()
    };
    let hits = hits_to_die_inner(
        damage_in,
        pools_full,
        ctx,
        params,
        1.0,
        &mut iterations,
        &mut recoup,
    );
    (hits, recoup)
}

/// `numberOfHitsToDie`'s recursive body (`cycles`/`iterations` correspond to
/// vendor DamageIn's same-named keys; `iterations`'s budget is shared across recursion).
#[allow(clippy::too_many_arguments)]
fn hits_to_die_inner(
    damage_in: &TypedDamage,
    pools_full: &PoolState,
    ctx: &PoolCtx,
    params: &EhpLoopParams,
    cycles: f64,
    iterations: &mut f64,
    recoup: &mut RecoupAccumulation,
) -> f64 {
    // :2060-2068 zero incoming damage → ∞.
    let per_hit_total = damage_in.total();
    if per_hit_total <= 0.0 {
        return f64::INFINITY;
    }
    // :2070-2074 Runic Ward only protects the first hit, so batched hits (cycles>1) run without it.
    let mut pool = pools_full.clone();
    if cycles > 1.0 {
        pool.ward = 0.0;
    }

    let gain_active = params.gain.any();
    let mut num_hits = 0.0_f64;
    let mut iteration_multiplier = 1.0_f64;
    let mut cycles_ran = false;
    let mut last_overkill = 0.0_f64;
    let mut last_damage_total = per_hit_total;
    // :3063 the main while loop.
    while pool.life > 0.0 && *iterations < params.max_iterations {
        *iterations += 1.0;
        let hit = scale_damage(damage_in, iteration_multiplier);
        // :2144-2151 a batched hit stands for several hits: grant the recovery of all
        // but the last up front, with the caps scaled to the batch.
        if gain_active && (iteration_multiplier > 1.0 || cycles > 1.0) {
            let gain_mult = iteration_multiplier * cycles;
            params
                .gain
                .apply(&mut pool, pools_full, gain_mult - 1.0, gain_mult);
        }
        let after = reduce_pools(&pool, &hit, ctx);
        last_overkill = after.overkill;
        // :2127-2143 `damageTotal` is this iteration's (possibly batched) damage.
        last_damage_total = per_hit_total * iteration_multiplier;
        // Recursive probes do not contribute; top-level batches count each hit.
        if cycles <= 1.0 {
            for (acc, v) in recoup.by_type.iter_mut().zip(after.recoupable_by_type) {
                *acc += v;
            }
            recoup.physical_prevented += recoup.physical_prevented_per_hit * iteration_multiplier;
        }
        pool = after.pools;
        // :3084 still alive and the single-hit damage already exceeds the ceiling → treated as surviving unlimited hits.
        if pool.life > 0.0 && last_damage_total >= params.max_damage {
            return f64::INFINITY;
        }
        // :2159-2164 recovery from this hit, only while still alive.
        if gain_active && pool.life > 0.0 {
            params.gain.apply(&mut pool, pools_full, 1.0, 1.0);
        }
        iteration_multiplier = 1.0;
        // :3095-3119 recursion acceleration: recomputes at speed_up× damage from the full pool, estimating a safe number of hits to skip.
        if !cycles_ran && pool.life > 0.0 && *iterations < params.max_iterations {
            let accelerated = scale_damage(damage_in, params.speed_up);
            let recursive_hits = hits_to_die_inner(
                &accelerated,
                pools_full,
                ctx,
                params,
                cycles * params.speed_up,
                iterations,
                recoup,
            );
            // :3112 recursion already knows it survives forever → straight to ∞.
            if recursive_hits.is_infinite() {
                return f64::INFINITY;
            }
            iteration_multiplier = ((recursive_hits - 1.0) * params.speed_up - 1.0).max(1.0);
            cycles_ran = true;
        }
        num_hits += iteration_multiplier;
    }
    // :3133-3135 fractional overkill folding (top level only, cycles=1, to avoid corrupting the acceleration estimate).
    if pool.life <= 0.0 && cycles <= 1.0 {
        num_hits -= last_overkill / last_damage_total;
    }
    // :3137-3140 final check: total damage taken exceeds the ceiling but still alive → ∞.
    let damage_total = per_hit_total * num_hits;
    if pool.life >= 0.0 && damage_total >= params.max_damage {
        return f64::INFINITY;
    }
    // :3141-3143 NaN → 0.
    if num_hits.is_nan() {
        return 0.0;
    }
    num_hits.max(0.0)
}

/// Per-type TotalHitPool (vendor `:2942-2960`'s MoM/ES base + `:3540-3596`'s
/// ward/aegis/guard/allies expansion layer; Track A primitives
/// [`total_hit_pool_base`] / [`extend_total_hit_pool`]).
pub fn total_hit_pools(
    mom: &MomHitPools,
    energy_shield_recovery_cap: f64,
    pools: &PoolState,
    ctx: &PoolCtx,
) -> [f64; 5] {
    total_hit_pool_stages(mom, energy_shield_recovery_cap, pools, ctx).1
}

/// The two stages of [`total_hit_pools`]: the Mind over Matter pool with
/// Energy Shield folded in, then with the Runic Ward/Aegis/Guard/ally layers.
fn total_hit_pool_stages(
    mom: &MomHitPools,
    energy_shield_recovery_cap: f64,
    pools: &PoolState,
    ctx: &PoolCtx,
) -> ([f64; 5], [f64; 5]) {
    let mut base_by_type = [0.0_f64; 5];
    let mut out = [0.0_f64; 5];
    for dtype in DAMAGE_TYPE_BY_INDEX {
        let i = dtype as usize;
        let base = total_hit_pool_base(
            dtype,
            mom.hit_pool_by_type[i],
            energy_shield_recovery_cap,
            ctx,
        );
        base_by_type[i] = base;
        out[i] = extend_total_hit_pool(base, dtype, pools, ctx);
    }
    (base_by_type, out)
}

/// Inputs for the new-view max-hit solver (assembled once per actor, reused per type).
#[derive(Debug, Clone, Copy)]
pub struct MaxHitInputs<'a> {
    pub mit: &'a MitigationCtx,
    /// The full-pool snapshot (conversion smoothing's reduce_pools baseline,
    /// vendor `:3670` passes nil = the full pool).
    pub pools_full: &'a PoolState,
    pub ctx: &'a PoolCtx,
    /// Per-type TotalHitPool (produced by [`total_hit_pools`]).
    pub total_hit_pool: [f64; 5],
    /// Armour coefficient (`armour_ratio`, Data.lua:193).
    pub armour_ratio: f64,
    /// Multi-conversion smoothing iteration ceiling (`max_hit_smoothing_passes`, Data.lua:241).
    pub smoothing_passes: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct DamageReductionTerms {
    pub armour: f64,
    pub flat: f64,
    pub overwhelm: f64,
    pub cap: f64,
}
impl DamageReductionTerms {
    pub(crate) fn taken_multiplier(self) -> f64 {
        1.0 - ((self.armour + self.flat).min(self.cap) - self.overwhelm).clamp(0.0, self.cap)
            / 100.0
    }
}
fn damage_reduction_terms(damage: f64, mit: &MitigationCtx, i: usize) -> DamageReductionTerms {
    DamageReductionTerms {
        armour: vendor_round(armour_reduction(mit.effective_applied_armour[i], damage) * 100.0)
            .min(mit.dr_max_pct[i]),
        flat: mit.flat_dr_pct[i],
        overwhelm: mit.overwhelm_pct[i],
        cap: mit.dr_max_pct[i],
    }
}

/// New-view per-type maximum survivable hit (vendor CalcDefence.lua:3601-3697).
///
/// For each conversion target in `shift[dtype]`, independently solves for
/// "the RAW value that exactly depletes that target's TotalHitPool":
/// - `convert ≤ 0`: takenFlat is judged on its own (`:3611-3613`);
/// - No armour and full conversion: a closed-form solution (`:3614-3617`);
/// - Otherwise: vendor's quadratic-equation solution (`:3608-3641` -- an
///   algebraic solution of the self-consistent condition
///   `takenHit(RAW) = TotalHitPool` where armour DR varies with hit size,
///   mathematically equivalent to fixed-point iteration), then clamped to
///   the noDR/maxDR bounds + floored;
/// - `partMin = min(each target)`; when partial conversion exists, runs
///   smoothing iteration (`:3663-3692`, using [`taken_hit_from_damage`] +
///   [`reduce_pools`] to measure overkill until it converges);
/// - Finally `round(partMin / enemyDamageMult)` (`:3660`/`:3696`).
pub fn max_hit_pob2(dtype: DamageType, inputs: &MaxHitInputs, enemy_damage_mult: f64) -> f64 {
    scale_max_hit_solution(max_hit_solution(dtype, inputs), enemy_damage_mult)
}

fn scale_max_hit_solution(solution: f64, enemy_damage_mult: f64) -> f64 {
    if solution == f64::INFINITY {
        solution
    } else {
        vendor_round(solution / enemy_damage_mult)
    }
}

fn max_hit_solution(dtype: DamageType, inputs: &MaxHitInputs) -> f64 {
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
        // :3607 totalTakenMulti = AfterReductionTakenHitMulti ×(1−VAA) (VAA has no source → 1).
        let total_taken_multi = mit.after_reduction_multi[c];
        let resist_mult = mit.resist_taken_multi[c];
        let hit_taken = if convert <= 0.0 {
            // :3611-3613 takenFlat only: if flat alone depletes the pool → 0, otherwise this target imposes no constraint (∞).
            let taken_without_incoming = taken_flat.max(0.0) * total_taken_multi;
            if taken_without_incoming >= total_pool {
                0.0
            } else {
                f64::INFINITY
            }
        } else if total_taken_multi <= 0.0 {
            // A taken multiplier of 0 (e.g. CI's ChaosDamageTaken MORE −100)
            // → immunity: in Lua, x/0 = inf naturally gives ∞; this is an
            // explicit short-circuit here to avoid a 0×∞ NaN branch.
            f64::INFINITY
        } else if eff_armour == 0.0 && convert >= 1.0 {
            // :3614-3617 a simplified closed form with no armour DR (here panel DR = clamp(min(drMax, flat) − ow)).
            let dr_pct = (mit.flat_dr_pct[c].min(mit.dr_max_pct[c]) - mit.overwhelm_pct[c])
                .clamp(0.0, mit.dr_max_pct[c]);
            let dr_multi = resist_mult * (1.0 - dr_pct / 100.0);
            (total_pool / convert / dr_multi - taken_flat).max(0.0) / total_taken_multi
        } else {
            // :3620-3641 the quadratic-equation solution + noDR/maxDR bounds + floor.
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
            // :3637-3639 the bounds (no DR / full DR); the ceiling uses the
            // **source** type's drMax (matching vendor's `:3638`).
            let no_dr_max_hit = total_pool / convert / resist_mult / total_taken_multi
                * (1.0 - taken_flat * total_taken_multi / total_pool);
            let max_dr_max_hit = no_dr_max_hit / (1.0 - (mit.dr_max_pct[src] - overwhelm) / 100.0);
            // :3641 smoothing is enabled under partial conversion (set only in the quadratic-equation branch, matching vendor).
            use_smoothing = use_smoothing || (convert - 1.0).abs() > f64::EPSILON;
            raw.min(max_dr_max_hit).max(no_dr_max_hit).floor()
        };
        part_min = part_min.min(hit_taken);
    }

    if part_min.is_infinite() {
        return f64::INFINITY;
    }
    if !use_smoothing {
        // :3696 no conversion: fold directly by the enemy damage multiplier.
        return part_min;
    }
    // :3663-3692 conversion smoothing: starting from partMin, measures the
    // overkill from actually running takenHit → reducePools, then converges
    // step by step per the overkill ratio (converges when |overkill| < 1).
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
        // :3672-3679 passRatio: takes the max of each impacted pool's (overkill+pool)/pool (≤0 → 1).
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
        // :3682-3686 step size: the ratio between two consecutive rounds' overkill (capped at 2) decides the adjustment direction and magnitude.
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
    pass_incoming
}

/// The "not hit" chance for all four hit variants (vendor CalcDefence.lua:2018-2026).
///
/// PoE2 has no dodge (vendor's factor for it is always 1); specificTypeAvoidance
/// has no source → AvoidProjectiles counts toward the Projectile/SpellProjectile
/// variants. `average_evade` is `:2025`'s original formula (melee/proj takes
/// Evade, spell/spellProj takes NotHit -- a literal copy of vendor's semantics).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct NotHitSuite {
    pub melee: f64,
    pub projectile: f64,
    pub spell: f64,
    pub spell_projectile: f64,
    /// `AverageNotHitChance` (the mean of all four variants, the Configured
    /// value for damageCategory=Average).
    pub average: f64,
    /// `AverageEvadeChance` (`:2025`; the evade-discount term for enemy crit chance).
    pub average_evade: f64,
}

/// Composes a [`NotHitSuite`] from the defence panel output (fields
/// decoupled through OutputTable by Track D/E). When D/E aren't wired up,
/// each input is 0 → every NotHit = 0 (neutral).
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

/// EHP PoB2-view fill (produces the output / the F-3 semantic switchover;
/// called as a single line at the end of perform's `fill_mechanics`, must
/// come **after** `fill_evade_stun` / `fill_defence_panels` -- the not-hit/
/// block/deflect layer reads the OutputTable fields they write; fields not
/// yet wired up default to 0 → neutral 1.0).
///
/// **F-3 semantic switchover**:
/// - `total_ehp` = the new view (`mitigatedHits / (1−notHit) × totalEnemyDamageIn`,
///   CalcDefence.lua:3271/:3322); the old lowest-max-hit value is kept in
///   `total_ehp_lowest_max_hit` (perform's old pipeline still writes it as
///   before, no code removed -- reverting this function's switchover section
///   at the end restores the old view).
/// - `*_max_hit` = the new view (the TotalHitPool pool-expansion layer +
///   taken-as, :3540-3697); `*_max_hit_pob2` is kept as a same-value alias.
/// - `avoid_stun` / the Stun system switches to the **real value**
///   totalTakenHit (per-hit taken damage, aggregated at :2444 → the ES
///   halving condition :2554-2557 / the threshold chance :2525-2643) --
///   replacing the reference_hit approximation from the Track E wiring period.
///
/// Return value (part of 13-G15): the per-type recoupable damage accumulated by
/// the mitigated EHP loop (`Σ <X>RecoupableDamageTaken`),
/// in Physical/Fire/Cold/Lightning/Chaos order — perform uses this as the
/// damage-taken base for the recoup panel rates (replacing the old life×10%
/// estimate). All zero when there's no recoup mod / the mitigated loop wasn't
/// recomputed (the consumer's recoup pct is likewise 0 → rate 0, consistent semantics).
/// Life recoup sums the array; ES recoup reads it per-type (elemental ES recoup
/// applies to fire/cold/lightning recoupable only).
pub fn fill_ehp_pob2(
    env: &mut Env,
    keystones: &DefenceKeystones,
    resistances: &ResistanceSuite,
) -> [f64; 5] {
    struct Computed {
        life_recoverable: f64,
        es_recovery_cap: f64,
        physical_dr: f64,
        n_hits: f64,
        n_mitigated: f64,
        total_hits: f64,
        total_in: f64,
        total_ehp_pob2: f64,
        max_hits: [f64; 5],
        avoid_stun: f64,
        stun_threshold: f64,
        self_stun_chance: f64,
        stun_duration: f64,
        recoupable_by_type: [f64; 5],
        terms: EhpTerms,
    }
    let computed = {
        let db = &env.player.mod_db;
        let enemy_db = &env.enemy.mod_db;
        let cfg = &env.cfg;
        let out = &env.player.output;

        // Pool view (:1411 the ES recovery cap / :2644-2657 recoverable life)
        // CappingES (flags like ArmourESRecoveryCap) and the lowLife/lowES config are left for later.
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
        let pools = build_pool_state(db, cfg, &base, out.deflect_chance);
        let mom = mom_hit_pools(&ctx, &base);

        // Mitigation snapshot (the Track B contract; deflect is folded into Track D's output, :2433)
        // Enemy elemental penetration (vendor CalcDefence.lua:2328/:2363):
        // `resMult = 1 − max(resist − enemyPen, 0)/100` (only deducted when
        // resist > 0; negative resistance is unaffected by pen). pen's
        // source = the `Enemy<X>Pen` placeholder injected by setup_enemy
        // (Pinnacle 3 / Uber 8, ConfigOptions.lua:2072-2074, Modules/Data.lua:231);
        // physical/chaos have no pen.
        let enemy_pen = |name: &str| enemy_base(enemy_db, cfg, name);
        // Physical and Chaos have no enemy penetration input.
        let enemy_pen = [
            0.0,
            enemy_pen("EnemyFirePen"),
            enemy_pen("EnemyColdPen"),
            enemy_pen("EnemyLightningPen"),
            0.0,
        ];
        let resist_before_pen = [
            0.0,
            resistances.fire,
            resistances.cold,
            resistances.lightning,
            resistances.chaos,
        ];
        let resist_after_pen = |resist: f64, pen: f64| -> f64 {
            if resist > 0.0 {
                (resist - pen).max(0.0)
            } else {
                resist
            }
        };
        let deflect_effect_pct = super::defence_panels::deflect_effect_terms(db, cfg).percent();
        let category = super::taken::HitCategory::from_enemy(enemy_db, cfg);
        let mut mit = build_mitigation_ctx_for_category(
            db,
            cfg,
            &MitigationInputs {
                armour: out.armour,
                evasion: out.evasion,
                energy_shield: out.energy_shield,
                resist_pct: std::array::from_fn(|i| {
                    resist_after_pen(resist_before_pen[i], enemy_pen[i])
                }),
                deflect_chance_pct: out.deflect_chance,
                deflect_effect_pct,
            },
            category,
        );
        mit.overwhelm_pct[DamageType::Physical as usize] +=
            enemy_base(enemy_db, cfg, "EnemyPhysicalOverwhelm");
        // CI chaos immunity: vendor's keystone mod set includes
        // `ChaosDamageTaken MORE -100` (ModParser.lua:2356/2360) →
        // ChaosTakenHitMult = 0. pobr's parse side doesn't emit this numeric
        // mod yet (to avoid disturbing the old view's output), so the new
        // pipeline applies the equivalent via the C-1 keystone snapshot instead.
        if keystones.chaos_inoculation {
            let chaos = DamageType::Chaos as usize;
            mit.after_reduction_multi[chaos] = 0.0;
            mit.hit_taken_multi[chaos] = 0.0;
            mit.attack_taken_multi[chaos] = 0.0;
            mit.spell_taken_multi[chaos] = 0.0;
        }

        // The not-hit layer (:2018-2026, reads Track E's evade for all four variants + the avoid output)
        let nh = not_hit_suite(out);
        let worst_of = enemy_db.override_(cfg, "EHPUnluckyWorstOf").unwrap_or(1.0);
        let unlucky = |percent: f64| 100.0 * (percent / 100.0).clamp(0.0, 1.0).powf(worst_of);
        let not_hit_before_unlucky = category.select(
            [nh.melee, nh.projectile, nh.spell, nh.spell_projectile],
            out.avoid_all_damage_from_hits,
        );
        let not_hit = unlucky(not_hit_before_unlucky);
        let evade = unlucky(if category == super::taken::HitCategory::Average {
            nh.average_evade
        } else {
            category.select(
                [
                    out.melee_evade_chance,
                    out.projectile_evade_chance,
                    out.spell_evade_chance,
                    out.spell_projectile_evade_chance,
                ],
                0.0,
            )
        });

        // Enemy incoming damage (:2040-2137; the placeholder is injected into the enemy modDB by setup_enemy)
        let crit_terms = enemy_crit_chance_terms(db, enemy_db, cfg, evade);
        let crit_effect =
            enemy_crit_effect_with(crit_terms, enemy_db, cfg, out.crit_extra_damage_reduction);
        let enemy_in = assemble_enemy_damage(enemy_db, cfg, crit_effect);

        // Retain the same conversion and mitigation terms used by the EHP loop.
        let hit = taken_hit_terms(&enemy_in.damage, &mit);
        let taken_hit = hit.hit;
        let physical_before_reduction = hit.damage[0];
        let physical_reduction = hit.reduction[0];

        // avoid_stun / Stun real-value wiring
        // vendor totalTakenHit = Σ <X>TakenHit (:2444); the ES halving condition
        // `ES > totalTakenHit && !EnergyShieldProtectsMana` (:2554-2557);
        // SelfStunChance's effective damage uses totalTakenHit/PhysicalTakenHit (:2525-2643).
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

        // Lethal hit count (:3148-3153)
        // preventedLifeLossTotal > 0 → LimitEHPSpeedup (:3151).
        let below_half_effective =
            (1.0 - ctx.prevented_life_loss / 100.0) * ctx.life_loss_below_half_prevented;
        let prevented_total = ctx.prevented_life_loss > 0.0 || below_half_effective > 0.0;
        let params = EhpLoopParams::from_constants(&cfg.constants, prevented_total);
        let n_hits = number_of_hits_to_die(&taken_hit, &pools, &ctx, &params);

        // The mitigation probability layer (:3155-3247)
        // Average block = the mean of all four variants (vendor `:1067`'s
        // EffectiveAverageBlockChance). The old two-variant mean missed
        // SpellProjectileBlock = max(spellBlock, projBlock) (`:1013`) --
        // underestimating shield builds (spellBlock 0, projBlock = block):
        // smith 13.65 vs vendor 20.475 (the root cause of TotalEHP being 0.92x).
        let avg_block_frac = unlucky(category.select(
            [
                out.effective_block_chance,
                out.effective_projectile_block_chance,
                out.effective_spell_block_chance,
                out.effective_spell_projectile_block_chance,
            ],
            0.0,
        )) / 100.0;
        // vendor's BlockEffect (blocked-off share %) = 100 − ΣBASE = 100 − out.block_effect (the damage-taken share).
        let block_effect_mult = 1.0 - avg_block_frac * (100.0 - out.block_effect) / 100.0;
        // :3195 the deflect multiplier (the chance<100 view; already folded
        // into afterReductionMulti when =100).
        let deflect_mult = if out.deflect_chance < 100.0 {
            1.0 - unlucky(out.deflect_chance) * deflect_effect_pct / 10_000.0
        } else {
            1.0
        };
        // Per-type hit avoidance (vendor CalcDefence.lua:3262/:3277-3300):
        // when there's an `Avoid<Type>DamageChance` source, averageAvoidChance
        // = the mean across the five types, folded into
        // configured_damage_chance; each type additionally gets the
        // "Average" damageCategory's ExtraAvoidChance = projectile
        // avoidance/2 (`:3262`), then clamped to 75. Without per-type
        // avoidance, averageAvoidChance = 0, unchanged bit-for-bit from the
        // old value (zero behavior change for every other build).
        let specific_type_avoidance = avoidance.avoid_typed_damage.iter().any(|&a| a > 0.0);
        let extra_avoid = avoidance.avoid_projectile_damage * category.projectile_share();
        let avoid_cap = crate::calc::defence::AVOID_HIT_CAP;
        let avoid: [f64; 5] = avoidance.avoid_typed_damage.map(|base| {
            if specific_type_avoidance {
                unlucky((base + extra_avoid).min(avoid_cap))
            } else {
                0.0
            }
        });
        let average_avoid_chance = avoid.iter().sum::<f64>() / 5.0;
        let configured_damage_chance =
            100.0 * block_effect_mult * deflect_mult * (1.0 - average_avoid_chance / 100.0);
        // F-4: anyRecoup now reads the mod itself (vendor `:1795-1812`'s
        // `Σ <Resource>Recoup` BASE; the recoup rate field hasn't been
        // written yet at this point -- its base value comes precisely from this section's mitigated loop accumulation).
        // Elemental ES recoup (Fire/Cold/LightningEnergyShieldRecoup) also
        // triggers the loop — otherwise a pure-elemental-recoup build would
        // never accumulate recoupable and the mods would stay unread.
        let any_recoup = [
            "LifeRecoup",
            "PhysicalDamagePreventedLifeRecoup",
            "ManaRecoup",
            "EnergyShieldRecoup",
            "FireEnergyShieldRecoup",
            "ColdEnergyShieldRecoup",
            "LightningEnergyShieldRecoup",
        ]
        .iter()
        .any(|name| db.sum(ModType::Base, cfg, &[ModName::from(*name)]) > 0.0);
        // Recovery on block (:1948-1960 outputs, :3261-3268 `× BlockChance`). PoBR
        // evaluates the "Average" damage category, so spell-block recovery counts
        // half (:3266-3267); `DisableEHPGainOnBlock` turns the whole layer off.
        let gain = if cfg
            .conditions
            .get("DisableEHPGainOnBlock")
            .copied()
            .unwrap_or(false)
        {
            GainWhenHit::default()
        } else {
            let on_block = |name: &str| db.sum(ModType::Base, cfg, &[ModName::from(name)]);
            GainWhenHit {
                life: if db.flag(cfg, "CannotRecoverLifeOutsideLeech") {
                    0.0
                } else {
                    on_block("LifeOnBlock") * avg_block_frac
                },
                mana: on_block("ManaOnBlock") * avg_block_frac,
                energy_shield: (on_block("EnergyShieldOnBlock")
                    + on_block("EnergyShieldOnSpellBlock") * category.spell_share())
                    * avg_block_frac,
                ward: on_block("WardOnBlock") * avg_block_frac,
            }
        };
        // Convention (no in-game test): only the Physical reduction stage
        // counts, after taken-as shifts and before damage-taken multipliers.
        // Blocked/deflected hits contribute nothing even when they deal damage.
        // Evasion is already outside this loop of landed hits; it changes the
        // number of attacks needed, not the accumulated damage before death.
        let physical_prevented_per_hit = physical_before_reduction
            * (1.0 - physical_reduction.taken_multiplier())
            * (1.0 - avg_block_frac)
            * (1.0 - unlucky(out.deflect_chance) / 100.0)
            * (1.0 - avoid[0] / 100.0);
        let (n_mitigated, recoup) = if (configured_damage_chance - 100.0).abs() > f64::EPSILON
            || any_recoup
            || prevented_total
            || gain.any()
        {
            // Per-type reduction (vendor :3277-3300's DamageIn[type] × (1 - avoid_type/100)) --
            // per-type avoidance can't go through scale_damage's uniform
            // factor (that's reused elsewhere in the hits_to_die iteration
            // and must stay uniform). With no per-type avoidance, avoid is
            // entirely 0, matching the old uniform scaling bit-for-bit.
            let base_mult = block_effect_mult * deflect_mult;
            let mitigated_in = super::pool_damage::TypedDamage {
                physical: taken_hit.physical * base_mult * (1.0 - avoid[0] / 100.0),
                fire: taken_hit.fire * base_mult * (1.0 - avoid[1] / 100.0),
                cold: taken_hit.cold * base_mult * (1.0 - avoid[2] / 100.0),
                lightning: taken_hit.lightning * base_mult * (1.0 - avoid[3] / 100.0),
                chaos: taken_hit.chaos * base_mult * (1.0 - avoid[4] / 100.0),
            };
            // :3338 LimitEHPSpeedup = TrackRecoupable or TrackLifeLossOverTime or GainWhenHit.
            let m_params = EhpLoopParams {
                gain,
                ..EhpLoopParams::from_constants(
                    &cfg.constants,
                    any_recoup || prevented_total || gain.any(),
                )
            };
            hits_to_die_with_recoup(
                &mitigated_in,
                &pools,
                &ctx,
                &m_params,
                physical_prevented_per_hit,
            )
        } else {
            (n_hits, RecoupAccumulation::default())
        };
        let recoupable_by_type = recoup.by_type;
        let recoupable_total = recoupable_by_type.iter().sum();

        // TotalEHP (`:3271`'s TotalNumberOfHits + `:3322`)
        let not_hit_frac = (not_hit / 100.0).clamp(0.0, 1.0);
        let total_hits = if not_hit_frac >= 1.0 {
            f64::INFINITY
        } else {
            n_mitigated / (1.0 - not_hit_frac)
        };
        // Under a bare Env (no enemy incoming damage), total_in = 0, total_hits = ∞ → neutral 0 (avoiding an ∞×0 NaN).
        let total_ehp_pob2 = if enemy_in.total_in > 0.0 {
            round(total_hits * enemy_in.total_in)
        } else {
            0.0
        };

        // New-view max hit (:3540-3697)
        let (base_hit_pool, pool_by_type) =
            total_hit_pool_stages(&mom, es_recovery_cap, &pools, &ctx);
        // Diagnostics: dumps the pool breakdown when POBR_DBG_EHPPOOL=1 (for
        // comparison against the oracle's <Type>TotalHitPool / MoMHitPool / Ward).
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
        let mut max_hit_solutions = [0.0_f64; 5];
        for dtype in DAMAGE_TYPE_BY_INDEX {
            let i = dtype as usize;
            max_hit_solutions[i] = max_hit_solution(dtype, &inputs);
            max_hits[i] = scale_max_hit_solution(max_hit_solutions[i], enemy_in.mult_by_type[i]);
        }

        Computed {
            life_recoverable,
            es_recovery_cap,
            physical_dr: 100.0 - physical_reduction.taken_multiplier() * 100.0,
            n_hits,
            n_mitigated,
            total_hits,
            total_in: enemy_in.total_in,
            total_ehp_pob2,
            max_hits,
            avoid_stun: avoidance.avoid_stun,
            stun_threshold: stun.threshold,
            self_stun_chance: stun.self_stun_chance,
            stun_duration: stun.stun_duration,
            recoupable_by_type,
            terms: EhpTerms {
                enemy_damage: by_type(&enemy_in.damage),
                taken_hit: by_type(&taken_hit),
                taken_damage: hit.damage,
                damage_shift: mit.shift,
                damage_reduction: hit.reduction,
                resist_taken_multi: mit.resist_taken_multi,
                taken_flat: mit.taken_flat,
                after_reduction_multi: mit.after_reduction_multi,
                energy_shield_bypass: ctx.es_bypass_by_type,
                block_effect_multiplier: block_effect_mult,
                deflect_multiplier: deflect_mult,
                average_avoid_chance,
                not_hit_before_unlucky,
                unlucky_worst_of: worst_of,
                enemy_crit_chance: crit_terms,
                total_hit_pool: pool_by_type,
                damaging_hits: n_hits,
                damage_chance: configured_damage_chance,
                mitigated_hits: n_mitigated,
                not_hit_chance: not_hit,
                enemy_damage_in: enemy_in.total_in,
                physical_reduction,
                physical_prevented: recoup.physical_prevented,
                recoupable_total,
                recoupable_by_type,
                max_hit_solutions,
                enemy_damage_multipliers: enemy_in.mult_by_type,
                enemy_pen,
                resist_before_pen,
                mitigation: mit,
                mom,
                life_hit_pool_terms: LifeHitPoolTerms {
                    life: base.life_recoverable,
                    max_life: base.max_life,
                    loss_prevented: ctx.prevented_life_loss,
                    below_half_loss_prevented: ctx.life_loss_below_half_prevented,
                },
                base_hit_pool,
            },
        }
    };
    let out = &mut env.player.output;
    out.enemy_crit_chance = computed.terms.enemy_crit_chance.chance();
    out.second_minimal_maximum_hit_taken = second_minimal_maximum_hit(computed.max_hits);
    out.ehp_terms = Some(computed.terms);
    out.life_recoverable = computed.life_recoverable;
    out.energy_shield_recovery_cap = computed.es_recovery_cap;
    out.physical_damage_reduction = computed.physical_dr;
    out.number_of_damaging_hits = computed.n_hits;
    out.number_of_mitigated_hits = computed.n_mitigated;
    out.total_number_of_hits = computed.total_hits;
    out.total_enemy_damage_in = computed.total_in;
    out.total_ehp_pob2 = computed.total_ehp_pob2;
    out.physical_max_hit_pob2 = computed.max_hits[DamageType::Physical as usize];
    out.fire_max_hit_pob2 = computed.max_hits[DamageType::Fire as usize];
    out.cold_max_hit_pob2 = computed.max_hits[DamageType::Cold as usize];
    out.lightning_max_hit_pob2 = computed.max_hits[DamageType::Lightning as usize];
    out.chaos_max_hit_pob2 = computed.max_hits[DamageType::Chaos as usize];

    // F-3 semantic switchover section
    // The canonical fields now hold the new-view values; the old
    // lowest-max-hit view is kept in `total_ehp_lowest_max_hit`, already
    // written by perform's old pipeline (code not removed).
    out.total_ehp = computed.total_ehp_pob2;
    out.physical_max_hit = computed.max_hits[DamageType::Physical as usize];
    out.fire_max_hit = computed.max_hits[DamageType::Fire as usize];
    out.cold_max_hit = computed.max_hits[DamageType::Cold as usize];
    out.lightning_max_hit = computed.max_hits[DamageType::Lightning as usize];
    out.chaos_max_hit = computed.max_hits[DamageType::Chaos as usize];
    // avoid_stun / the Stun system's real values (overwriting fill_evade_stun's reference_hit approximation).
    out.avoid_stun = computed.avoid_stun;
    out.stun_threshold = computed.stun_threshold;
    out.self_stun_chance = computed.self_stun_chance;
    out.stun_duration = computed.stun_duration;

    computed.recoupable_by_type
}
