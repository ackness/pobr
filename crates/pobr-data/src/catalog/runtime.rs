//! The runtime constants bundle injected into the calc engine.
//!
//! # Pipeline shape (the interface contract for each domain's phase-2 agent)
//!
//! [`RuntimeConstants`] aggregates "the stored data domains calc consumes"
//! (pobr-data catalog types), injected along this path, with pobr-core
//! staying zero-I/O:
//!
//! ```text
//! data/<version>/base/*.json
//!   → (pobr-gamedata's `GameData::load_ruleset()`, the sole I/O point) RuleSet { Option<each domain's Def> }
//!   → (pobr-build's `BuildData::load` merges: Some=data, None=Default fallback) RuntimeConstants
//!   → (pobr-build's `calculate_with_data` → `CalculationSession::set_constants`)
//!   → pobr-core's `CalcConfig.constants` (threaded through `cfg` to every calc function)
//! ```
//!
//! # Invariants
//!
//! - **`Default` = the fallback**: each domain field's `Default` must be
//!   value-equal to its corresponding JSON (W2 already locked JSON == the
//!   old hardcoded Rust; `Default` references the old Rust constants rather
//!   than duplicating literals), so "no GameData → Default" and "GameData
//!   present → injected" produce value-identical output — the structural
//!   guarantee behind the migration invariant (zero parity change).
//! - **How to extend (phase 2)**: for each new data domain wired in, **add
//!   a field** to this struct (typed as the corresponding catalog Def, with
//!   the `Default` impl living in pobr-data and referencing the existing
//!   Rust source of truth), and add a matching merge line to both
//!   `pobr-gamedata::RuleSet` and `pobr-build::BuildData::load`; the
//!   consumer reads it via `cfg.constants.<domain>`. For a large tabular
//!   domain (a per-level table, etc.) where cloning turns out too costly
//!   under measurement, consider hanging it off `Env` instead (consumed
//!   once at setup) — don't optimize preemptively.

use serde::{Deserialize, Serialize};

use super::enemy_presets::EnemyPresetsTable;
use super::game_constants::{
    CharacterConstantsDef, GameConstantsDef, GameMechanicsConstantsDef, MonsterConstantsDef,
};
use super::monster_scaling::MonsterScalingTable;
use super::unarmed_data::UnarmedDataTable;
use super::weapon_types::WeaponTypeTable;

/// The runtime constants bundle injected into calc. `Default` = the
/// fallback used when there's no GameData (value-equal to the JSON).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct RuntimeConstants {
    /// The three-section global game constants table (`base/game_constants.json`).
    pub game_constants: GameConstantsDef,
    /// Character level/attribute-derived constants
    /// (`base/character_constants.json`, consumed by
    /// `pobr-core::character`'s `CharacterBase` derivation formulas). Don't
    /// confuse this with [`Self::character`] (game_constants's character
    /// section, player-baseline magic numbers) — this domain carries the
    /// per-level / per-attribute derivation coefficients for life/mana/accuracy.
    pub character_constants: super::character_constants::CharacterConstantsDef,
    /// Monster per-level scaling table (`base/monster_scaling.json`;
    /// consumed by `setup_env`'s enemy assembly).
    pub monster_scaling: MonsterScalingTable,
    /// Enemy tier presets (`base/enemy_presets.json`; consumed by
    /// `setup_env`'s tier bonuses/penetration).
    pub enemy_presets: EnemyPresetsTable,
    /// Per-class unarmed base table (`base/unarmed_data.json`, sourced from
    /// PoB2's `data.unarmedWeaponData`). The weaponData source for attack
    /// skills when there's no main-hand weapon (consumed by pobr-build's
    /// `unarmed_contribution`).
    pub unarmed_data: UnarmedDataTable,
    /// Weapon type table (`base/weapon_types.json`, sourced from PoB2's
    /// `data.weaponTypeInfo`). The lookup source for weapon-grip/melee
    /// conditions and weapon-class damage keyword checks (consumed by
    /// pobr-build; the key space is the PoB base `type` name — mapping
    /// GGG's `item_class` to a table key is the consumer's job).
    pub weapon_types: WeaponTypeTable,
}

impl RuntimeConstants {
    /// The mechanic-formula magic-numbers section (resistance boundary /
    /// armour coefficient / server tick / ailment baseline / various caps).
    pub fn game(&self) -> &GameMechanicsConstantsDef {
        &self.game_constants.game
    }

    /// The player-inherent constants section (default max resistance /
    /// player base crit damage bonus, etc.).
    pub fn character(&self) -> &CharacterConstantsDef {
        &self.game_constants.character
    }

    /// The monster-inherent constants section (monster max resistance /
    /// monster base crit damage bonus, etc.).
    pub fn monster(&self) -> &MonsterConstantsDef {
        &self.game_constants.monster
    }
}

#[cfg(test)]
mod tests {
    use super::RuntimeConstants;

    /// Fallback-invariant spot check: `Default` is value-equal to the old
    /// Rust source of truth (the full Default == JSON comparison lives in
    /// `pobr-gamedata/tests/load_game_constants.rs`).
    #[test]
    fn default_matches_legacy_rust_constants() {
        let c = RuntimeConstants::default();
        assert_eq!(
            c.character().base_maximum_all_resistances_pct,
            crate::constants::DEFAULT_MAX_RESISTANCE
        );
        assert_eq!(
            c.game().resist_hard_cap,
            crate::constants::HARD_MAX_RESISTANCE
        );
        assert_eq!(c.game().resist_floor, crate::constants::RESIST_FLOOR);
        assert_eq!(c.game().armour_ratio, crate::constants::ARMOUR_RATIO);
        assert_eq!(
            c.game().server_tick_seconds,
            crate::constants::SERVER_TICK_SECONDS
        );
        assert_eq!(
            c.game().block_chance_cap,
            crate::constants::BLOCK_CHANCE_CAP
        );
        assert_eq!(c.game().dot_dps_cap, crate::constants::DOT_DPS_CAP);
        assert_eq!(
            c.game().shock_min_effect,
            crate::constants::SHOCK_MIN_EFFECT
        );
        assert_eq!(
            c.character().base_critical_hit_damage_bonus,
            crate::constants::PLAYER_BASE_CRIT_DAMAGE_BONUS
        );
        let legacy = crate::constants::GameConstants::poe2();
        assert_eq!(c.game().bleed_base_fraction, legacy.bleed_base_fraction);
        assert_eq!(c.game().bleed_base_duration, legacy.bleed_base_duration);
        assert_eq!(c.game().ignite_base_fraction, legacy.ignite_base_fraction);
        assert_eq!(c.game().ignite_base_duration, legacy.ignite_base_duration);
        assert_eq!(c.game().poison_base_fraction, legacy.poison_base_fraction);
        assert_eq!(c.game().poison_base_duration, legacy.poison_base_duration);
        assert_eq!(c.game().shock_default_effect, legacy.shock_default_effect);
    }

    /// Fallback-invariant spot check (monster-scaling domain): the per-level
    /// table / tier presets' `Default` is value-equal to the old Rust
    /// source of truth (the full comparison lives in each catalog module's
    /// own tests plus `pobr-gamedata/tests/load_monster_scaling.rs` /
    /// `load_enemy_presets.rs`).
    #[test]
    fn default_monster_domains_match_legacy_rust_sources() {
        let c = RuntimeConstants::default();
        assert_eq!(
            c.monster_scaling.accuracy_at(85),
            crate::monster::monster_accuracy(85)
        );
        assert_eq!(
            c.monster_scaling.damage_at(85),
            crate::monster::monster_damage(85)
        );
        assert_eq!(
            c.enemy_presets.max_enemy_level,
            crate::monster::MAX_ENEMY_LEVEL
        );
        let pinnacle = c
            .enemy_presets
            .tier_for(crate::monster::EnemyTier::Pinnacle)
            .expect("Pinnacle tier exists");
        assert_eq!(
            pinnacle.armour_mult_pct.value(),
            crate::monster::EnemyTier::Pinnacle.armour_mult_pct()
        );
    }
}
