use std::collections::{BTreeMap, BTreeSet};

use serde::Deserialize;

use crate::{GameData, LoadError};

type SpecialAffixPools = BTreeMap<String, BTreeMap<String, BTreeSet<String>>>;

#[derive(Deserialize)]
struct DisplayCatalog {
    #[serde(default)]
    bases: Vec<DisplayBase>,
    #[serde(default)]
    search_mods: Vec<DisplayMod>,
}

#[derive(Deserialize)]
struct DisplayBase {
    name: String,
    category: String,
}

#[derive(Deserialize)]
struct DisplayMod {
    id: String,
    source: String,
    categories: Vec<String>,
}

impl GameData {
    pub fn special_affix_pools(&self) -> Result<SpecialAffixPools, LoadError> {
        let catalog = match self
            .load_json_at::<DisplayCatalog>(self.overlay_path("trade_catalog.json"))
        {
            Ok(catalog) => catalog,
            Err(LoadError::Io { source, .. }) if source.kind() == std::io::ErrorKind::NotFound => {
                return Ok(BTreeMap::new());
            }
            Err(error) => return Err(error),
        };
        let mut categories = SpecialAffixPools::new();
        for modifier in catalog.search_mods {
            if !matches!(
                modifier.source.as_str(),
                "alloy" | "essence" | "desecrated" | "breach" | "influence" | "corrupted"
            ) {
                continue;
            }
            let Some((source, identifier)) = modifier.id.split_once(':') else {
                continue;
            };
            if source != modifier.source || identifier.is_empty() || identifier.contains(':') {
                continue;
            }
            for category in modifier.categories {
                categories
                    .entry(category)
                    .or_default()
                    .entry(identifier.to_string())
                    .or_default()
                    .insert(source.to_string());
            }
        }
        let mut base_categories: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        for base in catalog.bases {
            base_categories
                .entry(base.name)
                .or_default()
                .insert(base.category);
        }
        Ok(base_categories
            .into_iter()
            .filter_map(|(name, scopes)| {
                if scopes.len() != 1 {
                    return None;
                }
                categories
                    .get(scopes.first()?)
                    .cloned()
                    .map(|modifiers| (name, modifiers))
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::sha256_hex;
    use serde_json::json;

    const CATALOG_PATH: &str = "overlay/trade_catalog.json";
    const SPECIAL_CATALOG: &str = r#"{"bases":[{"name":"Gloves","category":"armour.gloves"}],"search_mods":[{"id":"influence:Special1","source":"influence","categories":["armour.gloves"]}]}"#;

    fn game(catalog: Option<&str>) -> GameData {
        let files = catalog
            .into_iter()
            .map(|content| (CATALOG_PATH.to_string(), content.as_bytes().to_vec()))
            .collect();
        GameData::from_memory(files)
    }

    fn sealed_files(catalog: Option<&str>) -> BTreeMap<String, Vec<u8>> {
        let mut files = BTreeMap::from([(
            "base/cost_types.json".to_string(),
            br#"[{"id":"Mana","divisor":1}]"#.to_vec(),
        )]);
        if let Some(content) = catalog {
            files.insert(CATALOG_PATH.to_string(), content.as_bytes().to_vec());
        }
        let inventory: BTreeMap<_, _> = files
            .iter()
            .map(|(path, bytes)| (path.clone(), sha256_hex(bytes)))
            .collect();
        let overlays = if catalog.is_some() {
            vec!["trade_catalog"]
        } else {
            Vec::new()
        };
        let manifest = json!({
            "schema_version": 3,
            "poe_version": "1.2.3",
            "contract_version": pobr_data::CONTRACT_VERSION,
            "languages": [],
            "domains": {
                "base": ["cost_types"],
                "overlay": overlays,
                "generated": [],
            },
            "files": inventory,
        });
        files.insert(
            "manifest.json".to_string(),
            serde_json::to_vec(&manifest).unwrap(),
        );
        files
    }

    fn memory_backends(files: BTreeMap<String, Vec<u8>>) -> [GameData; 2] {
        [
            GameData::from_memory(files.clone()),
            GameData::from_memory_releasing(files, false),
        ]
    }

    #[test]
    fn special_candidates_remain_scoped_to_exact_categories_and_source_ids() {
        let data = game(Some(
            r#"{"bases":[{"name":"Gloves","category":"armour.gloves"},{"name":"Ring","category":"accessory.ring"}],"search_mods":[{"id":"influence:Special1","source":"influence","categories":["armour.gloves"]},{"id":"influence:WrongSource","source":"essence","categories":["armour.gloves"]},{"id":"unknown:Untrusted","source":"unknown","categories":["accessory.ring"]}]}"#,
        ));
        let pools = data.special_affix_pools().unwrap();
        assert_eq!(
            pools["Gloves"],
            BTreeMap::from([(
                "Special1".to_string(),
                BTreeSet::from(["influence".to_string()]),
            )])
        );
        assert!(!pools.contains_key("Ring"));
    }

    #[test]
    fn different_sources_in_one_category_keep_their_native_id_associations() {
        let catalog = json!({
            "bases": [
                { "name": "Gloves", "category": "armour.gloves" },
                { "name": "Ring", "category": "accessory.ring" },
            ],
            "search_mods": [
                { "id": "influence:Influence1", "source": "influence", "categories": ["armour.gloves"] },
                { "id": "alloy:Alloy1", "source": "alloy", "categories": ["armour.gloves"] },
                { "id": "influence:Ring1", "source": "influence", "categories": ["accessory.ring"] },
            ],
        })
        .to_string();
        let pools = game(Some(&catalog)).special_affix_pools().unwrap();
        assert_eq!(
            pools,
            BTreeMap::from([
                (
                    "Gloves".to_string(),
                    BTreeMap::from([
                        ("Alloy1".to_string(), BTreeSet::from(["alloy".to_string()])),
                        (
                            "Influence1".to_string(),
                            BTreeSet::from(["influence".to_string()]),
                        ),
                    ]),
                ),
                (
                    "Ring".to_string(),
                    BTreeMap::from([(
                        "Ring1".to_string(),
                        BTreeSet::from(["influence".to_string()]),
                    )]),
                ),
            ])
        );
    }

    #[test]
    fn repeated_native_ids_preserve_all_sources_without_cross_category_leaks() {
        let catalog = json!({
            "bases": [
                { "name": "Gloves", "category": "armour.gloves" },
                { "name": "Ring", "category": "accessory.ring" },
            ],
            "search_mods": [
                { "id": "influence:Shared1", "source": "influence", "categories": ["armour.gloves", "accessory.ring"] },
                { "id": "alloy:Shared1", "source": "alloy", "categories": ["armour.gloves"] },
                { "id": "influence:Shared1", "source": "influence", "categories": ["armour.gloves"] },
            ],
        })
        .to_string();
        let pools = game(Some(&catalog)).special_affix_pools().unwrap();
        assert_eq!(
            pools,
            BTreeMap::from([
                (
                    "Gloves".to_string(),
                    BTreeMap::from([(
                        "Shared1".to_string(),
                        BTreeSet::from(["alloy".to_string(), "influence".to_string()]),
                    )]),
                ),
                (
                    "Ring".to_string(),
                    BTreeMap::from([(
                        "Shared1".to_string(),
                        BTreeSet::from(["influence".to_string()]),
                    )]),
                ),
            ])
        );
    }

    #[test]
    fn absent_optional_catalog_and_legacy_catalog_do_not_invent_special_pools() {
        assert!(game(None).special_affix_pools().unwrap().is_empty());
        assert!(
            game(Some(r#"{"bases":[]}"#))
                .special_affix_pools()
                .unwrap()
                .is_empty()
        );
        assert!(game(Some("invalid")).special_affix_pools().is_err());
    }

    #[test]
    fn ambiguous_base_categories_never_union_special_pools() {
        let data = game(Some(
            r#"{"bases":[{"name":"Ambiguous","category":"armour.gloves"},{"name":"Ambiguous","category":"accessory.ring"},{"name":"Gloves","category":"armour.gloves"},{"name":"Gloves","category":"armour.gloves"}],"search_mods":[{"id":"influence:Special1","source":"influence","categories":["armour.gloves"]},{"id":"influence:","source":"influence","categories":["armour.gloves"]},{"id":"influence:Nested:Special2","source":"influence","categories":["armour.gloves"]}]}"#,
        ));
        let pools = data.special_affix_pools().unwrap();
        assert!(!pools.contains_key("Ambiguous"));
        assert_eq!(
            pools["Gloves"],
            BTreeMap::from([(
                "Special1".to_string(),
                BTreeSet::from(["influence".to_string()]),
            )])
        );
    }

    #[test]
    fn schema3_unlisted_catalog_is_ignored_even_if_present() {
        for content in [None, Some(SPECIAL_CATALOG), Some("invalid")] {
            let mut files = sealed_files(None);
            if let Some(content) = content {
                files.insert(CATALOG_PATH.to_string(), content.as_bytes().to_vec());
            }
            for data in memory_backends(files) {
                assert!(data.special_affix_pools().unwrap().is_empty());
            }
        }
    }

    #[test]
    fn schema3_declared_catalog_missing_is_integrity_error() {
        let mut files = sealed_files(Some(SPECIAL_CATALOG));
        files.remove(CATALOG_PATH).unwrap();
        for data in memory_backends(files) {
            let error = data.special_affix_pools().unwrap_err();
            std::assert_matches!(&error, LoadError::Integrity { .. }, "{error}");
            assert!(error.to_string().contains(CATALOG_PATH), "{error}");
        }
    }

    #[test]
    fn schema3_declared_domain_without_inventory_entry_is_integrity_error() {
        let mut files = sealed_files(Some(SPECIAL_CATALOG));
        let mut manifest: serde_json::Value =
            serde_json::from_slice(&files["manifest.json"]).unwrap();
        manifest["files"]
            .as_object_mut()
            .unwrap()
            .remove(CATALOG_PATH)
            .unwrap();
        files.remove(CATALOG_PATH).unwrap();
        files.insert(
            "manifest.json".to_string(),
            serde_json::to_vec(&manifest).unwrap(),
        );
        for data in memory_backends(files) {
            let error = data.special_affix_pools().unwrap_err();
            std::assert_matches!(&error, LoadError::Integrity { .. }, "{error}");
            assert!(
                error
                    .to_string()
                    .contains("declared domain lacks an inventory entry: overlay/trade_catalog"),
                "{error}"
            );
        }
    }

    #[test]
    fn schema3_declared_catalog_hash_mismatch_is_integrity_error() {
        let mut files = sealed_files(Some(SPECIAL_CATALOG));
        files.insert(
            CATALOG_PATH.to_string(),
            br#"{"bases":[],"search_mods":[]}"#.to_vec(),
        );
        for data in memory_backends(files) {
            let error = data.special_affix_pools().unwrap_err();
            std::assert_matches!(&error, LoadError::Integrity { .. }, "{error}");
            assert!(error.to_string().contains(CATALOG_PATH), "{error}");
            assert!(error.to_string().contains("SHA-256 mismatch"), "{error}");
        }
    }

    #[test]
    fn schema3_declared_malformed_catalog_is_parse_error() {
        for content in ["invalid", r#"{"search_mods":{}}"#] {
            for data in memory_backends(sealed_files(Some(content))) {
                data.validate_manifest().unwrap();
                let error = data.special_affix_pools().unwrap_err();
                std::assert_matches!(&error, LoadError::Parse { .. }, "{error}");
                assert!(error.to_string().contains(CATALOG_PATH), "{error}");
            }
        }
    }

    #[test]
    fn releasing_other_domain_keeps_special_catalog_readable_twice() {
        let expected = BTreeMap::from([(
            "Gloves".to_string(),
            BTreeMap::from([(
                "Special1".to_string(),
                BTreeSet::from(["influence".to_string()]),
            )]),
        )]);
        for prechecked in [false, true] {
            let data =
                GameData::from_memory_releasing(sealed_files(Some(SPECIAL_CATALOG)), prechecked);
            assert_eq!(data.cost_types().unwrap()[0].divisor, 1);
            data.release_domain("cost_types.json");
            std::assert_matches!(
                data.files
                    .as_ref()
                    .unwrap()
                    .lock()
                    .get("base/cost_types.json"),
                Some(crate::MemoryEntry::Released)
            );
            for read in 1..=2 {
                assert_eq!(
                    data.special_affix_pools().unwrap(),
                    expected,
                    "prechecked={prechecked}, read={read}"
                );
            }
        }
    }
}
