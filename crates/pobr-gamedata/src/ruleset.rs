//! RuleSet aggregation entry point.
//!
//! `GameData::load_ruleset()` loads every rule/constant domain the calc
//! engine needs in one shot; pobr-build merges them into
//! [`pobr_data::catalog::RuntimeConstants`] and injects it into pobr-core
//! (via `CalculationSession::set_constants` → `CalcConfig.constants`,
//! keeping pobr-core zero-I/O).
//!
//! Fields are `Option`: `None` means that domain hasn't been data-driven
//! yet, or the data directory is missing that file — the consumer
//! (pobr-build's `BuildData::load`) falls back to `Default` for `None`
//! (the fallback is value-equal to the JSON, a migration invariant). Each
//! phase-2 domain agent appends its own loading here, domain by domain.

use std::collections::HashMap;

use pobr_data::catalog::character_constants::CharacterConstantsDef;
use pobr_data::catalog::config_def::ConfigOptionDef;
use pobr_data::catalog::enemy_presets::EnemyPresetsTable;
use pobr_data::catalog::game_constants::GameConstantsDef;
use pobr_data::catalog::high_precision_mods::HighPrecisionModsDef;
use pobr_data::catalog::jewel_radii::JewelRadiiDef;
use pobr_data::catalog::monster_scaling::MonsterScalingTable;
use pobr_data::catalog::parser_rules::SpecialTemplateDef;
use pobr_data::catalog::unarmed_data::UnarmedDataTable;
use pobr_data::catalog::weapon_types::WeaponTypeTable;

use crate::{GameData, LoadError};

/// A placeholder for parser rules (to be filled in from
/// `overlay/mod_parser_rules.json` etc.; the real type lives in
/// `pobr_data::catalog`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ParserRules;

/// The config options catalog (`overlay/config_options.json`).
///
/// `options` keeps the file's order (ascending by `var`, byte-stable);
/// `by_var` is a var → index map, for `config_interpreter` consumers to
/// look up an entry directly by the XML `<Input name>`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ConfigCatalog {
    /// All entries (schema in [`pobr_data::catalog::config_def`]).
    pub options: Vec<ConfigOptionDef>,
    /// `var` → index into [`Self::options`].
    pub by_var: HashMap<String, usize>,
}

impl ConfigCatalog {
    /// Builds from an entry list (a duplicate `var` has the later one win
    /// — the overlay's output has unique vars, so this branch shouldn't
    /// actually be reachable; taking the later write is a conservative
    /// choice consistent with the loader's general error tolerance).
    pub fn new(options: Vec<ConfigOptionDef>) -> Self {
        let by_var = options
            .iter()
            .enumerate()
            .map(|(index, option)| (option.var.clone(), index))
            .collect();
        Self { options, by_var }
    }

    /// Looks up an entry by `var`.
    pub fn get(&self, var: &str) -> Option<&ConfigOptionDef> {
        self.by_var.get(var).map(|&index| &self.options[index])
    }

    /// The count of entries that didn't pass oracle reconciliation at
    /// extraction time (`verified:false`, listed separately in the parity
    /// report, for monitoring).
    pub fn unverified_count(&self) -> usize {
        self.options.iter().filter(|o| !o.verified).count()
    }
}

/// The rule/constant aggregate injected into the calc engine.
///
/// A `None` field means the corresponding domain hasn't been data-driven
/// yet (or the data directory is missing that file) — during the
/// transition, consumers fall back to `Default` (value-equal to the
/// hardcoded path).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RuleSet {
    /// Modifier-text parsing rules (forms/name_map/flag_phrases/tag_phrases…).
    pub parser_rules: Option<ParserRules>,
    /// Game constants consumed by mechanic formulas.
    pub game_constants: Option<GameConstantsDef>,
    /// Character level/attribute-derived constants.
    pub character_constants: Option<CharacterConstantsDef>,
    /// Radius-jewel ring bands.
    /// Consumed on the pobr-build side (tree geometry; pobr-core doesn't
    /// consume this domain, so it's not part of `RuntimeConstants`).
    pub jewel_radii: Option<JewelRadiiDef>,
    /// Monster per-level scaling table.
    pub monster_scaling: Option<MonsterScalingTable>,
    /// Enemy tier presets.
    pub enemy_presets: Option<EnemyPresetsTable>,
    /// Per-class unarmed base table.
    pub unarmed_data: Option<UnarmedDataTable>,
    /// Weapon type table (grip/melee condition checks and weapon-class
    /// damage keyword lookup).
    pub weapon_types: Option<WeaponTypeTable>,
    /// Config options catalog (declarative effects + imply_conditions).
    pub config_catalog: Option<ConfigCatalog>,
    /// Rounding-precision exception table (ScaleAddMod / MORE aggregation
    /// precision, wired up; consumed by pobr-core's `HighPrecisionRules`).
    pub high_precision_mods: Option<HighPrecisionModsDef>,
    /// Special mod-line templates (the concatenation of
    /// `overlay/special_mods.json` + `generated/special_derived.json`,
    /// wired into the data plane). Consumer (the orchestrator): compiles
    /// once via `SpecialModRules::compile`, then does a whole-line lookup
    /// through `parse_mod_with_rules`; a missing table → `None` (the
    /// consumer falls back to pure generic parsing, unchanged behavior).
    /// The two tables' entries are concatenated in order; an id collision
    /// is a fail-fast at compile time.
    pub special_mods: Option<Vec<SpecialTemplateDef>>,
}

impl GameData {
    /// Loads the RuleSet aggregate: loads each already-data-driven JSON
    /// domain (`game_constants`) one by one.
    ///
    /// A missing file (`LoadError::Io`, e.g. an old data pack/test
    /// directory) is treated as "this domain isn't data-driven yet" and
    /// returns `None` (the consumer falls back to `Default`); a JSON parse
    /// error (`LoadError::Parse`) propagates upward instead — a file that
    /// exists but is broken isn't allowed to silently fall back.
    pub fn load_ruleset(&self) -> Result<RuleSet, LoadError> {
        let game_constants = match self.game_constants() {
            Ok(v) => Some(v),
            Err(LoadError::Io { .. }) => None,
            Err(e) => return Err(e),
        };
        let character_constants = match self.character_constants() {
            Ok(v) => Some(v),
            Err(LoadError::Io { .. }) => None,
            Err(e) => return Err(e),
        };
        let jewel_radii = match self.jewel_radii() {
            Ok(v) => Some(v),
            Err(LoadError::Io { .. }) => None,
            Err(e) => return Err(e),
        };
        let monster_scaling = match self.monster_scaling() {
            Ok(v) => Some(v),
            Err(LoadError::Io { .. }) => None,
            Err(e) => return Err(e),
        };
        let enemy_presets = match self.enemy_presets() {
            Ok(v) => Some(v),
            Err(LoadError::Io { .. }) => None,
            Err(e) => return Err(e),
        };
        // The domain loader returns a Vec (W2's convention); wrapped here
        // into the injection-table newtype.
        let unarmed_data = match self.unarmed_data() {
            Ok(v) => Some(UnarmedDataTable(v)),
            Err(LoadError::Io { .. }) => None,
            Err(e) => return Err(e),
        };
        let weapon_types = match self.weapon_types() {
            Ok(v) => Some(WeaponTypeTable(v)),
            Err(LoadError::Io { .. }) => None,
            Err(e) => return Err(e),
        };
        // Missing-table tolerance: table absent → None, the consumer falls
        // back to the old parse_config path.
        let config_catalog = match self.config_options() {
            Ok(v) => Some(ConfigCatalog::new(v.options)),
            Err(LoadError::Io { .. }) => None,
            Err(e) => return Err(e),
        };
        let high_precision_mods = match self.high_precision_mods() {
            Ok(v) => Some(v),
            Err(LoadError::Io { .. }) => None,
            Err(e) => return Err(e),
        };
        // Special mod-line templates: overlay (hand-curated, takes
        // priority) + generated (keystone-derived) + generated (batch
        // vendor extraction V0), concatenated from three sources. All
        // absent → None (the consumer falls back to pure generic parsing);
        // if any is present, concatenate entries in this order (an id
        // collision is caught fail-fast by the consumer's
        // `SpecialModRules::compile`, not deduplicated at load time — the
        // vendor batch was already deduplicated against the first two
        // sources by vendor_pattern/pattern at extraction time).
        let special_overlay = self.special_mods()?;
        let special_derived = self.special_derived()?;
        let special_vendor = self.special_vendor()?;
        let special_mods =
            if special_overlay.is_none() && special_derived.is_none() && special_vendor.is_none() {
                None
            } else {
                let mut entries = special_overlay.map(|d| d.entries).unwrap_or_default();
                entries.extend(special_derived.map(|d| d.entries).unwrap_or_default());
                entries.extend(special_vendor.map(|d| d.entries).unwrap_or_default());
                Some(entries)
            };
        Ok(RuleSet {
            parser_rules: None,
            game_constants,
            character_constants,
            jewel_radii,
            monster_scaling,
            enemy_presets,
            unarmed_data,
            weapon_types,
            config_catalog,
            high_precision_mods,
            special_mods,
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::GameData;

    /// When the data directory doesn't exist: load_ruleset still succeeds,
    /// with every domain unfilled (None) → the consumer falls back to Default.
    #[test]
    fn missing_dir_ruleset_loads_with_all_domains_unfilled() {
        let ruleset = GameData::new("nonexistent-dir").load_ruleset().unwrap();
        assert!(ruleset.parser_rules.is_none());
        assert!(ruleset.game_constants.is_none());
        assert!(ruleset.character_constants.is_none());
        assert!(ruleset.jewel_radii.is_none());
        assert!(ruleset.monster_scaling.is_none());
        assert!(ruleset.enemy_presets.is_none());
        assert!(ruleset.unarmed_data.is_none());
        assert!(ruleset.weapon_types.is_none());
        assert!(ruleset.config_catalog.is_none());
        assert!(ruleset.high_precision_mods.is_none());
        assert!(ruleset.special_mods.is_none());
    }

    /// The repo data directory: the special_mods domain is wired up (the
    /// overlay's curated entries plus generated's keystone-derived ones
    /// concatenated; once the consumer activates, downgraded shadow
    /// entries get filtered out). Only asserts non-empty and count ≥ the
    /// overlay's base count — the exact count grows with the curation
    /// batches, so it's not pinned.
    #[test]
    fn repo_data_ruleset_loads_special_mods() {
        let data = GameData::new(crate::current_data_dir());
        let ruleset = data.load_ruleset().unwrap();
        let entries = ruleset
            .special_mods
            .expect("special_mods domain should be wired up");
        assert!(
            entries.len() >= 60,
            "special entry count {} should be ≥ the overlay's first-batch baseline",
            entries.len()
        );
    }

    /// The repo data directory: the high_precision_mods domain is wired up
    /// (`overlay/high_precision_mods.json` ← vendor Data.lua:413-530).
    #[test]
    fn repo_data_ruleset_loads_high_precision_mods() {
        let data = GameData::new(crate::current_data_dir());
        let ruleset = data.load_ruleset().unwrap();
        let loaded = ruleset
            .high_precision_mods
            .expect("high_precision_mods domain should be wired up");
        assert_eq!(loaded.default_high_precision, 1);
        assert_eq!(
            loaded.mods.len(),
            38,
            "vendor highPrecisionMods has 38 entries"
        );
    }

    /// The repo data directory: the game_constants domain is wired up
    /// (Some), and value-equal to the Default fallback (a migration
    /// invariant: both the injected and fallback paths produce the same output).
    #[test]
    fn repo_data_ruleset_loads_game_constants_equal_to_default() {
        let data = GameData::new(crate::current_data_dir());
        let ruleset = data.load_ruleset().unwrap();
        let loaded = ruleset
            .game_constants
            .expect("game_constants domain should be wired up");
        assert_eq!(
            loaded,
            pobr_data::catalog::game_constants::GameConstantsDef::default(),
            "JSON must equal the Default fallback value-for-value (migration invariant)",
        );
    }

    /// The repo data directory: the character_constants domain is wired up
    /// (Some), and value-equal to the Default fallback (a migration
    /// invariant: both the injected and fallback paths produce the same output).
    #[test]
    fn repo_data_ruleset_loads_character_constants_equal_to_default() {
        let data = GameData::new(crate::current_data_dir());
        let ruleset = data.load_ruleset().unwrap();
        let loaded = ruleset
            .character_constants
            .expect("character_constants domain should be wired up");
        assert_eq!(
            loaded,
            pobr_data::catalog::character_constants::CharacterConstantsDef::default(),
            "JSON must equal the Default fallback value-for-value (migration invariant)",
        );
    }

    /// The repo data directory: the jewel_radii domain is wired up (Some),
    /// and value-equal to the Default fallback (a migration invariant:
    /// both the injected and fallback paths produce the same output).
    #[test]
    fn repo_data_ruleset_loads_jewel_radii_equal_to_default() {
        let data = GameData::new(crate::current_data_dir());
        let ruleset = data.load_ruleset().unwrap();
        let loaded = ruleset
            .jewel_radii
            .expect("jewel_radii domain should be wired up");
        assert_eq!(
            loaded,
            pobr_data::catalog::jewel_radii::JewelRadiiDef::default(),
            "JSON must equal the Default fallback value-for-value (migration invariant)",
        );
    }

    /// The repo data directory: the monster_scaling domain is wired up
    /// (Some), and the **whole table** is value-equal to the Default
    /// fallback (including the two vendor-only ally tables; a migration
    /// invariant: both the injected/fallback paths produce the same output).
    #[test]
    fn repo_data_ruleset_loads_monster_scaling_equal_to_default() {
        let data = GameData::new(crate::current_data_dir());
        let ruleset = data.load_ruleset().unwrap();
        let loaded = ruleset
            .monster_scaling
            .expect("monster_scaling domain should be wired up");
        assert_eq!(
            loaded,
            pobr_data::catalog::monster_scaling::MonsterScalingTable::default(),
            "JSON must equal the Default fallback value-for-value (migration invariant)",
        );
    }

    /// The repo data directory: the unarmed_data domain is wired up
    /// (Some), and the **whole table** is value-equal to the Default
    /// fallback (including the vendor-only class_id/weapon_type fields; a
    /// migration invariant).
    #[test]
    fn repo_data_ruleset_loads_unarmed_data_equal_to_default() {
        let data = GameData::new(crate::current_data_dir());
        let ruleset = data.load_ruleset().unwrap();
        let loaded = ruleset
            .unarmed_data
            .expect("unarmed_data domain should be wired up");
        assert_eq!(
            loaded,
            pobr_data::catalog::unarmed_data::UnarmedDataTable::default(),
            "JSON must equal the Default fallback value-for-value (migration invariant)",
        );
    }

    /// The repo data directory: the weapon_types domain is wired up
    /// (Some), and the **whole table** is value-equal to the Default
    /// fallback (19 entries of vendor weaponTypeInfo including label; a
    /// migration invariant).
    #[test]
    fn repo_data_ruleset_loads_weapon_types_equal_to_default() {
        let data = GameData::new(crate::current_data_dir());
        let ruleset = data.load_ruleset().unwrap();
        let loaded = ruleset
            .weapon_types
            .expect("weapon_types domain should be wired up");
        assert_eq!(
            loaded,
            pobr_data::catalog::weapon_types::WeaponTypeTable::default(),
            "JSON must equal the Default fallback value-for-value (migration invariant)",
        );
    }

    /// The repo data directory: the config_catalog domain is wired up —
    /// the entry count covers vendor's 542+, `by_var` indices correspond
    /// 1:1 with entries, a typical entry is look-up-able; and prints the
    /// count of `verified:false` entries
    #[test]
    fn repo_data_ruleset_loads_config_catalog() {
        let data = GameData::new(crate::current_data_dir());
        let ruleset = data.load_ruleset().unwrap();
        let catalog = ruleset
            .config_catalog
            .expect("config_catalog domain should be wired up");
        assert!(
            catalog.options.len() >= 542,
            "config entry count {} is below vendor's static entry count of 542",
            catalog.options.len()
        );
        // Vendor's ConfigOptions.lua has one place with a shared var
        // (conditionEnemyExitedPresenceRecently's var carries both the
        // Exited and Entered entries — vendor itself reuses the same input
        // key) — by_var converges to a unique index by taking the later write.
        let unique_vars: std::collections::BTreeSet<&str> =
            catalog.options.iter().map(|o| o.var.as_str()).collect();
        assert_eq!(
            catalog.by_var.len(),
            unique_vars.len(),
            "by_var indices must correspond 1:1 with the deduplicated var set"
        );
        for (var, &index) in &catalog.by_var {
            assert_eq!(
                &catalog.options[index].var, var,
                "by_var index points at the wrong entry"
            );
        }
        let stationary = catalog
            .get("conditionStationary")
            .expect("conditionStationary entry exists");
        assert_eq!(
            stationary.input_type,
            pobr_data::catalog::config_def::ConfigInputType::Count
        );
        assert!(catalog.get("nonexistent_var").is_none());

        // A6 monitoring: the count of verified:false entries (used as-is at
        // runtime, listed separately in the parity report).
        let unverified = catalog.unverified_count();
        println!(
            "config_catalog verified:false entry count = {unverified} / {}",
            catalog.options.len()
        );
        assert!(
            unverified <= 54,
            "verified:false entry count {unverified} exceeds the handler budget of 54 (a DSL-splitting failure signal)"
        );
    }

    /// The repo data directory: the enemy_presets domain is wired up
    /// (Some), and the **whole table** is value-equal to the Default
    /// fallback (including vendor-only mod groups/placeholder columns; a
    /// migration invariant).
    #[test]
    fn repo_data_ruleset_loads_enemy_presets_equal_to_default() {
        let data = GameData::new(crate::current_data_dir());
        let ruleset = data.load_ruleset().unwrap();
        let loaded = ruleset
            .enemy_presets
            .expect("enemy_presets domain should be wired up");
        assert_eq!(
            loaded,
            pobr_data::catalog::enemy_presets::EnemyPresetsTable::default(),
            "JSON must equal the Default fallback value-for-value (migration invariant)",
        );
    }
}
