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
        self.load_json_at(self.overlay_path("config_options.json"))
    }
}
