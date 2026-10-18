//! `overlay/config_options.json` loader — the config options catalog
//! (542+ entries of declarative effects DSL plus handler_id exceptions),
//! schema in [`pobr_data::catalog::config_def`].
//!
//! Sourced from `sync-pob-catalog extract-lua --what config-options`
//! (probe-based induction of ConfigOptions.lua's apply closures);
//! consumer = `pobr-core::rules::config_interpreter`.

use pobr_data::catalog::config_def::ConfigOptionsDef;

use crate::{GameData, LoadError};

impl GameData {
    /// Loads the config options catalog (always resolved under
    /// `overlay/`; `_meta` is ignored by serde).
    ///
    /// When the table is missing, the caller tolerates it and falls back
    /// to the old path (`xml_build::parse_config`'s existing branch).
    pub fn config_options(&self) -> Result<ConfigOptionsDef, LoadError> {
        let mut catalog: ConfigOptionsDef =
            self.load_json_at(self.overlay_path("config_options.json"))?;
        // PoBR-owned options are bundled for native and staged-memory loaders alike.
        let own: ConfigOptionsDef = serde_json::from_str(include_str!(
            "../../../../data/overlay-common/archon_config_options.json"
        ))
        .expect("bundled Archon config options");
        catalog.options = crate::paths::merge_by_key(own.options, catalog.options, |o| &o.var);
        // Standing on ground is per actor, not inferred for the player's allies.
        let ground: ConfigOptionsDef = serde_json::from_str(include_str!(
            "../../../../data/overlay-common/consecrated_ground_config_options.json"
        ))
        .expect("bundled Consecrated Ground config options");
        catalog.options = crate::paths::merge_by_key(catalog.options, ground.options, |o| &o.var);
        let beira: ConfigOptionsDef = serde_json::from_str(include_str!(
            "../../../../data/overlay-common/beira_config_options.json"
        ))
        .expect("bundled Beira config options");
        catalog.options = crate::paths::merge_by_key(catalog.options, beira.options, |o| &o.var);
        let arrows: ConfigOptionsDef = serde_json::from_str(include_str!(
            "../../../../data/overlay-common/cadiro_config_options.json"
        ))
        .expect("bundled Cadiro arrow config options");
        catalog.options = crate::paths::merge_by_key(catalog.options, arrows.options, |o| &o.var);
        let combo: ConfigOptionsDef = serde_json::from_str(include_str!(
            "../../../../data/overlay-common/combo_config_options.json"
        ))
        .expect("bundled Combo config options");
        catalog.options = crate::paths::merge_by_key(catalog.options, combo.options, |o| &o.var);
        let remnants: ConfigOptionsDef = serde_json::from_str(include_str!(
            "../../../../data/overlay-common/remnant_pickup_config_options.json"
        ))
        .expect("bundled Remnant pickup config options");
        catalog.options = crate::paths::merge_by_key(catalog.options, remnants.options, |o| &o.var);
        let volatility: ConfigOptionsDef = serde_json::from_str(include_str!(
            "../../../../data/overlay-common/volatility_config_options.json"
        ))
        .expect("bundled Volatility config options");
        catalog.options =
            crate::paths::merge_by_key(catalog.options, volatility.options, |o| &o.var);
        catalog.options.sort_by(|a, b| a.var.cmp(&b.var));
        Ok(catalog)
    }
}
