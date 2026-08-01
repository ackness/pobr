//! Stun system (13-G12; PoB2 `CalcDefence.lua:2525-2643` stun section).
//!
//! Produces three outputs: `StunThreshold` (base switching +
//! AddESToStunThreshold + BASE/INC/MORE aggregation), `SelfStunChance`
//! (`StunBaseMult × effective damage / threshold`, with physical weighted
//! ×0.25), and `StunDuration` (rounded up to the nearest server tick).
//!
//! All constants are injected via `cfg.constants`; the keystone flag
//! (ChaosInoculation) is passed in by the caller via [`StunInputs`] from the
//! C-1 `DefenceKeystones` snapshot — this module never reads keystones directly.

use pobr_data::prelude::*;

use crate::{CalcConfig, ModDb};

use super::round;

/// Inputs for stun calculation (pool values, hit-taken references, upstream
/// avoidance outputs, and keystone flags).
///
/// `total_taken_hit` / `physical_taken_hit` correspond to PoB2's
/// `output.totalTakenHit` / `output.PhysicalTakenHit` (:2617) — until Track F
/// is wired up, the caller approximates these with a single-hit reference
/// value; after F lands they switch to the real damage-pool-deduction values
/// (the formula itself is unchanged).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct StunInputs {
    /// Current life pool (`output.Life`, the default threshold base, :2542-2543).
    pub life: f64,
    /// Player character panel's flat base life (the scalar part of the CI
    /// branch's "Life before CI"; vendor `Sum("BASE","Life")` includes the
    /// character base, which in pobr lives in `ActorBaseStats` and must be
    /// supplied by the caller).
    pub life_base_flat: f64,
    /// Current ES pool (used by ES base switching / AddESToStunThreshold, :2529-2548).
    pub energy_shield: f64,
    /// Current mana pool (used by Mana base switching, :2533-2536).
    pub mana: f64,
    /// Total hit damage taken (`output.totalTakenHit`, :2617).
    pub total_taken_hit: f64,
    /// Physical hit damage taken (`output.PhysicalTakenHit`, the physical
    /// ×0.25-weighted term at :2617).
    pub physical_taken_hit: f64,
    /// Stun avoidance chance (`avoid_stun` from [`super::calc_avoidance`],
    /// already including the implicit ES halving; `notAvoidChance = 100 −
    /// avoid_stun`, :2554-2558).
    pub avoid_stun: f64,
    /// CI keystone (threshold base becomes "Life before CI", :2537-2539);
    /// supplied by the caller from the C-1 `DefenceKeystones::chaos_inoculation` snapshot.
    pub chaos_inoculation: bool,
}

/// Stun calculation result (feeds `stun_threshold` / `self_stun_chance` /
/// `stun_duration` in W0.2).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct StunResult {
    /// Stun threshold (:2552).
    pub threshold: f64,
    /// Self-stun chance (%, :2623-2624, already multiplied by 1 − avoidance).
    pub self_stun_chance: f64,
    /// Stun duration (seconds, rounded up to the nearest server tick, :2594;
    /// avoidance ≥100% → 0, :2584-2586).
    pub stun_duration: f64,
}

/// Stun threshold (PoB2 CalcDefence.lua:2526-2552).
///
/// Base switching (mutually exclusive branches, :2528-2543):
/// - `StunThresholdBasedOnEnergyShieldInsteadOfLife` → `ES × ΣBASE StunThresholdEnergyShieldPercent / 100`;
/// - `StunThresholdBasedOnManaInsteadOfLife` → `Mana × ΣBASE StunThresholdManaPercent / 100`;
/// - `ChaosInoculation` → Life before CI (flat base + ΣBASE MaximumLife, :2537-2539);
/// - default → `output.Life`.
///
/// Addition (:2544-2548): `AddESToStunThreshold` → `+ ES × ΣBASE ESToStunThresholdPercent / 100`.
/// Aggregation (:2549-2552): `(base + ΣBASE StunThreshold) × (1 + ΣINC/100) × ΠMORE`.
pub fn calc_stun_threshold(db: &ModDb, cfg: &CalcConfig, inp: &StunInputs) -> f64 {
    let names = [ModName::from("StunThreshold")];
    let base = if db.flag(
        cfg,
        ModName::from("StunThresholdBasedOnEnergyShieldInsteadOfLife"),
    ) {
        let pct = db.sum(
            ModType::Base,
            cfg,
            &[ModName::from("StunThresholdEnergyShieldPercent")],
        );
        inp.energy_shield * pct / 100.0
    } else if db.flag(cfg, ModName::from("StunThresholdBasedOnManaInsteadOfLife")) {
        let pct = db.sum(
            ModType::Base,
            cfg,
            &[ModName::from("StunThresholdManaPercent")],
        );
        inp.mana * pct / 100.0
    } else if inp.chaos_inoculation {
        // Life before CI (vendor `Sum("BASE","Life")`; pobr's life-pool BASE
        // mod is named MaximumLife, with the character's flat base supplied
        // via life_base_flat).
        inp.life_base_flat + db.sum(ModType::Base, cfg, &[ModName::from("MaximumLife")])
    } else {
        inp.life
    };
    let base = if db.flag(cfg, ModName::from("AddESToStunThreshold")) {
        let pct = db.sum(
            ModType::Base,
            cfg,
            &[ModName::from("ESToStunThresholdPercent")],
        );
        base + inp.energy_shield * pct / 100.0
    } else {
        base
    };
    let flat = db.sum(ModType::Base, cfg, &names);
    let inc = 1.0 + db.sum(ModType::Inc, cfg, &names) / 100.0;
    let more = db.more(cfg, &names);
    round((base + flat) * inc * more)
}

/// Full stun pipeline (PoB2 CalcDefence.lua:2525-2643; threshold → duration → self-stun chance).
///
/// # Formulas (line by line)
/// - Duration (:2584-2595): `StunAvoidChance ≥ 100` → 0; otherwise
///   `StunDuration = ceil(StunBaseDuration × (1+INC StunDuration) / (1+INC StunRecovery)
///   × ServerTickRate) / ServerTickRate` (rounded up to the tick;
///   `ServerTickRate = 1/server_tick_seconds`, Data.lua:172-173).
/// - Effective stun damage (:2617): `totalTakenHit + PhysicalTakenHit × 0.25`;
///   the default `damageCategoryConfig` "Average" branch then multiplies by
///   `PhysicalStunMult` (:2620-2621, `monster.physical_hit_stun_multiplier_pct/100 = 1.0`).
///   The non-Average branch's `× PhysicalStunMult × (1 + MeleeStunMult×3)/4`
///   (:2618-2619) depends on config input; once config_interpreter is wired
///   up this gains parameters, but the formula skeleton doesn't change.
/// - Chance (:2623-2624): `baseStunChance = min(StunBaseMult × effective damage / threshold, 100)`;
///   below `MinStunChanceNeeded`(20) it's zeroed; then `× notAvoidChance / 100`.
pub fn calc_stun(db: &ModDb, cfg: &CalcConfig, inp: &StunInputs) -> StunResult {
    let game = cfg.constants.game();
    let threshold = calc_stun_threshold(db, cfg, inp);

    // :2554-2558 notAvoidChance is recovered from the upstream avoid_stun
    // (already includes StunImmune / the implicit ES halving).
    let not_avoid = (100.0 - inp.avoid_stun).max(0.0);

    // Duration (:2584-2595)
    let stun_duration = if not_avoid <= 0.0 {
        // :2584-2586 "Cannot be Stunned" → 0.
        0.0
    } else {
        let dur_inc = 1.0 + db.sum(ModType::Inc, cfg, &[ModName::from("StunDuration")]) / 100.0;
        let recovery = 1.0 + db.sum(ModType::Inc, cfg, &[ModName::from("StunRecovery")]) / 100.0;
        let tick = game.server_tick_seconds;
        // :2594 m_ceil(base × dur / recovery × ServerTickRate) / ServerTickRate.
        let raw = game.stun_base_duration_seconds * dur_inc / recovery;
        round((raw / tick).ceil() * tick)
    };

    // Self-stun chance (:2617-2624)
    // :2617 physical weighted ×0.25; :2620-2621 default "Average" scope × PhysicalStunMult.
    let effective_damage = (inp.total_taken_hit + inp.physical_taken_hit * 0.25)
        * (cfg.constants.monster().physical_hit_stun_multiplier_pct / 100.0);
    // :2623 vendor divides by zero when threshold is 0 (in practice threshold
    // is always ≥1); defensively: any damage always stuns, no damage never does.
    let base_chance = if threshold > 0.0 {
        (game.stun_base_mult * effective_damage / threshold).min(100.0)
    } else if effective_damage > 0.0 {
        100.0
    } else {
        0.0
    };
    // :2624 below MinStunChanceNeeded(20) it's zeroed, then multiplied by (1 − avoidance).
    let gated = if base_chance > game.min_stun_chance_needed {
        base_chance
    } else {
        0.0
    };
    let self_stun_chance = round(gated * not_avoid / 100.0);

    StunResult {
        threshold,
        self_stun_chance,
        stun_duration,
    }
}
