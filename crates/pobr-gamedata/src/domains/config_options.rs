//! `overlay/config_options.json` loader — the config options catalog
//! (542+ entries of declarative effects DSL plus handler_id exceptions),
//! schema in [`pobr_data::catalog::config_def`].
//!
//! PoB2-derived compatibility data (probe-based induction of
//! ConfigOptions.lua's apply closures) imported from the RePoE owner
//! package; consumer = `pobr-core::rules::config_interpreter`.

use pobr_data::catalog::config_def::ConfigOptionsDef;

use crate::{GameData, LoadError};

/// PoBR-owned config options (`config_options/*_config_options.json` next to
/// this module), compiled in so native and staged-memory loaders see the same
/// set. They are engine code, not data-pack input: they count toward the
/// engine identity, not the data identity. Each file replaces catalog entries
/// with the same `var`. Every such file in that directory must be listed here
/// (checked by a unit test).
const OWNED_CONFIG_OPTIONS: &[(&str, &str)] = &[
    (
        "archon_config_options.json",
        include_str!("config_options/archon_config_options.json"),
    ),
    // Standing on ground is per actor, not inferred for the player's allies.
    (
        "consecrated_ground_config_options.json",
        include_str!("config_options/consecrated_ground_config_options.json"),
    ),
    (
        "beira_config_options.json",
        include_str!("config_options/beira_config_options.json"),
    ),
    (
        "cadiro_config_options.json",
        include_str!("config_options/cadiro_config_options.json"),
    ),
    (
        "combo_config_options.json",
        include_str!("config_options/combo_config_options.json"),
    ),
    (
        "energy_config_options.json",
        include_str!("config_options/energy_config_options.json"),
    ),
    (
        "remnant_pickup_config_options.json",
        include_str!("config_options/remnant_pickup_config_options.json"),
    ),
    (
        "volatility_config_options.json",
        include_str!("config_options/volatility_config_options.json"),
    ),
    (
        "enemy_type_and_consumption_config_options.json",
        include_str!("config_options/enemy_type_and_consumption_config_options.json"),
    ),
];

impl GameData {
    /// Loads the config options catalog (always resolved under
    /// `overlay/`; `_meta` is ignored by serde) and applies the PoBR-owned
    /// options on top of it.
    ///
    /// When the table is missing, the caller tolerates it and falls back
    /// to the old path (`xml_build::parse_config`'s existing branch).
    pub fn config_options(&self) -> Result<ConfigOptionsDef, LoadError> {
        let mut catalog: ConfigOptionsDef =
            self.load_json_at(self.overlay_path("config_options.json"))?;
        // The imported catalog repeats `conditionEnemyExitedPresenceRecently`
        // (PoB2 gives the "exited" and "entered" Presence checks one input).
        // Keep one entry per var; the later entry wins.
        catalog.options = crate::paths::merge_by_key(Vec::new(), catalog.options, |o| &o.var);
        for (name, text) in OWNED_CONFIG_OPTIONS {
            let own: ConfigOptionsDef = serde_json::from_str(text)
                .unwrap_or_else(|err| panic!("bundled {name} is invalid: {err}"));
            catalog.options = crate::paths::merge_by_key(catalog.options, own.options, |o| &o.var);
        }
        catalog.options.sort_by(|a, b| a.var.cmp(&b.var));
        Ok(catalog)
    }
}

#[cfg(test)]
mod tests {
    use super::OWNED_CONFIG_OPTIONS;
    use crate::GameData;

    /// A new `*_config_options.json` in `config_options/` that is not
    /// bundled would silently have no runtime effect.
    #[test]
    fn every_owned_config_options_file_is_bundled() {
        let dir =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/domains/config_options");
        let mut on_disk: Vec<String> = std::fs::read_dir(&dir)
            .expect("config_options/ is readable")
            .map(|entry| {
                entry
                    .expect("directory entry")
                    .file_name()
                    .to_string_lossy()
                    .into_owned()
            })
            .filter(|name| name.ends_with("_config_options.json"))
            .collect();
        on_disk.sort();
        let mut bundled: Vec<String> = OWNED_CONFIG_OPTIONS
            .iter()
            .map(|(name, _)| (*name).to_owned())
            .collect();
        bundled.sort();
        assert_eq!(on_disk, bundled);
    }

    /// Owned options replace a catalog entry with the same `var`, and a var
    /// repeated in the catalog keeps a single entry.
    #[test]
    fn owned_options_override_the_catalog() {
        let catalog = r#"{"options":[
            {"var":"energyMinionPower","input_type":"check","section":"Old","label":"stale catalog copy","effects":[]},
            {"var":"conditionAffectedByArchon","input_type":"check","section":"Old","label":"stale catalog copy","effects":[]},
            {"var":"zzCatalogOnly","input_type":"check","section":"Old","label":"catalog only","effects":[]},
            {"var":"zzRepeated","input_type":"check","section":"Old","label":"first","effects":[]},
            {"var":"zzRepeated","input_type":"check","section":"Old","label":"second","effects":[]}
        ]}"#;
        let data = GameData::from_memory(std::collections::BTreeMap::from([(
            "overlay/config_options.json".to_owned(),
            catalog.as_bytes().to_vec(),
        )]));
        let options = data.config_options().expect("config options load").options;
        let label = |var: &str| {
            options
                .iter()
                .find(|o| o.var == var)
                .unwrap_or_else(|| panic!("missing {var}"))
                .label
                .clone()
        };
        let stale = Some("stale catalog copy".to_owned());
        assert_ne!(label("energyMinionPower"), stale);
        assert_ne!(label("conditionAffectedByArchon"), stale);
        assert_eq!(label("zzCatalogOnly").as_deref(), Some("catalog only"));
        // A var repeated in the catalog keeps only its later entry.
        assert_eq!(options.iter().filter(|o| o.var == "zzRepeated").count(), 1);
        assert_eq!(label("zzRepeated").as_deref(), Some("second"));
        assert!(options.windows(2).all(|pair| pair[0].var <= pair[1].var));
    }
}
