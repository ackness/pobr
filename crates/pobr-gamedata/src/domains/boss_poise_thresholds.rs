//! Game-sourced boss poise percentages, with a shared curated fallback file.

use pobr_data::catalog::boss_poise_thresholds::BossPoiseThresholdsDef;

use crate::{GameData, LoadError};

impl GameData {
    /// Loads the version's overlay first, then `overlay-common/`.
    pub fn boss_poise_thresholds(&self) -> Result<BossPoiseThresholdsDef, LoadError> {
        self.load_overlay_or_common("boss_poise_thresholds.json")
    }
}
