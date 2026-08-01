//! The damage pool deduction state machine (13-G2/G5) -- corresponds to
//! PoB2 `CalcDefence.lua:461-678 reducePoolsByDamage` (order: allies → aegis
//! → guard → ward → ES(bypass) → MoM → loss-prevention → life → overkill)
//! and `:3540-3601`'s max-hit TotalHitPool expansion layer.
//!
//! Module split:
//! - This file = the state machine evaluation ([`reduce_pools`]) + pool
//!   formula primitives ([`pool_protected`] / [`life_hit_pool_with_loss_prevention`]
//!   / [`apply_protected_layer`]) + the max-hit pool layer
//!   ([`total_hit_pool_base`] / [`extend_total_hit_pool`]);
//! - Setup (reading bypass/MoM/guard/aegis etc. from the ModDb to build
//!   [`PoolCtx`]/[`PoolState`]) lives in `pool_setup.rs` -- **this file never
//!   reads the ModDb** (setup is kept separate from evaluation).
//!
//! Pure function convention: inputs and outputs are all value types, this
//! **never writes `Env`** and holds no shared mutable state; writes to
//! `Env`/`OutputTable` are still centralized in `perform.rs` (Track F).

use pobr_data::constants::DamageType;

/// PoB2's defence-side damage type traversal order (`CalcDefence.lua:27`'s
/// `dmgTypeList = {"Physical", "Lightning", "Cold", "Fire", "Chaos"}`).
///
/// **Differs** from pobr's [`DamageType`] enum order (Physical/Fire/Cold/
/// Lightning/Chaos, the per-type array index): the state machine deducts
/// allies→ward in this order forward, and ES→life in this order
/// **reversed** (Chaos first) (`:578`'s `for i=#dmgTypeList,1,-1`). Shared
/// pools (shared aegis/guard/ward/ES/mana/life) are consumed sequentially
/// across types, so traversal order affects the result and must match vendor.
pub const POB2_DAMAGE_ORDER: [DamageType; 5] = [
    DamageType::Physical,
    DamageType::Lightning,
    DamageType::Cold,
    DamageType::Fire,
    DamageType::Chaos,
];

/// The five-type damage vector (after the taken factor, before entering the pool; equivalent to PoB2's damageTable).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct TypedDamage {
    pub physical: f64,
    pub fire: f64,
    pub cold: f64,
    pub lightning: f64,
    pub chaos: f64,
}

impl TypedDamage {
    /// Gets a component by [`DamageType`].
    pub fn get(&self, dtype: DamageType) -> f64 {
        match dtype {
            DamageType::Physical => self.physical,
            DamageType::Fire => self.fire,
            DamageType::Cold => self.cold,
            DamageType::Lightning => self.lightning,
            DamageType::Chaos => self.chaos,
        }
    }

    /// Sum of the five components (total incoming damage).
    pub fn total(&self) -> f64 {
        self.physical + self.fire + self.cold + self.lightning + self.chaos
    }
}

/// The allies before-you layer (frost shield / spectre / totem / vaal
/// rejuvenation totem / radiance sentinel / soul link, etc.; PoB2 poolTable's
/// allies section, CalcDefence.lua:466-489).
///
/// Each layer deducts its `{remaining, percent}` share first, and **doesn't
/// count toward recoupable** (per the `:529-530` comment "taken before you does not count as you taking damage").
#[derive(Debug, Clone, PartialEq)]
pub struct AllyLayer {
    /// Layer ID (a stable identifier, used for breakdown / resources_lost), e.g. `"frostShield"`.
    pub id: &'static str,
    /// This layer's remaining absorbable amount.
    pub remaining: f64,
    /// This layer's share (%, 0-100; CalcDefence.lua:527's `damageRemainder × percent` split).
    pub mitigation_pct: f64,
    /// A layer that only applies to a single damage type (`:526`'s
    /// `allyValues.damageType` filter; `None` = all types).
    pub damage_type: Option<DamageType>,
}

/// A complete pool snapshot before deduction (corresponds to PoB2's
/// poolTable; built once, passed by value through the EHP loop).
///
/// Per-type array index convention = [`DamageType`]'s enum order
/// (Physical/Fire/Cold/Lightning/Chaos), matching [`TypedDamage`]'s fields
/// one-to-one; see [`POB2_DAMAGE_ORDER`] for the traversal order.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PoolState {
    /// The allies before-you layer (an empty Vec when the player has no
    /// frost shield etc. source this stage; the structure is kept regardless).
    pub allies: Vec<AllyLayer>,
    /// Shared Aegis (all types, CalcDefence.lua:551-556).
    pub aegis_shared: f64,
    /// Elemental shared Aegis (only elemental types can deduct it, :545-550).
    pub aegis_shared_elemental: f64,
    /// Per-type Aegis (:539-544).
    pub aegis_by_type: [f64; 5],
    /// Shared Guard pool and absorption rate (%; CalcDefence.lua:563-568 / set up at :2823-2826).
    pub guard_shared: f64,
    pub guard_shared_rate: f64,
    /// Per-type Guard pool and absorption rate (%; :557-562 / set up at :2838-2845).
    pub guard_by_type: [f64; 5],
    pub guard_rate_by_type: [f64; 5],
    /// Ward (absorbs `×(1−WardBypass/100)`, returned when `WardNotBreak`, :569-573).
    pub ward: f64,
    /// Energy shield pool (the EHP view = EnergyShieldRecoveryCap).
    pub energy_shield: f64,
    /// Mana pool (the EHP view = ManaUnreserved; consumed by MoM/EB).
    pub mana: f64,
    /// Life pool (the EHP view = LifeRecoverable).
    pub life: f64,
    /// Accumulated deferred loss redirected by loss-prevention (the
    /// above-half / below-half segments, CalcDefence.lua:611-651).
    pub life_loss_lost_over_time: f64,
    pub life_below_half_loss_lost_over_time: f64,
}

/// Context that doesn't change per hit (flags / ratios read from the ModDb
/// and resolved; the state machine itself never reads the ModDb).
///
/// Compare against the setup entry point (`pool_setup.rs::build_pool_ctx`):
/// per-type ES bypass CalcDefence.lua:2707-2722, MoM shared/per-type
/// :2728/:2773, loss-prevention :2662-2665.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PoolCtx {
    /// Maximum life (the half-life baseline for the segmented life hit pool, vendor `output.Life`).
    pub max_life: f64,
    /// Per-type ES bypass (%, 0-100; vendor :2715's Override or Σ BASE,
    /// clamped at :2720. Note that in PoE2, chaos does **not** default to
    /// bypass -- chaos double-deducting ES (:582) takes its place instead).
    pub es_bypass_by_type: [f64; 5],
    /// Shared MoM ratio (%, 0-100; `min(Σ DamageTakenFromManaBeforeLife, 100)`, :2728).
    pub mom_shared: f64,
    /// Per-type MoM ratio (%; `<X>DamageTakenFromManaBeforeLife`,
    /// `min(Σ, 100 − shared)`, :2773).
    pub mom_by_type: [f64; 5],
    /// Ward bypass (%, expected 0-100; vendor :572 doesn't clamp, so this preserves the raw-value semantics).
    pub ward_bypass: f64,
    /// The EternalLife branch (CalcDefence.lua:587-594, mutually exclusive
    /// with the normal ES branch; the bypass portion is directly "waived" rather than passing through to life).
    pub eternal_life: bool,
    /// EB (the `EnergyShieldProtectsMana` flag): ES nests to protect Mana
    /// (the manaProtected formula from pool setup :2735-2744 /
    /// :2782-2791; the state machine itself doesn't branch on this
    /// directly, it takes effect through the setup result).
    pub eb: bool,
    /// Chaos doesn't double-deduct ES (defaults to chaos's
    /// `esDamageTypeMultiplier = 2`, :582).
    pub chaos_not_double_es: bool,
    /// WardNotBreak: ward doesn't break after absorbing (`:509` returns the
    /// original value); in EHP, when damage is below ward, the lethal hit
    /// count = ∞ (`:3030`, Track F).
    pub ward_not_break: bool,
    /// Life loss prevention (%; `min(Σ LifeLossPrevented, 100)`, :2662).
    pub prevented_life_loss: f64,
    /// Below-half-life loss prevention (%; the raw Σ `LifeLossBelowHalfPrevented` BASE sum, :514/:2664).
    pub life_loss_below_half_prevented: f64,
}

impl PoolCtx {
    /// Vendor's `output.preventedLifeLossBelowHalf` (:2665):
    /// `(1 − preventedLifeLoss/100) × Σ LifeLossBelowHalfPrevented`.
    /// This is `reduce_pools`'s `:623` branch condition (≠0 takes the above/below-half segments).
    fn prevented_life_loss_below_half_effective(&self) -> f64 {
        (1.0 - self.prevented_life_loss / 100.0) * self.life_loss_below_half_prevented
    }
}

/// The complete result of one pool deduction (equivalent to PoB2's
/// reducePoolsByDamage return, `poolsRemaining`, plus per-deduction detail).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PoolsAfter {
    /// The pool snapshot after deduction (the `ward` field follows return
    /// semantics: restored to its original value under `WardNotBreak`, else 0, :509/:666).
    pub pools: PoolState,
    /// Per-type recoupable damage (the allies before-you layer **does not
    /// count** toward this, CalcDefence.lua:529-530).
    pub recoupable_by_type: [f64; 5],
    /// Overflow kill amount (damage left over after the life pool is
    /// depleted, used for the EHP fractional-hit-count calculation, :619-621/:656).
    pub overkill: f64,
    /// Hit pool remainder (`:659-660`: the segmented hit pool from the
    /// post-deduction life, plus the remaining MoM pool, plus the remaining ES pool, floored).
    pub hit_pool_remaining: f64,
    /// Per-type, per-layer deduction detail (for breakdown): (damage type,
    /// layer ID, amount deducted). The same (type, layer) pair may appear
    /// multiple times (e.g. the loss-prevention segment and the normal
    /// segment each deduct life once); the caller aggregates them as needed.
    /// Layer ID values (matching a snake_case version of vendor's
    /// resourcesLostToTypeDamage keys):
    /// `"ally:<id>"` / `"aegis"` / `"shared_elemental_aegis"` / `"shared_aegis"` /
    /// `"guard"` / `"shared_guard"` / `"ward"` / `"energy_shield"` /
    /// `"eternal_life_prevented"` / `"mana"` / `"life"` / `"life_loss_prevented"` /
    /// `"overkill"`.
    /// Note: vendor only records a breakdown row when the amount is ≥1 (a
    /// display-layer truncation); this implementation keeps every detail >0, losing no precision.
    pub resources_lost: Vec<(DamageType, &'static str, f64)>,
}

/// The damage pool deduction state machine (a pure function, mirroring CalcDefence.lua:461-678 line by line).
///
/// Fixed order:
/// 1. **Forward** ([`POB2_DAMAGE_ORDER`]) per type: allies (:525-531, deducts
///    its share first, doesn't count toward recoupable) → recoupable
///    bookkeeping (:531) → aegis (per-type :539 → sharedElemental, elemental
///    only, :545 → shared :551) → guard (per-type :557 and shared :563 each
///    absorb by AbsorbRate% share) → ward (:569 `×(1−WardBypass/100)`);
/// 2. **Reversed** (:578, Chaos first) per type: ES (chaos doubles unless
///    `ChaosNotDoubleESDamage`, :582; per-type bypass; `EternalLife`
///    :587-594 is mutually exclusive with the normal branch :594-601) → MoM
///    (:602-609 `MoMPool = min(lifeHitPool/(1−MoM)−lifeHitPool, mana)`) →
///    loss-prevention (:611-651 the above/below-half segments; any amount
///    exceeding what life can still take is recorded as overkill first) →
///    life (:651-655) → overkill (:656).
///
/// Types with a damage component ≤0 are skipped (vendor gates this on
/// damageTable key existence; EHP/max-hit callers only pass positive components).
pub fn reduce_pools(pools: &PoolState, hit: &TypedDamage, ctx: &PoolCtx) -> PoolsAfter {
    let mut allies = pools.allies.clone();
    let mut aegis_by_type = pools.aegis_by_type;
    let mut aegis_shared_elemental = pools.aegis_shared_elemental;
    let mut aegis_shared = pools.aegis_shared;
    let mut guard_by_type = pools.guard_by_type;
    let mut guard_shared = pools.guard_shared;
    let mut ward = pools.ward;
    let mut energy_shield = pools.energy_shield;
    let mut mana = pools.mana;
    let mut life = pools.life;
    let mut life_loss_lost_over_time = pools.life_loss_lost_over_time;
    let mut life_below_half_loss_lost_over_time = pools.life_below_half_loss_lost_over_time;

    // :509 ward return semantics: WardNotBreak keeps the original value, otherwise it's zeroed after the hit.
    let restore_ward = if ctx.ward_not_break { ward } else { 0.0 };

    let mut recoupable_by_type = [0.0_f64; 5];
    let mut overkill = 0.0_f64;
    let mut resources_lost: Vec<(DamageType, &'static str, f64)> = Vec::new();
    // :520-521: the MoM/ES pool remainder takes the min across types; the
    // m_huge sentinel means that pool was never evaluated.
    let mut mom_pool_remaining = f64::INFINITY;
    let mut es_pool_remaining = f64::INFINITY;

    // Records one deduction detail entry (keeps every detail >0; vendor only
    // records breakdown rows ≥1, a display-layer difference).
    let lose = |list: &mut Vec<(DamageType, &'static str, f64)>,
                dtype: DamageType,
                layer: &'static str,
                amount: f64| {
        if amount > 0.0 {
            list.push((dtype, layer, amount));
        }
    };

    // First half: forward-order allies → aegis → guard → ward (:524-575)
    let mut remainder_before_es = [0.0_f64; 5];
    for dtype in POB2_DAMAGE_ORDER {
        let idx = dtype as usize;
        let mut rem = hit.get(dtype);
        if rem <= 0.0 {
            continue;
        }
        // allies (:525-530): each layer shares by percent, capped at that layer's remainder.
        for ally in allies.iter_mut() {
            if (ally.damage_type.is_none() || ally.damage_type == Some(dtype))
                && ally.remaining > 0.0
            {
                let temp = (rem * ally.mitigation_pct / 100.0).min(ally.remaining);
                ally.remaining -= temp;
                rem -= temp;
                if temp > 0.0 {
                    resources_lost.push((dtype, ally.id, temp));
                }
            }
        }
        // :531 recoupable is only counted after allies (the before-you layer doesn't count as "damage you took").
        recoupable_by_type[idx] += rem;
        // aegis: per-type (:539) → sharedElemental, elemental only (:545) → shared (:551).
        if aegis_by_type[idx] > 0.0 {
            let temp = rem.min(aegis_by_type[idx]);
            aegis_by_type[idx] -= temp;
            rem -= temp;
            lose(&mut resources_lost, dtype, "aegis", temp);
        }
        if DamageType::ELEMENTAL.contains(&dtype) && aegis_shared_elemental > 0.0 {
            let temp = rem.min(aegis_shared_elemental);
            aegis_shared_elemental -= temp;
            rem -= temp;
            lose(&mut resources_lost, dtype, "shared_elemental_aegis", temp);
        }
        if aegis_shared > 0.0 {
            let temp = rem.min(aegis_shared);
            aegis_shared -= temp;
            rem -= temp;
            lose(&mut resources_lost, dtype, "shared_aegis", temp);
        }
        // guard: per-type (:557) and shared (:563) each absorb by AbsorbRate% share, capped at the pool amount.
        if guard_by_type[idx] > 0.0 {
            let temp = (rem * pools.guard_rate_by_type[idx] / 100.0).min(guard_by_type[idx]);
            guard_by_type[idx] -= temp;
            rem -= temp;
            lose(&mut resources_lost, dtype, "guard", temp);
        }
        if guard_shared > 0.0 {
            let temp = (rem * pools.guard_shared_rate / 100.0).min(guard_shared);
            guard_shared -= temp;
            rem -= temp;
            lose(&mut resources_lost, dtype, "shared_guard", temp);
        }
        // ward (:569-573): the `×(1−WardBypass/100)` portion is absorbed by ward.
        if ward > 0.0 {
            let temp = (rem * (1.0 - ctx.ward_bypass / 100.0)).min(ward);
            ward -= temp;
            rem -= temp;
            lose(&mut resources_lost, dtype, "ward", temp);
        }
        remainder_before_es[idx] = rem;
    }

    // Second half: reversed-order (Chaos first) ES → MoM → loss-prevention → life → overkill (:578-657)
    for dtype in POB2_DAMAGE_ORDER.into_iter().rev() {
        let idx = dtype as usize;
        let mut rem = remainder_before_es[idx];
        if rem <= 0.0 {
            continue;
        }
        // :582 chaos doubles against ES (unless ChaosNotDoubleESDamage).
        let es_mult = if dtype == DamageType::Chaos && !ctx.chaos_not_double_es {
            2.0
        } else {
            1.0
        };
        let es_bypass = ctx.es_bypass_by_type[idx] / 100.0;
        // :584 the segmented life hit pool (recomputed from the current life).
        let life_hit_pool = life_hit_pool_with_loss_prevention(
            life,
            ctx.max_life,
            ctx.prevented_life_loss,
            ctx.life_loss_below_half_prevented,
        );
        // :585-586 the MoM ratio and the MoM pool.
        let mom_effect = (ctx.mom_shared + ctx.mom_by_type[idx]).min(100.0) / 100.0;
        let mom_pool = if mom_effect < 1.0 {
            (life_hit_pool / (1.0 - mom_effect) - life_hit_pool).min(mana)
        } else {
            mana
        };
        if energy_shield > 0.0 && ctx.eternal_life {
            // :587-594 the EternalLife branch: the bypass portion is
            // entirely waived (eternalLifePrevented), not passing through to life.
            let temp = rem.min(energy_shield / (1.0 - es_bypass) / es_mult);
            energy_shield -= temp * (1.0 - es_bypass) * es_mult;
            es_pool_remaining = es_pool_remaining.min(energy_shield);
            rem -= temp;
            lose(
                &mut resources_lost,
                dtype,
                "energy_shield",
                temp * (1.0 - es_bypass) * es_mult,
            );
            lose(
                &mut resources_lost,
                dtype,
                "eternal_life_prevented",
                temp * es_bypass,
            );
        } else if energy_shield > 0.0 && es_bypass < 1.0 {
            // :594-601 the normal branch: the ES amount available is limited
            // by the (MoM+life) pool nested via bypass (MoMEBPool), with the
            // chaos-doubling factor applied.
            let mom_eb_pool = if es_bypass > 0.0 {
                ((mom_pool + life_hit_pool) / es_bypass * es_mult - (mom_pool + life_hit_pool))
                    .min(energy_shield)
            } else {
                energy_shield
            };
            let temp = (rem * (1.0 - es_bypass)).min(mom_eb_pool / es_mult);
            es_pool_remaining = es_pool_remaining.min(mom_eb_pool - temp * es_mult);
            energy_shield -= temp * es_mult;
            rem -= temp;
            lose(&mut resources_lost, dtype, "energy_shield", temp * es_mult);
        }
        if mom_effect > 0.0 && mana > 0.0 {
            // :602-608 MoM: rem's MoM share goes into mana, capped at MoMPool.
            let mom_damage = rem * mom_effect;
            let temp = mom_damage.min(mom_pool);
            mom_pool_remaining = mom_pool_remaining.min(mom_pool - temp);
            mana -= temp;
            rem -= temp;
            lose(&mut resources_lost, dtype, "mana", temp);
        } else {
            // :609 the no-MoM path: the MoM pool remainder is recorded as 0
            // (this feeds into the `:659` hit pool remainder sum).
            mom_pool_remaining = 0.0;
        }
        // :611-651 loss-prevention (gate = preventedLifeLossTotal > 0,
        // derived per :2670: prev>0 or belowHalf's effective value >0).
        let below_half_eff = ctx.prevented_life_loss_below_half_effective();
        if ctx.prevented_life_loss > 0.0 || below_half_eff > 0.0 {
            let half_life = ctx.max_life * 0.5;
            let life_over_half = (life - half_life).max(0.0);
            let prevent_pct = ctx.prevented_life_loss / 100.0;
            let pool_above_low = life_over_half / (1.0 - prevent_pct);
            let prevent_below_half_pct = ctx.life_loss_below_half_prevented / 100.0;
            // :617-618 how much damage life (with prevention folded in) can
            // still take; the excess is recorded as overkill first.
            let damage_that_life_can_still_take = pool_above_low
                + life.min(half_life).max(0.0)
                    / (1.0 - prevent_below_half_pct)
                    / (1.0 - prevent_pct);
            if damage_that_life_can_still_take < rem {
                overkill += rem - damage_that_life_can_still_take;
                rem = damage_that_life_can_still_take;
            }
            if below_half_eff != 0.0 {
                // :623-641 the above/below-half segments: first splits off
                // the above-half-life pool's share, then folds the rest
                // through two steps -- "non-specific below-half prevention → specific below-half prevention".
                let damage_to_split = rem.min(pool_above_low);
                let lost_life = damage_to_split * (1.0 - prevent_pct);
                let prevented_loss = damage_to_split * prevent_pct;
                rem -= damage_to_split;
                life_loss_lost_over_time += prevented_loss;
                life -= lost_life;
                lose(&mut resources_lost, dtype, "life", lost_life);
                let mut prevented_total_this_type = prevented_loss;
                if life <= half_life {
                    let unspecific = rem * prevent_pct;
                    life_loss_lost_over_time += unspecific;
                    rem -= unspecific;
                    let specific = rem * prevent_below_half_pct;
                    life_below_half_loss_lost_over_time += specific;
                    rem -= specific;
                    prevented_total_this_type += unspecific + specific;
                }
                lose(
                    &mut resources_lost,
                    dtype,
                    "life_loss_prevented",
                    prevented_total_this_type,
                );
            } else {
                // :643-647 above-half prevention only: a fixed ratio is redirected into deferred loss.
                let temp = rem * ctx.prevented_life_loss / 100.0;
                life_loss_lost_over_time += temp;
                rem -= temp;
                lose(&mut resources_lost, dtype, "life_loss_prevented", temp);
            }
        }
        // :651-655 life.
        if life > 0.0 {
            let temp = rem.min(life);
            life -= temp;
            rem -= temp;
            lose(&mut resources_lost, dtype, "life", temp);
        }
        // :656-657 overkill.
        overkill += rem;
        lose(&mut resources_lost, dtype, "overkill", rem);
    }

    // :659-660 hit pool remainder = the segmented hit pool from the
    // post-deduction life, plus the MoM/ES remainders (unevaluated m_huge
    // sentinels don't count), floored.
    let life_hit_pool_after = life_hit_pool_with_loss_prevention(
        life,
        ctx.max_life,
        ctx.prevented_life_loss,
        ctx.life_loss_below_half_prevented,
    );
    let hit_pool_remaining = (life_hit_pool_after
        + if mom_pool_remaining.is_finite() {
            mom_pool_remaining
        } else {
            0.0
        }
        + if es_pool_remaining.is_finite() {
            es_pool_remaining
        } else {
            0.0
        })
    .floor();

    PoolsAfter {
        pools: PoolState {
            allies,
            aegis_shared,
            aegis_shared_elemental,
            aegis_by_type,
            guard_shared,
            guard_shared_rate: pools.guard_shared_rate,
            guard_by_type,
            guard_rate_by_type: pools.guard_rate_by_type,
            ward: restore_ward,
            energy_shield,
            mana,
            life,
            life_loss_lost_over_time,
            life_below_half_loss_lost_over_time,
        },
        recoupable_by_type,
        overkill,
        hit_pool_remaining,
        resources_lost,
    }
}

/// The generic X-protects-Y primitive (CalcDefence.lua:2746 / :2827 / :3547 / :3563):
/// `poolProtected = source_pool / rate × (1 − rate)`.
///
/// `rate_fraction` is the protection ratio (a fraction):
/// - `rate ≥ 1` → full protection (PoB2's `m_huge`), returns `f64::INFINITY`;
/// - `rate ≤ 0` → no protection layer, returns 0;
/// - otherwise → the formula's value (when the source pool shares by rate, how much "the other side" is protected for).
///
/// MoM / Guard / Ward bypass / SoulLink / EB nesting all reuse this primitive.
pub fn pool_protected(source_pool: f64, rate_fraction: f64) -> f64 {
    if rate_fraction >= 1.0 {
        return f64::INFINITY;
    }
    if rate_fraction <= 0.0 {
        return 0.0;
    }
    source_pool / rate_fraction * (1.0 - rate_fraction)
}

/// Folds a "proportional share" protection layer into the target pool (the
/// shared shape of PoB2 :2753-2754 / :3549 / :3564 / :3571):
/// `pool' = max(pool − protected, 0) + min(pool, protected) / passthrough`.
///
/// `protected` = [`pool_protected`]'s output; `passthrough_fraction` = the
/// fraction of damage that passes through this layer to reach the target
/// pool (= 1 − the layer's absorption ratio). When `protected = ∞` (full
/// protection), vendor's value numerically becomes `min(pool,∞)/passthrough`;
/// a fully-absorbing layer with passthrough ≤ 0 doesn't fit this formula
/// (the caller follows vendor's flat-addition branch instead, e.g. :3560-3561).
pub fn apply_protected_layer(pool: f64, protected: f64, passthrough_fraction: f64) -> f64 {
    (pool - protected).max(0.0) + pool.min(protected) / passthrough_fraction
}

/// The segmented life hit pool (CalcDefence.lua:450-454's `calcLifeHitPoolWithLossPrevention`):
///
/// ```text
/// halfLife = maxLife × 0.5
/// aboveLow = max(life − halfLife, 0)
/// pool = aboveLow / (1 − lossPrev/100)
///      + min(life, halfLife) / (1 − belowHalfPrev/100) / (1 − lossPrev/100)
/// ```
///
/// `loss_prev_pct` / `below_half_prev_pct` are percentages (0-100); when
/// either reaches 100, the denominator is 0, and the pool naturally becomes
/// ∞ (matching vendor's divide-by-zero → m_huge behavior, no extra clamp applied).
pub fn life_hit_pool_with_loss_prevention(
    life: f64,
    max_life: f64,
    loss_prev_pct: f64,
    below_half_prev_pct: f64,
) -> f64 {
    let half_life = max_life * 0.5;
    let above_low = (life - half_life).max(0.0);
    above_low / (1.0 - loss_prev_pct / 100.0)
        + life.min(half_life) / (1.0 - below_half_prev_pct / 100.0) / (1.0 - loss_prev_pct / 100.0)
}

/// The max-hit TotalHitPool base: folds ES (bypass / chaos-doubling /
/// EternalLife branch) on top of the MoM hit pool (CalcDefence.lua:2942-2960).
///
/// `mom_hit_pool` = `<X>MoMHitPool` (produced by pool_setup's `mom_hit_pools`);
/// `energy_shield` = EnergyShieldRecoveryCap. Track F stacks
/// [`extend_total_hit_pool`]'s ward/aegis/guard/allies layers on top of this.
pub fn total_hit_pool_base(
    dtype: DamageType,
    mom_hit_pool: f64,
    energy_shield: f64,
    ctx: &PoolCtx,
) -> f64 {
    let es_bypass = ctx.es_bypass_by_type[dtype as usize] / 100.0;
    // :2947 chaos doubles against ES → ES is halved when entering the pool.
    let chaos_es_mult = if dtype == DamageType::Chaos && !ctx.chaos_not_double_es {
        2.0
    } else {
        1.0
    };
    if ctx.eternal_life {
        // :2948-2950 EternalLife: the bypass portion is waived → ES can actually cover ES/(1−bypass).
        mom_hit_pool + energy_shield / (1.0 - es_bypass) / chaos_es_mult
    } else if es_bypass < 1.0 {
        if es_bypass > 0.0 {
            // :2952-2955 bypass nesting: poolProtected = EScap/(1−bypass)×bypass/chaosMult.
            let protected = energy_shield / (1.0 - es_bypass) * es_bypass / chaos_es_mult;
            apply_protected_layer(mom_hit_pool, protected, es_bypass)
        } else {
            // :2956-2958 no bypass: added flat (chaos-doubling halves it).
            mom_hit_pool + energy_shield / chaos_es_mult
        }
    } else {
        // bypass ≥ 100%: ES doesn't participate in this type's pool.
        mom_hit_pool
    }
}

/// The max-hit TotalHitPool expansion layer (CalcDefence.lua:3540-3596):
/// stacks ward (bypass via poolProtected, :3544-3553), aegis (added flat
/// using the strongest view, :3554-3555), guard (:3556-3566), and allies
/// (each layer via poolProtected, :3567-3595) on top of [`total_hit_pool_base`].
///
/// Note that vendor :3557/:3559's Lua operator precedence (`a or 0 + b or 0`
/// parses as `a or (0+b) or 0`) makes **only shared guard actually take
/// effect** in the guard section (sharedGuardAbsorbRate is always non-nil);
/// to align value-for-value with parity, this mirrors that evaluation semantics rather than the literal formula.
pub fn extend_total_hit_pool(
    base_pool: f64,
    dtype: DamageType,
    pools: &PoolState,
    ctx: &PoolCtx,
) -> f64 {
    let idx = dtype as usize;
    let mut pool = base_pool;
    // ward (:3544-3553): nested via poolProtected when bypass>0, otherwise added flat.
    if ctx.ward_bypass > 0.0 {
        let bypass = ctx.ward_bypass / 100.0;
        // :3547 protected = Ward/(1−bypass)×bypass = pool_protected(Ward, 1−bypass).
        let protected = pool_protected(pools.ward, 1.0 - bypass);
        pool = apply_protected_layer(pool, protected, bypass);
    } else {
        pool += pools.ward;
    }
    // aegis (:3555): added flat as max(per-type, shared, and for elemental
    // types, per-type+sharedElemental) (AegisDisplay = perType + sharedElemental, :2879).
    let aegis_display = if DamageType::ELEMENTAL.contains(&dtype) {
        pools.aegis_by_type[idx] + pools.aegis_shared_elemental
    } else {
        0.0
    };
    pool += pools.aegis_by_type[idx]
        .max(pools.aegis_shared)
        .max(aegis_display);
    // guard (:3556-3566): vendor's evaluation semantics = shared guard only (see the function docs).
    let guard_rate = pools.guard_shared_rate;
    if guard_rate > 0.0 {
        let guard_absorb = pools.guard_shared;
        if guard_rate >= 100.0 {
            pool += guard_absorb;
        } else {
            let protected = pool_protected(guard_absorb, guard_rate / 100.0);
            pool = apply_protected_layer(pool, protected, 1.0 - guard_rate / 100.0);
        }
    }
    // allies (:3567-3595): each layer is folded in via poolProtected in turn
    // (when mitigation is 0, protected=∞ and the formula naturally degenerates to the original pool).
    for ally in &pools.allies {
        if ally.remaining > 0.0 && (ally.damage_type.is_none() || ally.damage_type == Some(dtype)) {
            let rate = ally.mitigation_pct / 100.0;
            let protected = pool_protected(ally.remaining, rate);
            pool = apply_protected_layer(pool, protected, 1.0 - rate);
        }
    }
    pool
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Pins the poolProtected formula (CalcDefence.lua:2746: `source/(rate)×(1−rate)`).
    /// Hand-computed: 1000/0.3×0.7 = 2333.33…; 500/0.5×0.5 = 500.
    #[test]
    fn pool_protected_formula_locked() {
        assert!((pool_protected(1000.0, 0.3) - 1000.0 / 0.3 * 0.7).abs() < 1e-9);
        assert_eq!(pool_protected(500.0, 0.5), 500.0);
    }

    /// rate ≥ 1 → ∞ protection (vendor `sharedMindOverMatter >= 100` → m_huge, :2748-2751);
    /// rate ≤ 0 → no protection layer (0).
    #[test]
    fn pool_protected_boundary_rates() {
        assert_eq!(pool_protected(800.0, 1.0), f64::INFINITY);
        assert_eq!(pool_protected(800.0, 1.5), f64::INFINITY);
        assert_eq!(pool_protected(800.0, 0.0), 0.0);
        assert_eq!(pool_protected(800.0, -0.2), 0.0);
    }

    /// apply_protected_layer's two-segment semantics (the CalcDefence.lua:2753-2754 shape):
    /// pool ≤ protected → pool/passthrough; pool > protected → the excess kept as-is + the protected segment amplified.
    /// Hand-computed: pool=1000, protected=2400, pass=0.5 → 0 + 1000/0.5 = 2000;
    ///       pool=3000, protected=2400, pass=0.5 → 600 + 2400/0.5 = 5400.
    #[test]
    fn apply_protected_layer_two_segments() {
        assert_eq!(apply_protected_layer(1000.0, 2400.0, 0.5), 2000.0);
        assert_eq!(apply_protected_layer(3000.0, 2400.0, 0.5), 5400.0);
    }

    /// The segmented pool degenerates to life's own value with no loss
    /// prevention (CalcDefence.lua:450-454, prev=0 → aboveLow + min(life, half) = life).
    #[test]
    fn life_hit_pool_without_prevention_equals_life() {
        assert_eq!(
            life_hit_pool_with_loss_prevention(1000.0, 1000.0, 0.0, 0.0),
            1000.0
        );
        assert_eq!(
            life_hit_pool_with_loss_prevention(400.0, 1000.0, 0.0, 0.0),
            400.0
        );
    }

    /// Full-range 20% loss prevention: full health 1000/1000 → 500/0.8 + 500/0.8 = 1250
    /// (hand-computed, CalcDefence.lua:453's two segments both divide by (1−lossPrev/100)).
    #[test]
    fn life_hit_pool_with_full_range_prevention() {
        assert!(
            (life_hit_pool_with_loss_prevention(1000.0, 1000.0, 20.0, 0.0) - 1250.0).abs() < 1e-9
        );
    }

    /// Below-half 50% prevention: full health 1000/1000 → 500 + 500/0.5 = 1500;
    /// both segments combined (life=800, max=1000, lossPrev=20, belowHalf=50) →
    /// 300/0.8 + 500/0.5/0.8 = 375 + 1250 = 1625 (hand-computed).
    #[test]
    fn life_hit_pool_below_half_prevention_segments() {
        assert!(
            (life_hit_pool_with_loss_prevention(1000.0, 1000.0, 0.0, 50.0) - 1500.0).abs() < 1e-9
        );
        assert!(
            (life_hit_pool_with_loss_prevention(800.0, 1000.0, 20.0, 50.0) - 1625.0).abs() < 1e-9
        );
    }

    /// The below-half segment only applies to min(life, halfLife): when
    /// current life is below half, aboveLow=0, so the whole pool =
    /// life/(1−belowHalf/100) (400/0.5 = 800, hand-computed).
    #[test]
    fn life_hit_pool_when_life_below_half() {
        assert_eq!(
            life_hit_pool_with_loss_prevention(400.0, 1000.0, 0.0, 50.0),
            800.0
        );
    }

    /// 100% loss prevention → pool ∞ (equivalent to vendor's divide-by-zero → m_huge, not clamped).
    #[test]
    fn life_hit_pool_full_prevention_is_infinite() {
        assert_eq!(
            life_hit_pool_with_loss_prevention(1000.0, 1000.0, 100.0, 0.0),
            f64::INFINITY
        );
    }

    /// TypedDamage's accessors and PoolState/PoolCtx/PoolsAfter's Default
    /// construction (the contract compiles + neutral defaults are pinned).
    #[test]
    fn contract_types_construct_with_neutral_defaults() {
        let hit = TypedDamage {
            physical: 100.0,
            fire: 50.0,
            ..Default::default()
        };
        assert_eq!(hit.get(DamageType::Physical), 100.0);
        assert_eq!(hit.get(DamageType::Chaos), 0.0);
        assert_eq!(hit.total(), 150.0);

        let pools = PoolState::default();
        assert!(pools.allies.is_empty());
        assert_eq!(pools.life, 0.0);

        let ctx = PoolCtx::default();
        assert!(!ctx.eb);
        assert_eq!(ctx.mom_shared, 0.0);

        let after = PoolsAfter::default();
        assert_eq!(after.overkill, 0.0);
        assert!(after.resources_lost.is_empty());
    }

    /// Pins the POB2 traversal order (CalcDefence.lua:27): forward order
    /// Physical→Lightning→Cold→Fire→Chaos, while array indices still follow pobr's DamageType enum order.
    #[test]
    fn pob2_damage_order_locked() {
        assert_eq!(
            POB2_DAMAGE_ORDER,
            [
                DamageType::Physical,
                DamageType::Lightning,
                DamageType::Cold,
                DamageType::Fire,
                DamageType::Chaos,
            ]
        );
        assert_eq!(DamageType::Physical as usize, 0);
        assert_eq!(DamageType::Chaos as usize, 4);
    }
}
