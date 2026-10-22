//! Reproducibility metadata shared by native and WASM calculation reports.

use pobr_gamedata::{GameData, LoadError};
use serde::Serialize;

/// Identifies the engine release and its runtime input pack. The data digest
/// includes shared overlays and applicable user patches, not just the game version.
/// It identifies input bytes, not equality of calculated results. Development
/// builds of the same release still need their source revision to be reproduced.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CalculationProvenance {
    pub engine_version: String,
    pub data_version: Option<String>,
    pub data_sha256: String,
}

impl CalculationProvenance {
    /// The application supplies its release version; library versions are
    /// intentionally independent of the released CLI/WASM version.
    pub fn from_data(data: &GameData, engine_version: &str) -> Result<Self, LoadError> {
        let identity = data.identity()?;
        Ok(Self {
            engine_version: engine_version.to_owned(),
            data_version: identity.data_version.clone(),
            data_sha256: identity.sha256.clone(),
        })
    }
}
