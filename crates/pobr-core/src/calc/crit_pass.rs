//! Crit / non-crit dual pass.
//!
//! PoB2 aggregates the damage body separately for each `CriticalStrike`
//! condition state within each hand pass (`CalcOffence.lua:3978-3980`:
//! `for pass = 1, 2 do cfg.skillCond["CriticalStrike"] = (pass == 1)`, pass 1
//! is the crit leg). The crit leg gets `allMult ×= CritMultiplier`
//! (`:4028-4032`); both legs' pre-resist averages are stored to
//! `Stored<Type>CritAvg/HitAvg/CombinedAvg` (`:4047-4057`, feeding ailment
//! magnitude); finally CritBlend merges them (`:4395`
//! `AverageHit = totalHitAvg×(1−c) + totalCritAvg×c`).
//!
//! This module replaces `offence.rs`'s single-factor
//! `total_hit_avg = non_crit_hit_avg × crit.effect`.
//!
//! ## Equivalence short circuit (I5, the correctness proof for the fallback switch)
//!
//! When no mod is conditioned on `CriticalStrike`, both legs' aggregation
//! inputs are bit-for-bit identical (component vectors + per-type lucky
//! chance comparison), and the old single-factor formula is used instead
//! (mathematical identity I5: `blend(c, x×m, x) = x×(1+(m−1)c) = x×crit.effect`,
//! and **reusing the old formula's rounding order** guarantees byte-for-byte
//! equivalence — pinned by the equivalence tests). Only when the legs differ
//! (crit-leg-only mods, CritLucky, etc.) does it fall through to a real
//! dual-leg blend.
//!
//! ## T3 multiplier wiring (contract 2/3, m4-t3-wiring-notes.md §2)
//!
//! - `ScaledDamageEffect`: shared by both legs (vendor `:4023-4025` allMult),
//!   supplied by the caller (`scaled_damage_effect(db, enemy_db, cfg, crit.chance)`).
//! - lucky: the lucky chance is folded into the average per (pass, damageType)
//!   **after** min/max are multiplied by allMult (`:4035-4046`).
//! - canDeal: zeroed in place after the conversion chain but before
//!   aggregation (`:3989` consumption point).
//!
//! ## Conservative choice (tracked, to be revisited in its own behavior commit)
//!
//! Enemy armour mitigation's `raw_hit` still uses the component average
//! **before** lucky folding (matching the pre-replacement implementation;
//! vendor runs the armour formula on damage already multiplied by allMult
//! within the pass `:4060+` — the crit leg's raw_hit already including
//! ×CritMultiplier is a per-pass refinement; in this implementation a leg's
//! raw_hit is just that leg's component average, so the short-circuit path is
//! shaped identically to the old implementation).

use pobr_data::prelude::*;

use crate::{CalcConfig, ModDb};

use super::crit::CritOutcome;
use super::damage::{DamageComponent, apply_can_deal, calculate_components, lucky_hit_chance};
use super::output::StoredDamageRange;
use super::round;
use super::scaled_damage::{AllMultExtras, ScaledDamage, all_mult};

/// Result of the crit/non-crit dual pass.
#[derive(Debug, Clone, PartialEq)]
pub struct CritPassOutput {
    /// Non-crit leg components (after canDeal gating + allMult scaling; the
    /// current source for the top-level `damage_components` field and ailment magnitude).
    pub non_crit_components: Vec<DamageComponent>,
    /// Crit leg components (with the extra ×CritMultiplier).
    pub crit_components: Vec<DamageComponent>,
    /// `Stored<Type>CritAvg` (pre-resist, includes allMult and ×CritMultiplier, `:4049`).
    pub stored_crit_avg: Vec<(DamageType, f64)>,
    /// `Stored<Type>HitAvg` (non-crit leg, `:4054`).
    pub stored_hit_avg: Vec<(DamageType, f64)>,
    /// `Stored<Type>CombinedAvg` (both legs, accumulated weighted by crit chance, `:4048/:4053`).
    pub stored_combined_avg: Vec<(DamageType, f64)>,
    /// `Stored<Type>{Hit,Crit}{Min,Max}` (`:4050-4056`; min/max are **not**
    /// folded by lucky (vendor's lucky folding only applies to the `*Avg`
    /// family) — this is the input surface for damaging ailments and
    /// RollAverage interpolation, appended in place).
    pub stored_ranges: Vec<StoredDamageRange>,
    /// Player-side total average after CritBlend (excludes enemy mitigation;
    /// this is the `total_hit_avg` field's semantics).
    pub total_hit_avg: f64,
    /// Effective total average after CritBlend (after enemy mitigation; used for DPS).
    pub total_hit_avg_mitigated: f64,
    /// Whether the equivalence short circuit was taken (both legs'
    /// aggregation inputs bit-identical → old single-factor formula).
    pub short_circuited: bool,
}

/// Aggregation output for a single leg.
struct Leg {
    components: Vec<DamageComponent>,
    /// (type, average after lucky folding); order matches `components`.
    avgs: Vec<(DamageType, f64)>,
    /// Component averages before lucky folding (used for enemy armour
    /// mitigation's raw_hit; see the module docs for the conservative choice).
    raw_avgs: Vec<f64>,
}

/// Main entry point for the crit/non-crit dual pass (called within each hand
/// pass — a 2×2 nesting with hand on the outside and crit on the inside,
/// mirroring PoB2's structure).
///
/// `mitigation(pass_cfg, type, raw_hit)` is the enemy's total damage-taken
/// multiplier for that type (only called under `mode_effective`; supplied by
/// an offence closure so `enemy_damage_multiplier` stays private). `scaled`
/// is the multiplier group (T3 contract 2, shared by both legs, with the crit
/// leg getting the extra ×CritMultiplier).
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
    // vendor `:3979`: both passes set the condition explicitly (an absolute
    // assignment, not a toggle).
    let cfg_crit = cfg.clone().with_condition("CriticalStrike", true);
    let cfg_hit = cfg.clone().with_condition("CriticalStrike", false);

    // Unscaled aggregation per leg (conversion chain + canDeal) — computed
    // only twice, and the short-circuit check reuses these directly.
    let mut hit_unscaled = calculate_components(db, &cfg_hit, base_hit_min, base_hit_max);
    apply_can_deal(&mut hit_unscaled, db, &cfg_hit);
    let mut crit_unscaled = calculate_components(db, &cfg_crit, base_hit_min, base_hit_max);
    apply_can_deal(&mut crit_unscaled, db, &cfg_crit);

    // Equivalence short-circuit check: unscaled components bit-identical +
    // per-type lucky chance identical ⇔ no CriticalStrike-conditioned mod
    // participates in damage aggregation / canDeal / lucky.
    let short_circuit = hit_unscaled == crit_unscaled
        && hit_unscaled.iter().all(|component| {
            lucky_hit_chance(db, &cfg_hit, component.damage_type, false)
                == lucky_hit_chance(db, &cfg_crit, component.damage_type, true)
        });

    let base_mult = all_mult(scaled, &AllMultExtras::default());
    // Crit leg's extra multiplier (`:4028-4032` pass==1 allMult ×= CritMultiplier).
    let crit_leg_mult = base_mult * crit.multiplier;

    let non_crit = finish_leg(db, &cfg_hit, hit_unscaled, base_mult, false);
    let crit_leg = finish_leg(db, &cfg_crit, crit_unscaled, crit_leg_mult, true);

    let c = crit.chance;

    // Stored family (pre-resist, `:4047-4057`).
    let stored_crit_avg = crit_leg.avgs.clone();
    let stored_hit_avg = non_crit.avgs.clone();
    let stored_combined_avg: Vec<(DamageType, f64)> = non_crit
        .avgs
        .iter()
        .zip(crit_leg.avgs.iter())
        .map(|((ty, hit_avg), (_, crit_avg))| (*ty, crit_avg * c + hit_avg * (1.0 - c)))
        .collect();
    // Stored min/max family (`:4050-4056`): non-crit leg's range plus the
    // crit leg's range for the same type (folded in as 0 if the crit leg
    // lacks that type, matching vendor's `or 0` semantics; min/max are not
    // folded by lucky).
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
        // Old single-factor formula (rounding order copied byte-for-byte
        // from the pre-replacement offence.rs implementation):
        // total = round(Σavg × crit.effect); identity I5 guarantees this
        // matches a real blend mathematically.
        let player_side: f64 = leg_total(&non_crit);
        let mitigated: f64 = if mode_effective {
            // The mitigated side **cannot** use "single-leg mitigation ×
            // crit.effect": enemy armour mitigation depends on the
            // single-hit damage amount (vendor pass1 computes DR from the
            // post-crit hit amount — a bigger crit means smaller DR), so
            // this raw dependency breaks identity I5 along the mitigation
            // dimension. Under the short circuit, crit leg = non-crit leg ×
            // crit.multiplier, so blend per vendor `:4395` by mitigating
            // each leg separately first; when mitigation has no dependency
            // on raw (a pure resistance/damage-taken chain), this degenerates
            // mathematically to the old formula with every value unchanged.
            let hit_leg = leg_total_mitigated(&non_crit, &cfg_hit, &mitigation);
            let crit_leg_mitigated: f64 = non_crit
                .avgs
                .iter()
                .zip(non_crit.raw_avgs.iter())
                .map(|((ty, avg), raw)| {
                    avg * crit.multiplier * mitigation(&cfg_crit, *ty, raw * crit.multiplier)
                })
                .sum();
            hit_leg * (1.0 - c) + crit_leg_mitigated * c
        } else {
            player_side * crit.effect
        };
        (round(player_side * crit.effect), round(mitigated))
    } else {
        // Real dual-leg blend (`:4395`): mitigate each leg against the enemy separately, then weight by c.
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

/// Finishes a leg: ×allMult (`:4033-4034`) → fold lucky into avg (`:4035-4046`).
/// Skips scaling when `mult == 1.0`, keeping it bit-identical to the pre-replacement implementation.
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
