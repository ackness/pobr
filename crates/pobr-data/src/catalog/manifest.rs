//! Data-pack envelope (manifest) schema: describes which domains and
//! languages are stored for a given PoE2 version.
//!
//! Since v2, `domains` is split by the three physical directory layers
//! (`base`/`overlay`/`generated`); deserialization stays
//! compatible with v1's flat array shape (treated as all belonging to `base`).

use serde::{Deserialize, Serialize};

/// The current catalog schema version. Bumped by 1 on a breaking structural change.
///
/// v2: manifest's `domains` changed from a flat array to the three-section
/// [`DomainSections`].
pub const CATALOG_SCHEMA_VERSION: u32 = 2;

/// The data-pack envelope: describes which domains and languages are
/// stored for a given PoE2 version.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DataManifest {
    pub schema_version: u32,
    /// The CDN patch version, e.g. `4.5.0.3.4` (public version 0.5.0).
    pub poe_version: String,
    /// Language tags that have a generated i18n sidecar, e.g. `["zh-TW"]`
    /// (English is canonical and isn't counted here).
    pub languages: Vec<String>,
    /// The data-domain filenames that have been generated (without
    /// extension), split by the three directory layers.
    pub domains: DomainSections,
}

/// Manifest v2's three-section domains (corresponds to
/// `data/<version>/{base/, overlay/, generated/}`).
///
/// Always serializes to the v2 object shape; deserialization stays
/// compatible with v1's flat array (treated as entirely the `base` section).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct DomainSections {
    /// `base/`: domains fully auto-regenerated from `.dat` (produced by the
    /// pipeline + adapter, hand edits forbidden).
    pub base: Vec<String>,
    /// `overlay/`: hand-curated domains extracted from vendor Lua
    /// (produced by extract-lua, only tool-regeneration allowed).
    pub overlay: Vec<String>,
    /// `generated/`: cached domains deterministically derived from base +
    /// overlay (produced by precompile).
    pub generated: Vec<String>,
}

/// An intermediate deserialization shape: either v1's flat array or v2's
/// three-section object.
#[derive(Deserialize)]
#[serde(untagged)]
enum DomainSectionsRepr {
    /// v1: a flat `["base_items", ...]`, all treated as the base section.
    Flat(Vec<String>),
    /// v2: `{"base": [...], "overlay": [...], "generated": [...]}`.
    Sections {
        base: Vec<String>,
        #[serde(default)]
        overlay: Vec<String>,
        #[serde(default)]
        generated: Vec<String>,
    },
}

impl<'de> Deserialize<'de> for DomainSections {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        Ok(match DomainSectionsRepr::deserialize(deserializer)? {
            DomainSectionsRepr::Flat(base) => Self {
                base,
                overlay: Vec::new(),
                generated: Vec::new(),
            },
            DomainSectionsRepr::Sections {
                base,
                overlay,
                generated,
            } => Self {
                base,
                overlay,
                generated,
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A v1 manifest (flat domains array) should deserialize, with
    /// everything landing in the base section.
    #[test]
    fn deserializes_v1_flat_domains_as_base() {
        let json = r#"{
            "schema_version": 1,
            "poe_version": "4.5.0.3.4",
            "languages": ["zh-TW"],
            "domains": ["base_items", "mods"]
        }"#;
        let manifest: DataManifest = serde_json::from_str(json).unwrap();
        assert_eq!(manifest.schema_version, 1);
        assert_eq!(manifest.domains.base, vec!["base_items", "mods"]);
        assert!(manifest.domains.overlay.is_empty());
        assert!(manifest.domains.generated.is_empty());
    }

    /// A v2 manifest (three-section domains object) should deserialize per section.
    #[test]
    fn deserializes_v2_sectioned_domains() {
        let json = r#"{
            "schema_version": 2,
            "poe_version": "4.5.0.3.4",
            "languages": ["zh-TW"],
            "domains": {
                "base": ["base_items"],
                "overlay": ["skill_stat_map"],
                "generated": ["parsed_mods"]
            }
        }"#;
        let manifest: DataManifest = serde_json::from_str(json).unwrap();
        assert_eq!(manifest.schema_version, 2);
        assert_eq!(manifest.domains.base, vec!["base_items"]);
        assert_eq!(manifest.domains.overlay, vec!["skill_stat_map"]);
        assert_eq!(manifest.domains.generated, vec!["parsed_mods"]);
    }

    /// A v2 manifest missing the overlay/generated sections should default to empty
    /// (forward compatibility for a partial write-out).
    #[test]
    fn v2_missing_sections_default_to_empty() {
        let json = r#"{
            "schema_version": 2,
            "poe_version": "4.5.0.3.4",
            "languages": [],
            "domains": { "base": ["stats"] }
        }"#;
        let manifest: DataManifest = serde_json::from_str(json).unwrap();
        assert_eq!(manifest.domains.base, vec!["stats"]);
        assert!(manifest.domains.overlay.is_empty());
        assert!(manifest.domains.generated.is_empty());
    }

    /// Serialization always uses the v2 three-section object shape, and the round trip is lossless.
    #[test]
    fn serializes_v2_shape_roundtrip() {
        let manifest = DataManifest {
            schema_version: CATALOG_SCHEMA_VERSION,
            poe_version: crate::DATA_VERSION.into(),
            languages: vec!["zh-TW".into()],
            domains: DomainSections {
                base: vec!["base_items".into()],
                overlay: vec![],
                generated: vec![],
            },
        };
        let json = serde_json::to_string(&manifest).unwrap();
        assert!(json.contains(r#""domains":{"base":["base_items"],"overlay":[],"generated":[]}"#));
        let back: DataManifest = serde_json::from_str(&json).unwrap();
        assert_eq!(back, manifest);
    }
}
