//! Trigger domain: cooldown-gated trigger rate caps + energy-driven meta-gem model +
//! multi-skill rotation + CWC.
//!
//! ## Structure
//!
//! ### §1  Cooldown-gated trigger rate cap
//! Corresponds to agent-docs/triggers.md §3: `TriggerRateCap = 1/(ceil(cd × ServerTickRate)/ServerTickRate)`,
//! double-gated by `SkillTriggerRate = min(cap, sourceRate)`, with ICDR shortening the trigger
//! cooldown as a divisor.
//! Source: PoB2 `CalcTriggers.lua` (`modActionCooldown / rateCapAdjusted / SkillTriggerRate`),
//!       PoB2 `Data.lua` (`ServerTickTime = 0.033`).
//!
//! ### §2  Energy-driven (Energy / Meta Gem) model
//! Cast on X (Cast on Critical, Cast on Ailment, etc.) uses an Energy counter to decide when to
//! trigger.
//! - `max_energy = Σ(socketed base_cast_time / 0.1) × 10`; total-use-time modifiers count double.
//! - Generation: `centienergy_per_hit = MonsterPower × baseCentienergy × scale`, Crit/Ignite/Shock=100,
//!   Freeze=1000 (centienergy = 1/100 energy); CoC additionally multiplies by (raw damage / ailment
//!   threshold).
//! - Level bonus: `energy_generated_+%` boosts the generation rate (doesn't change the base or cap).
//! - Trigger frequency estimate: `≈ source_rate × energy_per_event / max_energy`, capped at
//!   `trigger_rate_cap`.
//!
//! Source: agent-docs/triggers.md §2; PoB2 `act_int.lua` / `other.lua`; PoE2 Wiki CoC; PoE2DB Energy.
//!
//! ### §3  Multi-skill rotation
//! Ports PoB2 `calcMultiSpellRotationImpact`: a deterministic simulation of 1000 trigger
//! opportunities, frame-aligned cooldowns, and a geometric-distribution conversion.
//! - Each trigger opportunity fires the first skill (in rotation order) that has left cooldown;
//!   if all skills are on cooldown, that opportunity is wasted.
//! - `next_trig = ceil_tick(floor_tick(now) + cd)` (cooldown counts from the current frame boundary).
//! - When trigger chance < 100%, the actual trigger rate is converted using the geometric
//!   distribution's expected value.
//!
//! Source: agent-docs/triggers.md §5; PoB2 `CalcTriggers.lua::calcMultiSpellRotationImpact`.
//!
//! ### §4  CWC (Cast While Channelling)
//! Channelling trigger: `triggerTime` (the channel fires once every N seconds, rounded to the
//! server tick) sets the base cadence, further clamped by the triggered skill's cooldown.
//! Optional `SpellCastTimeAddedToCooldownIfTriggered` (adds cast time to the cooldown).
//! `TriggeredDamage` INC/MORE act as a Damage factor on the triggered skill (not injected here,
//! left for the integration layer to reference).
//! Source: agent-docs/triggers.md §4.2; PoB2 `CalcTriggers.lua::CWCHandler`.
//!
//! ## Parallel-safety
//! This module **only touches trigger.rs and tests/trigger.rs**, never perform/output/offence/env/actor/mod_db.
//! New pub functions are re-exported via `calc/mod.rs` (append only, never edit existing
//! re-export lines).
//!
//! ## Deferred
//! Full Monte Carlo precision alignment for the energy model (would need frame-by-frame server-tick
//! simulation) is left for a golden fixture; PoB2's own support for energy-driven meta gems "needs
//! an entire overhaul", so pobr's current energy-trigger-rate estimate is a **deterministic
//! approximation** — see the deviation notes on `EnergyTriggerRate`.

use super::round;

// §1  Server-tick utilities & cooldown-gated basics

/// Server tick rate (actions/s), `1 / tick_seconds ≈ 30.3`.
/// Source: PoB2 Data.lua `ServerTickRate = 1/0.033`.
///
/// `tick_seconds` is injected by the caller's constant pack
/// (`cfg.constants.game().server_tick_seconds`; falls back to the old const's value unchanged).
pub fn server_tick_rate(tick_seconds: f64) -> f64 {
    1.0 / tick_seconds
}

/// Rounds a cooldown up to the server tick: `ceil(cd × rate) / rate`.
///
/// Triggers can only happen on frame boundaries, so the real cooldown gets rounded up to the
/// next frame. This is the root cause of the "stair-step" pattern in trigger rates.
/// Source: agent-docs/triggers.md §3.2; PoB2 CalcTriggers.lua.
pub fn round_cooldown_to_tick(cooldown: f64, tick_rate: f64) -> f64 {
    if cooldown <= 0.0 || tick_rate <= 0.0 {
        return 0.0;
    }
    round((cooldown * tick_rate).ceil() / tick_rate)
}

/// Pure trigger-rate-cap function: `cap = 1 / (ceil(cd × rate) / rate)`.
///
/// `cd` is the actual action cooldown (already the result of `max(triggeredCD, triggerCD/icdr)`);
/// `tick_rate` is the server tick rate (defaults to `server_tick_rate(SERVER_TICK_SECONDS)`).
/// Returns the trigger cap in triggers/second.
/// Source: agent-docs/triggers.md §3.1; PoB2 CalcTriggers.lua
/// `TriggerRateCap = 1/(ceil(modActionCooldown × ServerTickRate)/ServerTickRate)`.
pub fn trigger_rate_cap(cooldown: f64, tick_rate: f64) -> f64 {
    let rounded = round_cooldown_to_tick(cooldown, tick_rate);
    if rounded > 0.0 {
        round(1.0 / rounded)
    } else {
        0.0
    }
}

/// Computes the actual action cooldown: `max(triggeredCD, triggerCD / icdr)`.
///
/// - `trigger_cd`: the trigger gem's own cooldown (`triggeredBy.grantedEffect.levels[lvl].cooldown`).
/// - `triggered_cd`: the triggered skill's cooldown (`skillData.cooldown`); pass 0 if none.
/// - `icdr`: the cooldown-recovery-rate factor (`CooldownRecovery`, the multiplier after
///   folding INC/MORE, ≥0), used as a **divisor** to shorten the trigger gem's cooldown.
///
/// Source: agent-docs/triggers.md §3.1; PoB2 CalcTriggers.lua
/// `modActionCooldown = max(triggeredCD, triggerCD / icdrSkill)`.
pub fn action_cooldown(trigger_cd: f64, triggered_cd: f64, icdr: f64) -> f64 {
    let effective_trigger = if icdr > 0.0 {
        trigger_cd / icdr
    } else {
        trigger_cd
    };
    effective_trigger.max(triggered_cd)
}

/// Settlement result of the trigger rate cap (cooldown-gated version).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TriggerRate {
    /// The actual action cooldown (seconds) after taking the max, before rounding.
    pub action_cooldown: f64,
    /// The cooldown (seconds) after rounding up to the server tick.
    pub rate_cap_cooldown: f64,
    /// Trigger rate cap (triggers/second) = 1 / rate_cap_cooldown.
    pub trigger_rate_cap: f64,
    /// Actual trigger rate (triggers/second) = min(cap, effective source rate).
    pub skill_trigger_rate: f64,
    /// Whether the source rate is the gating factor (source rate < cap).
    pub limited_by_source: bool,
}

/// End-to-end: derives the actual trigger rate from trigger/triggered-skill cooldowns + ICDR +
/// the effective source rate.
///
/// `SkillTriggerRate = min(TriggerRateCap, EffectiveSourceRate)` — no matter how high the damage,
/// if the source attack speed is low or the cooldown is long, the trigger rate is still slow
/// (double gating). Source: agent-docs/triggers.md §3.3; PoB2 CalcTriggers.lua.
pub fn resolve_trigger_rate(
    trigger_cd: f64,
    triggered_cd: f64,
    icdr: f64,
    effective_source_rate: f64,
    tick_seconds: f64,
) -> TriggerRate {
    let tick_rate = server_tick_rate(tick_seconds);
    let cd = action_cooldown(trigger_cd, triggered_cd, icdr);
    let rate_cap_cooldown = round_cooldown_to_tick(cd, tick_rate);
    let cap = if rate_cap_cooldown > 0.0 {
        1.0 / rate_cap_cooldown
    } else {
        0.0
    };

    let source = effective_source_rate.max(0.0);
    let (skill_rate, limited_by_source) = if source > 0.0 && source < cap {
        (source, true)
    } else {
        (cap, false)
    };

    TriggerRate {
        action_cooldown: round(cd),
        rate_cap_cooldown: round(rate_cap_cooldown),
        trigger_rate_cap: round(cap),
        skill_trigger_rate: round(skill_rate),
        limited_by_source,
    }
}

// §2  Energy-driven meta-gem model

/// The trigger-condition type — determines the centienergy base and how generation is computed.
///
/// Source: agent-docs/triggers.md §2.2; PoB2 `act_int.lua` centienergy constant table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TriggerCondition {
    /// Cast on Critical: centienergy = MonsterPower × 100 × (hit_damage / ailment_threshold).
    /// Since 0.5.0 this also depends on the ratio of the critical hit's raw damage to the
    /// ailment threshold.
    CriticalStrike,
    /// Cast on Ignite: centienergy = MonsterPower × 100 (adjusted by the ignite magnitude/threshold ratio).
    Ignite,
    /// Cast on Shock: centienergy = MonsterPower × 100.
    Shock,
    /// Cast on Freeze: centienergy = MonsterPower × 1000 (the freeze base is 10× Crit/Ignite/Shock).
    Freeze,
    /// Cast on Melee Kill / Cast on Minion Death / Hit, etc. — a fixed amount of energy per
    /// event (centienergy=100 by default).
    Other,
}

impl TriggerCondition {
    /// The base centienergy (1/100 energy) for this trigger condition, per MonsterPower per event.
    ///
    /// Source: agent-docs/triggers.md §2.2 table; PoB2 act_int.lua
    /// `cast_on_crit_gain_X_centienergy_per_monster_power_on_crit = 100`,
    /// `cast_on_freeze_gain_X_centienergy_per_monster_power_on_freeze = 1000`.
    pub fn base_centienergy(self) -> f64 {
        match self {
            TriggerCondition::CriticalStrike
            | TriggerCondition::Ignite
            | TriggerCondition::Shock
            | TriggerCondition::Other => 100.0,
            TriggerCondition::Freeze => 1000.0,
        }
    }
}

/// Parameters for the max-energy calculation (each socketed spell's base cast time +
/// total-use-time modifier).
///
/// Source: agent-docs/triggers.md §2.1; PoB2 other.lua
/// `generic_ongoing_trigger_1_maximum_energy_per_Xms_total_cast_time = 10`,
/// `generic_ongoing_trigger_maximum_energy_is_total_of_socketed_skills`.
#[derive(Debug, Clone, PartialEq)]
pub struct SocketedSpellInfo {
    /// This slot's spell's base cast time (seconds).
    pub base_cast_time: f64,
    /// The total-use-time modifier percentage (%); counted ×2 when computing max energy.
    /// Source: agent-docs/triggers.md §2.1 "modifiers to Total use time are treated as though
    /// they were double the value".
    pub use_time_increase_pct: f64,
}

impl SocketedSpellInfo {
    pub fn new(base_cast_time: f64) -> Self {
        Self {
            base_cast_time,
            use_time_increase_pct: 0.0,
        }
    }

    pub fn with_use_time_increase(mut self, pct: f64) -> Self {
        self.use_time_increase_pct = pct;
        self
    }

    /// The "effective total use time" used for the max-energy calculation:
    /// `base_cast_time × (1 + use_time_increase_pct/100 × 2)`.
    ///
    /// The total-use-time modifier counts double in the energy calculation (effectively
    /// amplifying the penalty for casting slower).
    /// Source: agent-docs/triggers.md §2.1; PoE2 Wiki CoC; PoE2DB Energy.
    pub fn effective_cast_time_for_energy(&self) -> f64 {
        self.base_cast_time * (1.0 + self.use_time_increase_pct / 100.0 * 2.0)
    }
}

/// Computes the max energy of an energy-driven meta gem: `Σ(effective_cast_time / 0.1) × 10`.
///
/// Equivalent to `Σ effective_cast_time × 100` (10 energy per 0.1s of base cast time).
/// Higher max energy → harder to trigger (has to build up to the cap before firing).
///
/// Source: agent-docs/triggers.md §2.1; PoB2 other.lua
/// `generic_ongoing_trigger_1_maximum_energy_per_Xms_total_cast_time = 10`, i.e. "Has 10 maximum
/// Energy per 0.1 seconds of base cast time of Socketed Spells".
pub fn calc_max_energy(socketed_spells: &[SocketedSpellInfo]) -> f64 {
    if socketed_spells.is_empty() {
        return 0.0;
    }
    let total: f64 = socketed_spells
        .iter()
        .map(|s| (s.effective_cast_time_for_energy() / 0.1) * 10.0)
        .sum();
    round(total)
}

/// Energy generated per trigger event (hit/crit/kill/etc.), not centienergy.
///
/// Formula (CoC): `energy = MonsterPower × (hit_damage / ailment_threshold) × scale`
/// where `scale = energy_generated_pct_bonus / 100` (gem-level bonus, ≥ 1.0).
///
/// Other conditions (Ignite/Shock/Freeze/Other):
/// - `energy = MonsterPower × base_centienergy / 100 × scale`.
/// - CoC additionally multiplies by "raw damage / ailment threshold" (more damage → more energy).
///
/// Source: agent-docs/triggers.md §2.2; PoE2 Wiki CoC 0.5.0 formula.
///
/// # Parameters
/// - `condition`: the trigger condition type (determines the centienergy base).
/// - `monster_power`: the enemy's power (usually 0.5–3; rare monsters multiply by a rarity factor
///   of 1/2/5, unique=20).
/// - `hit_damage`: the raw damage of this hit (before mitigation); only meaningful for CoC, pass
///   0 for other conditions.
/// - `ailment_threshold`: the monster's ailment threshold; only meaningful for CoC, pass 1.0 for
///   other conditions to avoid division by zero.
/// - `energy_generated_scale`: the gem level's "energy_generated_+%"/100 + 1 (i.e. the multiplier,
///   e.g. 1.57).
pub fn calc_energy_per_event(
    condition: TriggerCondition,
    monster_power: f64,
    hit_damage: f64,
    ailment_threshold: f64,
    energy_generated_scale: f64,
) -> f64 {
    let base_centienergy = condition.base_centienergy();
    let monster_power = monster_power.max(0.0);
    let scale = energy_generated_scale.max(1.0);

    let centienergy = match condition {
        TriggerCondition::CriticalStrike => {
            // CoC 0.5.0: generation also depends on raw damage / the monster's ailment threshold.
            // For reliable triggering, crit damage needs to be roughly 10× the monster's ailment threshold.
            let threshold = ailment_threshold.max(1.0);
            let damage_ratio = (hit_damage / threshold).max(0.0);
            monster_power * base_centienergy * damage_ratio * scale
        }
        _ => {
            // Ignite/Shock/Freeze/Other: generation scales linearly with MonsterPower × base_centienergy.
            monster_power * base_centienergy * scale
        }
    };
    // centienergy / 100 = energy.
    round(centienergy / 100.0)
}

/// Settlement result of the energy-driven trigger rate.
///
/// **Note (deviation from PoB2)**: the current implementation is a **deterministic
/// approximation** (not a frame-by-frame Monte Carlo simulation).
/// - Assumes each "trigger event" (each hit/crit) generates a constant amount of energy (a mean
///   value stands in for the distribution).
/// - `effective_trigger_rate` = `source_rate × energy_per_event / max_energy`, capped at
///   `trigger_rate_cap` (cooldown gating).
/// - PoB2's precise version is a "server-tick × frame-by-frame simulation" — deviations show up
///   in high-damage-variance scenarios (widely spread crit damage) or with non-uniform MonsterPower.
/// - Full Monte Carlo precision alignment is deferred (the golden fixture test framework is kept
///   in place for it).
///
/// Source: agent-docs/triggers.md §2, §"Implications for pobr" #2;
///       PoE2 Wiki CoC; PoE2DB Energy; PoB2 DeepWiki (the "needs an entire overhaul" warning).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EnergyTriggerRate {
    /// The meta gem's max energy.
    pub max_energy: f64,
    /// Energy generated per trigger event (mean value, used for the rate estimate).
    pub energy_per_event: f64,
    /// Total energy generated per second (`energy_per_event × source_rate`).
    pub energy_per_second: f64,
    /// Estimated raw energy-driven trigger frequency (triggers/second): `energy_per_second / max_energy`.
    pub raw_trigger_rate: f64,
    /// The effective trigger rate (triggers/second) after clamping to the cooldown cap.
    pub effective_trigger_rate: f64,
    /// The cooldown-driven trigger rate cap (triggers/second); used to clamp the energy rate.
    pub cooldown_rate_cap: f64,
    /// Whether the cooldown cap is the clamping factor (energy generation outpaces the cooldown).
    pub limited_by_cooldown: bool,
}

/// End-to-end trigger rate estimate for an energy-driven meta gem (deterministic approximation).
///
/// # Parameters
/// - `socketed_spells`: cast-time info for each socketed spell (determines max_energy).
/// - `condition`: the trigger condition (Crit/Ignite/Shock/Freeze/Other).
/// - `monster_power`: the enemy's power (per event); for a pack of monsters, pass the combined
///   power (15–20).
/// - `hit_damage`: the hit's raw damage (used by CoC; pass 0 for other conditions).
/// - `ailment_threshold`: the monster's ailment threshold (used by CoC; pass 1.0 for other conditions).
/// - `energy_generated_scale`: the gem-level bonus multiplier (1.0 + energy_generated_+%/100).
/// - `source_rate`: the source skill's (hit/attack/channel) events per second.
/// - `trigger_cd`: the trigger gem's own cooldown (seconds; used for the cooldown-gated cap).
/// - `triggered_cd`: the triggered skill's cooldown (seconds; pass 0 if none).
/// - `icdr`: the cooldown-recovery-rate multiplier (≥ 1.0).
///
/// Source: agent-docs/triggers.md §2; PoE2DB Energy; PoE2 Wiki CoC 0.5.0.
#[allow(clippy::too_many_arguments)]
pub fn calc_energy_trigger_rate(
    socketed_spells: &[SocketedSpellInfo],
    condition: TriggerCondition,
    monster_power: f64,
    hit_damage: f64,
    ailment_threshold: f64,
    energy_generated_scale: f64,
    source_rate: f64,
    trigger_cd: f64,
    triggered_cd: f64,
    icdr: f64,
    tick_seconds: f64,
) -> EnergyTriggerRate {
    let max_energy = calc_max_energy(socketed_spells);
    let energy_per_event = calc_energy_per_event(
        condition,
        monster_power,
        hit_damage,
        ailment_threshold,
        energy_generated_scale,
    );

    let source = source_rate.max(0.0);
    let energy_per_second = round(energy_per_event * source);
    let raw_rate = if max_energy > 0.0 {
        round(energy_per_second / max_energy)
    } else {
        0.0
    };

    // Cooldown-gated cap (can't exceed the cap determined by the cooldown).
    let cd = action_cooldown(trigger_cd, triggered_cd, icdr);
    let tick_rate = server_tick_rate(tick_seconds);
    let rate_cap_cd = round_cooldown_to_tick(cd, tick_rate);
    let cd_cap = if rate_cap_cd > 0.0 {
        round(1.0 / rate_cap_cd)
    } else {
        // No cooldown → gated only by the energy rate, no cap clamping.
        f64::INFINITY
    };

    let effective_rate = raw_rate.min(cd_cap);
    let limited_by_cooldown = cd_cap.is_finite() && raw_rate > cd_cap;

    EnergyTriggerRate {
        max_energy,
        energy_per_event,
        energy_per_second,
        raw_trigger_rate: raw_rate,
        effective_trigger_rate: round(effective_rate),
        cooldown_rate_cap: if cd_cap.is_finite() {
            round(cd_cap)
        } else {
            0.0
        },
        limited_by_cooldown,
    }
}

// §3  Multi-skill rotation

/// Parameters for a single skill in the rotation.
///
/// Each trigger opportunity fires the first skill (in rotation order) that has left cooldown;
/// if all skills are on cooldown, that opportunity is wasted.
/// Source: agent-docs/triggers.md §5; PoB2 CalcTriggers.lua `calcMultiSpellRotationImpact`.
#[derive(Debug, Clone, PartialEq)]
pub struct RotationSkill {
    /// The skill's effective cooldown (seconds; already includes the ICDR division and
    /// max(triggeredCD, triggerCD/icdr)). If the caller has already computed `action_cooldown()`,
    /// pass that result straight in.
    pub effective_cd: f64,
    /// The trigger chance per trigger opportunity (0.0–1.0; 1.0 = always triggers).
    /// Source: agent-docs/triggers.md §5 "Chance conversion — geometric distribution expectation".
    pub trigger_chance: f64,
    /// Extra cooldown added (SpellCastTimeAddedToCooldownIfTriggered, seconds; pass 0 if none).
    /// Source: agent-docs/triggers.md §4.3; PoB2 CalcTriggers.lua `addsCastTime`.
    pub added_cooldown: f64,
}

impl RotationSkill {
    pub fn new(effective_cd: f64) -> Self {
        Self {
            effective_cd,
            trigger_chance: 1.0,
            added_cooldown: 0.0,
        }
    }

    pub fn with_trigger_chance(mut self, chance: f64) -> Self {
        self.trigger_chance = chance.clamp(0.0, 1.0);
        self
    }

    pub fn with_added_cooldown(mut self, added_cd: f64) -> Self {
        self.added_cooldown = added_cd.max(0.0);
        self
    }

    /// The effective total cooldown (including the addition).
    pub fn total_cd(&self) -> f64 {
        (self.effective_cd + self.added_cooldown).max(0.0)
    }
}

/// Result of the multi-skill rotation simulation: each skill's steady-state trigger rate.
#[derive(Debug, Clone, PartialEq)]
pub struct RotationResult {
    /// Each skill's steady-state trigger rate (triggers/second) in the rotation. Order matches
    /// the input `skills`.
    pub rates: Vec<f64>,
    /// The fraction of trigger opportunities where "every skill was on cooldown, so the
    /// opportunity was wasted" (steady-state estimate, 0–1).
    pub wasted_fraction: f64,
}

/// Deterministic simulation of the multi-skill rotation: ports PoB2 `calcMultiSpellRotationImpact`.
///
/// Algorithm:
/// 1. Simulate `SIM_ROUNDS` (=1000) trigger opportunities, spaced `1 / source_rate` seconds apart.
/// 2. Each opportunity fires the first skill (in rotation order) whose cooldown has elapsed.
/// 3. Cooldowns count from the current frame, aligned to the server tick:
///    `next_trig = ceil_tick(floor_tick(now) + cd)` (`ceil_tick/floor_tick = ±round to frame`).
/// 4. When trigger chance < 100%, "on average it takes 1/chance opportunities to trigger" is
///    folded into the steady-state trigger rate:
///    `rate = triggers_in_sim / (SIM_TIME + expected_extra_wait)`.
///
/// Difference from PoB2's Monte Carlo: this implementation is a **deterministic** simulation —
/// when trigger_chance < 1 it substitutes the expected value for random sampling (no random
/// numbers), matching PoB2 exactly at chance=1.0 and approximating it in expectation for chance < 1.
///
/// Source: agent-docs/triggers.md §5; PoB2 CalcTriggers.lua L460-520 (calcMultiSpellRotationImpact).
///
/// # Parameters
/// - `skills`: the skills in the rotation (ordered list; rotation follows this order).
/// - `source_rate`: the source skill's trigger opportunities per second (e.g. attack speed / cast frequency).
///
/// # Returns
/// [`RotationResult`], each skill's steady-state trigger rate (triggers/second) and the waste fraction.
pub fn calc_multi_spell_rotation(
    skills: &[RotationSkill],
    source_rate: f64,
    tick_seconds: f64,
) -> RotationResult {
    if skills.is_empty() || source_rate <= 0.0 {
        return RotationResult {
            rates: Vec::new(),
            wasted_fraction: 0.0,
        };
    }

    let trigger_increment = 1.0 / source_rate; // Interval between trigger opportunities (seconds).
    const SIM_ROUNDS: usize = 1000;
    let sim_time = trigger_increment * SIM_ROUNDS as f64;

    let n = skills.len();
    let mut trigger_counts = vec![0u64; n];
    // next_available[i]: the time (seconds) at which skill i is next available to trigger;
    // starts at 0 (all immediately available).
    let mut next_available = vec![0.0f64; n];
    let mut wasted_count = 0u64;

    for round_idx in 0..SIM_ROUNDS {
        let now = trigger_increment * round_idx as f64;

        // Find the first skill available right now (in rotation order).
        let triggered_idx = next_available
            .iter()
            .enumerate()
            .take(n)
            .find(|(_, avail)| now >= **avail)
            .map(|(i, _)| i);

        match triggered_idx {
            None => {
                wasted_count += 1;
            }
            Some(i) => {
                let skill = &skills[i];
                trigger_counts[i] += 1;

                // Next available time: frame-aligned.
                // PoB2: `next_trig = ceil_b(floor_b(now, ServerTickTime) + cd, ServerTickTime)`
                let floor_now = (now / tick_seconds).floor() * tick_seconds;
                let cd = skill.total_cd().max(tick_seconds); // Minimum cooldown = 1 frame.
                let raw_next = floor_now + cd;
                let ceil_next = (raw_next / tick_seconds).ceil() * tick_seconds;
                next_available[i] = ceil_next;
            }
        }
    }

    // Fold "trigger chance < 1" into the rate (geometric distribution expectation).
    // PoB2's chance conversion: rate = count / (sim_time + (1/chance - 1) × triggerIncrement × count)
    // Simplification: if chance=1, rate = count/sim_time directly.
    let rates = skills
        .iter()
        .enumerate()
        .map(|(i, skill)| {
            let count = trigger_counts[i] as f64;
            if count == 0.0 {
                return 0.0;
            }
            let chance = skill.trigger_chance.max(1e-9);
            // Expected 1/chance opportunities per trigger; each opportunity is trigger_increment
            // seconds apart. Extra wait time = (1/chance - 1) × trigger_increment × count (sum
            // over all extra opportunities).
            let extra_wait = (1.0 / chance - 1.0) * trigger_increment * count;
            let effective_time = sim_time + extra_wait;
            round(count / effective_time)
        })
        .collect();

    let wasted_fraction = wasted_count as f64 / SIM_ROUNDS as f64;

    RotationResult {
        rates,
        wasted_fraction: round(wasted_fraction),
    }
}

// §4  CWC (Cast While Channelling)

/// Settlement result of the CWC (Cast While Channelling) trigger rate.
///
/// CWC's base cadence is set by the channelling interval `triggerTime` (rounded up to the
/// server tick), further clamped by the triggered skill's cooldown.
/// Source: agent-docs/triggers.md §4.2; PoB2 CalcTriggers.lua `CWCHandler`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CwcTriggerRate {
    /// The channelling trigger interval (seconds) after rounding to the server tick =
    /// `ceil(triggerTime × ServerTickRate)/ServerTickRate`.
    pub adjusted_trigger_interval: f64,
    /// The channelling trigger base frequency (triggers/second) = `1 / adjusted_trigger_interval`.
    pub channelling_trigger_rate: f64,
    /// The triggered skill's effective cooldown (seconds; includes
    /// SpellCastTimeAddedToCooldownIfTriggered, after ICDR division).
    pub effective_triggered_cd: f64,
    /// The final trigger rate cap (triggers/second) =
    /// `min(channelling_trigger_rate, 1/effective_triggered_cd)`.
    pub trigger_rate_cap: f64,
    /// Whether the triggered skill's cooldown is the limiting factor (triggered_cd > the
    /// channelling interval).
    pub limited_by_triggered_cd: bool,
}

/// Computes the CWC (Cast While Channelling) trigger rate.
///
/// CWC flow:
/// 1. `adjInterval = ceil(triggerTime × ServerTickRate) / ServerTickRate` (frame-aligned).
/// 2. `channelingRate = 1 / adjInterval` (channelling base frequency).
/// 3. `effTriggeredCD = max(triggered_cd, adds_cast_time) / icdr`.
///    - `adds_cast_time`: `SpellCastTimeAddedToCooldownIfTriggered` = the triggered spell's base
///      cast time / cast speed.
///    - Pass 0 if there is no such addition.
/// 4. `TriggerRateCap = min(channelingRate, 1/ceil(effTriggeredCD × rate)/rate)`.
///
/// Source: agent-docs/triggers.md §4.2; PoB2 `CalcTriggers.lua::CWCHandler`:
/// ```lua
/// adjTriggerInterval = ceil(triggerTime × ServerTickRate) / ServerTickRate
/// triggerRateOfTrigger = 1 / adjTriggerInterval
/// triggeredTotalCD = (cooldownOverride or max(triggeredCD, addsCastTime)) / icdr
/// TriggerRateCap = min(1/effCDTriggeredSkill, triggerRateOfTrigger)
/// ```
///
/// **`TriggeredDamage` injection**:
/// `TriggeredDamage INC/MORE` modifiers need to be injected into the triggered skill's `Damage`
/// factor by the integration layer (this function doesn't touch the ModDb). The integration
/// layer can read the `TriggeredDamageInc` / `TriggeredDamageMore` mods during perform and append
/// them to the skill's damage pipeline.
///
/// # Parameters
/// - `trigger_time`: the channelling base trigger interval (seconds; from `skillData.triggerTime`
///   or PoB2 gem data).
/// - `triggered_cd`: the triggered skill's base cooldown (seconds; pass 0 if none).
/// - `adds_cast_time`: the cooldown added by `SpellCastTimeAddedToCooldownIfTriggered` (seconds;
///   pass 0 if none).
/// - `icdr`: the cooldown-recovery-rate multiplier (≥ 1.0; 1.0 = no ICDR bonus).
pub fn calc_cwc_trigger_rate(
    trigger_time: f64,
    triggered_cd: f64,
    adds_cast_time: f64,
    icdr: f64,
    tick_seconds: f64,
) -> CwcTriggerRate {
    let tick_rate = server_tick_rate(tick_seconds);

    // Round the channelling interval to the server tick.
    let adj_interval = round_cooldown_to_tick(trigger_time.max(0.0), tick_rate);
    let channelling_rate = if adj_interval > 0.0 {
        round(1.0 / adj_interval)
    } else {
        0.0
    };

    // Triggered skill's effective cooldown: max(triggered_cd, adds_cast_time) / icdr.
    let icdr_eff = if icdr > 0.0 { icdr } else { 1.0 };
    let raw_triggered_cd = triggered_cd.max(adds_cast_time).max(0.0);
    let eff_triggered_cd = raw_triggered_cd / icdr_eff;

    // The rate cap gated by the triggered skill's cooldown.
    let cd_rate_cap = if eff_triggered_cd > 0.0 {
        round(1.0 / round_cooldown_to_tick(eff_triggered_cd, tick_rate))
    } else {
        // No cooldown: driven purely by the channelling frequency.
        channelling_rate
    };

    let final_cap = channelling_rate.min(cd_rate_cap);
    let limited_by_triggered_cd = eff_triggered_cd > 0.0 && cd_rate_cap < channelling_rate;

    CwcTriggerRate {
        adjusted_trigger_interval: adj_interval,
        channelling_trigger_rate: channelling_rate,
        effective_triggered_cd: round(eff_triggered_cd),
        trigger_rate_cap: round(final_cap),
        limited_by_triggered_cd,
    }
}

/// Computes the added cooldown (seconds) from `SpellCastTimeAddedToCooldownIfTriggered`.
///
/// Some triggers **add the triggered spell's cast time to the cooldown**, making slower-casting
/// spells trigger more slowly:
/// `adds_cast_time = base_cast_time / cast_speed_multiplier`.
///
/// Source: agent-docs/triggers.md §4.3; PoB2 `CalcTriggers.lua::processAddedCastTime`.
///
/// # Parameters
/// - `base_cast_time`: the triggered spell's base cast time (seconds).
/// - `cast_speed_multiplier`: the total cast-speed factor (> 0); = 1.0 + cast_speed_pct/100.
pub fn spell_cast_time_added_to_cooldown(base_cast_time: f64, cast_speed_multiplier: f64) -> f64 {
    if base_cast_time <= 0.0 || cast_speed_multiplier <= 0.0 {
        return 0.0;
    }
    round(base_cast_time / cast_speed_multiplier)
}

// §5  TraceGraph attribution extensions (breaking trigger rate down to sources)

use crate::{TraceGraph, TraceNodeId, TraceOperation};
use pobr_data::prelude::{SourceId, SourceKind};

/// The attributed version of the cooldown-gated trigger rate: adds trigger_cd, triggered_cd,
/// icdr, source_rate each to the TraceGraph.
///
/// Returns `(TriggerRate, skill_trigger_rate_node)`. Callers can chain downstream nodes (e.g. DPS)
/// off this node.
/// Source: agent-docs/triggers.md §"Implications for pobr" #5 (attributing trigger rate to a SourceId).
pub fn resolve_trigger_rate_traced(
    trigger_cd: f64,
    triggered_cd: f64,
    icdr: f64,
    effective_source_rate: f64,
    tick_seconds: f64,
    trace: &mut TraceGraph,
) -> (TriggerRate, TraceNodeId) {
    let result = resolve_trigger_rate(
        trigger_cd,
        triggered_cd,
        icdr,
        effective_source_rate,
        tick_seconds,
    );

    let trigger_cd_node = trace.add_source_node(
        "trigger cooldown (gem)",
        trigger_cd,
        SourceId::new(SourceKind::SkillGem, "trigger.cooldown"),
    );
    let triggered_cd_node = trace.add_source_node(
        "triggered skill cooldown",
        triggered_cd,
        SourceId::new(SourceKind::SkillGem, "triggered.cooldown"),
    );
    let icdr_node = trace.add_source_node(
        "ICDR (cooldown recovery rate)",
        icdr,
        SourceId::new(SourceKind::CharacterBase, "icdr"),
    );
    let source_rate_node = trace.add_source_node(
        "effective source rate (attacks/s)",
        effective_source_rate,
        SourceId::new(SourceKind::CharacterBase, "source.rate"),
    );

    let action_cd_node = trace.add_node(
        "action cooldown = max(triggeredCD, triggerCD/icdr)",
        result.action_cooldown,
        TraceOperation::SelectMax,
    );
    trace.add_edge(trigger_cd_node, action_cd_node);
    trace.add_edge(triggered_cd_node, action_cd_node);
    trace.add_edge(icdr_node, action_cd_node);

    let cap_node = trace.add_node(
        "trigger rate cap (frame-aligned)",
        result.trigger_rate_cap,
        TraceOperation::Cap,
    );
    trace.add_edge(action_cd_node, cap_node);

    let rate_node = trace.add_node(
        "skill trigger rate = min(cap, sourceRate)",
        result.skill_trigger_rate,
        TraceOperation::SelectMax,
    );
    trace.add_edge(cap_node, rate_node);
    trace.add_edge(source_rate_node, rate_node);

    (result, rate_node)
}

/// The attributed version of the energy-driven trigger rate: adds max_energy, energy_per_event,
/// source_rate to the TraceGraph.
///
/// Returns `(EnergyTriggerRate, effective_trigger_rate_node)`.
#[allow(clippy::too_many_arguments)]
pub fn calc_energy_trigger_rate_traced(
    socketed_spells: &[SocketedSpellInfo],
    condition: TriggerCondition,
    monster_power: f64,
    hit_damage: f64,
    ailment_threshold: f64,
    energy_generated_scale: f64,
    source_rate: f64,
    trigger_cd: f64,
    triggered_cd: f64,
    icdr: f64,
    tick_seconds: f64,
    trace: &mut TraceGraph,
) -> (EnergyTriggerRate, TraceNodeId) {
    let result = calc_energy_trigger_rate(
        socketed_spells,
        condition,
        monster_power,
        hit_damage,
        ailment_threshold,
        energy_generated_scale,
        source_rate,
        trigger_cd,
        triggered_cd,
        icdr,
        tick_seconds,
    );

    let max_energy_node = trace.add_source_node(
        "max energy (socketed spell cast times)",
        result.max_energy,
        SourceId::new(SourceKind::SkillGem, "energy.max"),
    );
    let energy_per_event_node = trace.add_source_node(
        "energy per event (MonsterPower × baseCentienergy / 100)",
        result.energy_per_event,
        SourceId::new(SourceKind::CharacterBase, "energy.per_event"),
    );
    let source_rate_node = trace.add_source_node(
        "source rate (events/s)",
        source_rate,
        SourceId::new(SourceKind::CharacterBase, "source.rate"),
    );
    let raw_rate_node = trace.add_node(
        "raw energy trigger rate (energy_per_second / max_energy)",
        result.raw_trigger_rate,
        TraceOperation::Multiply,
    );
    trace.add_edge(max_energy_node, raw_rate_node);
    trace.add_edge(energy_per_event_node, raw_rate_node);
    trace.add_edge(source_rate_node, raw_rate_node);

    let effective_node = trace.add_node(
        "effective trigger rate (min(raw, cd_cap))",
        result.effective_trigger_rate,
        TraceOperation::Cap,
    );
    trace.add_edge(raw_rate_node, effective_node);

    (result, effective_node)
}

/// The attributed version of the CWC trigger rate: adds trigger_time, triggered_cd,
/// adds_cast_time, icdr to the TraceGraph.
///
/// Returns `(CwcTriggerRate, trigger_rate_cap_node)`.
pub fn calc_cwc_trigger_rate_traced(
    trigger_time: f64,
    triggered_cd: f64,
    adds_cast_time: f64,
    icdr: f64,
    tick_seconds: f64,
    trace: &mut TraceGraph,
) -> (CwcTriggerRate, TraceNodeId) {
    let result = calc_cwc_trigger_rate(
        trigger_time,
        triggered_cd,
        adds_cast_time,
        icdr,
        tick_seconds,
    );

    let trigger_time_node = trace.add_source_node(
        "CWC triggerTime (channelling interval)",
        trigger_time,
        SourceId::new(SourceKind::SkillGem, "cwc.triggerTime"),
    );
    let triggered_cd_node = trace.add_source_node(
        "triggered skill cooldown",
        triggered_cd,
        SourceId::new(SourceKind::SkillGem, "triggered.cooldown"),
    );
    let adds_cast_time_node = trace.add_source_node(
        "SpellCastTimeAddedToCooldownIfTriggered",
        adds_cast_time,
        SourceId::new(SourceKind::SkillGem, "triggered.addsCastTime"),
    );
    let icdr_node = trace.add_source_node(
        "ICDR",
        icdr,
        SourceId::new(SourceKind::CharacterBase, "icdr"),
    );

    let interval_node = trace.add_node(
        "adjusted trigger interval (frame-aligned)",
        result.adjusted_trigger_interval,
        TraceOperation::Cap,
    );
    trace.add_edge(trigger_time_node, interval_node);

    let eff_cd_node = trace.add_node(
        "effective triggered CD = max(triggered_cd, adds_cast_time) / icdr",
        result.effective_triggered_cd,
        TraceOperation::SelectMax,
    );
    trace.add_edge(triggered_cd_node, eff_cd_node);
    trace.add_edge(adds_cast_time_node, eff_cd_node);
    trace.add_edge(icdr_node, eff_cd_node);

    let rate_cap_node = trace.add_node(
        "CWC trigger rate cap",
        result.trigger_rate_cap,
        TraceOperation::SelectMax,
    );
    trace.add_edge(interval_node, rate_cap_node);
    trace.add_edge(eff_cd_node, rate_cap_node);

    (result, rate_cap_node)
}

// §6  Trigger source stats

/// The complete sub-calculation stats of the **source skill** that does the triggering.
///
/// Built by the build layer (orchestrator) after running one full sub-calculation for the source
/// skill (a minimal equivalent of PoB2's GlobalCache, `CalcTriggers.lua:74-86`
/// `cachedData[uuid].HitSpeed or Speed`), injected via the BASE mod channel
/// (`TriggerSourceRate` / `TriggerSourceHitChance` / `TriggerSourceCritChance`) for consumption
/// by perform's `fill_trigger`:
/// - `action_rate`: the source skill's **post-calculation** effective action rate (includes
///   attack-speed inc/more factors, fixing the 14-G2 correctness-level bug where "a CoC build
///   stacking attack speed didn't see its source rate grow with attack speed");
/// - `hit_chance` / `crit_chance`: the source skill's hit rate / crit rate (fraction 0-1), folded
///   into the trigger chance (`CalcTriggers.lua:716-770`
///   `triggerChance ×= sourceHitChance × sourceCritChance`; crit only applies on the
///   triggerOnCrit path).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct TriggerSourceStats {
    /// The source skill's effective action rate (triggers/second; post-calculation, =
    /// OutputTable.effective_action_rate).
    pub action_rate: f64,
    /// The source skill's hit rate (fraction 0-1; 0 = not injected/not applicable, consumer skips
    /// folding it in).
    pub hit_chance: f64,
    /// The source skill's crit rate (fraction 0-1; 0 = not injected/not applicable, consumer
    /// skips folding it in).
    pub crit_chance: f64,
}

impl TriggerSourceStats {
    /// The trigger-chance conversion multiplier (fraction 0-1): `hit × (triggerOnCrit ? crit : 1)`.
    ///
    /// Matches PoB2 `defaultTriggerHandler` (CalcTriggers.lua:716-770):
    /// - When the source hit rate ≠ 100%, multiply by `sourceHitChance` (:721-742; on the
    ///   triggerOnUse path, the injecting side doesn't inject a hit_chance value, so 0 = skip here);
    /// - When `triggerOnCrit`, also multiply by `sourceCritChance` (:744-769).
    ///
    /// Dual-wield's independently-rolled effective hit/crit (:725-731
    /// doubleHitsWhenDualWielding) is deferred to when per-hand output is wired in; currently
    /// the overall value is used.
    pub fn chance_multiplier(&self, trigger_on_crit: bool) -> f64 {
        let mut chance = 1.0_f64;
        if self.hit_chance > 0.0 && self.hit_chance < 1.0 {
            chance *= self.hit_chance.clamp(0.0, 1.0);
        }
        if trigger_on_crit && self.crit_chance > 0.0 && self.crit_chance < 1.0 {
            chance *= self.crit_chance.clamp(0.0, 1.0);
        }
        chance.clamp(0.0, 1.0)
    }
}

// §1  Internal unit tests (cooldown-gated basics)

#[cfg(test)]
mod tests {
    use super::*;
    use pobr_data::prelude::SERVER_TICK_SECONDS;

    #[test]
    fn server_tick_rate_matches_constant() {
        // 1 / 0.033 ≈ 30.30/s.
        assert!((server_tick_rate(SERVER_TICK_SECONDS) - 30.303_030_303).abs() < 1e-6);
    }

    #[test]
    fn cooldown_rounds_up_to_frame() {
        // At 30.3/s: a 0.10s cooldown → ceil(0.10 × 30.303) = ceil(3.03) = 4 frames → 4/30.303 ≈ 0.132s.
        let rate = server_tick_rate(SERVER_TICK_SECONDS);
        let rounded = round_cooldown_to_tick(0.10, rate);
        assert!((rounded - 4.0 / rate).abs() < 1e-9);
        assert!(rounded > 0.10); // Rounding lengthens the cooldown.
    }

    #[test]
    fn cap_is_inverse_of_rounded_cooldown() {
        let rate = server_tick_rate(SERVER_TICK_SECONDS);
        let cd = 0.15;
        let cap = trigger_rate_cap(cd, rate);
        let rounded = round_cooldown_to_tick(cd, rate);
        assert!((cap - 1.0 / rounded).abs() < 1e-6);
    }

    #[test]
    fn icdr_shortens_trigger_cooldown() {
        // trigger_cd=0.3, icdr=1.5 → 0.2; triggered skill has no cooldown → action_cd=0.2.
        let cd = action_cooldown(0.3, 0.0, 1.5);
        assert!((cd - 0.2).abs() < 1e-9);
    }

    #[test]
    fn larger_of_two_cooldowns_wins() {
        // triggered_cd=0.5 is larger than trigger_cd/icdr=0.3 → action_cd=0.5.
        let cd = action_cooldown(0.3, 0.5, 1.0);
        assert!((cd - 0.5).abs() < 1e-9);
    }

    #[test]
    fn source_rate_gates_trigger_rate() {
        // The cap is far above the 2/s source rate → the actual rate is gated to 2/s by the source.
        let r = resolve_trigger_rate(0.05, 0.0, 1.0, 2.0, SERVER_TICK_SECONDS);
        assert!(r.limited_by_source);
        assert!((r.skill_trigger_rate - 2.0).abs() < 1e-6);
    }

    #[test]
    fn cap_gates_when_source_is_fast() {
        // Source rate of 100/s exceeds the cap → the actual rate = the cap.
        let r = resolve_trigger_rate(0.3, 0.0, 1.0, 100.0, SERVER_TICK_SECONDS);
        assert!(!r.limited_by_source);
        assert!((r.skill_trigger_rate - r.trigger_rate_cap).abs() < 1e-9);
    }

    // §2  Energy model tests

    #[test]
    fn max_energy_single_spell_0_5s() {
        // base_cast_time = 0.5s → effective = 0.5s → (0.5/0.1)×10 = 50.
        let spells = [SocketedSpellInfo::new(0.5)];
        assert!((calc_max_energy(&spells) - 50.0).abs() < 1e-6);
    }

    #[test]
    fn max_energy_two_spells() {
        // 0.3s + 0.6s = 0.9s → 90 energy.
        let spells = [SocketedSpellInfo::new(0.3), SocketedSpellInfo::new(0.6)];
        assert!((calc_max_energy(&spells) - 90.0).abs() < 1e-6);
    }

    #[test]
    fn max_energy_use_time_penalty_doubled() {
        // base=0.5s, use_time_increase=20% → effective = 0.5 × (1 + 0.20 × 2) = 0.5 × 1.4 = 0.7s.
        // max_energy = (0.7/0.1)×10 = 70.
        let spell = SocketedSpellInfo::new(0.5).with_use_time_increase(20.0);
        assert!((spell.effective_cast_time_for_energy() - 0.7).abs() < 1e-9);
        let spells = [spell];
        assert!((calc_max_energy(&spells) - 70.0).abs() < 1e-6);
    }

    #[test]
    fn energy_per_event_freeze_10x_crit_same_ratio() {
        // Freeze base_centienergy=1000 vs Crit base_centienergy=100 (at ratio=1) → 10x.
        // At CoC ratio=1 (hit_damage=ailment_threshold): crit = 1×100×1/100 = 1.
        // Freeze: freeze = 1×1000/100 = 10.
        let crit_ratio1 =
            calc_energy_per_event(TriggerCondition::CriticalStrike, 1.0, 100.0, 100.0, 1.0);
        let freeze = calc_energy_per_event(TriggerCondition::Freeze, 1.0, 0.0, 1.0, 1.0);
        assert!(
            (crit_ratio1 - 1.0).abs() < 1e-6,
            "crit_ratio1={crit_ratio1}"
        );
        assert!((freeze - 10.0).abs() < 1e-6, "freeze={freeze}");
        assert!(
            (freeze / crit_ratio1 - 10.0).abs() < 1e-3,
            "freeze={freeze} crit={crit_ratio1}"
        );
    }

    #[test]
    fn energy_per_event_coc_damage_ratio() {
        // CoC: MonsterPower=1, hit_damage=500, threshold=100 → ratio=5 → energy=1×100×5/100=5.
        let e = calc_energy_per_event(TriggerCondition::CriticalStrike, 1.0, 500.0, 100.0, 1.0);
        assert!((e - 5.0).abs() < 1e-6);
    }

    #[test]
    fn energy_per_event_scale_increases_gain() {
        // energy_generated_scale=1.57 (lvl20 +57%) should generate 57% more energy than 1.0.
        let base = calc_energy_per_event(TriggerCondition::Shock, 2.0, 0.0, 1.0, 1.0);
        let scaled = calc_energy_per_event(TriggerCondition::Shock, 2.0, 0.0, 1.0, 1.57);
        assert!((scaled / base - 1.57).abs() < 1e-3);
    }

    #[test]
    fn energy_trigger_rate_increases_with_source_rate() {
        // effective_trigger_rate is monotonically non-decreasing as source_rate increases
        // (clamped by the cooldown cap).
        let spells = [SocketedSpellInfo::new(0.5)];
        let low = calc_energy_trigger_rate(
            &spells,
            TriggerCondition::Shock,
            5.0,
            0.0,
            1.0,
            1.0,
            2.0,
            0.3,
            0.0,
            1.0,
            SERVER_TICK_SECONDS,
        );
        let high = calc_energy_trigger_rate(
            &spells,
            TriggerCondition::Shock,
            5.0,
            0.0,
            1.0,
            1.0,
            5.0,
            0.3,
            0.0,
            1.0,
            SERVER_TICK_SECONDS,
        );
        assert!(high.effective_trigger_rate >= low.effective_trigger_rate);
    }

    #[test]
    fn energy_trigger_rate_limited_by_cooldown() {
        // Very high source_rate and ample energy → should be clamped by the cooldown cap.
        let spells = [SocketedSpellInfo::new(0.1)]; // max_energy=10 (small → fast generation)
        let r = calc_energy_trigger_rate(
            &spells,
            TriggerCondition::Freeze,
            20.0, // High MonsterPower
            0.0,
            1.0,
            1.0,
            100.0, // Very high source rate
            0.5,   // Trigger gem cooldown 0.5s → cap ≈ 2/s
            0.0,
            1.0,
            SERVER_TICK_SECONDS,
        );
        assert!(r.limited_by_cooldown, "should be limited by cooldown cap");
        // effective ≤ cd_cap.
        assert!(r.effective_trigger_rate <= r.cooldown_rate_cap + 1e-6);
    }

    #[test]
    fn energy_trigger_rate_no_spells_yields_zero() {
        let r = calc_energy_trigger_rate(
            &[],
            TriggerCondition::Shock,
            5.0,
            0.0,
            1.0,
            1.0,
            3.0,
            0.3,
            0.0,
            1.0,
            SERVER_TICK_SECONDS,
        );
        assert_eq!(r.max_energy, 0.0);
        assert_eq!(r.effective_trigger_rate, 0.0);
    }

    // §3  Multi-skill rotation tests

    #[test]
    fn single_skill_rotation_no_waste() {
        // Single-skill rotation: every trigger opportunity fires this skill (no waste), rate ≈ source_rate (bounded by the cooldown cap).
        let skill = RotationSkill::new(0.15); // 0.15s cooldown.
        let source_rate = 4.0; // 4/s source rate.
        let result = calc_multi_spell_rotation(&[skill], source_rate, SERVER_TICK_SECONDS);
        assert_eq!(result.rates.len(), 1);
        // Rate upper bound = source_rate (0.15s cooldown << 0.25s trigger interval, not a bottleneck).
        assert!(result.rates[0] > 0.0);
        assert_eq!(result.wasted_fraction, 0.0); // No waste with a single skill.
    }

    #[test]
    fn two_skills_share_trigger_opportunities() {
        // Two skills, source rate 4/s, each skill's cooldown longer than the trigger interval →
        // they share trigger opportunities, each skill's rate < the source rate.
        let skill_a = RotationSkill::new(0.5);
        let skill_b = RotationSkill::new(0.5);
        let source_rate = 4.0;
        let result =
            calc_multi_spell_rotation(&[skill_a, skill_b], source_rate, SERVER_TICK_SECONDS);
        assert_eq!(result.rates.len(), 2);
        let total: f64 = result.rates.iter().sum::<f64>();
        // Total trigger rate ≤ the source rate (there may be waste).
        assert!(total <= source_rate + 1e-6);
        // Both skills have a non-zero rate.
        assert!(result.rates[0] > 0.0);
        assert!(result.rates[1] > 0.0);
    }

    #[test]
    fn rotation_with_long_cooldowns_causes_waste() {
        // All skills have extremely long cooldowns (10s) with a high trigger frequency (10/s) →
        // most trigger opportunities are wasted.
        let skills: Vec<RotationSkill> = (0..3).map(|_| RotationSkill::new(10.0)).collect();
        let source_rate = 10.0;
        let result = calc_multi_spell_rotation(&skills, source_rate, SERVER_TICK_SECONDS);
        // With such long cooldowns, most trigger opportunities go to waste.
        assert!(
            result.wasted_fraction > 0.5,
            "wasted={}",
            result.wasted_fraction
        );
    }

    #[test]
    fn rotation_trigger_chance_reduces_rate() {
        // A skill with a 50% trigger chance has a steady-state rate roughly half that of chance=1
        // (geometric distribution expectation approximation).
        let full_chance = RotationSkill::new(0.3).with_trigger_chance(1.0);
        let half_chance = RotationSkill::new(0.3).with_trigger_chance(0.5);
        let source_rate = 3.0;
        let r_full = calc_multi_spell_rotation(&[full_chance], source_rate, SERVER_TICK_SECONDS);
        let r_half = calc_multi_spell_rotation(&[half_chance], source_rate, SERVER_TICK_SECONDS);
        // The 50%-chance rate should be significantly lower than the 100%-chance rate.
        assert!(r_half.rates[0] < r_full.rates[0]);
    }

    #[test]
    fn empty_rotation_returns_empty() {
        let result = calc_multi_spell_rotation(&[], 5.0, SERVER_TICK_SECONDS);
        assert!(result.rates.is_empty());
    }

    #[test]
    fn rotation_zero_source_rate_returns_zeros() {
        let skill = RotationSkill::new(0.3);
        let result = calc_multi_spell_rotation(&[skill], 0.0, SERVER_TICK_SECONDS);
        assert!(result.rates.is_empty() || result.rates.iter().all(|&r| r == 0.0));
    }

    #[test]
    fn added_cooldown_slows_rotation() {
        // Compared to no added_cooldown, having one lowers the trigger rate.
        let no_add = RotationSkill::new(0.3).with_added_cooldown(0.0);
        let with_add = RotationSkill::new(0.3).with_added_cooldown(0.5);
        let source_rate = 5.0;
        let r_no = calc_multi_spell_rotation(&[no_add], source_rate, SERVER_TICK_SECONDS);
        let r_with = calc_multi_spell_rotation(&[with_add], source_rate, SERVER_TICK_SECONDS);
        assert!(r_with.rates[0] <= r_no.rates[0] + 1e-9);
    }

    // §4  CWC tests

    #[test]
    fn cwc_basic_trigger_rate() {
        // triggerTime=0.3s → ceil(0.3 × 30.303) = 10 frames → 10/30.303 ≈ 0.33s → rate ≈ 3.03/s.
        let r = calc_cwc_trigger_rate(0.3, 0.0, 0.0, 1.0, SERVER_TICK_SECONDS);
        let tick_rate = server_tick_rate(SERVER_TICK_SECONDS);
        let expected_interval = round_cooldown_to_tick(0.3, tick_rate);
        assert!((r.adjusted_trigger_interval - expected_interval).abs() < 1e-9);
        assert!((r.channelling_trigger_rate - 1.0 / expected_interval).abs() < 1e-6);
        assert!(!r.limited_by_triggered_cd);
    }

    #[test]
    fn cwc_triggered_cd_limits_rate() {
        // triggered_cd=1.0s >> triggerTime=0.1s → the triggered skill's cooldown becomes the bottleneck.
        let r = calc_cwc_trigger_rate(0.1, 1.0, 0.0, 1.0, SERVER_TICK_SECONDS);
        assert!(r.limited_by_triggered_cd, "triggered CD should limit rate");
        assert!(r.trigger_rate_cap < r.channelling_trigger_rate);
    }

    #[test]
    fn cwc_icdr_increases_trigger_rate() {
        // ICDR=2.0 shortens a 0.6s triggered_cd to 0.3s → the rate cap goes up.
        let r_no_icdr = calc_cwc_trigger_rate(0.2, 0.6, 0.0, 1.0, SERVER_TICK_SECONDS);
        let r_icdr = calc_cwc_trigger_rate(0.2, 0.6, 0.0, 2.0, SERVER_TICK_SECONDS);
        assert!(r_icdr.trigger_rate_cap >= r_no_icdr.trigger_rate_cap);
    }

    #[test]
    fn cwc_adds_cast_time_increases_effective_cd() {
        // adds_cast_time=0.5s added to the cooldown → effective_triggered_cd is larger than triggered_cd=0.2s.
        let r = calc_cwc_trigger_rate(0.2, 0.2, 0.5, 1.0, SERVER_TICK_SECONDS);
        // max(0.2, 0.5) = 0.5s → effective_triggered_cd = 0.5.
        assert!((r.effective_triggered_cd - 0.5).abs() < 1e-6);
        assert!(r.limited_by_triggered_cd);
    }

    #[test]
    fn spell_cast_time_to_cooldown_basic() {
        // base=0.5s, cast_speed=1.5 → 0.5/1.5 ≈ 0.333s.
        let added = spell_cast_time_added_to_cooldown(0.5, 1.5);
        assert!((added - 0.5 / 1.5).abs() < 1e-6);
    }

    #[test]
    fn spell_cast_time_to_cooldown_no_speed_bonus() {
        // No cast-speed bonus (multiplier=1.0) → added cooldown = the base cast time.
        let added = spell_cast_time_added_to_cooldown(0.8, 1.0);
        assert!((added - 0.8).abs() < 1e-9);
    }

    // §5  Attribution tests

    #[test]
    fn trace_graph_nodes_created_for_trigger_rate() {
        let mut trace = TraceGraph::new();
        let (result, rate_node) =
            resolve_trigger_rate_traced(0.3, 0.0, 1.0, 5.0, SERVER_TICK_SECONDS, &mut trace);
        assert!(trace.nodes().len() >= 5); // At least 5 nodes.
        let node = trace.node(rate_node).unwrap();
        assert!((node.value - result.skill_trigger_rate).abs() < 1e-9);
        // rate_node should have incoming edges from cap_node and source_rate_node.
        let incoming = trace.incoming(rate_node);
        assert!(incoming.len() >= 2);
    }

    #[test]
    fn trace_graph_energy_trigger_rate() {
        let spells = [SocketedSpellInfo::new(0.5)];
        let mut trace = TraceGraph::new();
        let (result, node) = calc_energy_trigger_rate_traced(
            &spells,
            TriggerCondition::Shock,
            5.0,
            0.0,
            1.0,
            1.0,
            3.0,
            0.3,
            0.0,
            1.0,
            SERVER_TICK_SECONDS,
            &mut trace,
        );
        let n = trace.node(node).unwrap();
        assert!((n.value - result.effective_trigger_rate).abs() < 1e-9);
    }

    #[test]
    fn trace_graph_cwc_trigger_rate() {
        let mut trace = TraceGraph::new();
        let (result, node) =
            calc_cwc_trigger_rate_traced(0.3, 0.5, 0.0, 1.0, SERVER_TICK_SECONDS, &mut trace);
        let n = trace.node(node).unwrap();
        assert!((n.value - result.trigger_rate_cap).abs() < 1e-9);
        assert!(trace.nodes().len() >= 4);
    }

    // §6  Trigger source stats tests

    /// Manual check: triggerChance = source hit × source crit
    /// (CoC: hit 80% × crit 35% = 28%).
    #[test]
    fn source_stats_chance_folds_hit_and_crit() {
        let stats = TriggerSourceStats {
            action_rate: 3.0,
            hit_chance: 0.80,
            crit_chance: 0.35,
        };
        assert!((stats.chance_multiplier(true) - 0.28).abs() < 1e-9);
        // A non-triggerOnCrit path only folds in the hit chance.
        assert!((stats.chance_multiplier(false) - 0.80).abs() < 1e-9);
    }

    /// Unset (0-valued) fields skip the fold-in; 100% hit/crit doesn't slow the rate.
    #[test]
    fn source_stats_chance_skips_unset_and_full() {
        let unset = TriggerSourceStats::default();
        assert_eq!(unset.chance_multiplier(true), 1.0);
        let full = TriggerSourceStats {
            action_rate: 2.0,
            hit_chance: 1.0,
            crit_chance: 1.0,
        };
        assert_eq!(full.chance_multiplier(true), 1.0);
    }

    /// Directionality for CoC: higher trigger chance → higher trigger rate (in the
    /// source-rate-gated regime).
    #[test]
    fn coc_directional_higher_crit_higher_rate() {
        let low_crit = TriggerSourceStats {
            action_rate: 2.0,
            hit_chance: 0.9,
            crit_chance: 0.2,
        };
        let high_crit = TriggerSourceStats {
            crit_chance: 0.5,
            ..low_crit
        };
        // Multiply by chance after double-gating (same formula as perform): when the cap is far
        // above the source rate, rate ∝ chance.
        let r = resolve_trigger_rate(0.05, 0.0, 1.0, 2.0, SERVER_TICK_SECONDS);
        let low = r.skill_trigger_rate * low_crit.chance_multiplier(true);
        let high = r.skill_trigger_rate * high_crit.chance_multiplier(true);
        assert!(high > low, "crit↑ 应使触发速率↑（{high} vs {low}）");
    }
}
