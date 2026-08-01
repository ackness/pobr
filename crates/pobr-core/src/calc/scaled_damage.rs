//! Multiplier groups and the DPS end factors: the Double/Triple Damage
//! multiplier + the allMult placeholder struct + the two DPS end factors.
//!
//! **Modules first, wiring last**: this file only provides self-contained
//! calculation units with a frozen signature; consumption by `offence.rs` /
//! `crit_pass` happens via T2 wiring (contract 2, §3.3).

use pobr_data::prelude::*;

use crate::{CalcConfig, ModDb};

/// Double/Triple Damage multiplier resolution result (PoB2 `CalcOffence.lua:3840-3861`).
///
/// - [`double_chance`](Self::double_chance) / [`triple_chance`](Self::triple_chance):
///   **percentages 0..=100** (aligned with vendor `output.DoubleDamageChance` / `TripleDamageChance`).
///   `double_chance` already has the Triple deduction applied
///   (`DD = max(DD − TD×DD/100, 0)`, `:3858`).
/// - [`effect`](Self::effect): `ScaledDamageEffect = 1 × (1 + DD/100 + 2×TD/100)` (`:3861`).
///   Vendor initializes `ScaledDamageEffect = 1` and **only** DD/TD multiply into it (`:3840`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScaledDamage {
    /// `1 + DoubleDamageEffect + TripleDamageEffect`, multiplied into allMult.
    pub effect: f64,
    /// Final Double Damage chance (percentage, Triple already deducted).
    pub double_chance: f64,
    /// Final Triple Damage chance (percentage).
    pub triple_chance: f64,
}

/// Double/Triple Damage multiplier.
///
/// `crit_chance` is a **fraction 0..=1** (i.e. [`crate::calc::resolve_crit`]'s
/// `chance`; equivalent to vendor `output.CritChance / 100` — the
/// `OnCrit × CritChance / 100` folding at `:3844/:3849`).
///
/// Line-by-line mirror of PoB2 `CalcOffence.lua:3842-3861`:
///
/// - `TripleDamageChanceOnCrit = min(Sum(BASE), 100)`; `TripleDamageChance =
///   min(Sum(BASE) + enemy.SelfTripleDamageChance(effective mode only) + OnCrit×crit, 100)`.
/// - Double mirrors the same structure (`:3848-3849`).
/// - Intimidate (`:3850-3854`): DD=100 under `Condition:WarcryMaxHit`,
///   otherwise `+IntimidatingUpTimeRatio` — the warcry mechanic is not
///   implemented, so this input is always absent and the whole section is
///   skipped (TODO(warcry): fill in this branch once `IntimidatingUpTimeRatio` is wired up).
/// - Triple deducts from Double (`:3855-3859`): `DD = max(DD − TD×DD/100, 0)`.
/// - `ScaledDamageEffect = 1 × (1 + DD/100 + 2×TD/100)` (`:3860-3861`).
///
/// Note: vendor `:3845` (the Triple line) reads `Sum(...) or 0 + (...)`; due
/// to Lua operator precedence (`+` binds tighter than `or`) this actually
/// only evaluates to `Sum(...)`, dropping the enemy/OnCrit terms — which is
/// structurally inconsistent with the neighboring Double line (`:3849`, no
/// `or`), so this is judged a vendor typo; this implementation follows the
/// apparent **intended semantics** (mirroring Double).
///
/// TODO(globalLimit): the DOUBLED form of `chance to deal Double Damage` mods
/// carries a globalLimit — this depends on the T1 limit primitive; once that
/// lands it takes effect on the Sum side, and this function needs no changes.
pub fn scaled_damage_effect(
    db: &ModDb,
    enemy_db: &ModDb,
    cfg: &CalcConfig,
    crit_chance: f64,
) -> ScaledDamage {
    let triple_on_crit = db
        .sum(
            ModType::Base,
            cfg,
            &[ModName::from("TripleDamageChanceOnCrit")],
        )
        .min(100.0);
    let enemy_self_triple = if cfg.mode_effective {
        enemy_db.sum(
            ModType::Base,
            cfg,
            &[ModName::from("SelfTripleDamageChance")],
        )
    } else {
        0.0
    };
    let triple_chance = (db.sum(ModType::Base, cfg, &[ModName::from("TripleDamageChance")])
        + enemy_self_triple
        + triple_on_crit * crit_chance)
        .min(100.0);

    let double_on_crit = db
        .sum(
            ModType::Base,
            cfg,
            &[ModName::from("DoubleDamageChanceOnCrit")],
        )
        .min(100.0);
    let enemy_self_double = if cfg.mode_effective {
        enemy_db.sum(
            ModType::Base,
            cfg,
            &[ModName::from("SelfDoubleDamageChance")],
        )
    } else {
        0.0
    };
    let mut double_chance = (db.sum(ModType::Base, cfg, &[ModName::from("DoubleDamageChance")])
        + enemy_self_double
        + double_on_crit * crit_chance)
        .min(100.0);

    // Triple deducts from Double: when both roll, Triple takes precedence, so the overlapping probability is subtracted (:3855-3859).
    if triple_chance > 0.0 {
        double_chance = (double_chance - triple_chance * double_chance / 100.0).max(0.0);
    }

    ScaledDamage {
        effect: 1.0 + double_chance / 100.0 + 2.0 * triple_chance / 100.0,
        double_chance,
        triple_chance,
    }
}

/// Placeholder struct for allMult's remaining five factors (PoB2 `CalcOffence.lua:4023-4025`).
///
/// `allMult = ScaledDamageEffect × FistOfWarDamageEffect × AncestralCallDamageEffect ×
/// AncestralEmpowermentDamageEffect × AncestralEmpowermentCombinedDamageEffect ×
/// OffensiveWarcryEffect (swaps to the Max variant under WarcryMaxHit)`.
///
/// Only ScaledDamageEffect is implemented; warcry / ancestral are separate
/// mechanics, so this struct defaults every factor to 1.0 as a placeholder,
/// keeping the interface stable to avoid rework later.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AllMultExtras {
    pub fist_of_war: f64,
    pub ancestral_call: f64,
    pub ancestral_empowerment: f64,
    pub ancestral_empowerment_combined: f64,
    /// `OffensiveWarcryEffect`, or (under `Condition:WarcryMaxHit`)
    /// `MaxOffensiveWarcryEffect`, to be selected and supplied by a future warcry implementation.
    pub offensive_warcry: f64,
}

impl Default for AllMultExtras {
    fn default() -> Self {
        Self {
            fist_of_war: 1.0,
            ancestral_call: 1.0,
            ancestral_empowerment: 1.0,
            ancestral_empowerment_combined: 1.0,
            offensive_warcry: 1.0,
        }
    }
}

impl AllMultExtras {
    /// Product of the five factors.
    pub fn product(&self) -> f64 {
        self.fist_of_war
            * self.ancestral_call
            * self.ancestral_empowerment
            * self.ancestral_empowerment_combined
            * self.offensive_warcry
    }
}

/// Full allMult (`ScaledDamageEffect × the five factors`, PoB2 `CalcOffence.lua:4023-4025`).
/// Degenerates to `scaled.effect` when extras defaults to all 1.0.
pub fn all_mult(scaled: &ScaledDamage, extras: &AllMultExtras) -> f64 {
    scaled.effect * extras.product()
}

/// The two DPS end factors (PoB2 `CalcOffence.lua:3128-3130` / `:3863` / `:4407`).
///
/// `TotalDPS = AverageDamage × (HitSpeed or Speed) × dps_multiplier × quantity_multiplier`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DpsEndFactors {
    /// `skillData.dpsMultiplier × calcLib.mod(skillModList, cfg, "DPS")` (`:3863`).
    pub dps_multiplier: f64,
    /// `max(Sum(BASE, cfg, "QuantityMultiplier"), 1)` (`:3128`, floors at 1.0).
    pub quantity_multiplier: f64,
}

/// Resolves the two DPS end factors.
///
/// - `skill_dps_multiplier`: the `dpsMultiplier` carried by skill data
///   (catalog `SkillStatSetDef/SkillLevelDef.dps_multiplier`, passed through
///   from the T4 data layer; defaults to 1.0 when absent — vendor
///   `skillData.dpsMultiplier or 1`).
/// - The `"DPS"` ModName's inc/more are consumed here (`calcLib.mod` =
///   `(1 + Sum(INC)/100) × More`, CalcTools.lua:16-18).
/// - `QuantityMultiplier` is aggregated via ModDb BASE and floored at 1.0
///   (vendor only writes `output.QuantityMultiplier` when >1, which is
///   numerically equivalent to a floor).
pub fn dps_end_factors(
    db: &ModDb,
    cfg: &CalcConfig,
    skill_dps_multiplier: Option<f64>,
) -> DpsEndFactors {
    let dps_names = [ModName::from("DPS")];
    //  Grenade second detonation (vendor CalcOffence.lua:1124-1127):
    // `DPS MORE min(Sum(BASE,"GrenadeActivateTwice"),100)`. Vendor gates this
    // folding on skillTypes[Grenade]; PoBR doesn't add that gate here —
    // this ModName is only produced by SupportPayload's statmap stat
    // (SkillStatMap.lua:2795-2797), and Payload's
    // require_skill_types = Grenade (sup_dex.lua:3561) already confines its
    // source to grenade skills in the support adaptation decision, so
    // non-grenade skills' db always has this name absent (=0).
    let activate_twice = db
        .sum(ModType::Base, cfg, &[ModName::from("GrenadeActivateTwice")])
        .min(100.0);
    // Barrage repeats (vendor CalcOffence.lua:962-976): when a Barrageable
    // skill is granted SequentialProjectiles + BarrageRepeats by the Barrage
    // buff, it writes `DPS MORE (1 + Σ BarrageRepeats) × mod(BarrageRepeatDamage)`.
    // Vendor treats this value **as-is** as a MORE percentage (repeats=1 →
    // MORE 2 → ×1.02, confirmed by oracle on spirit-walker-twister
    // DpsMultiplier=1.02) — faithfully reproduced, not converted to a multiplier.
    // ponytail: the else branch of that same vendor block (crossbow barrage's
    // additionalProjectiles → attack speed penalty ReplaceMod) has no fixture
    // coverage and is not implemented for now.
    let barrage_repeats_more = {
        let barrageable = cfg.skill_types.intersects(SkillTypes::BARRAGEABLE)
            && db.flag(cfg, ModName::from("SequentialProjectiles"))
            && !db.flag(cfg, ModName::from("OneShotProj"))
            && !db.flag(cfg, ModName::from("NoAdditionalProjectiles"))
            && !db.flag(cfg, ModName::from("TriggeredBySnipe"));
        let repeats = db.sum(ModType::Base, cfg, &[ModName::from("BarrageRepeats")]);
        if barrageable && repeats > 0.0 {
            let repeat_damage_names = [ModName::from("BarrageRepeatDamage")];
            let repeat_damage = (1.0 + db.sum(ModType::Inc, cfg, &repeat_damage_names) / 100.0)
                * db.more(cfg, &repeat_damage_names);
            let dps_multi = (1.0 + repeats) * repeat_damage;
            // Vendor's MoreInternal rounds each named bucket's percentage
            // (ModList.lua:143 `result * round(modResult, 2)`): the Barrage
            // Repeats DPS MORE bucket is folded and then rounded to 0.01
            // (1.65 → ×1.0165 → ×1.02, confirmed by oracle on spirit-walker
            // DpsMultiplier=1.02).
            // ponytail: rounds this separately from db.more("DPS")'s bucket —
            // vendor merges both into the same "DPS" named bucket, multiplies,
            // then rounds; no corpus case has them coexist, merge the buckets
            // if one shows up.
            ((1.0 + dps_multi / 100.0) * 100.0).round() / 100.0
        } else {
            1.0
        }
    };
    let dps_multiplier = skill_dps_multiplier.unwrap_or(1.0)
        * (1.0 + db.sum(ModType::Inc, cfg, &dps_names) / 100.0)
        * db.more(cfg, &dps_names)
        * (1.0 + activate_twice / 100.0)
        * barrage_repeats_more;
    let quantity_multiplier = db
        .sum(ModType::Base, cfg, &[ModName::from("QuantityMultiplier")])
        .max(1.0);
    DpsEndFactors {
        dps_multiplier,
        quantity_multiplier,
    }
}
