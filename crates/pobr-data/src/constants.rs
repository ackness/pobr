//! **Deprecation note**: this file has been downgraded from the source of
//! truth for calc constants to a **fallback layer** — the source of truth
//! has moved to `data/<poe_version>/base/game_constants.json` (schema in
//! [`crate::catalog::game_constants`], locked equal to this file by the W2
//! value-by-value comparison test).
//!
//! - The calc consumer side (pobr-core) has switched to reading the
//!   injected [`crate::catalog::RuntimeConstants`] (via
//!   `CalcConfig.constants`); **no new calc-path consumers of this file's
//!   constants are allowed**;
//! - What this file still does, and only this: provide the single numeric
//!   source of truth for the catalog Def types' `Default` (the fallback
//!   used when there's no GameData) — `Default` refers directly to the
//!   consts here, so there's no double authority of literals. The enum and
//!   struct types (`DamageType` / `AilmentType`, etc.) are L4 framework
//!   semantics and live here long-term.
//! - The numeric-constant section will be deleted entirely once all
//!   fallback dependents are cleared out.

use serde::{Deserialize, Serialize};

/// Default max elemental / chaos resistance (percent). Resistance above
/// this is recorded as over-cap.
pub const DEFAULT_MAX_RESISTANCE: f64 = 75.0;
/// Resistance hard cap (percent); no maximum-resistance increase can push past it.
pub const HARD_MAX_RESISTANCE: f64 = 90.0;
/// Resistance floor (the lowest negative resistance can stack to).
pub const RESIST_FLOOR: f64 = -200.0;
/// PoE2's armour coefficient (armour damage reduction =
/// `armour/(armour + ARMOUR_RATIO*raw_hit)`; PoE1 used 5).
pub const ARMOUR_RATIO: f64 = 10.0;
/// Server tick time for non-channelling actions (~33ms → a 30.3 actions/s
/// cap), see skill-speed.md.
pub const SERVER_TICK_SECONDS: f64 = 0.033;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DamageType {
    Physical,
    Fire,
    Cold,
    Lightning,
    Chaos,
}

impl DamageType {
    /// The three elemental damage types (what the `ElementalDamage`
    /// modifier matches — excludes chaos).
    pub const ELEMENTAL: [DamageType; 3] =
        [DamageType::Fire, DamageType::Cold, DamageType::Lightning];

    /// Whether this is an elemental damage type (fire/cold/lightning).
    pub fn is_elemental(self) -> bool {
        matches!(
            self,
            DamageType::Fire | DamageType::Cold | DamageType::Lightning
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ClassId {
    Marauder,
    Duelist,
    Ranger,
    Shadow,
    Witch,
    Templar,
    Scion,
}

/// An elemental sub-type, used to aggregate `ElementalDamage` (fire/cold/lightning).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ElementalType {
    Fire,
    Cold,
    Lightning,
}

impl ElementalType {
    pub fn damage_type(self) -> DamageType {
        match self {
            ElementalType::Fire => DamageType::Fire,
            ElementalType::Cold => DamageType::Cold,
            ElementalType::Lightning => DamageType::Lightning,
        }
    }
}

/// Damage-source bucket: distinguishes hit damage from the various
/// damage-over-time / secondary damage kinds (08-mechanics §2.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DamageSource {
    Attack,
    Spell,
    Secondary,
    Thorns,
    Ailment,
    Debuff,
}

/// Hit vs. sustained: DoT can't crit, and ailment magnitude is based on the
/// pre-mitigation Hit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DamageKind {
    Hit,
    Dot,
}

/// An ailment type. Kept separate from [`DamageType`] to support physical
/// DoTs like Corrupted Blood that aren't bleeding.
///
/// Reference: `agent-docs/ailments.md` + `agent-docs/damage-types.md` (PoE2 0.5.0).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AilmentType {
    Bleed,
    Ignite,
    Shock,
    Chill,
    Freeze,
    Poison,
    Electrocute,
    CorruptedBlood,
}

/// Whether an ailment deals damage (for quick grouping, so higher-level
/// matches don't get bloated).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AilmentCategory {
    Damaging,
    NonDamaging,
}

impl AilmentType {
    /// Whether this ailment deals damage.
    pub fn category(self) -> AilmentCategory {
        match self {
            AilmentType::Bleed
            | AilmentType::Ignite
            | AilmentType::Poison
            | AilmentType::CorruptedBlood => AilmentCategory::Damaging,
            AilmentType::Shock
            | AilmentType::Chill
            | AilmentType::Freeze
            | AilmentType::Electrocute => AilmentCategory::NonDamaging,
        }
    }

    /// The damage type a damaging ailment corresponds to; `None` for
    /// non-damaging ones.
    ///
    /// Bleed / Corrupted Blood are physical, Ignite is fire, Poison is
    /// chaos (its magnitude is generated from the hit's physical + chaos
    /// damage).
    pub fn damage_type(self) -> Option<DamageType> {
        match self {
            AilmentType::Bleed | AilmentType::CorruptedBlood => Some(DamageType::Physical),
            AilmentType::Ignite => Some(DamageType::Fire),
            AilmentType::Poison => Some(DamageType::Chaos),
            AilmentType::Shock
            | AilmentType::Chill
            | AilmentType::Freeze
            | AilmentType::Electrocute => None,
        }
    }
}

/// PoE2's base crit damage bonus for player/minions (+100%; i.e. the crit
/// damage multiplier is 2.0 when its base is 0).
/// Source: agent-docs/critical-hits.md §crit damage, the PoE2 wiki's
/// Critical hit page, and CalcOffence.lua's `Sum("BASE","CritMultiplier")`
/// defaulting to +100%.
pub const PLAYER_BASE_CRIT_DAMAGE_BONUS: f64 = 100.0;

/// Block chance hard cap (PoB2's `data.misc.BlockChanceCap = 90`,
/// agent-docs/block.md).
pub const BLOCK_CHANCE_CAP: f64 = 90.0;

/// Shock's minimum effective magnitude (PoE2 0.5.0: BaseShockMagnitude =
/// 20, agent-docs/ailments.md §Shock).
pub const SHOCK_MIN_EFFECT: f64 = 20.0;

/// Global DoT DPS cap (`(2^31 - 1) / 60`, roughly 3.579×10^7).
///
/// Source: PoB2 `src/Modules/Data.lua`:
///   `DotDpsCap = 35791394, -- (2 ^ 31 - 1) / 60 (int max / 60 seconds)`.
/// Every ailment/DoT panel's DPS must be clamped to this (including TotalDotDPS).
pub const DOT_DPS_CAP: f64 = 35_791_394.0;

/// Central home for PoE2 calc constants, so magic numbers don't scatter
/// across the formulas.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct GameConstants {
    pub resist_default_max: f64,
    pub resist_hard_cap: f64,
    pub resist_floor: f64,
    pub server_tick_seconds: f64,
    pub armour_ratio: f64,
    /// Bleed's base magnitude as a fraction of pre-mitigation physical hit
    /// damage (per second).
    pub bleed_base_fraction: f64,
    /// Bleed's base duration (seconds).
    pub bleed_base_duration: f64,
    /// Ignite's base magnitude as a fraction of pre-mitigation fire hit
    /// damage (per second).
    pub ignite_base_fraction: f64,
    pub ignite_base_duration: f64,
    /// Poison's base magnitude as a fraction of pre-mitigation hit damage
    /// (per second).
    pub poison_base_fraction: f64,
    pub poison_base_duration: f64,
    /// Default shock increased-damage-taken magnitude
    /// (agent-docs/ailments.md's BaseShockMagnitude=20, 0.5.0).
    pub shock_default_effect: f64,
    /// Base crit damage bonus for player/minions (PoE2 +100%; see
    /// PLAYER_BASE_CRIT_DAMAGE_BONUS).
    pub player_base_crit_damage_bonus: f64,
    /// Block chance hard cap (PoE2 90%; see BLOCK_CHANCE_CAP).
    pub block_chance_cap: f64,
}

impl GameConstants {
    /// The default PoE2 0.5.0 constant set.
    pub fn poe2() -> Self {
        Self {
            resist_default_max: DEFAULT_MAX_RESISTANCE,
            resist_hard_cap: HARD_MAX_RESISTANCE,
            resist_floor: RESIST_FLOOR,
            server_tick_seconds: SERVER_TICK_SECONDS,
            armour_ratio: ARMOUR_RATIO,
            bleed_base_fraction: 0.15,
            bleed_base_duration: 5.0,
            ignite_base_fraction: 0.20,
            ignite_base_duration: 4.0,
            poison_base_fraction: 0.20,
            poison_base_duration: 2.0,
            shock_default_effect: SHOCK_MIN_EFFECT / 100.0,
            player_base_crit_damage_bonus: PLAYER_BASE_CRIT_DAMAGE_BONUS,
            block_chance_cap: BLOCK_CHANCE_CAP,
        }
    }

    /// The server-tick rate cap for non-channelling actions (actions/s).
    pub fn server_tick_rate(&self) -> f64 {
        1.0 / self.server_tick_seconds
    }
}

impl Default for GameConstants {
    fn default() -> Self {
        Self::poe2()
    }
}

/// A damage range (an element of a skill's per-level base damage table).
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct DamageRange {
    pub min: f64,
    pub max: f64,
}

impl DamageRange {
    pub fn new(min: f64, max: f64) -> Self {
        Self { min, max }
    }

    /// The range's average value.
    pub fn avg(&self) -> f64 {
        (self.min + self.max) / 2.0
    }
}

/// A skill's cost type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SkillCostKind {
    Mana,
    Life,
    Spirit,
    EnergyShield,
    NoCost,
}

/// A skill's cost (type + amount).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SkillCost {
    pub kind: SkillCostKind,
    pub value: f64,
}

impl Default for SkillCost {
    fn default() -> Self {
        Self {
            kind: SkillCostKind::NoCost,
            value: 0.0,
        }
    }
}
