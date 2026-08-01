//! Schema for the vendor PoB2 ModName → PoBR canonical StatId alias table
//! (`overlay/vendor_name_aliases.json`, `vendor_name_aliases/v1`).
//!
//! Background: the legacy hand-written parser produces **PoBR's own
//! vocabulary** (`MaximumLife`/`Strength`/`ColdResistance`…), while the new
//! engine faithfully stores **vendor PoB2's vocabulary**
//! (`Life`/`Str`/`ColdResist`…), and every downstream consumer expects
//! PoBR's vocabulary. This table bridges the two vocabularies with a single
//! `vendor_name → pobr_stat_id` alias map, bootstrapped from the
//! intersection of the legacy `parse_name` phrase mapping and the engine's
//! `name_map` (aligned by the phrase that triggers both).
//!
//! **Zero-consumer discipline**: this module only defines the serde shape
//! (with round-trip unit tests as a safety net) — it's **not consumed by
//! any calc / loader / parser path**. The wiring point for the two
//! switchover routes (A/B) hasn't been decided yet, so it stays unwired
//! until the owner decides.
//! At that point this table will serve either "runtime translation" (A) or
//! "extraction-time normalization" (B) — the schema doesn't change either way.
//!
//! From the consumer's perspective: serde ignores the top-level `_meta` and
//! `structural_deferrals` by default (provenance info / a log of structural
//! disagreements, kept for reference but not consumed by calc — the same
//! convention as the existing overlay schemas). This module has zero
//! logic, zero I/O; new fields use `#[serde(default)]`.

use serde::{Deserialize, Serialize};

/// Schema identifier (the expected value of `_meta.schema`).
pub const VENDOR_NAME_ALIASES_SCHEMA: &str = "vendor_name_aliases/v1";

/// Top-level document for the alias table (`overlay/vendor_name_aliases.json`).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct VendorNameAliasesDoc {
    /// vendor → PoBR alias entries (bootstrapped, plus manual review).
    #[serde(default)]
    pub aliases: Vec<VendorNameAliasDef>,
}

/// A single vendor → PoBR alias.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct VendorNameAliasDef {
    /// The vendor PoB2 ModName the engine produces (e.g. `Life` / `Str` / `ColdResistMax`).
    pub vendor_name: String,
    /// The normalization target — PoBR's canonical StatId (e.g.
    /// `MaximumLife` / `Strength`).
    pub pobr_stat_id: String,
    /// Whether the names are identical (`vendor_name == pobr_stat_id`) —
    /// identity aliases are the majority; the entries that actually rename
    /// something (identity=false) are the subset most likely to cause
    /// downstream misses during the switchover.
    #[serde(default)]
    pub identity: bool,
    /// Bootstrap evidence: the trigger phrase both legacy and the engine
    /// recognized and produced this pair from (an empty string means
    /// manually added).
    #[serde(default)]
    pub via_phrase: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// serde round-trip: the top-level document structure is stable (the
    /// only safety net for a zero-consumer asset).
    #[test]
    fn doc_roundtrips_through_json() {
        let doc = VendorNameAliasesDoc {
            aliases: vec![
                VendorNameAliasDef {
                    vendor_name: "Life".into(),
                    pobr_stat_id: "MaximumLife".into(),
                    identity: false,
                    via_phrase: "maximum life".into(),
                },
                VendorNameAliasDef {
                    vendor_name: "Armour".into(),
                    pobr_stat_id: "Armour".into(),
                    identity: true,
                    via_phrase: "armour".into(),
                },
            ],
        };
        let json = serde_json::to_string(&doc).expect("serialize");
        let back: VendorNameAliasesDoc = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(doc, back);
    }

    /// Default fields: minimal JSON with only `aliases` still deserializes;
    /// the top-level `_meta` / `structural_deferrals` are ignored by serde
    /// (doesn't break the consumer).
    #[test]
    fn minimal_json_deserializes_and_ignores_meta() {
        let json = r#"{
            "_meta": {"schema": "vendor_name_aliases/v1"},
            "structural_deferrals": {"note": "see m6-alias-table.md"},
            "aliases": [{"vendor_name": "Str", "pobr_stat_id": "Strength"}]
        }"#;
        let doc: VendorNameAliasesDoc = serde_json::from_str(json).expect("deserialize");
        assert_eq!(doc.aliases.len(), 1);
        assert_eq!(doc.aliases[0].vendor_name, "Str");
        assert_eq!(doc.aliases[0].pobr_stat_id, "Strength");
        assert!(!doc.aliases[0].identity);
        assert!(doc.aliases[0].via_phrase.is_empty());
    }
}
