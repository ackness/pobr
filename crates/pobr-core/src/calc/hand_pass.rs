//! MH/OH dual pass and combineStat merging.
//!
//! PoB2's offence outer layer runs the pipeline once each for main hand and
//! off hand (`CalcOffence.lua:2369-2449`'s passList: one pass each for
//! `weapon1Attack`/`weapon2Attack`, labeled "Main Hand"/"Off Hand", each with
//! its own weaponData source and weapon1Cfg/weapon2Cfg); at the end,
//! `combineStat` (`:2451-2545`, 8 modes) merges the two hands per [`COMBINE_TABLE`].
//!
//! PoBR's shape: the orchestration layer (pobr-build's `calc_orchestrator`
//! weapon section) assembles the weapon bases into [`HandSource`]s, this
//! module runs `calculate_minimal_vs_enemy` once per source (per-hand
//! condition flips plus weapon base injected into [`MinimalInput`]), then merges per the vendor modes.
//!
//! ## Single-pass passthrough invariant (I3, the correctness proof for the fallback switch)
//!
//! - `passes` empty: equivalent to calling `calculate_minimal_vs_enemy`
//!   directly (a non-attack skill, vendor's single "Skill" pass).
//! - A single [`HandSource`]: when vendor's `not skillFlags.bothWeaponAttack`
//!   holds, **every** stat takes the OR passthrough (`:2453`
//!   `if mode == "OR" or not skillFlags.bothWeaponAttack`), and the output is
//!   **value-for-value equal** to "the weapon base folded into
//!   `MinimalInput` and run once" — pinned by the equivalence tests.
//!
//! ## Per-hand weapon flags
//!
//! Per-hand cfg has two flip paths (equivalent to PoB2's weapon1Cfg/weapon2Cfg):
//! 1. Condition (`MainHandAttack`/`OffHandAttack`);
//! 2. Weapon flags: `cfg.flags = cfg.flags.replace_weapon_flags(weapon.flags)` —
//!    when [`WeaponBase::flags`] is non-empty, the cfg's whole
//!    `WEAPON_SEGMENT` is replaced with **that hand's** weapon flags (so a
//!    `with Maces` mod's flag routing only reaches the matching hand); when
//!    empty (a non-weapon-attack source), it identically passes through the
//!    upstream flags unchanged. See the `ModFlags::replace_weapon_flags` docs for the equivalence proof.

use pobr_data::prelude::ModFlags;

use crate::{CalcConfig, CombineMode, HandTag, ModDb};

use super::offence::{MinimalInput, MinimalOutput, calculate_minimal_vs_enemy};
use super::output::HandOutput;
use super::{BreakdownStep, round};

/// One hand's weapon base contribution (assembled by the orchestration
/// layer, already folded to exactly the same semantics as the current
/// `MinimalInput` injection: local mods × quality × skill baseMultiplier /
/// attackSpeedMultiplier are all already multiplied in).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct WeaponBase {
    /// Base hit damage lower/upper bound (added to `MinimalInput::base_hit_min/max`).
    pub hit_min: f64,
    pub hit_max: f64,
    /// Attack rate override (`Some` overrides
    /// `MinimalInput::base_action_rate`; `None`/non-positive means the input
    /// rate is used as-is — same semantics as the current orchestration's `w.attack_rate > 0.0` gate).
    pub attack_rate: Option<f64>,
    /// Weapon base crit chance (percentage points, e.g. 5.0). Currently
    /// injected globally by the orchestration layer via `CriticalStrikeChance
    /// BASE` (keeping current behavior); once the dual pass is truly enabled,
    /// per-hand crit base will be consumed from here instead.
    pub crit_chance: f64,
    /// This hand's ModFlags weapon flags (vendor `getWeaponFlags`, derived by
    /// the orchestration layer from `weapon_types.json` via
    /// `ModFlags::weapon_flags`). When non-empty, the per-hand cfg's weapon
    /// flag segment is replaced with this value (see the module docs); a
    /// non-weapon-attack source always has `NONE` (identically passes
    /// through the upstream cfg).
    pub flags: ModFlags,
}

/// Per-hand calculation context override (equivalent to PoB2's weapon1Cfg/weapon2Cfg).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct HandCfg {
    /// Condition flips (e.g. `("MainHandAttack", true)`).
    pub conditions: Vec<(String, bool)>,
}

/// A single hand's pass input.
#[derive(Debug, Clone, PartialEq)]
pub struct HandSource {
    pub label: HandTag,
    pub weapon: WeaponBase,
    pub cfg_overrides: HandCfg,
}

impl HandSource {
    /// Main hand pass: sets the `MainHandAttack` condition (PoB2's weapon1Cfg).
    pub fn main_hand(weapon: WeaponBase) -> Self {
        Self {
            label: HandTag::MainHand,
            weapon,
            cfg_overrides: HandCfg {
                conditions: vec![("MainHandAttack".to_string(), true)],
            },
        }
    }

    /// Off hand pass: sets the `OffHandAttack` condition (PoB2's weapon2Cfg;
    /// a non-weapon-attack source like Shield Wall is also off-hand, `CalcOffence.lua:2418-2431`).
    pub fn off_hand(weapon: WeaponBase) -> Self {
        Self {
            label: HandTag::OffHand,
            weapon,
            cfg_overrides: HandCfg {
                conditions: vec![("OffHandAttack".to_string(), true)],
            },
        }
    }
}

/// The end-of-pipeline merge table (a **verbatim copy** of vendor
/// `CalcOffence.lua`'s combineStat call sites; kept as a framework since this
/// mechanic's logic has been stable across versions). Currently covers the
/// stat subset the PoBR offence model implements plus already-planned
/// fields; vendor's remaining entries (the leech family `:4563-4587` all
/// DPS, the crossbow family `:4602-4610` all AVERAGE, the ailment family
/// `:5737-5755` AVERAGE/CHANCE_AILMENT/CHANCE) get added the same way once
/// the corresponding mechanic lands.
pub const COMBINE_TABLE: &[(&str, CombineMode)] = &[
    ("AccuracyHitChance", CombineMode::Average),      // :3023
    ("HitChance", CombineMode::Average),              // :3024
    ("Speed", CombineMode::HarmonicMean),             // :3026
    ("HitSpeed", CombineMode::Or),                    // :3027
    ("HitTime", CombineMode::Or),                     // :3028
    ("PreEffectiveCritChance", CombineMode::Average), // :4554
    ("CritChance", CombineMode::Crit { double_hits: false }), // :4555 (doubleHits flipped by skill data)
    ("CritMultiplier", CombineMode::Average),                 // :4557
    ("AverageDamage", CombineMode::Dps { double_hits: false }), // :4559
    ("TotalDPS", CombineMode::Dps { double_hits: false }),    // :4561
    ("StoredCombinedAvg", CombineMode::Dps { double_hits: false }), // :4588 (per damage type)
];

/// Looks up [`COMBINE_TABLE`]; the table's `double_hits` is a `false`
/// placeholder, flipped by the `double_hits` argument (vendor reads
/// `skillData.doubleHitsWhenDualWielding` inside combineStat).
pub fn combine_mode_for(stat: &str, double_hits: bool) -> Option<CombineMode> {
    COMBINE_TABLE
        .iter()
        .find(|(name, _)| *name == stat)
        .map(|(_, mode)| match mode {
            CombineMode::Dps { .. } => CombineMode::Dps { double_hits },
            CombineMode::Crit { .. } => CombineMode::Crit { double_hits },
            other => *other,
        })
}

/// Result of running the dual pass: the merged output plus per-hand sub-tables.
#[derive(Debug, Clone, PartialEq)]
pub struct HandPassOutput {
    /// Output after combineStat (a single-hand build = OR passthrough, value-for-value equal to a single pass).
    pub combined: MinimalOutput,
    pub main_hand: Option<HandOutput>,
    pub off_hand: Option<HandOutput>,
}

/// Runs the offence pipeline once per hand source for an attack skill, then
/// merges per vendor combineStat.
///
/// - `passes` empty = a non-attack skill's single "Skill" pass: passes through to `calculate_minimal_vs_enemy`.
/// - `double_hits` = skill data's `doubleHitsWhenDualWielding` (extracted
///   incidentally by the schema; `false` until the orchestration layer wires it up).
pub fn run_hand_passes(
    db: &ModDb,
    enemy_db: &ModDb,
    cfg: &CalcConfig,
    passes: &[HandSource],
    input: &MinimalInput,
    double_hits: bool,
) -> HandPassOutput {
    match passes {
        [] => HandPassOutput {
            combined: calculate_minimal_vs_enemy(db, enemy_db, cfg, input),
            main_hand: None,
            off_hand: None,
        },
        [single] => {
            // Single hand: vendor's `not bothWeaponAttack` → every stat takes the OR passthrough (:2453).
            let leg = run_single_pass(db, enemy_db, cfg, single, input);
            let hand = HandOutput::from_minimal(&leg);
            let (main_hand, off_hand) = match single.label {
                HandTag::MainHand | HandTag::Single => (Some(hand), None),
                HandTag::OffHand => (None, Some(hand)),
            };
            HandPassOutput {
                combined: leg,
                main_hand,
                off_hand,
            }
        }
        [first, second, ..] => {
            debug_assert_eq!(passes.len(), 2, "passList 至多 MH/OH 两个 pass");
            let mh_leg = run_single_pass(db, enemy_db, cfg, first, input);
            let oh_leg = run_single_pass(db, enemy_db, cfg, second, input);
            let combined = combine_legs(&mh_leg, &oh_leg, double_hits);
            HandPassOutput {
                combined,
                main_hand: Some(HandOutput::from_minimal(&mh_leg)),
                off_hand: Some(HandOutput::from_minimal(&oh_leg)),
            }
        }
    }
}

/// Runs a single hand: the weapon base is injected into a copy of
/// `MinimalInput` (in exactly the same shape as the current orchestration
/// folding), then the pipeline is run once with per-hand condition flips applied.
fn run_single_pass(
    db: &ModDb,
    enemy_db: &ModDb,
    cfg: &CalcConfig,
    hand: &HandSource,
    input: &MinimalInput,
) -> MinimalOutput {
    let (hand_cfg, hand_input) = hand_scope(hand, cfg, input);
    calculate_minimal_vs_enemy(db, enemy_db, &hand_cfg, &hand_input)
}

/// Derives the per-hand calculation scope (weapon base injected into a copy
/// of `MinimalInput` + per-hand condition flips + weapon flag segment
/// replacement). Shared between `run_single_pass` and the warcry uptime
/// budget (`calc::warcry` needs to resolve Speed against the main-hand scope,
/// since vendor CalcOffence.lua:3235 reads that same pass's `globalOutput.Speed`),
/// guaranteeing the two rates agree bit-for-bit.
pub(crate) fn hand_scope(
    hand: &HandSource,
    cfg: &CalcConfig,
    input: &MinimalInput,
) -> (CalcConfig, MinimalInput) {
    let mut hand_input = *input;
    hand_input.base_hit_min += hand.weapon.hit_min;
    hand_input.base_hit_max += hand.weapon.hit_max;
    if let Some(rate) = hand.weapon.attack_rate
        && rate > 0.0
    {
        hand_input.base_action_rate = rate;
    }
    let mut hand_cfg = cfg.clone();
    for (name, enabled) in &hand.cfg_overrides.conditions {
        hand_cfg.conditions.insert(name.clone(), *enabled);
    }
    // Per-hand weapon flags: when non-empty, replaces the cfg's weapon flag
    // segment with that hand's weapon flags (empty = identity, zero behavior
    // change for legacy flag tables / non-weapon-attack sources).
    hand_cfg.flags = hand_cfg.flags.replace_weapon_flags(hand.weapon.flags);
    (hand_cfg, hand_input)
}

/// combineStat: merges the two legs per [`COMBINE_TABLE`] (vendor `:2451-2545` + call sites).
///
/// The defence/resource family (life/mana/resistances) isn't in passList
/// (vendor computes it globally outside the hand pass), so both legs' values
/// are always equal — takes the MH leg's value and debug-asserts they match.
fn combine_legs(mh: &MinimalOutput, oh: &MinimalOutput, double_hits: bool) -> MinimalOutput {
    debug_assert_eq!(mh.life, oh.life, "防御族不在 hand pass 维度内");
    debug_assert_eq!(mh.mana, oh.mana, "防御族不在 hand pass 维度内");

    let combine = |stat: &str, mh_v: f64, oh_v: f64| -> f64 {
        let mode = combine_mode_for(stat, double_hits)
            .unwrap_or_else(|| unreachable!("combine_legs 只对 COMBINE_TABLE 内 stat 调用"));
        mode.combine(&[mh_v, oh_v])
            .expect("COMBINE_TABLE 内全部为自给模式")
    };

    // CritChance is internally a fraction (0..=1), but vendor's CRIT mode
    // formula is defined in percentage terms (the doubleHits cross term
    // divides by 100) -- convert to percentage space, merge, then convert back.
    let crit_chance =
        round(combine("CritChance", mh.crit_chance * 100.0, oh.crit_chance * 100.0) / 100.0);
    let pre_effective_crit_chance = round(
        combine(
            "PreEffectiveCritChance",
            mh.pre_effective_crit_chance * 100.0,
            oh.pre_effective_crit_chance * 100.0,
        ) / 100.0,
    );
    let crit_multiplier = round(combine(
        "CritMultiplier",
        mh.crit_multiplier,
        oh.crit_multiplier,
    ));
    let total_hit_avg = round(combine("AverageDamage", mh.total_hit_avg, oh.total_hit_avg));
    let hit_chance = round(combine("HitChance", mh.hit_chance, oh.hit_chance));
    let action_rate = round(combine("Speed", mh.action_rate, oh.action_rate));
    let dps = round(combine("TotalDPS", mh.dps, oh.dps));

    MinimalOutput {
        // Defence/resource family: pass-independent, takes the MH leg.
        life: mh.life,
        mana: mh.mana,
        fire_resistance: mh.fire_resistance,
        cold_resistance: mh.cold_resistance,
        lightning_resistance: mh.lightning_resistance,
        max_fire_resistance: mh.max_fire_resistance,
        max_cold_resistance: mh.max_cold_resistance,
        max_lightning_resistance: mh.max_lightning_resistance,
        fire_resistance_over_cap: mh.fire_resistance_over_cap,
        cold_resistance_over_cap: mh.cold_resistance_over_cap,
        lightning_resistance_over_cap: mh.lightning_resistance_over_cap,
        crit_chance,
        pre_effective_crit_chance,
        crit_multiplier,
        // Top-level component vector: currently takes the MH leg when dual
        // wielding (per-hand components live in HandOutput; migrating
        // ailment magnitude consumption to the Stored family is a wiring
        // task, and since the orchestration layer never produces a second
        // HandSource yet, there's no consumer exercising this branch).
        damage_components: mh.damage_components.clone(),
        total_hit_avg,
        hit_chance,
        action_rate,
        dps,
        // Stored family: CombinedAvg is merged with the DPS mode per damage
        // type as vendor does at :4588; Crit/HitAvg are per-leg diagnostic
        // values (vendor never merges these across hands), so the top level takes the MH leg.
        stored_crit_avg: mh.stored_crit_avg.clone(),
        stored_hit_avg: mh.stored_hit_avg.clone(),
        // min/max family, same as Crit/HitAvg: per-leg diagnostic values
        // (vendor never merges Stored min/max across hands; ailments merge
        // with the DPS CHANCE_AILMENT mode after computing per-hand), so the top level takes the MH leg.
        stored_ranges: mh.stored_ranges.clone(),
        stored_combined_avg: combine_stored_by_type(
            &mh.stored_combined_avg,
            &oh.stored_combined_avg,
            double_hits,
        ),
        breakdown: combined_breakdown(
            mh,
            crit_chance,
            crit_multiplier,
            total_hit_avg,
            hit_chance,
            action_rate,
            dps,
        ),
    }
}

/// Merged breakdown: reuses single-pass step names (display/CLI consumers
/// look values up by name); offence-family steps use the merged value, defence-family steps keep the MH leg.
#[allow(clippy::too_many_arguments)]
fn combined_breakdown(
    mh: &MinimalOutput,
    crit_chance: f64,
    crit_multiplier: f64,
    total_hit_avg: f64,
    hit_chance: f64,
    action_rate: f64,
    dps: f64,
) -> Vec<BreakdownStep> {
    mh.breakdown
        .iter()
        .map(|step| {
            let value = match step.name {
                "crit_chance" => crit_chance,
                "crit_multiplier" => crit_multiplier,
                "total_hit_avg" => total_hit_avg,
                "hit_chance" => hit_chance,
                "action_rate" => action_rate,
                "dps" => dps,
                _ => step.value,
            };
            BreakdownStep {
                name: step.name,
                value,
            }
        })
        .collect()
}

/// Cross-hand merge for `Stored<Type>CombinedAvg` (vendor `:4588`'s per-damage-type DPS mode).
/// The two legs' type sets are aligned in MH order; a type missing from OH is folded in as 0 (vendor's `or 0` semantics).
fn combine_stored_by_type(
    mh: &[(pobr_data::prelude::DamageType, f64)],
    oh: &[(pobr_data::prelude::DamageType, f64)],
    double_hits: bool,
) -> Vec<(pobr_data::prelude::DamageType, f64)> {
    let mode = combine_mode_for("StoredCombinedAvg", double_hits)
        .expect("StoredCombinedAvg 在 COMBINE_TABLE 内");
    mh.iter()
        .map(|(ty, mh_v)| {
            let oh_v = oh
                .iter()
                .find(|(oh_ty, _)| oh_ty == ty)
                .map_or(0.0, |(_, v)| *v);
            let combined = mode.combine(&[*mh_v, oh_v]).expect("DPS 是自给模式");
            (*ty, combined)
        })
        .collect()
}

impl HandOutput {
    /// Extracts combineStat's input surface from one leg's `MinimalOutput`.
    pub fn from_minimal(leg: &MinimalOutput) -> Self {
        Self {
            hit_chance: leg.hit_chance,
            crit_chance: leg.crit_chance,
            pre_effective_crit_chance: leg.pre_effective_crit_chance,
            crit_multiplier: leg.crit_multiplier,
            speed: leg.action_rate,
            damage_components: leg.damage_components.clone(),
            average_hit: leg.total_hit_avg,
            // vendor AverageDamage = AverageHit × HitChance (around :4406);
            // the panel view uses the player-side total_hit_avg (same source as the top-level field).
            average_damage: round(leg.total_hit_avg * leg.hit_chance),
            total_dps: leg.dps,
            //  Stored family (produced by crit_pass; the vendor-view input for ailment magnitude).
            stored_crit_avg: leg.stored_crit_avg.clone(),
            stored_hit_avg: leg.stored_hit_avg.clone(),
            stored_combined_avg: leg.stored_combined_avg.clone(),
            stored_ranges: leg.stored_ranges.clone(),
        }
    }
}
