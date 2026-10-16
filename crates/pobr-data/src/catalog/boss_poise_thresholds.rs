//! Monster-variety poise percentages, curated from the official game table.
//! Provenance and representative rows live in `overlay-common/boss_poise_thresholds.json`.

use serde::{Deserialize, Serialize};

use crate::monster::EnemyTier;

/// Raw `MonsterVarieties.PoiseThreshold` percentages (100 means a multiplier of one).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BossPoiseThresholdsDef {
    pub boss: u32,
    pub pinnacle: u32,
    pub uber: u32,
}

impl BossPoiseThresholdsDef {
    pub fn for_tier(&self, tier: EnemyTier) -> u32 {
        match tier {
            EnemyTier::None => 100,
            EnemyTier::Boss => self.boss,
            EnemyTier::Pinnacle => self.pinnacle,
            EnemyTier::Uber => self.uber,
        }
    }
}

impl Default for BossPoiseThresholdsDef {
    fn default() -> Self {
        // Patch 0.5.5.4.2: MAP boss mode (rows 275/437/687), Xesht (1946).
        // There is no separate Uber Xesht variety in this export.
        Self {
            boss: 313,
            pinnacle: 1700,
            uber: 1700,
        }
    }
}
