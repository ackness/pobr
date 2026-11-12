//! PoBR's game-patch to passive-tree compatibility labels.

use crate::{GameData, LoadError};

// New patch families need an explicit entry once their tree label is known.
const TREE_VERSIONS: &[(&str, &str)] = &[
    ("0.1", "0_1"),
    ("0.2", "0_2"),
    ("0.3", "0_3"),
    ("0.4", "0_4"),
    ("0.5", "0_5"),
];

impl GameData {
    /// Resolves PoBR's tree label, checking any imported compatibility label.
    /// Unknown patch families and disagreements are errors, never guessed labels.
    pub fn tree_version(&self) -> Result<&'static str, LoadError> {
        if let Some(version) = self.tree_version.get() {
            return Ok(version);
        }
        let data_version = self.validate_manifest()?.poe_version;
        let expected = TREE_VERSIONS
            .iter()
            .find(|(prefix, _)| {
                data_version == *prefix
                    || data_version
                        .strip_prefix(prefix)
                        .is_some_and(|rest| rest.starts_with('.'))
            })
            .map(|(_, label)| *label)
            .ok_or_else(|| LoadError::UnknownTreeVersion {
                data_version: data_version.clone(),
            })?;

        #[derive(serde::Deserialize)]
        struct CompatibilityLabel {
            tree_version: Option<String>,
        }
        let compatibility: Result<CompatibilityLabel, _> =
            self.load_json_at(self.overlay_path("passive_jewels.json"));
        match compatibility {
            Ok(CompatibilityLabel {
                tree_version: Some(actual),
            }) if actual != expected => {
                return Err(LoadError::TreeVersionMismatch {
                    data_version,
                    expected,
                    actual,
                });
            }
            Err(LoadError::Io { source, .. }) if source.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
            _ => {}
        }
        let _ = self.tree_version.set(expected);
        Ok(expected)
    }
}
