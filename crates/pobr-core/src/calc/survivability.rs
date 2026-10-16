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
    // "No inherent Mana Regeneration" removes only the character's base
    // regeneration, not flat/percentage regeneration granted by other sources.
    let mut inherent_cfg;
    let cfg = if stat == "ManaRegen" && db.flag(cfg, "Condition:NoInherentManaRegen") {
        inherent_cfg = cfg.clone();
        inherent_cfg
            .conditions
            .insert("NoInherentManaRegen".into(), true);
        &inherent_cfg
    } else {
        cfg
    };
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
    /// Current charge stacks, floored at the minimum; explicit counts may exceed the maximum.
    pub current: f64,
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
    if db.flag(cfg, full_flag_name) {
        return maximum;
    }

    let min_val = db.sum(ModType::Base, cfg, &[kind.min_mod_name()]);
    (min_val.max(0.0) as u32).min(maximum)
}

/// Resolves charge state: reads the current stack count from
/// `CalcConfig.multipliers`, floored at the ModDb's minimum without truncation.
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
    let current = raw_current.max(f64::from(minimum));
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
    pub fn total(&self) -> f64 {
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

/// Inputs for each charge kind: its count override modifier and use condition.
/// The plain count comes from these inputs (or the minimum while disabled);
/// `Removable<Kind>Charge` is that count minus the minimum, after gain routing.
/// Diagnostics resolve the use condition through the config catalog rather
/// than maintaining a second list of config option ids. Every entry has an
/// off/on ledger fixture in `pobr-build/tests/skills/line_usage.rs`.
pub const CHARGE_MULTIPLIER_INPUTS: &[(ChargeKind, &str, &str)] = &[
    (ChargeKind::Power, "PowerCharges", "UsePowerCharges"),
    (ChargeKind::Frenzy, "FrenzyCharges", "UseFrenzyCharges"),
    (
        ChargeKind::Endurance,
        "EnduranceCharges",
        "UseEnduranceCharges",
    ),
];

/// The charge-use condition driving a plain or removable charge multiplier.
/// The catalog maps this condition to the player's charge configuration.
pub fn charge_multiplier_use_condition(multiplier: &str) -> Option<&'static str> {
    let count = multiplier.strip_prefix("Removable").unwrap_or(multiplier);
    CHARGE_MULTIPLIER_INPUTS
        .iter()
        .find(|(kind, _, _)| kind.multiplier_key() == count)
        .map(|(_, _, condition)| *condition)
}

/// Fills the charge-stack multipliers, returning a derived cfg.
///
/// When `UseXCharges` is enabled, a matching
/// `XCharges` OVERRIDE supplies the count, otherwise the maximum is used.
/// The result is floored at the always-on minimum, including when use is off.
/// Explicit counts are neither truncated nor upper-clamped. The config interpreter
/// already treats a count input of zero as unset and applies the Combat tag.
///
/// Positive legacy/programmatic multipliers retain their existing fallback
/// precedence when no enabled OVERRIDE applies.
///
/// Also fills how many charges of each kind a skill could consume
/// (`RemovablePowerCharge`, ..., and their sum `RemovableTotalCharges`), for
/// "per charge consumed" effects.
pub fn charge_multipliers_panel_default(db: &ModDb, cfg: &CalcConfig) -> CalcConfig {
    let mut out = cfg.clone();
    // Project the runtime mode only for charge-override queries. Do not activate
    // unrelated Combat-tagged effects in the returned calculation context.
    let original_combat = cfg.conditions.get("Combat").copied();
    let mut projected_combat = false;
    let mut removable_total = 0.0;
    for &(kind, override_name, use_condition) in CHARGE_MULTIPLIER_INPUTS {
        let key = kind.multiplier_key();
        let maximum = charge_maximum(db, cfg, kind);
        let minimum = f64::from(charge_minimum(db, cfg, kind, maximum));
        let enabled = cfg.condition(use_condition);
        if enabled && !projected_combat {
            if let Some(combat) = out.conditions.get_mut("Combat") {
                *combat = cfg.mode_combat;
            } else {
                out.conditions.insert("Combat".into(), cfg.mode_combat);
            }
            projected_combat = true;
        }
        let explicit = enabled.then(|| db.override_(&out, override_name)).flatten();
        let legacy = cfg.multiplier(key);
        let current = explicit
            .unwrap_or_else(|| {
                if legacy > 0.0 {
                    legacy
                } else if enabled {
                    f64::from(maximum)
                } else {
                    minimum
                }
            })
            .max(minimum);
        if current != legacy {
            out = out.with_multiplier(key, current);
        }
    }
    // Resonance redirects gains once, simultaneously: Frenzy -> Power,
    // Power -> Endurance, Endurance -> Frenzy. It does not convert maxima
    // or always-held minima. With no gain-event model, configured removable
    // counts are the incoming gains; destination capacity caps the result.
    let kinds = [ChargeKind::Power, ChargeKind::Frenzy, ChargeKind::Endurance];
    let redirects = [
        ("GainEnduranceChargesInsteadOfPower", 2),
        ("GainPowerChargesInsteadOfFrenzy", 0),
        ("GainFrenzyChargesInsteadOfEndurance", 1),
    ];
    let redirected = redirects.map(|(flag, destination)| (db.flag(cfg, flag), destination));
    if redirected.iter().any(|(active, _)| *active) {
        let minima = kinds
            .map(|kind| f64::from(charge_minimum(db, cfg, kind, charge_maximum(db, cfg, kind))));
        let mut gains = [0.0; 3];
        for (i, kind) in kinds.iter().enumerate() {
            let destination = if redirected[i].0 { redirected[i].1 } else { i };
            gains[destination] += (out.multiplier(kind.multiplier_key()) - minima[i]).max(0.0);
        }
        for (i, kind) in kinds.iter().enumerate() {
            out.multipliers.insert(
                kind.multiplier_key().into(),
                (minima[i] + gains[i]).min(f64::from(charge_maximum(db, cfg, *kind))),
            );
        }
    }
    // Conduit sends incoming gains to allies. As with Resonance, only counts
    // above the always-held minima represent gains in this snapshot model.
    let gains_go_to_allies = db.flag(cfg, "ChargesGainedByAllies");
    for kind in kinds {
        let key = kind.multiplier_key();
        let minimum = f64::from(charge_minimum(db, cfg, kind, charge_maximum(db, cfg, kind)));
        if gains_go_to_allies {
            out.multipliers.insert(key.into(), minimum);
        }
        let removable = (out.multiplier(key) - minimum).max(0.0);
        removable_total += removable;
        out.multipliers.insert(format!("Removable{key}"), removable);
    }
    out.multipliers
        .insert("RemovableTotalCharges".into(), removable_total);
    if projected_combat {
        if let Some(original) = original_combat {
            *out.conditions
                .get_mut("Combat")
                .expect("temporary Combat projection") = original;
        } else {
            out.conditions.remove("Combat");
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

/// Base leech duration: the game's Life/Mana/Energy Shield Leech keywords
/// say recovery occurs over one second: https://poe2db.tw/us/Keywords.
pub const LEECH_DURATION_SECONDS: f64 = 1.0;

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

/// One leech instance, before frequency and competing instances are considered.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LeechResult {
    pub instance_total: f64,
    pub rate_cap_per_second: f64,
    pub display_rate_per_second: f64,
}

/// The game's leech keywords recover the damage-derived amount over one
/// second, not 10% of the pool over a duration inferred from a rate ceiling.
/// https://poe2db.tw/us/Keywords
pub fn calc_leech(
    pool: f64,
    leech_pct: f64,
    hit_damage: f64,
    resource: LeechResource,
) -> LeechResult {
    leech_instance(
        pool,
        hit_damage.clamp(0.0, LEECH_EFFECTIVE_MAX_HIT_DAMAGE) * leech_pct.max(0.0) / 100.0,
        resource,
        1.0,
        1.0,
    )
}

/// Apply speed to the instance, and increases to the maximum before capping.
/// Speed changes the recovery time, never the amount recovered.
pub(crate) fn leech_instance(
    pool: f64,
    amount: f64,
    resource: LeechResource,
    speed: f64,
    maximum: f64,
) -> LeechResult {
    let amount = if pool > 0.0 { amount.max(0.0) } else { 0.0 };
    let rate_cap = pool.max(0.0) * resource.max_rate_pct() / 100.0 * maximum.max(0.0);
    LeechResult {
        instance_total: round(amount),
        rate_cap_per_second: round(rate_cap),
        display_rate_per_second: round(
            (amount / LEECH_DURATION_SECONDS * speed.max(0.0)).min(rate_cap),
        ),
    }
}

pub(crate) fn leech_modifier_factor(db: &ModDb, cfg: &CalcConfig, name: &str) -> f64 {
    let names = [ModName::from(name)];
    ((1.0 + db.sum(ModType::Inc, cfg, &names) / 100.0) * db.more(cfg, &names)).max(0.0)
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
    // Proxy damage does not grant the player ordinary leech. PoB2
    // CalcOffence.lua:4413-4418 only permits DamageLifeLeechToPlayer for
    // mine/trap/totem damage; that explicit transfer is not modeled here.
    // Trappable/Mineable merely describe eligibility and must not be gated.
    let proxy_types = pobr_data::skill::SkillTypes::TRAPPED
        | pobr_data::skill::SkillTypes::REMOTE_MINED
        | pobr_data::skill::SkillTypes::from_pob2_name("UsedByTotem").unwrap_or_default()
        | pobr_data::skill::SkillTypes::from_pob2_name("UsedByProxy").unwrap_or_default();
    if cfg.skill_types.intersects(proxy_types) {
        return LeechResult::zero(pool, resource);
    }
    // CannotLeechXxx flag short-circuit
    let cannot_flag = match resource {
        LeechResource::Life => "CannotLeechLife",
        LeechResource::Mana => "CannotLeechMana",
        LeechResource::EnergyShield => "CannotLeechEnergyShield",
    };
    if db.flag(cfg, cannot_flag) {
        return LeechResult::zero(pool, resource);
    }
    // The typed names are emitted by PoB's equipment/passive parser. The
    // legacy untyped names remain accepted by the minimal calculation API.
    // hit_damage must be the physical component after conversion, never the
    // total hit (which would let elemental added damage fuel physical leech).
    let physical_name = match resource {
        LeechResource::Life => "PhysicalDamageLifeLeech",
        LeechResource::Mana => "PhysicalDamageManaLeech",
        LeechResource::EnergyShield => "PhysicalDamageEnergyShieldLeech",
    };
    let leech_pct = db.sum(
        ModType::Base,
        cfg,
        &[resource.leech_mod_name(), ModName::from(physical_name)],
    );
    let name = resource.leech_mod_name().to_string();
    leech_instance(
        pool,
        hit_damage.clamp(0.0, LEECH_EFFECTIVE_MAX_HIT_DAMAGE) * leech_pct.max(0.0) / 100.0
            * leech_modifier_factor(db, cfg, &format!("{name}Amount")),
        resource,
        leech_modifier_factor(db, cfg, &format!("{name}Rate")),
        leech_modifier_factor(db, cfg, &format!("Max{name}Rate")),
    )
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
pub(crate) struct RecoupTerms {
    pub damage: f64,
    pub percent: f64,
    pub duration: f64,
    pub recovery: f64,
}

pub fn calc_recoup_from_db(
    db: &ModDb,
    cfg: &CalcConfig,
    damage_taken: f64,
    resource: RecoupResource,
) -> RecoupResult {
    let t = recoup_terms(db, cfg, damage_taken, resource);
    calc_recoup(t.damage, t.percent, t.duration, t.recovery)
}

/// Recoup duration for a resource: the global `4SecondRecoup` flag or the
/// resource's own `4SecondXRecoup` flag selects the 4s duration, otherwise
/// the 8s default.
fn recoup_duration(db: &ModDb, cfg: &CalcConfig, resource: RecoupResource) -> f64 {
    let four_sec = db.flag(cfg, RecoupResource::four_sec_flag_global())
        || db.flag(cfg, resource.four_sec_flag());
    if four_sec {
        RECOUP_DURATION_4S
    } else {
        RECOUP_DURATION_DEFAULT
    }
}

/// Recovery rate multiplier for a resource: its `XRecoveryRate` INC/MORE
/// merged into a multiplier (shared with regen).
fn recoup_recovery_rate(db: &ModDb, cfg: &CalcConfig, resource: RecoupResource) -> f64 {
    let name = match resource {
        RecoupResource::Life => ModName::from("LifeRecoveryRate"),
        RecoupResource::Mana => ModName::from("ManaRecoveryRate"),
        RecoupResource::EnergyShield => ModName::from("EnergyShieldRecoveryRate"),
    };
    let inc = db.sum(ModType::Inc, cfg, std::slice::from_ref(&name));
    (1.0 + inc / 100.0) * db.more(cfg, &[name])
}

/// Generic recoup percentage for a resource: its `XRecoup` BASE, plus
/// `LifeRecoup` for Energy Shield when `LifeRecoupAlsoAppliesToEnergyShield`
/// (copies the percentage; the recipient keeps its own timing and recovery).
fn recoup_base_percent(db: &ModDb, cfg: &CalcConfig, resource: RecoupResource) -> f64 {
    let mut pct = db.sum(ModType::Base, cfg, &[resource.recoup_mod_name()]);
    // life_recoup_also_applies_to_energy_shield copies the percentage;
    // the recipient retains its own recovery rate and recoup duration.
    if resource == RecoupResource::EnergyShield
        && db.flag(cfg, "LifeRecoupAlsoAppliesToEnergyShield")
    {
        pct += db.sum(ModType::Base, cfg, &[ModName::from("LifeRecoup")]);
    }
    pct
}

pub(crate) fn recoup_terms(
    db: &ModDb,
    cfg: &CalcConfig,
    damage_taken: f64,
    resource: RecoupResource,
) -> RecoupTerms {
    let recoup_pct = recoup_base_percent(db, cfg, resource);
    if recoup_pct <= 0.0 {
        return RecoupTerms {
            damage: damage_taken,
            percent: recoup_pct,
            duration: RECOUP_DURATION_DEFAULT,
            recovery: 1.0,
        };
    }

    RecoupTerms {
        damage: damage_taken,
        percent: recoup_pct,
        duration: recoup_duration(db, cfg, resource),
        recovery: recoup_recovery_rate(db, cfg, resource),
    }
}

/// Energy Shield recoup with per-damage-type percentages, shared by the panel
/// rate and the EsRecoupRate derived detail so the detail's inputs reconstruct it.
///
/// `percent_by_type` is Physical/Fire/Cold/Lightning/Chaos order: the generic
/// ES percentage on every type, plus `Fire/Cold/LightningEnergyShieldRecoup`
/// (game stat `recoup_%_elemental_damage_as_energy_shield`, parsed as three
/// per-type BASE mods) on its own type. Duration and recovery are the shared
/// ES timing and Energy Shield recovery rate; `rate_per_second` is
/// `round(Σ recoupable × percent / 100 / duration × recovery)`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EsRecoupTerms {
    pub recoupable_by_type: [f64; 5],
    pub percent_by_type: [f64; 5],
    pub duration: f64,
    pub recovery: f64,
    pub rate_per_second: f64,
}

pub fn es_recoup_terms(
    db: &ModDb,
    cfg: &CalcConfig,
    recoupable_by_type: [f64; 5],
) -> EsRecoupTerms {
    use pobr_data::constants::DamageType as DT;
    let base = recoup_base_percent(db, cfg, RecoupResource::EnergyShield);
    let specific = [
        (DT::Fire, "FireEnergyShieldRecoup"),
        (DT::Cold, "ColdEnergyShieldRecoup"),
        (DT::Lightning, "LightningEnergyShieldRecoup"),
    ];
    let mut percent_by_type = [base; 5];
    for (dtype, name) in specific {
        percent_by_type[dtype as usize] += db.sum(ModType::Base, cfg, &[ModName::from(name)]);
    }
    let duration = recoup_duration(db, cfg, RecoupResource::EnergyShield);
    let recovery = recoup_recovery_rate(db, cfg, RecoupResource::EnergyShield);
    let mut total = 0.0;
    for (dmg, pct) in recoupable_by_type.iter().zip(percent_by_type.iter()) {
        if *dmg > 0.0 && *pct > 0.0 {
            total += dmg.max(0.0) * pct / 100.0;
        }
    }
    let rate_per_second = if total <= 0.0 || duration <= 0.0 {
        0.0
    } else {
        round(total / duration * recovery.max(0.0))
    };
    EsRecoupTerms {
        recoupable_by_type,
        percent_by_type,
        duration,
        recovery,
        rate_per_second,
    }
}

/// The ES recoup per-second rate from per-type recoupable damage
/// (`recoupable_by_type` in Physical/Fire/Cold/Lightning/Chaos order).
///
/// This reads `Fire/Cold/LightningEnergyShieldRecoup` (was unread): 15%
/// elemental ES recoup recoups 15% of fire/cold/lightning damage taken, and
/// nothing of physical/chaos damage — so it appears for fire hits only (when
/// the recoupable is pure fire).
pub fn calc_es_recoup_rate_by_type(
    db: &ModDb,
    cfg: &CalcConfig,
    recoupable_by_type: [f64; 5],
) -> f64 {
    es_recoup_terms(db, cfg, recoupable_by_type).rate_per_second
}
