//! `BuildConfig`: Build-level calculation context configuration, produces a [`CalcConfig`].
//!
//! `pobr-data::build_config` only holds language-independent, logic-free stable enums
//! ([`ViewMode`] / [`BanditChoice`]). The version with logic and a `pobr-core`
//! dependency lives here, adapted to the REAL [`CalcConfig`] via
//! [`BuildConfig::to_calc_config`].

use std::collections::HashMap;

use pobr_core::rules::config_interpreter::RawConfigInputs;
use pobr_core::{CalcConfig, CampaignProgress};
use pobr_data::build_config::BanditChoice;
use pobr_data::monster::EnemyTier;
use pobr_data::prelude::{DamageType, ModFlags, SkillTypes};

/// Build-level configuration corresponding to PoB's "Configuration" panel.
///
/// The fields are a stable subset of the calculation context: whether the main skill is
/// attack/spell, the target version, bandit choice, plus arbitrary condition /
/// multiplier overrides (a 1:1 match to PoB ConfigOptions.lua's toggles, but keyed by
/// stable keys here, with display text going through i18n).
#[derive(Debug, Clone, Default)]
pub struct BuildConfig {
    /// Whether the main skill is an attack (drives `ModFlags::ATTACK` / `SkillTypes::ATTACK`).
    pub is_attack: bool,
    /// Whether the main skill is a spell (drives `ModFlags::SPELL`).
    pub is_spell: bool,
    /// Main skill's primary damage type (if known).
    pub damage_type: Option<DamageType>,
    /// Bandit quest reward choice.
    pub bandit: BanditChoice,
    /// Boolean condition overrides (stable key, e.g. `"UseFrenzyCharges"`).
    pub conditions: HashMap<String, bool>,
    /// Numeric multiplier overrides (stable key, e.g. `"PowerCharge"`).
    pub multipliers: HashMap<String, f64>,
    /// Quest reward / global config `<Input string="...">` mod text (PoB2
    /// `questRewards` etc.), injected as **global** modifiers (e.g.
    /// `15% increased Global Armour, Evasion and Energy Shield`).
    pub global_modifier_texts: Vec<String>,
    /// Campaign progress (PoB2 Config `resistancePenalty` tier, determines elemental
    /// resistance penalty 0/-10/…/-60). `None` = not explicitly set in the XML; calc
    /// falls back to PoB2's default `configInput.resistancePenalty or -60` (i.e.
    /// [`CampaignProgress::Endgame`]).
    pub campaign_progress: Option<CampaignProgress>,
    /// Enemy tier (PoB2 Config `enemyIsBoss`, four tiers None/Boss/Pinnacle/Uber).
    /// `None` = not explicitly set in the XML; calc falls back to the orchestrator
    /// options' tier (PoB2 `defaultIndex = 3`, i.e. defaults to Pinnacle, matching
    /// [`EnemyTier::default`]).
    pub enemy_tier: Option<EnemyTier>,
    /// Raw `<Config>` `<Input name bool|number|string>` key-values (primary path):
    /// `parse_build` captures these losslessly via `parse_config_inputs`; when a
    /// `ConfigCatalog` is available the orchestrator consumes them through the
    /// `config_interpreter::interpret` primary path (see `crate::config_resolve`).
    /// When the catalog is missing (old data pack / `BuildData::empty`), it falls back
    /// to the legacy parse_config output carried by this struct's other fields (tolerant
    /// of a missing catalog).
    pub raw_inputs: RawConfigInputs,
}

impl BuildConfig {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_attack(mut self, is_attack: bool) -> Self {
        self.is_attack = is_attack;
        self
    }

    pub fn with_spell(mut self, is_spell: bool) -> Self {
        self.is_spell = is_spell;
        self
    }

    pub fn with_damage_type(mut self, damage_type: DamageType) -> Self {
        self.damage_type = Some(damage_type);
        self
    }

    pub fn with_bandit(mut self, bandit: BanditChoice) -> Self {
        self.bandit = bandit;
        self
    }

    pub fn with_condition(mut self, key: impl Into<String>, enabled: bool) -> Self {
        self.conditions.insert(key.into(), enabled);
        self
    }

    pub fn with_multiplier(mut self, key: impl Into<String>, value: f64) -> Self {
        self.multipliers.insert(key.into(), value);
        self
    }

    /// Sets the campaign progress (elemental resistance penalty tier).
    pub fn with_campaign_progress(mut self, progress: CampaignProgress) -> Self {
        self.campaign_progress = Some(progress);
        self
    }

    /// Sets the enemy tier (`enemyIsBoss`).
    pub fn with_enemy_tier(mut self, tier: EnemyTier) -> Self {
        self.enemy_tier = Some(tier);
        self
    }

    /// Adapts to the REAL [`CalcConfig`].
    ///
    /// Translates Build-level toggles into the calc engine's flags / skill_types /
    /// damage_type / conditions / multipliers. The attack and spell flags aren't
    /// mutually exclusive (some hybrid skills carry both).
    pub fn to_calc_config(&self) -> CalcConfig {
        let mut flags = ModFlags::NONE;
        let mut skill_types = SkillTypes::NONE;

        if self.is_attack {
            flags |= ModFlags::ATTACK;
            skill_types = SkillTypes::ATTACK;
        }
        if self.is_spell {
            flags |= ModFlags::SPELL;
        }

        let mut cfg = CalcConfig::new()
            .with_flags(flags)
            .with_skill_types(skill_types);

        if let Some(dt) = self.damage_type {
            cfg = cfg.with_damage_type(dt);
        }

        for (key, &enabled) in &self.conditions {
            cfg = cfg.with_condition(key.clone(), enabled);
        }
        for (key, &value) in &self.multipliers {
            cfg = cfg.with_multiplier(key.clone(), value);
        }

        cfg
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attack_config_sets_attack_flag() {
        let cfg = BuildConfig::new().with_attack(true).to_calc_config();
        assert!(cfg.flags.intersects(ModFlags::ATTACK));
        assert!(cfg.skill_types.intersects(SkillTypes::ATTACK));
    }

    #[test]
    fn spell_config_sets_spell_flag() {
        let cfg = BuildConfig::new().with_spell(true).to_calc_config();
        assert!(cfg.flags.intersects(ModFlags::SPELL));
    }

    #[test]
    fn conditions_and_multipliers_pass_through() {
        let cfg = BuildConfig::new()
            .with_condition("UseFrenzyCharges", true)
            .with_multiplier("PowerCharge", 3.0)
            .to_calc_config();
        assert!(cfg.condition("UseFrenzyCharges"));
        assert_eq!(cfg.multiplier("PowerCharge"), 3.0);
    }

    #[test]
    fn damage_type_propagates() {
        let cfg = BuildConfig::new()
            .with_damage_type(DamageType::Fire)
            .to_calc_config();
        assert_eq!(cfg.damage_type, Some(DamageType::Fire));
    }
}
