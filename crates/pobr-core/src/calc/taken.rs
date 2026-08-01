//! The taken-as pipeline plus effectiveAppliedArmour (13-G1 / 13-G7).
//!
//! Vendor PoB2 mirror (`Modules/CalcDefence.lua`, line numbers verified 2026-06-11):
//! - `:2171-2190` assembles the defence side's `actor.damageShiftTable` (a
//!   BASE sum of `<Src>DamageTakenAs<Dst>` / `<Src>DamageFromHitsTakenAs<Dst>`
//!   plus elemental variants; the source type retains `max(100−total, 0)`) →
//!   [`damage_shift_table`]; the offence side's parallel sum is at `:356-365`'s
//!   `applyDmgTakenConversion`.
//! - `:2336-2362` composes per-type `EffectiveAppliedArmour`
//!   (`ArmourAppliesTo<X>DamageTaken` percentage × `(1 + ArmourDefense)` plus
//!   Evasion/ES borrowed terms) → [`effective_applied_armour`]; `:1862-1863`'s
//!   implicit physical `ArmourAppliesToPhysicalDamageTaken` BASE 100 (vendor
//!   writes this into modDB; this implementation folds it in equivalently inside the function and **does not write to ModDb**).
//! - `:422-455`'s `takenHitFromDamage(rawDamage, damageType, actor)` equivalent
//!   entry point → [`taken_hit_from_damage`] (per converted type: effArmour
//!   armour mitigation + flat DR − overwhelm (clamped to the per-type
//!   `DamageReductionMax`) × the resistance-taken multiplier + takenFlat, then × `AfterReductionTakenHitMulti`).
//!
//! Design:
//! - [`MitigationCtx`] is a mitigation snapshot that doesn't change per hit —
//!   assembly ([`build_mitigation_ctx`], reads ModDb) is kept separate from
//!   evaluation ([`taken_hit_from_damage`], pure arithmetic), mirroring
//!   `pool_damage`'s PoolCtx/evaluation split; its consumer is Track F's new max-hit / EHP pipeline.
//! - **Transitional semantics**: before F is wired up, the old `ehp.rs`'s
//!   `armour_applies_to_element: [bool;3]` path keeps its original behavior;
//!   this module's percentage model is only consumed by new tests and by F
//!   (starting at B-2, `perform` derives that `[bool;3]` from this ctx, unifying the mod as a single source of truth).
//!
//! Per-type array index convention = `DamageType as usize`
//! (Physical/Fire/Cold/Lightning/Chaos, same convention as `pool_damage::PoolState`).

use pobr_data::prelude::*;

use crate::{CalcConfig, ModDb};

use super::defence::{armour_reduction, taken_mult_for_type_default};
use super::round;

/// Damage types in enum order (the per-type array index order).
///
/// Note this differs from PoB2's defence traversal order
/// (`pool_damage::POB2_DAMAGE_ORDER`); this module's per-type calculations
/// are mutually independent with no shared pool consumption, so traversal
/// order doesn't affect the result, hence enum order is used.
const DAMAGE_TYPE_BY_INDEX: [DamageType; 5] = [
    DamageType::Physical,
    DamageType::Fire,
    DamageType::Cold,
    DamageType::Lightning,
    DamageType::Chaos,
];

/// DamageType → mod name prefix (PoB2's ModName convention).
fn dt_prefix(dt: DamageType) -> &'static str {
    match dt {
        DamageType::Physical => "Physical",
        DamageType::Fire => "Fire",
        DamageType::Cold => "Cold",
        DamageType::Lightning => "Lightning",
        DamageType::Chaos => "Chaos",
    }
}

/// Vendor's `round(val)` (`Modules/Common.lua`: `m_floor(val + 0.5)`, rounds to the nearest integer).
/// [`taken_hit_from_damage`]'s final taken damage is rounded at the same point as vendor (CalcDefence.lua:442).
fn vendor_round(value: f64) -> f64 {
    (value + 0.5).floor()
}

// damage shift table (13-G1)

/// Builds the defence side's taken-as conversion matrix `shift[src][dst]` (fraction, 0-1).
///
/// Vendor: CalcDefence.lua:2171-2190 (the hit-view shiftTable; the DoT
/// view's `damageOverTimeShiftTable` has no FromHits mods, left to be
/// extended when Track F/DoT is wired up):
/// - `dst ≠ src`: `Σ BASE(<Src>DamageTakenAs<Dst>, <Src>DamageFromHitsTakenAs<Dst>
///   [, ElementalDamageTakenAs<Dst>, ElementalDamageFromHitsTakenAs<Dst> if src is elemental]) / 100`;
/// - `dst = src` (source retention): `max(1 − Σtargets/100, 0)` — when the
///   total conversion exceeds 100%, only the source is clamped to 0, and
///   individual target shares are **not normalized** (matching vendor's
///   semantics: total damage taken can exceed raw).
pub fn damage_shift_table(db: &ModDb, cfg: &CalcConfig) -> [[f64; 5]; 5] {
    let mut shift = [[0.0; 5]; 5];
    for src in DAMAGE_TYPE_BY_INDEX {
        let s = src as usize;
        let src_name = dt_prefix(src);
        let mut total_pct = 0.0;
        for dst in DAMAGE_TYPE_BY_INDEX {
            if dst == src {
                continue;
            }
            let dst_name = dt_prefix(dst);
            let mut names = vec![
                ModName::from(format!("{src_name}DamageTakenAs{dst_name}")),
                ModName::from(format!("{src_name}DamageFromHitsTakenAs{dst_name}")),
            ];
            if src.is_elemental() {
                // Elemental sources also pick up the Elemental family (vendor :2181/:2183's isElemental branch).
                names.push(ModName::from(format!("ElementalDamageTakenAs{dst_name}")));
                names.push(ModName::from(format!(
                    "ElementalDamageFromHitsTakenAs{dst_name}"
                )));
            }
            let pct = db.sum(ModType::Base, cfg, &names);
            shift[s][dst as usize] = pct / 100.0;
            total_pct += pct;
        }
        // Source retention max(1−total, 0) (vendor :2189's `m_max(100 - destTotal, 0)`).
        shift[s][s] = (1.0 - total_pct / 100.0).max(0.0);
    }
    shift
}

// effectiveAppliedArmour (13-G7 percentage model)

/// The "armour applies percentage" for a damage type (`ArmourAppliesTo<X>DamageTaken`, %).
///
/// Vendor: CalcDefence.lua:2353-2358 —
/// - This type's share is 0 under the `ArmourDoesNotApplyTo<X>DamageTaken`
///   flag (exclusive to the "instead" variant, ModParser.lua:2523);
/// - The implicit physical BASE 100 (vendor :1862-1863's
///   `NewMod("ArmourAppliesToPhysicalDamageTaken", "BASE", 100)` injects into
///   modDB; this implementation folds it in as `+100` inside the non-flag
///   branch instead, without writing to ModDb);
/// - Elemental types additionally add `ArmourAppliesToElementalDamageTaken`
///   (independently gated by the `ArmourDoesNotApplyToElementalDamageTaken` flag).
pub fn armour_applies_pct(db: &ModDb, cfg: &CalcConfig, dtype: DamageType) -> f64 {
    let name = dt_prefix(dtype);
    let mut pct = if db.flag(
        cfg,
        ModName::from(format!("ArmourDoesNotApplyTo{name}DamageTaken")),
    ) {
        0.0
    } else {
        let base = db.sum(
            ModType::Base,
            cfg,
            &[ModName::from(format!("ArmourAppliesTo{name}DamageTaken"))],
        );
        // Implicit physical BASE 100 (:1862-1863): implemented inline here, not written to ModDb.
        if dtype == DamageType::Physical {
            base + 100.0
        } else {
            base
        }
    };
    if dtype.is_elemental()
        && !db.flag(
            cfg,
            ModName::from("ArmourDoesNotApplyToElementalDamageTaken"),
        )
    {
        pct += db.sum(
            ModType::Base,
            cfg,
            &[ModName::from("ArmourAppliesToElementalDamageTaken")],
        );
    }
    pct
}

/// The effective applied armour (`EffectiveAppliedArmour`) for a damage type.
///
/// Vendor: CalcDefence.lua:2336-2362 —
/// `Armour × pct/100 × (1 + ArmourDefense)` (ArmourDefense =
/// `Max("ArmourDefense")/100`, :1392) `+ max(Evasion × pctEvasion/100, 0) +
/// max(ES × pctES/100, 0)`; the Evasion/ES shares are each controlled by the
/// `<Def>DoesNotApplyTo<X>DamageTaken` flag and the `<Def>AppliesTo<X>DamageTaken` BASE.
pub fn effective_applied_armour(
    db: &ModDb,
    cfg: &CalcConfig,
    armour: f64,
    evasion: f64,
    energy_shield: f64,
    dtype: DamageType,
) -> f64 {
    let name = dt_prefix(dtype);
    let armour_pct = armour_applies_pct(db, cfg, dtype);
    // ArmourDefense: vendor :1392's `(modDB:Max(nil, "ArmourDefense") or 0) / 100`.
    let armour_defense = db
        .max_of(ModType::Base, cfg, &[ModName::from("ArmourDefense")])
        .max(0.0)
        / 100.0;
    let other_pct = |def: &str| -> f64 {
        if db.flag(
            cfg,
            ModName::from(format!("{def}DoesNotApplyTo{name}DamageTaken")),
        ) {
            0.0
        } else {
            db.sum(
                ModType::Base,
                cfg,
                &[ModName::from(format!("{def}AppliesTo{name}DamageTaken"))],
            )
        }
    };
    let from_armour = (armour * armour_pct / 100.0) * (1.0 + armour_defense);
    // Evasion/ES borrowed terms each get their own max(…, 0) (vendor :2356/:2358).
    let from_evasion = (evasion * other_pct("Evasion") / 100.0).max(0.0);
    let from_es = (energy_shield * other_pct("EnergyShield") / 100.0).max(0.0);
    round(from_armour + from_evasion + from_es)
}

// MitigationCtx: a per-actor mitigation snapshot (assembly kept separate from evaluation)

/// A mitigation context that doesn't change per hit (per-type array index = `DamageType as usize`).
///
/// Corresponds to the per-type intermediate values on vendor's `actor.output`
/// (written by CalcDefence.lua:2336-2437, consumed by `:422-455`'s
/// `takenHitFromDamage`). Assembly entry point: [`build_mitigation_ctx`];
/// evaluation entry point: [`taken_hit_from_damage`] (pure arithmetic, doesn't read ModDb).
#[derive(Debug, Clone, PartialEq)]
pub struct MitigationCtx {
    /// The taken-as conversion matrix `shift[src][dst]` (fraction, includes source retention; [`damage_shift_table`]).
    pub shift: [[f64; 5]; 5],
    /// Per-type armour-applies percentage ([`armour_applies_pct`]; physical includes the implicit 100).
    pub armour_applies_pct: [f64; 5],
    /// Per-type effective applied armour (`<X>EffectiveAppliedArmour`, :2434).
    pub effective_applied_armour: [f64; 5],
    /// Per-type flat damage reduction (`Base<X>DamageReduction`, %; already
    /// `max(0)`'d and clamped to the per-type ceiling, :2336-2340).
    pub flat_dr_pct: [f64; 5],
    /// Per-type damage reduction ceiling (`<X>DamageReductionMax`, %; taken as the min with the global ceiling, :2333).
    pub dr_max_pct: [f64; 5],
    /// Per-type enemy overwhelm (`<X>EnemyOverwhelm`, %; :2045/:2134 — pobr
    /// models this as the player-side `Enemy<X>Overwhelm` BASE).
    pub overwhelm_pct: [f64; 5],
    /// Per-type resistance-taken multiplier (`<X>ResistTakenHitMulti = 1 −
    /// resist/100`, :2363/:2435; always 1 for physical, which has no
    /// resistance. Enemy penetration is left for config_interpreter).
    pub resist_taken_multi: [f64; 5],
    /// Per-type flat added damage taken on hit (`<X>takenFlat`, :2365-2373, Average view).
    pub taken_flat: [f64; 5],
    /// Per-type post-reduction multiplier (`<X>AfterReductionTakenHitMulti` =
    /// taken inc/more (Average view) × the deflect multiplier, :2436-2437;
    /// PoE2 has no spell suppression).
    pub after_reduction_multi: [f64; 5],
}

impl Default for MitigationCtx {
    /// The neutral snapshot: identity shift, no armour/reduction, a ceiling
    /// of 90 (vendor Data.lua:178's DamageReductionCap, same value as the
    /// `game_constants` fallback), all multipliers 1.
    fn default() -> Self {
        let mut shift = [[0.0; 5]; 5];
        for (i, row) in shift.iter_mut().enumerate() {
            row[i] = 1.0;
        }
        Self {
            shift,
            armour_applies_pct: [100.0, 0.0, 0.0, 0.0, 0.0],
            effective_applied_armour: [0.0; 5],
            flat_dr_pct: [0.0; 5],
            dr_max_pct: [90.0; 5],
            overwhelm_pct: [0.0; 5],
            resist_taken_multi: [1.0; 5],
            taken_flat: [0.0; 5],
            after_reduction_multi: [1.0; 5],
        }
    }
}

/// [`build_mitigation_ctx`]'s non-ModDb inputs (final panel armour/evasion/ES plus final resistances).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct MitigationInputs {
    /// Panel armour (the final value after global inc/more, vendor `output.Armour`).
    pub armour: f64,
    /// Panel evasion (`output.Evasion`, the base for the Evasion borrowed term).
    pub evasion: f64,
    /// Panel ES (`output.EnergyShield`, the base for the ES borrowed term).
    pub energy_shield: f64,
    /// Per-type final resistance (%, after cap; the Physical slot is always
    /// 0 — physical mitigation goes through armour/flat DR instead).
    pub resist_pct: [f64; 5],
    /// Deflect chance (%; 0 until Track D is wired up. Vendor :2433: the
    /// deflect multiplier only takes effect when `DeflectChance == 100`).
    pub deflect_chance_pct: f64,
    /// Deflect mitigation magnitude (%; `DeflectEffect`).
    pub deflect_effect_pct: f64,
}

/// Assembles a [`MitigationCtx`] from a ModDb (assembling the per-type
/// intermediate values from vendor CalcDefence.lua:2326-2437's defence mitigation section).
pub fn build_mitigation_ctx(
    db: &ModDb,
    cfg: &CalcConfig,
    inputs: &MitigationInputs,
) -> MitigationCtx {
    // Global damage reduction ceiling: `Max("DamageReductionMax") or DamageReductionCap(=90)` (:1862).
    let global_dr_max = {
        let v = db.max_of(ModType::Base, cfg, &[ModName::from("DamageReductionMax")]);
        if v > 0.0 {
            v
        } else {
            cfg.constants
                .character()
                .maximum_physical_damage_reduction_pct
        }
    };
    // Deflect multiplier: `DeflectChance == 100 and (1 − DeflectEffect/100) or 1` (:2433).
    let deflect_multi = if inputs.deflect_chance_pct >= 100.0 {
        1.0 - inputs.deflect_effect_pct / 100.0
    } else {
        1.0
    };

    let mut ctx = MitigationCtx {
        shift: damage_shift_table(db, cfg),
        ..MitigationCtx::default()
    };
    for dt in DAMAGE_TYPE_BY_INDEX {
        let i = dt as usize;
        let name = dt_prefix(dt);
        // Per-type damage reduction ceiling, taken as the min with the global ceiling (:2333).
        ctx.dr_max_pct[i] = {
            let v = db.max_of(
                ModType::Base,
                cfg,
                &[ModName::from(format!("{name}DamageReductionMax"))],
            );
            if v > 0.0 {
                v.min(global_dr_max)
            } else {
                global_dr_max
            }
        };
        // Flat damage reduction: `max(0, Σ BASE(<X>DamageReduction[, ElementalDamageReduction]))`
        // then clamped to the per-type ceiling (:2336-2340).
        ctx.flat_dr_pct[i] = {
            let mut names = vec![ModName::from(format!("{name}DamageReduction"))];
            if dt.is_elemental() {
                names.push(ModName::from("ElementalDamageReduction"));
            }
            db.sum(ModType::Base, cfg, &names)
                .clamp(0.0, ctx.dr_max_pct[i])
        };
        // Enemy overwhelm (pobr's `Enemy<X>Overwhelm` BASE mod; currently only physical has a source).
        ctx.overwhelm_pct[i] = db.sum(
            ModType::Base,
            cfg,
            &[ModName::from(format!("Enemy{name}Overwhelm"))],
        );
        // Resistance-taken multiplier (:2363; enemy penetration is left for config).
        ctx.resist_taken_multi[i] = 1.0 - inputs.resist_pct[i] / 100.0;
        // takenFlat (Average view, :2365-2372): the base hit family, plus
        // Attack/Spell each halved, plus the projectile variants each quartered.
        ctx.taken_flat[i] = db.sum(
            ModType::Base,
            cfg,
            &[
                ModName::from("DamageTaken"),
                ModName::from(format!("{name}DamageTaken")),
                ModName::from("DamageTakenWhenHit"),
                ModName::from(format!("{name}DamageTakenWhenHit")),
            ],
        ) + db.sum(
            ModType::Base,
            cfg,
            &[
                ModName::from("DamageTakenFromAttacks"),
                ModName::from(format!("{name}DamageTakenFromAttacks")),
            ],
        ) / 2.0
            + db.sum(
                ModType::Base,
                cfg,
                &[ModName::from(format!(
                    "{name}DamageTakenFromProjectileAttacks"
                ))],
            ) / 4.0
            + db.sum(
                ModType::Base,
                cfg,
                &[
                    ModName::from("DamageTakenFromSpells"),
                    ModName::from(format!("{name}DamageTakenFromSpells")),
                ],
            ) / 2.0
            + db.sum(
                ModType::Base,
                cfg,
                &[
                    ModName::from("DamageTakenFromSpellProjectiles"),
                    ModName::from(format!("{name}DamageTakenFromSpellProjectiles")),
                ],
            ) / 4.0;
        // Post-reduction multiplier (:2429/:2436-2437): taken inc/more (Average) × deflect; PoE2 has no suppression.
        ctx.after_reduction_multi[i] = taken_mult_for_type_default(db, cfg, dt) * deflect_multi;
        // Armour-applies percentage and effective applied armour (13-G7 percentage model).
        ctx.armour_applies_pct[i] = armour_applies_pct(db, cfg, dt);
        ctx.effective_applied_armour[i] = effective_applied_armour(
            db,
            cfg,
            inputs.armour,
            inputs.evasion,
            inputs.energy_shield,
            dt,
        );
    }
    ctx
}

// The takenHitFromDamage equivalent entry point

/// Converts single-type raw incoming damage into actual damage taken after
/// taken-as conversion and mitigation (total, plus per-type components).
///
/// Vendor: CalcDefence.lua:422-455's `takenHitFromDamage` — for each
/// conversion target type in `shift[src]`:
/// ```text
/// armourDR  = armourReductionF(<conv>EffectiveAppliedArmour, convertedDamage)
/// totalDR   = min(<src>DamageReductionMax, armourDR + <conv>flatDR)
/// drMulti   = 1 − max(min(<src>DamageReductionMax, totalDR − <conv>overwhelm), 0)/100
/// reduced   = round(max(converted × <conv>ResistTakenHitMulti × drMulti + <conv>takenFlat, 0)
///                   × <conv>AfterReductionTakenHitMulti)
/// ```
/// Note: the damage reduction ceiling is indexed by the **source** type
/// (vendor :429/:431's `output[damageType..…]` closure captures the outer
/// damageType), while everything else is indexed by the converted type.
/// `VaalArcticArmourMitigation` (:441, a PoE1 leftover) has no mod source and is omitted.
pub fn taken_hit_from_damage(
    raw_damage: f64,
    dtype: DamageType,
    mit: &MitigationCtx,
) -> (f64, [f64; 5]) {
    let src = dtype as usize;
    let dr_max = mit.dr_max_pct[src];
    let mut total = 0.0;
    let mut parts = [0.0; 5];
    for conv in DAMAGE_TYPE_BY_INDEX {
        let c = conv as usize;
        let convert_frac = mit.shift[src][c];
        let taken_flat = mit.taken_flat[c];
        if convert_frac <= 0.0 && taken_flat == 0.0 {
            continue;
        }
        let converted = raw_damage * convert_frac;
        let armour_dr_pct = armour_reduction(mit.effective_applied_armour[c], converted) * 100.0;
        let total_dr_pct = (armour_dr_pct + mit.flat_dr_pct[c]).min(dr_max);
        // vendor :431's `m_max(m_min(drMax, totalDR − overwhelm), 0)`; dr_max is always >0, so clamp is equivalent.
        let dr_multi = 1.0 - (total_dr_pct - mit.overwhelm_pct[c]).clamp(0.0, dr_max) / 100.0;
        let mult = mit.resist_taken_multi[c] * dr_multi;
        let reduced =
            vendor_round((converted * mult + taken_flat).max(0.0) * mit.after_reduction_multi[c]);
        total += reduced;
        parts[c] = reduced;
    }
    (total, parts)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Per-type array index convention (same as pool_damage): enum order, Physical=0 … Chaos=4.
    #[test]
    fn damage_type_index_convention() {
        assert_eq!(DamageType::Physical as usize, 0);
        assert_eq!(DamageType::Fire as usize, 1);
        assert_eq!(DamageType::Cold as usize, 2);
        assert_eq!(DamageType::Lightning as usize, 3);
        assert_eq!(DamageType::Chaos as usize, 4);
    }

    /// Vendor round = floor(x + 0.5) (Modules/Common.lua).
    #[test]
    fn vendor_round_half_up() {
        assert_eq!(vendor_round(357.142_857), 357.0);
        assert_eq!(vendor_round(0.5), 1.0);
        assert_eq!(vendor_round(124.999), 125.0);
    }

    /// Neutral ctx: identity shift, all multipliers 1 → damage taken = round(raw).
    #[test]
    fn neutral_ctx_identity() {
        let ctx = MitigationCtx::default();
        let (sum, parts) = taken_hit_from_damage(1000.0, DamageType::Fire, &ctx);
        assert_eq!(sum, 1000.0);
        assert_eq!(parts[DamageType::Fire as usize], 1000.0);
        assert_eq!(parts[DamageType::Physical as usize], 0.0);
    }
}
