//! Survivability helper calculations (reservation / regen / capped chance / charges / leech / recoup).
//!
//! References: `agent-docs/active-defences.md`, `agent-docs/block.md`,
//! `agent-docs/recovery-charges-buffs.md` (PoE2 0.5.0).
//!
//! - **Reservation**: auras / guards reserve `Σ flat + pool * (Σ % / 100)`, clamped to [0, pool].
//! - **Regen**: `base_flat + pool * (Σ %regen / 100)`, then the recovery rate factor (inc/more, including RecoveryRateMod).
//! - **Capped chance**: chance-based stats (block) are summed then clamped to [0, cap].
//! - **Charges**: charge count/ceiling resolution; PoE2 charges have no
//!   inherent stats, they only serve as a reference for per-charge mod multipliers.
//! - **Leech**: reworked in 0.5.0 -- one instance per resource (takes the
//!   highest rate), three ceilings, physical only by default.
//! - **Recoup**: a fraction of damage taken is returned over 8s (or 4s).

use crate::{CalcConfig, ModDb};
use pobr_data::prelude::*;

use super::round;

// Reservation

/// Reservation result: amount reserved + remaining available.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Reservation {
    pub reserved: f64,
    pub unreserved: f64,
}

/// Calculates the reservation amount for a pool (life / mana).
///
/// `flat` is the sum of flat reservations, `percent` is the sum of
/// percentage reservations (e.g. 50 means 50%). The result is clamped to
/// `[0, pool]`, `unreserved = pool - reserved`.
/// A convenience wrapper with neutral multiplier/efficiency (see [`reservation_with_efficiency`]).
pub fn reservation(pool: f64, flat: f64, percent: f64) -> Reservation {
    reservation_with_efficiency(pool, flat, percent, 1.0, 0.0, 1.0)
}

/// Reservation calculation including `ReservationMultiplier` and
/// Reservation Efficiency (13-G11; PoB2 CalcDefence.lua:172-350's
/// `doActorLifeManaSpiritReservation`).
///
/// Vendor's per-skill formula (`:249-258`):
/// `reservedFlat = max(round(baseFlat × mult × (100+inc)/100 × more
/// / (1 + efficiency/100) / efficiencyMore), 0)`, where
/// - `mult = floor(More(ReservationMultiplier), 4)` (`:197`, rounded down to 4 decimal places);
/// - `efficiency = max(Σinc(<X>ReservationEfficiency, ReservationEfficiency), −100)`
///   (`:240`, **division** semantics -- higher efficiency → less reservation);
/// - `efficiencyMore = More(the same name set)` (`:241`, also a divisor).
///
/// PoBR's aggregate view (when per-skill granularity isn't available):
/// `raw = (flat + pool×pct/100) × mult ÷ (1+eff_inc/100) ÷ eff_more` -- the
/// multiply/divide factors apply to the summed reservation amount as a
/// whole, equivalent to vendor's "apply per skill then sum" when the
/// factors are globally consistent. `eff_inc` is floored at −100 within
/// this function (vendor `:240`); when the divisor ≤ 0 (efficiency = −100),
/// the reservation is treated as infinite → clamped to the full pool.
pub fn reservation_with_efficiency(
    pool: f64,
    flat: f64,
    percent: f64,
    reservation_mult_more: f64,
    efficiency_inc: f64,
    efficiency_more: f64,
) -> Reservation {
    if pool <= 0.0 {
        return Reservation {
            reserved: 0.0,
            unreserved: 0.0,
        };
    }
    // vendor :197 `floor(more, 4)` (rounded down to 4 decimal places).
    let mult = (reservation_mult_more * 10_000.0).floor() / 10_000.0;
    let divisor = (1.0 + efficiency_inc.max(-100.0) / 100.0) * efficiency_more;
    let base_raw = (flat + pool * (percent / 100.0)) * mult;
    let raw = if divisor > 0.0 {
        base_raw / divisor
    } else if base_raw > 0.0 {
        // efficiency −100%: the divisor hits zero → reservation diverges, clamped to the full pool.
        pool
    } else {
        0.0
    };
    let reserved = round(raw.clamp(0.0, pool));
    Reservation {
        reserved,
        unreserved: round(pool - reserved),
    }
}

// Regeneration

/// Calculates per-second regen.
///
/// `base_flat` is the sum of flat per-second recovery, `percent` is the sum
/// of pool-percentage recovery, `inc` / `more` are the recovery rate bonus
/// (% addition + more product).
pub fn regen(pool: f64, base_flat: f64, percent: f64, inc: f64, more: f64) -> f64 {
    let base = base_flat + pool * (percent / 100.0);
    round(base * (1.0 + inc / 100.0) * more)
}

/// Calculates per-second regen, including the `RecoveryRateMod` global recovery rate multiplier.
///
/// PoB2 `CalcDefence.lua`: `regen × RecoveryRateMod` (each of the three
/// resources has its own `XRecoveryRate`, whose inc/more are already folded
/// into `regen`'s inc/more parameters; this additionally multiplies by the
/// externally-passed `recovery_rate_mod`, corresponding to
/// `output.LifeRecoveryRateMod` / `ManaRecoveryRateMod` / `EnergyShieldRecoveryRateMod`).
///
/// Source: agent-docs/recovery-charges-buffs.md §2.1;
///       PoB2 `src/Modules/CalcDefence.lua`'s regen section.
pub fn regen_with_rate(
    pool: f64,
    base_flat: f64,
    percent: f64,
    inc: f64,
    more: f64,
    recovery_rate_mod: f64,
) -> f64 {
    let base_regen = regen(pool, base_flat, percent, inc, more);
    round(base_regen * recovery_rate_mod.max(0.0))
}

/// Queries and calculates a resource's per-second regen rate from the ModDb.
///
/// `stat` takes values: `"LifeRegen"` / `"ManaRegen"` / `"EnergyShieldRegen"`.
/// inc/more consult both `<stat>Rate` (the dedicated rate) and
/// `XRecoveryRate` (the global recovery rate), matching PoB2
/// `CalcDefence.lua`'s "merge `XRegen` + `XRecoveryRate`" behavior.
///
/// Source: agent-docs/recovery-charges-buffs.md §2.1;
///       PoB2 `src/Modules/CalcDefence.lua`'s regen section comments.
pub fn calc_regen(db: &ModDb, cfg: &CalcConfig, pool: f64, stat: &str) -> f64 {
    let bare_name = ModName::from(stat);
    let flat = db.sum(ModType::Base, cfg, std::slice::from_ref(&bare_name));
    let percent = db.sum(
        ModType::Base,
        cfg,
        &[ModName::from(format!("{stat}Percent"))],
    );
    // inc/more: the bare `<stat>` name (vendor CalcDefence.lua:1642-1643's
    // `Sum("INC", nil, resource.."Regen", resource.."RecoveryRate")` --
    // mod_parser's "increased Mana Regeneration Rate" and the statmap buff
    // domain (Clarity's `ManaRegen INC`) produce the bare name; Arcane
    // Surge's `ManaRegen MORE`, CalcPerform.lua:1586, uses the same name)
    // plus the dedicated `<stat>Rate` (an existing PoBR test name; vendor
    // has no such name, kept for compatibility) plus the generic `XRecoveryRate`.
    let rate_name = ModName::from(format!("{stat}Rate"));
    let recovery_rate_name = recovery_rate_mod_name(stat);
    let inc = db.sum(ModType::Inc, cfg, std::slice::from_ref(&bare_name))
        + db.sum(ModType::Inc, cfg, std::slice::from_ref(&rate_name))
        + db.sum(ModType::Inc, cfg, std::slice::from_ref(&recovery_rate_name));
    let more = db.more(cfg, &[bare_name])
        * db.more(cfg, &[rate_name])
        * db.more(cfg, &[recovery_rate_name]);
    regen(pool, flat, percent, inc, more)
}

/// Returns the corresponding `XRecoveryRate` ModName for a resource name.
fn recovery_rate_mod_name(stat: &str) -> ModName {
    let prefix = stat
        .trim_end_matches("Regen")
        .trim_end_matches("RegenPercent");
    ModName::from(format!("{prefix}RecoveryRate"))
}

// Capped Chance

/// Chance-stat aggregation: summed then clamped to `[0, cap]` (cap is typically 75% or 100%).
pub fn capped_chance(percent_sum: f64, cap: f64) -> f64 {
    round(percent_sum.clamp(0.0, cap))
}

/// Block chance (PoE2's hard cap of 90%, `data.misc.BlockChanceCap = 90`).
///
/// **Bug#11 fix (block-chance-cap-wrong)**: PoE2's block cap is 90%, not PoE1's 75%.
/// Source: agent-docs/block.md §Passive block, PoB2 DeepWiki `BlockChanceCap = 90`.
///
///  The cap now comes from the injected constants pack via the caller
/// (`cfg.constants.game().block_chance_cap`, fallback == old const, value unchanged).
pub fn block_chance(percent_sum: f64, cap: f64) -> f64 {
    capped_chance(percent_sum, cap)
}

// Charges -- PoE2: charges have no inherent stats, they only serve as a reference for per-charge mods

/// Default max charge stacks (Power / Frenzy / Endurance; PoB2 `Data/Misc.lua`).
///
/// Source: agent-docs/recovery-charges-buffs.md §1.3;
///       PoB2 `src/Data/Misc.lua`: `max_power_charges = max_frenzy_charges = max_endurance_charges = 3`.
pub const DEFAULT_MAX_CHARGES: u32 = 3;

/// Default charge duration (seconds; changed from 20s to 15s in 0.5.0).
///
/// Source: agent-docs/recovery-charges-buffs.md §1.3;
///       PoB2 `src/Modules/CalcSetup.lua`: `NewMod("ChargeDuration","BASE",15,"Base")`.
pub const DEFAULT_CHARGE_DURATION_SECONDS: f64 = 15.0;

/// Charge kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ChargeKind {
    Power,
    Frenzy,
    Endurance,
}

impl ChargeKind {
    /// The corresponding `XChargesMax` ModName (the max charge stack modifier name).
    pub fn max_mod_name(self) -> ModName {
        match self {
            ChargeKind::Power => ModName::from("PowerChargesMax"),
            ChargeKind::Frenzy => ModName::from("FrenzyChargesMax"),
            ChargeKind::Endurance => ModName::from("EnduranceChargesMax"),
        }
    }

    /// The corresponding `XChargesMin` ModName (the min charge stack modifier name).
    pub fn min_mod_name(self) -> ModName {
        match self {
            ChargeKind::Power => ModName::from("PowerChargesMin"),
            ChargeKind::Frenzy => ModName::from("FrenzyChargesMin"),
            ChargeKind::Endurance => ModName::from("EnduranceChargesMin"),
        }
    }

    /// The multiplier name (queried via `CalcConfig.multipliers`, mirroring
    /// `Modifier::effective_number`'s Multiplier tag).
    ///
    /// Source: agent-docs/recovery-charges-buffs.md §1.1;
    ///       PoB2 `CalcSetup.lua`: `modDB.multipliers["PowerCharge" | "FrenzyCharge" | "EnduranceCharge"]`.
    pub fn multiplier_key(self) -> &'static str {
        match self {
            ChargeKind::Power => "PowerCharge",
            ChargeKind::Frenzy => "FrenzyCharge",
            ChargeKind::Endurance => "EnduranceCharge",
        }
    }
}

/// Charge stack resolution result.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ChargeState {
    /// Current charge stacks (already clamped to the max/min).
    pub current: u32,
    /// Max charge stacks (includes `+N to Maximum X Charges` mods).
    pub maximum: u32,
    /// Min charge stacks (the always-on stack count).
    pub minimum: u32,
}

/// Resolves a charge kind's max stacks from the ModDb (default 3 + `+N to Maximum X Charges` BASE mods).
///
/// Source: agent-docs/recovery-charges-buffs.md §1.3;
///       PoB2 `CalcSetup.lua`: max base = 3, boosted by `XChargesMax` BASE mods.
pub fn charge_maximum(db: &ModDb, cfg: &CalcConfig, kind: ChargeKind) -> u32 {
    let extra = db.sum(ModType::Base, cfg, &[kind.max_mod_name()]);
    let max_val = DEFAULT_MAX_CHARGES as f64 + extra;
    max_val.max(0.0) as u32
}

/// Resolves a charge kind's min stacks from the ModDb (default 0; `+N to
/// Minimum X Charges` can set a constant floor).
///
/// Source: agent-docs/recovery-charges-buffs.md §1.5;
///       PoB2 `CalcSetup.lua`: the `MinimumXChargesIsMaximumXCharges` flag can raise the minimum to the maximum.
pub fn charge_minimum(db: &ModDb, cfg: &CalcConfig, kind: ChargeKind, maximum: u32) -> u32 {
    // MinimumXChargesIsMaximumXCharges: min charges = max charges (always at full stacks).
    let full_flag_name = match kind {
        ChargeKind::Power => "MinimumPowerChargesIsMaximumPowerCharges",
        ChargeKind::Frenzy => "MinimumFrenzyChargesIsMaximumFrenzyCharges",
        ChargeKind::Endurance => "MinimumEnduranceChargesIsMaximumEnduranceCharges",
    };
    if db.flag(cfg, ModName::from(full_flag_name)) {
        return maximum;
    }

    let min_val = db.sum(ModType::Base, cfg, &[kind.min_mod_name()]);
    (min_val.max(0.0) as u32).min(maximum)
}

/// Resolves charge state: reads the current stack count from
/// `CalcConfig.multipliers`, clamped by the ModDb's max/min.
///
/// PoB2 exposes the current stack count via
/// `modDB.multipliers["PowerCharge"]` for per-charge mods to reference (the
/// Multiplier tag); pobr uses `CalcConfig.multipliers` to mirror this.
///
/// # Parameters
/// - `db` -- the player ModDb (for querying max/min charge stack mods).
/// - `cfg` -- the current calculation config (`cfg.multiplier("PowerCharge")` etc. hold the current stack count).
/// - `kind` -- charge kind (Power / Frenzy / Endurance).
///
/// Source: agent-docs/recovery-charges-buffs.md §1.1 & §1.5;
///       PoB2 `src/Modules/CalcSetup.lua`'s charges section.
pub fn resolve_charge_state(db: &ModDb, cfg: &CalcConfig, kind: ChargeKind) -> ChargeState {
    let maximum = charge_maximum(db, cfg, kind);
    let minimum = charge_minimum(db, cfg, kind, maximum);
    let raw_current = cfg.multiplier(kind.multiplier_key());
    let current = (raw_current.max(0.0) as u32).clamp(minimum, maximum);
    ChargeState {
        current,
        maximum,
        minimum,
    }
}

/// Aggregate state of all three charge kinds.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AllChargeStates {
    pub power: ChargeState,
    pub frenzy: ChargeState,
    pub endurance: ChargeState,
}

impl AllChargeStates {
    /// Total stacks across all three charge kinds (used by the `TotalCharges` multiplier).
    ///
    /// Source: agent-docs/recovery-charges-buffs.md §Charge multipliers;
    ///       PoB2 `CalcSetup.lua`: `modDB.multipliers["TotalCharges"]`.
    pub fn total(&self) -> u32 {
        self.power.current + self.frenzy.current + self.endurance.current
    }
}

/// Resolves the complete state of all three charge kinds.
pub fn resolve_all_charges(db: &ModDb, cfg: &CalcConfig) -> AllChargeStates {
    AllChargeStates {
        power: resolve_charge_state(db, cfg, ChargeKind::Power),
        frenzy: resolve_charge_state(db, cfg, ChargeKind::Frenzy),
        endurance: resolve_charge_state(db, cfg, ChargeKind::Endurance),
    }
}

/// Fills the charge-stack multipliers per PoB2's charge semantics, returning a derived cfg.
///
/// PoB2 (CalcPerform.lua L831-832 / L899): only when `Condition:UseXCharges`
/// is true does `output.XCharges = output.XChargesMax`, after which
/// `modDB.multipliers["XCharge"] = output.XCharges`, causing `per X charge`
/// mods to expand at full stacks. **For a build that hasn't enabled this
/// charge, PoB2's panel shows current=0** (e.g. stormweaver's
/// `PowerCharges value="0"`), so this also stays at 0, avoiding incorrectly
/// applying `per charge` penalties/bonuses. pobr mirrors this with
/// `CalcConfig.multipliers` and `Condition:UseXCharges`.
///
/// **Override semantics**: if cfg already has an explicit positive value
/// (a build-imported `XCharges` count override / not at full stacks), that
/// value is kept as-is; under `MinimumXChargesIsMaximumXCharges` (always at
/// full stacks), `charge_minimum` returns maximum, so it's filled at the always-on stack count even without the use condition checked.
pub fn charge_multipliers_panel_default(db: &ModDb, cfg: &CalcConfig) -> CalcConfig {
    let mut out = cfg.clone();
    for kind in [ChargeKind::Power, ChargeKind::Frenzy, ChargeKind::Endurance] {
        let key = kind.multiplier_key();
        // Respect the build's override when it's already explicitly set to a positive value.
        if out.multiplier(key) > 0.0 {
            continue;
        }
        let maximum = charge_maximum(db, cfg, kind);
        let minimum = charge_minimum(db, cfg, kind, maximum);
        // Use condition (PoB2's `Condition:UseXCharges`): checked → full stacks, otherwise the always-on minimum (usually 0).
        let use_cond = match kind {
            ChargeKind::Power => "UsePowerCharges",
            ChargeKind::Frenzy => "UseFrenzyCharges",
            ChargeKind::Endurance => "UseEnduranceCharges",
        };
        let current = if cfg.condition(use_cond) {
            maximum.max(minimum)
        } else {
            minimum
        };
        if current > 0 {
            out = out.with_multiplier(key, current as f64);
        }
    }
    out
}

// Leech -- reworked in 0.5.0

/// Leech rate ceiling (Life/Mana: 20% of the pool; ES: 10% of the pool).
///
/// Source: agent-docs/recovery-charges-buffs.md §2.2;
///       PoB2 `src/Modules/CalcSetup.lua`: `MaxLifeLeechRate=20, MaxManaLeechRate=20, MaxEnergyShieldLeechRate=10`.
pub const LEECH_MAX_LIFE_RATE_PCT: f64 = 20.0;
pub const LEECH_MAX_MANA_RATE_PCT: f64 = 20.0;
pub const LEECH_MAX_ES_RATE_PCT: f64 = 10.0;

/// Leech single-instance ceiling (10% of each resource pool).
///
/// Source: agent-docs/recovery-charges-buffs.md §2.2;
///       PoB2 `src/Modules/CalcSetup.lua`: `MaxLifeLeechInstance=10`.
pub const LEECH_MAX_INSTANCE_PCT: f64 = 10.0;

/// Effective single-hit damage ceiling used for leech calculation (anything above is truncated).
///
/// Source: agent-docs/recovery-charges-buffs.md §2.2;
///       PoB2 `src/Data/Misc.lua`: `EffectiveMaxDamageForLeech = 40000`.
pub const LEECH_EFFECTIVE_MAX_HIT_DAMAGE: f64 = 40000.0;

/// Resource type being leeched.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LeechResource {
    Life,
    Mana,
    EnergyShield,
}

impl LeechResource {
    /// This resource's "leech rate ceiling (% of the pool)".
    pub fn max_rate_pct(self) -> f64 {
        match self {
            LeechResource::Life => LEECH_MAX_LIFE_RATE_PCT,
            LeechResource::Mana => LEECH_MAX_MANA_RATE_PCT,
            LeechResource::EnergyShield => LEECH_MAX_ES_RATE_PCT,
        }
    }

    /// The leech source's ModName (`LifeLeech` / `ManaLeech` / `EnergyShieldLeech`, BASE%).
    pub fn leech_mod_name(self) -> ModName {
        match self {
            LeechResource::Life => ModName::from("LifeLeech"),
            LeechResource::Mana => ModName::from("ManaLeech"),
            LeechResource::EnergyShield => ModName::from("EnergyShieldLeech"),
        }
    }
}

/// Leech calculation output (0.5.0's single-instance view).
///
/// 0.5.0's key change: each resource has **only one leech instance at a
/// time**; pobr estimates the panel value by "taking the highest-rate instance".
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LeechResult {
    /// Single-instance leech total (= `effective_hit × leech_pct%`, clamped by instance_cap).
    pub instance_total: f64,
    /// Leech rate ceiling (per second; = `pool × max_rate_pct%`).
    pub rate_cap_per_second: f64,
    /// Panel leech rate (per second; min(instance_total × 2%/s, rate_cap)).
    ///
    /// The PoE series' leech defaults to recovering 2% of the leeched amount
    /// per second (PoB2's `CalcSetup.lua` has no such constant, but it's an
    /// industry-consensus value, back-derived from leech total / duration;
    /// under 0.5.0's single-instance model this rate is the panel value directly).
    pub display_rate_per_second: f64,
}

/// Calculates a resource's leech panel rate (0.5.0's single-instance model).
///
/// # Parameters
/// - `pool` -- the corresponding resource pool's (life / mana / ES) final value.
/// - `leech_pct` -- this hit's damage × leech%, i.e. the sum of leech mod
///   percentages (e.g. `LifeLeech` BASE 0.5 means 0.5%).
/// - `hit_damage` -- the hit damage used for leech (physical/elemental;
///   truncated by `LEECH_EFFECTIVE_MAX_HIT_DAMAGE`).
/// - `resource` -- the leech resource type (determines the rate ceiling).
///
/// # Notes
/// - PoE2 defaults leech to "instance rate = leech total × 2%/s" (a PoE series convention);
/// - 0.5.0's single instance: only takes the single instance with the
///   highest rate (this function computes a single instance; the caller can take the max);
/// - Single-instance total ceiling: `pool × LEECH_MAX_INSTANCE_PCT%`;
/// - Rate ceiling: `pool × max_rate_pct%`;
/// - The `CannotLeechXxx` flag is short-circuited by the caller before
///   passing in (returns `LeechResult::zero(pool, resource)`).
///
/// Source: agent-docs/recovery-charges-buffs.md §2.2;
///       PoB2 `src/Modules/CalcDefence.lua`'s leech section / `CalcSetup.lua`'s ceiling constants.
pub fn calc_leech(
    pool: f64,
    leech_pct: f64,
    hit_damage: f64,
    resource: LeechResource,
) -> LeechResult {
    if pool <= 0.0 || leech_pct <= 0.0 {
        return LeechResult::zero(pool, resource);
    }
    // Effective single-hit damage ceiling (PoB2 Data/Misc.lua's EffectiveMaxDamageForLeech = 40000)
    let effective_hit = hit_damage.clamp(0.0, LEECH_EFFECTIVE_MAX_HIT_DAMAGE);
    // Single-instance leech total: subject to the single-instance ceiling (pool × 10%)
    let instance_cap = pool * LEECH_MAX_INSTANCE_PCT / 100.0;
    let instance_total = round((effective_hit * leech_pct / 100.0).min(instance_cap));

    // Rate ceiling: pool × max_rate_pct% /s (CalcSetup.lua's MaxLifeLeechRate=20, etc.).
    // Panel display rate = min(rate_cap, instance_total × (rate_cap / instance_cap))
    // = min(rate_cap, instance_total × max_rate_pct / max_instance_pct)
    // = min(rate_cap, instance_total × 2)  when max_rate = 20%, max_inst = 10%
    // Derivation: instance duration = instance_total / rate_cap; rate = instance_total / duration.
    // Simplified: if instance_total == instance_cap → display_rate = rate_cap (max rate);
    //        if instance_total < instance_cap → display_rate = instance_total × (rate_cap / instance_cap).
    let rate_cap = pool * resource.max_rate_pct() / 100.0;
    let ratio = resource.max_rate_pct() / LEECH_MAX_INSTANCE_PCT; // rate_cap / instance_cap (normalized)
    let display_rate_per_second = round((instance_total * ratio).min(rate_cap));

    LeechResult {
        instance_total,
        rate_cap_per_second: round(rate_cap),
        display_rate_per_second,
    }
}

/// Calculates a resource's leech result from the ModDb.
///
/// `leech_mod` is internalized as `resource.leech_mod_name()`; the caller is
/// responsible for supplying `hit_damage` (usually physical/hit damage; PoE2 defaults to physical only).
///
/// Source: agent-docs/recovery-charges-buffs.md §2.2;
///       PoB2 `src/Modules/CalcDefence.lua`'s leech section.
pub fn calc_leech_from_db(
    db: &ModDb,
    cfg: &CalcConfig,
    pool: f64,
    hit_damage: f64,
    resource: LeechResource,
) -> LeechResult {
    // CannotLeechXxx flag short-circuit
    let cannot_flag = match resource {
        LeechResource::Life => "CannotLeechLife",
        LeechResource::Mana => "CannotLeechMana",
        LeechResource::EnergyShield => "CannotLeechEnergyShield",
    };
    if db.flag(cfg, ModName::from(cannot_flag)) {
        return LeechResult::zero(pool, resource);
    }
    let leech_pct = db.sum(ModType::Base, cfg, &[resource.leech_mod_name()]);
    calc_leech(pool, leech_pct, hit_damage, resource)
}

impl LeechResult {
    /// An empty result with zero leech (pool = 0 or no leech mod).
    pub fn zero(pool: f64, resource: LeechResource) -> Self {
        Self {
            instance_total: 0.0,
            rate_cap_per_second: round(pool * resource.max_rate_pct() / 100.0),
            display_rate_per_second: 0.0,
        }
    }
}

// Recoup

/// Recoup's default return duration (seconds).
///
/// Source: agent-docs/recovery-charges-buffs.md §2.3;
///       PoB2 `CalcPerform.lua`: defaults to 8 seconds.
pub const RECOUP_DURATION_DEFAULT: f64 = 8.0;

/// The duration corresponding to Recoup's 4-second flag.
///
/// Source: agent-docs/recovery-charges-buffs.md §2.3;
///       PoB2 `CalcPerform.lua`: `4SecondRecoup` / `4SecondLifeRecoup` flags.
pub const RECOUP_DURATION_4S: f64 = 4.0;

/// Recoup resource type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecoupResource {
    Life,
    Mana,
    EnergyShield,
}

impl RecoupResource {
    /// The corresponding `XRecoup` ModName (BASE%, e.g. `LifeRecoup` is the return ratio for damage taken).
    pub fn recoup_mod_name(self) -> ModName {
        match self {
            RecoupResource::Life => ModName::from("LifeRecoup"),
            RecoupResource::Mana => ModName::from("ManaRecoup"),
            RecoupResource::EnergyShield => ModName::from("EnergyShieldRecoup"),
        }
    }

    /// The global 4-second Recoup flag name (`4SecondRecoup` applies to every resource).
    pub fn four_sec_flag_global() -> &'static str {
        "4SecondRecoup"
    }

    /// The per-resource 4-second Recoup flag name (`4SecondLifeRecoup`, etc.).
    pub fn four_sec_flag(self) -> &'static str {
        match self {
            RecoupResource::Life => "4SecondLifeRecoup",
            RecoupResource::Mana => "4SecondManaRecoup",
            RecoupResource::EnergyShield => "4SecondEnergyShieldRecoup",
        }
    }
}

/// Recoup calculation output.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RecoupResult {
    /// The returned per-second rate (damage taken × recoup_pct% / duration).
    pub rate_per_second: f64,
    /// Recoup duration (seconds; 8 or 4).
    pub duration: f64,
}

/// Calculates Recoup's per-second return rate (pure-function version).
///
/// Recoup = `damage_taken × recoup_pct%`, returned evenly over `duration`
/// seconds; multiplied by `recovery_rate_mod` (`XRecoveryRateMod`).
///
/// # Parameters
/// - `damage_taken` -- the damage taken in a single hit (already mitigated;
///   the portion that reaches the resource pool).
/// - `recoup_pct` -- the sum of recoup mod percentages (e.g. `LifeRecoup` BASE 15 means 15%).
/// - `duration` -- the return duration (8 seconds or 4 seconds).
/// - `recovery_rate_mod` -- `XRecoveryRateMod` (fraction; 1.0 = no bonus).
///
/// Source: agent-docs/recovery-charges-buffs.md §2.3;
///       PoB2 `src/Modules/CalcDefence.lua`'s Recoup section.
pub fn calc_recoup(
    damage_taken: f64,
    recoup_pct: f64,
    duration: f64,
    recovery_rate_mod: f64,
) -> RecoupResult {
    if damage_taken <= 0.0 || recoup_pct <= 0.0 || duration <= 0.0 {
        return RecoupResult {
            rate_per_second: 0.0,
            duration: duration.max(RECOUP_DURATION_DEFAULT),
        };
    }
    let total = damage_taken * recoup_pct / 100.0;
    let rate_per_second = round(total / duration * recovery_rate_mod.max(0.0));
    RecoupResult {
        rate_per_second,
        duration,
    }
}

/// Calculates the Recoup result from the ModDb.
///
/// - Duration: checks `4SecondRecoup` (global) first, then the
///   resource-specific `4SecondXRecoup` flag; either being true → 4 seconds, otherwise 8 seconds.
/// - `recovery_rate_mod`: computed from `XRecoveryRate` INC/MORE (shares the factor with regen).
///
/// Source: agent-docs/recovery-charges-buffs.md §2.3;
///       PoB2 `src/Modules/CalcDefence.lua`'s Recoup section;
///       PoB2 `src/Modules/CalcPerform.lua`'s `4SecondRecoup` flag handling.
pub fn calc_recoup_from_db(
    db: &ModDb,
    cfg: &CalcConfig,
    damage_taken: f64,
    resource: RecoupResource,
) -> RecoupResult {
    let recoup_pct = db.sum(ModType::Base, cfg, &[resource.recoup_mod_name()]);
    if recoup_pct <= 0.0 {
        return RecoupResult {
            rate_per_second: 0.0,
            duration: RECOUP_DURATION_DEFAULT,
        };
    }

    // Duration: the global 4s flag or the per-resource 4s flag
    let four_sec = db.flag(cfg, ModName::from(RecoupResource::four_sec_flag_global()))
        || db.flag(cfg, ModName::from(resource.four_sec_flag()));
    let duration = if four_sec {
        RECOUP_DURATION_4S
    } else {
        RECOUP_DURATION_DEFAULT
    };

    // RecoveryRateMod: this resource's XRecoveryRate inc/more merged into a multiplier
    let recovery_rate_name = match resource {
        RecoupResource::Life => ModName::from("LifeRecoveryRate"),
        RecoupResource::Mana => ModName::from("ManaRecoveryRate"),
        RecoupResource::EnergyShield => ModName::from("EnergyShieldRecoveryRate"),
    };
    let rate_inc = db.sum(ModType::Inc, cfg, std::slice::from_ref(&recovery_rate_name));
    let rate_more = db.more(cfg, &[recovery_rate_name]);
    let recovery_rate_mod = (1.0 + rate_inc / 100.0) * rate_more;

    calc_recoup(damage_taken, recoup_pct, duration, recovery_rate_mod)
}
