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
//! (pobr-build's `BuildData::load`) uses compatibility defaults for `None`. Values
//! may differ from those defaults as the game changes.

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
/// transition, consumers fall back to compatibility defaults.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RuleSet {
    /// Nonfatal data limitations for the caller to surface.
    pub degraded: Vec<String>,
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
    /// An invalid snapshot manifest is rejected before aggregation.
    pub fn load_ruleset(&self) -> Result<RuleSet, LoadError> {
        self.snapshot_manifest()?;
        // The rules name skill types; install the pack's names before anyone compiles them.
        let degraded = self.skill_type_diagnostics()?;
        let game_constants = match self.game_constants() {
            Ok(v) => Some(v),
            Err(LoadError::Io { source, .. }) if source.kind() == std::io::ErrorKind::NotFound => {
                None
            }
            Err(e) => return Err(e),
        };
        let character_constants = match self.character_constants() {
            Ok(v) => Some(v),
            Err(LoadError::Io { source, .. }) if source.kind() == std::io::ErrorKind::NotFound => {
                None
            }
            Err(e) => return Err(e),
        };
        let jewel_radii = match self.jewel_radii() {
            Ok(v) => Some(v),
            Err(LoadError::Io { source, .. }) if source.kind() == std::io::ErrorKind::NotFound => {
                None
            }
            Err(e) => return Err(e),
        };
        let monster_scaling = match self.monster_scaling() {
            Ok(v) => Some(v),
            Err(LoadError::Io { source, .. }) if source.kind() == std::io::ErrorKind::NotFound => {
                None
            }
            Err(e) => return Err(e),
        };
        let enemy_presets = match self.enemy_presets() {
            Ok(v) => Some(v),
            Err(LoadError::Io { source, .. }) if source.kind() == std::io::ErrorKind::NotFound => {
                None
            }
            Err(e) => return Err(e),
        };
        // The domain loader returns a Vec (W2's convention); wrapped here
        // into the injection-table newtype.
        let unarmed_data = match self.unarmed_data() {
            Ok(v) => Some(UnarmedDataTable(v)),
            Err(LoadError::Io { source, .. }) if source.kind() == std::io::ErrorKind::NotFound => {
                None
            }
            Err(e) => return Err(e),
        };
        let weapon_types = match self.weapon_types() {
            Ok(v) => Some(WeaponTypeTable(v)),
            Err(LoadError::Io { source, .. }) if source.kind() == std::io::ErrorKind::NotFound => {
                None
            }
            Err(e) => return Err(e),
        };
        // Missing-table tolerance: table absent → None, the consumer falls
        // back to the old parse_config path.
        let config_catalog = match self.config_options() {
            Ok(v) => Some(ConfigCatalog::new(v.options)),
            Err(LoadError::Io { source, .. }) if source.kind() == std::io::ErrorKind::NotFound => {
                None
            }
            Err(e) => return Err(e),
        };
        let high_precision_mods = match self.high_precision_mods() {
            Ok(v) => Some(v),
            Err(LoadError::Io { source, .. }) if source.kind() == std::io::ErrorKind::NotFound => {
                None
            }
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
            degraded,
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

    /// A version directory holding only the current pack's skill type table.
    fn table_only(root: &std::path::Path) {
        let _ = std::fs::remove_dir_all(root);
        std::fs::create_dir_all(root.join("base")).unwrap();
        std::fs::copy(
            crate::current_data_dir().join("base/active_skill_types.json"),
            root.join("base/active_skill_types.json"),
        )
        .unwrap();
    }

    /// A directory without the skill type table is refused: the rules name
    /// skill types and the engine has no copy of the table.
    #[test]
    fn missing_dir_ruleset_is_refused_for_its_skill_type_table() {
        let error = GameData::new("nonexistent-dir").load_ruleset().unwrap_err();
        assert!(
            error.to_string().contains(crate::MISSING_SKILL_TYPES),
            "{error}"
        );
    }

    /// With only the skill type table: load_ruleset succeeds with every
    /// other domain unfilled (None) → the consumer falls back to Default.
    #[test]
    fn a_table_only_ruleset_loads_with_all_domains_unfilled() {
        let root = std::env::temp_dir().join(format!("pobr-ruleset-empty-{}", std::process::id()));
        table_only(&root);
        let ruleset = GameData::new(&root).load_ruleset();
        std::fs::remove_dir_all(&root).unwrap();
        let ruleset = ruleset.unwrap();
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

    #[test]
    fn existing_unreadable_domain_does_not_use_legacy_fallback() {
        let root = std::env::temp_dir().join(format!("pobr-ruleset-io-{}", std::process::id()));
        table_only(&root);
        std::fs::create_dir_all(root.join("game_constants.json")).unwrap();
        let result = GameData::new(&root).load_ruleset();
        std::fs::remove_dir_all(root).unwrap();
        assert!(matches!(result, Err(crate::LoadError::Io { .. })));
    }

    /// The precision exceptions are looked up by the engine's modifier
    /// names: Critical Hit Chance is `CriticalStrikeChance` (PoB2's
    /// `CritChance` matches nothing the engine stores).
    #[test]
    fn repo_high_precision_mods_use_engine_names() {
        let ruleset = GameData::new(crate::current_data_dir())
            .load_ruleset()
            .unwrap();
        let mods = ruleset
            .high_precision_mods
            .expect("high_precision_mods")
            .mods;
        assert_eq!(
            mods.get("CriticalStrikeChance")
                .and_then(|types| types.get("BASE")),
            Some(&2)
        );
        assert!(!mods.contains_key("CritChance"));
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

    /// Aggregation must preserve the values returned by each domain loader.
    /// Balance values belong to the package, independently of fallback defaults.
    #[test]
    fn ruleset_preserves_loaded_constant_domains() {
        let data = GameData::new(crate::current_data_dir());
        let ruleset = data.load_ruleset().unwrap();
        assert!(ruleset.degraded.is_empty(), "{:?}", ruleset.degraded);
        assert_eq!(ruleset.game_constants, Some(data.game_constants().unwrap()));
        assert_eq!(
            ruleset.character_constants,
            Some(data.character_constants().unwrap())
        );
        assert_eq!(ruleset.jewel_radii, Some(data.jewel_radii().unwrap()));
        assert_eq!(ruleset.enemy_presets, Some(data.enemy_presets().unwrap()));
        assert_eq!(
            ruleset.weapon_types,
            Some(pobr_data::catalog::weapon_types::WeaponTypeTable(
                data.weapon_types().unwrap()
            ))
        );
        // Current package values can equal defaults; a changed value must survive too.
        let mut changed = data.game_constants().unwrap();
        changed.game.armour_ratio += 1.0;
        let changed_data = GameData::from_memory(std::collections::BTreeMap::from([
            (
                "base/active_skill_types.json".into(),
                serde_json::to_vec(&data.active_skill_types().unwrap()).unwrap(),
            ),
            (
                "base/game_constants.json".into(),
                serde_json::to_vec(&changed).unwrap(),
            ),
        ]));
        assert_eq!(
            changed_data.load_ruleset().unwrap().game_constants,
            Some(changed)
        );
    }

    /// The repo data directory: the monster_scaling domain is wired up
    /// (Some) with all nine 100-level tables; it is the only copy (the
    /// `Default` is empty, so production loading must find this file).
    #[test]
    fn repo_data_ruleset_loads_monster_scaling() {
        let data = GameData::new(crate::current_data_dir());
        let ruleset = data.load_ruleset().unwrap();
        let loaded = ruleset
            .monster_scaling
            .expect("monster_scaling domain should be wired up");
        let len = pobr_data::catalog::monster_scaling::MONSTER_SCALING_TABLE_LEN;
        for (name, n) in [
            ("accuracy", loaded.accuracy.len()),
            ("evasion", loaded.evasion.len()),
            ("armour", loaded.armour.len()),
            ("life", loaded.life.len()),
            ("ally_life", loaded.ally_life.len()),
            ("damage", loaded.damage.len()),
            ("ally_damage", loaded.ally_damage.len()),
            ("ailment_threshold", loaded.ailment_threshold.len()),
            ("poise_threshold", loaded.poise_threshold.len()),
        ] {
            assert_eq!(n, len, "{name} length");
        }
        assert_ne!(
            loaded,
            pobr_data::catalog::monster_scaling::MonsterScalingTable::default()
        );
    }

    #[test]
    fn ruleset_preserves_loaded_unarmed_rows() {
        let data = GameData::new(crate::current_data_dir());
        let loaded = data.load_ruleset().unwrap().unarmed_data.unwrap();
        assert_eq!(loaded.0, data.unarmed_data().unwrap());
        assert!(
            loaded.for_class("Scion").is_some(),
            "legacy compatibility row"
        );
        assert!(loaded.for_class("Druid").is_some(), "current source row");
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
}
