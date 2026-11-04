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
            // CalcSetup.lua:964 `WardRegenPercent BASE BaseWardRegenerationPercentPerMinute / 60`.
            base_modifier(
                "WardRegenPercent",
                constants.ward_regeneration_per_minute_pct / 60.0,
                "character base runic ward regeneration",
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
