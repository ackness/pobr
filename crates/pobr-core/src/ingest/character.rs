//! Modifier ingest for PoE2 character base values.
//!
//! Converts the innate base values derived from class level and attributes
//! (life / mana / accuracy) into `BASE` modifiers attributed with
//! [`SourceKind::CharacterBase`], which then feed into `ModDb` and participate
//! in the standard stat pipeline `(base + Σbase) * (1 + Σinc/100) * Π(1 + more/100)`.
//!
//! Formula source: PoB2 `CalcSetup.lua` (`data.characterConstants`, ModStore
//! Multiplier semantics `value × Level + base`; oracle-verified at L99: Life
//! base 1204 = 12×99+16, Mana base 426 = 4×99+30).
//!
//! Constant injection: the derivation formulas' **numeric coefficients** are
//! read from the injected [`CharacterConstantsDef`]
//! (`base/character_constants.json` → `RuntimeConstants.character_constants`),
//! while the formula logic stays in this module; the caller (pobr-build
//! orchestrator) pulls it from `BuildData.constants` and passes it in. Paths
//! without GameData pass `CharacterConstantsDef::default()` (value-equal to
//! the JSON, so behavior is unchanged).

use pobr_data::catalog::character_constants::CharacterConstantsDef;
use pobr_data::prelude::*;

use crate::Modifier;

/// Fallback anchor for the source of truth (a migration invariant, now downgraded to test-only use).
///
/// The values have been moved out to `data/<version>/base/character_constants.json`
/// (schema = `pobr_data::catalog::character_constants::CharacterConstantsDef`),
/// whose `Default` is value-equal to this set of constants (locked by this
/// file's tests; pobr-data's dependency direction disallows referencing back,
/// so `Default` is hardcoded as literals and anchored by the test here).
/// **Do not add new calc-path consumers** — derivation formulas must always
/// read the injected `CharacterConstantsDef`.
#[allow(dead_code)] // Only consumed by the cfg(test) lock test; no reference from the lib target (kept intentionally).
mod legacy_anchor {
    /// Character's innate base life constant (PoB2 `Life BASE 12 × Level + 16`).
    pub(super) const BASE_LIFE_CONSTANT: f64 = 16.0;
    /// Max life granted per player level.
    pub(super) const LIFE_PER_LEVEL: f64 = 12.0;
    /// Max life granted per point of Strength.
    pub(super) const LIFE_PER_STRENGTH: f64 = 2.0;

    /// Character's innate base mana constant (PoB2 `Mana BASE 4 × Level + 30`).
    pub(super) const BASE_MANA_CONSTANT: f64 = 30.0;
    /// Max mana granted per player level.
    pub(super) const MANA_PER_LEVEL: f64 = 4.0;
    /// Max mana granted per point of Intelligence.
    pub(super) const MANA_PER_INTELLIGENCE: f64 = 2.0;

    /// Character's innate accuracy constant (PoB2 `Accuracy BASE 6 × Level − 6`).
    pub(super) const BASE_ACCURACY_CONSTANT: f64 = -6.0;
    /// Accuracy granted per player level.
    pub(super) const ACCURACY_PER_LEVEL: f64 = 6.0;
    /// Accuracy granted per point of Dexterity.
    pub(super) const ACCURACY_PER_DEXTERITY: f64 = 6.0;

    /// Character's innate base evasion (PoB2 `characterConstants.base_evasion_rating`).
    pub(super) const BASE_EVASION: f64 = 7.0;
}

/// PoE2 character base value entry point.
///
/// Attributes should be passed in as totals (class starting values + tree +
/// equipment, etc.), determined by the caller before modifier aggregation;
/// this entry point only turns the currently known innate derived values into
/// modifiers.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CharacterBase {
    pub level: u32,
    pub strength: f64,
    pub dexterity: f64,
    pub intelligence: f64,
}

impl CharacterBase {
    fn level(&self) -> f64 {
        f64::from(self.level)
    }

    /// Derived innate maximum life: `life_per_level*level + base_life_constant +
    /// life_per_strength*Strength` (default `12*level + 16 + 2*Str`).
    pub fn base_life(&self, constants: &CharacterConstantsDef) -> f64 {
        constants.base_life_constant
            + constants.life_per_level * self.level()
            + constants.life_per_strength * self.strength
    }

    /// Derived innate maximum mana: `mana_per_level*level + base_mana_constant +
    /// mana_per_intelligence*Intelligence` (default `4*level + 30 + 2*Int`).
    pub fn base_mana(&self, constants: &CharacterConstantsDef) -> f64 {
        constants.base_mana_constant
            + constants.mana_per_level * self.level()
            + constants.mana_per_intelligence * self.intelligence
    }

    /// Derived innate accuracy: `accuracy_per_level*level + base_accuracy_constant +
    /// accuracy_per_dexterity*Dexterity` (default `6*level − 6 + 6*Dex`).
    pub fn base_accuracy(&self, constants: &CharacterConstantsDef) -> f64 {
        constants.base_accuracy_constant
            + constants.accuracy_per_level * self.level()
            + constants.accuracy_per_dexterity * self.dexterity
    }

    /// Generates the list of `BASE` modifiers for character base values, all attributed to `CharacterBase`.
    pub fn modifiers(&self, constants: &CharacterConstantsDef) -> Vec<Modifier> {
        vec![
            base_modifier(
                "MaximumLife",
                self.base_life(constants),
                "character base maximum life",
            ),
            base_modifier(
                "MaximumMana",
                self.base_mana(constants),
                "character base maximum mana",
            ),
            base_modifier(
                "Accuracy",
                self.base_accuracy(constants),
                "character base accuracy rating",
            ),
            base_modifier(
                "Evasion",
                constants.base_evasion,
                "character base evasion rating",
            ),
        ]
    }
}

fn base_modifier(stat: &str, value: f64, label: &str) -> Modifier {
    let origin = ModifierSource::new(SourceId::new(
        SourceKind::CharacterBase,
        format!("base.{stat}"),
    ))
    .with_raw_text(label);
    Modifier::number(stat, ModType::Base, value).with_origin(origin)
}

#[cfg(test)]
mod tests {
    use super::legacy_anchor::*;
    use super::*;

    /// Migration invariant lock: the injected domain's `Default` fallback must
    /// be value-equal to this module's source-of-truth constants (pobr-data's
    /// dependency direction can't reference the source of truth back, so this
    /// test is what locks it — changing the source of truth fails this test).
    #[test]
    fn default_constants_match_legacy_character_source() {
        let c = CharacterConstantsDef::default();
        assert_eq!(c.base_life_constant, BASE_LIFE_CONSTANT);
        assert_eq!(c.life_per_level, LIFE_PER_LEVEL);
        assert_eq!(c.life_per_strength, LIFE_PER_STRENGTH);
        assert_eq!(c.base_mana_constant, BASE_MANA_CONSTANT);
        assert_eq!(c.mana_per_level, MANA_PER_LEVEL);
        assert_eq!(c.mana_per_intelligence, MANA_PER_INTELLIGENCE);
        assert_eq!(c.base_accuracy_constant, BASE_ACCURACY_CONSTANT);
        assert_eq!(c.accuracy_per_level, ACCURACY_PER_LEVEL);
        assert_eq!(c.accuracy_per_dexterity, ACCURACY_PER_DEXTERITY);
        assert_eq!(c.base_evasion, BASE_EVASION);
    }
}
