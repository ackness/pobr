//! Mods + Stats domain adapter: raw `Mods.json` / `Stats.json` -> PoBR's minimal JSON.
//!
//! - `Stats.json` -> `stats.json` (the stat registry: id / is_local / semantic / category).
//! - `Mods.json` -> `mods.json` (the affix pool: merges the Stat1..4 foreign
//!   keys with the Stat1Value..4Value rolled ranges, resolves the Tags
//!   foreign key, and keeps domain / generation_type / mod_type / level).
//!   Families resolve independently through `ModFamily.Id`; `group` stays `ModType.Name`.
//! - zh-TW affix names (when they differ from English) -> the `i18n/zh-TW/mods.json` sidecar.
//!
//! Filtering rules:
//! - Stats: keep everything (the registry needs to be complete for Mods' foreign-key resolution).
//! - Mods: skip pure placeholder/internal shell entries that have neither a
//!   display name (`Name` empty) **nor** any stat slots; anything with a
//!   stat or a name is kept (an unnamed internal mod that carries a stat is
//!   still needed for calculation, so it stays).

use std::collections::BTreeMap;
use std::path::Path;

use pobr_data::catalog::{ModDef, ModStat, SpawnWeight, StatDef};
use serde::Deserialize;

use crate::{RawIndexed, RawNamed, read_json, resolve, write_pretty};

// Raw .dat JSON row structures (only the columns we need)

#[derive(Deserialize)]
struct RawStat {
    #[serde(rename = "Id")]
    id: String,
    #[serde(rename = "IsLocal")]
    is_local: Option<bool>,
    #[serde(rename = "Semantic")]
    semantic: Option<u32>,
    #[serde(rename = "Category")]
    category: Option<u32>,
}

#[derive(Deserialize)]
struct RawMod {
    #[serde(rename = "Id")]
    id: String,
    #[serde(rename = "Name")]
    name: Option<String>,
    #[serde(rename = "ModType")]
    mod_type: Option<u32>,
    #[serde(rename = "Domain")]
    domain: Option<u32>,
    #[serde(rename = "GenerationType")]
    generation_type: Option<u32>,
    #[serde(rename = "Level")]
    level: Option<i64>,
    #[serde(rename = "Stat1")]
    stat1: Option<usize>,
    #[serde(rename = "Stat2")]
    stat2: Option<usize>,
    #[serde(rename = "Stat3")]
    stat3: Option<usize>,
    #[serde(rename = "Stat4")]
    stat4: Option<usize>,
    #[serde(rename = "Stat1Value", default)]
    stat1_value: [i64; 2],
    #[serde(rename = "Stat2Value", default)]
    stat2_value: [i64; 2],
    #[serde(rename = "Stat3Value", default)]
    stat3_value: [i64; 2],
    #[serde(rename = "Stat4Value", default)]
    stat4_value: [i64; 2],
    #[serde(rename = "Tags", default)]
    tags: Vec<usize>,
    #[serde(rename = "Families")]
    families: Option<Vec<usize>>,
    #[serde(rename = "SpawnWeight_Tags", default)]
    spawn_weight_tags: Vec<usize>,
    #[serde(rename = "SpawnWeight_Values", default)]
    spawn_weight_values: Vec<i64>,
}

/// Adapts the Stats + Mods domains, writing `base/stats.json` /
/// `base/mods.json` / `i18n/zh-TW/mods.json` (the i18n sidecar stays at the
/// version root `version_dir`). Returns `(stats count, mods kept, mods
/// filtered, zh-TW name count)`.
pub fn adapt(
    en: &Path,
    tw: &Path,
    stat_lookup: &[String],
    tags_lookup: &[String],
    base_dir: &Path,
    version_dir: &Path,
) -> Result<(usize, usize, usize, usize), String> {
    let stats = adapt_stats(en, base_dir)?;
    let (kept, filtered, zh) = adapt_mods(en, tw, stat_lookup, tags_lookup, base_dir, version_dir)?;
    Ok((stats, kept, filtered, zh))
}

/// Builds a position-indexed group-name lookup from `ModType.json`
/// (`_index` + `Name`). Degrades to an empty table when older table
/// snapshots lack this table (the group field is missing for the whole column; non-fatal).
fn mod_type_lookup(en: &Path) -> Vec<String> {
    #[derive(Deserialize)]
    struct RawModType {
        #[serde(rename = "_index")]
        index: usize,
        #[serde(rename = "Name")]
        name: Option<String>,
    }
    let Ok(rows) = read_json::<Vec<RawModType>>(&en.join("ModType.json")) else {
        eprintln!(
            "warning: pobr-data-adapter: ModType.json missing/unreadable -- the entire group field column in mods.json will default"
        );
        return Vec::new();
    };
    let max = rows.iter().map(|r| r.index).max().map_or(0, |m| m + 1);
    let mut table = vec![String::new(); max];
    for r in rows {
        if let Some(n) = r.name {
            table[r.index] = n;
        }
    }
    table
}

fn mod_family_lookup(en: &Path, mods: &[RawMod]) -> Result<BTreeMap<usize, String>, String> {
    // Legacy exports and explicitly empty lists need no lookup table.
    let Some((source, first_index)) = mods
        .iter()
        .find_map(|raw| raw.families.as_ref()?.first().map(|&index| (raw, index)))
    else {
        return Ok(BTreeMap::new());
    };
    let rows = read_json::<Vec<RawIndexed>>(&en.join("ModFamily.json")).map_err(|error| {
        format!(
            "Mods {}: Families reference {first_index} -> ModFamily.Id: {error}",
            source.id
        )
    })?;
    let mut families = BTreeMap::new();
    for row in rows {
        if families.insert(row.index, row.id).is_some() {
            return Err(format!("ModFamily.json: duplicate _index {}", row.index));
        }
    }
    Ok(families)
}

fn resolve_families(
    raw: &RawMod,
    lookup: &BTreeMap<usize, String>,
) -> Result<Option<Vec<String>>, String> {
    raw.families
        .as_ref()
        .map(|indices| {
            indices
                .iter()
                .map(|index| {
                    lookup
                        .get(index)
                        .filter(|id| !id.is_empty())
                        .cloned()
                        .ok_or_else(|| {
                            format!(
                                "Mods {}: dangling Families reference {index} -> ModFamily.Id",
                                raw.id
                            )
                        })
                })
                .collect()
        })
        .transpose()
}

fn adapt_stats(en: &Path, base_dir: &Path) -> Result<usize, String> {
    let raw = read_json::<Vec<RawStat>>(&en.join("Stats.json"))?;
    let mut stats: Vec<StatDef> = raw
        .into_iter()
        .filter(|s| !s.id.is_empty())
        .map(|s| StatDef {
            id: s.id,
            is_local: s.is_local.unwrap_or(false),
            semantic: s.semantic,
            category: s.category,
        })
        .collect();
    stats.sort_by(|a, b| a.id.cmp(&b.id));
    write_pretty(&base_dir.join("stats.json"), &stats)?;
    Ok(stats.len())
}

fn adapt_mods(
    en: &Path,
    tw: &Path,
    stat_lookup: &[String],
    tags_lookup: &[String],
    base_dir: &Path,
    version_dir: &Path,
) -> Result<(usize, usize, usize), String> {
    let raw_mods = read_json::<Vec<RawMod>>(&en.join("Mods.json"))?;
    let mod_types = mod_type_lookup(en);
    let mod_families = mod_family_lookup(en, &raw_mods)?;
    let tw_names = read_json::<Vec<RawNamed>>(&tw.join("Mods.json"))?;
    let tw_by_index: BTreeMap<usize, String> = tw_names
        .into_iter()
        .filter_map(|r| r.name.map(|n| (r.index, n)))
        .collect();

    let total = raw_mods.len();
    let mut mods: Vec<ModDef> = Vec::new();
    let mut i18n_zh: BTreeMap<String, String> = BTreeMap::new();

    for (index, raw) in raw_mods.into_iter().enumerate() {
        let families = resolve_families(&raw, &mod_families)?;
        let stats = build_stats(&raw, stat_lookup);
        let name = raw.name.filter(|n| !n.is_empty());

        // Filter: a pure placeholder shell with neither a display name nor any stat slots.
        if name.is_none() && stats.is_empty() {
            continue;
        }

        if let Some(en_name) = &name
            && let Some(zh) = tw_by_index.get(&index)
            && !zh.is_empty()
            && zh != en_name
        {
            i18n_zh.insert(raw.id.clone(), zh.clone());
        }

        let tags: Vec<String> = raw
            .tags
            .iter()
            .filter_map(|&i| resolve(tags_lookup, i))
            .collect();

        // SpawnWeight_Tags/Values are parallel arrays; order matters (resolution takes the first matching tag).
        let spawn_weights: Vec<SpawnWeight> = raw
            .spawn_weight_tags
            .iter()
            .zip(raw.spawn_weight_values.iter())
            .filter_map(|(&tag_idx, &weight)| {
                Some(SpawnWeight {
                    tag: resolve(tags_lookup, tag_idx)?,
                    weight: weight.max(0) as u32,
                })
            })
            .collect();

        mods.push(ModDef {
            id: raw.id,
            name,
            mod_type: raw.mod_type,
            domain: raw.domain.unwrap_or(0),
            generation_type: raw.generation_type,
            level: raw.level.unwrap_or(0).max(0) as u32,
            stats,
            tags,
            group: raw.mod_type.and_then(|i| resolve(&mod_types, i as usize)),
            families,
            spawn_weights,
        });
    }

    mods.sort_by(|a, b| a.id.cmp(&b.id));
    let kept = mods.len();
    let filtered = total - kept;

    write_pretty(&base_dir.join("mods.json"), &mods)?;
    write_pretty(&version_dir.join("i18n/zh-TW/mods.json"), &i18n_zh)?;

    Ok((kept, filtered, i18n_zh.len()))
}

/// Merges the Stat1..4 foreign keys plus their StatNValue rolled ranges into
/// a stats array, skipping empty slots and unresolvable foreign keys.
fn build_stats(raw: &RawMod, stat_lookup: &[String]) -> Vec<ModStat> {
    let slots = [
        (raw.stat1, raw.stat1_value),
        (raw.stat2, raw.stat2_value),
        (raw.stat3, raw.stat3_value),
        (raw.stat4, raw.stat4_value),
    ];
    slots
        .into_iter()
        .filter_map(|(stat, value)| {
            let idx = stat?;
            let stat_id = resolve(stat_lookup, idx)?;
            Some(ModStat {
                stat_id,
                min: value[0],
                max: value[1],
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};
    use std::fs;

    fn adapt_sample(name: &str, mods: Value, families: Option<&str>) -> Result<Value, String> {
        let root =
            std::env::temp_dir().join(format!("pobr-mod-families-{}-{name}", std::process::id()));
        let en = root.join("English");
        let tw = root.join("Traditional Chinese");
        let base = root.join("out/base");
        fs::create_dir_all(&en).unwrap();
        fs::create_dir_all(&tw).unwrap();
        fs::create_dir_all(&base).unwrap();
        fs::create_dir_all(root.join("out/i18n/zh-TW")).unwrap();
        fs::write(en.join("Mods.json"), mods.to_string()).unwrap();
        fs::write(
            en.join("ModType.json"),
            r#"[{"_index":9,"Name":"TierGroup"}]"#,
        )
        .unwrap();
        fs::write(
            tw.join("Mods.json"),
            r#"[{"_index":11,"Name":"Translated"}]"#,
        )
        .unwrap();
        if let Some(table) = families {
            fs::write(en.join("ModFamily.json"), table).unwrap();
        }
        let result = adapt_mods(
            &en,
            &tw,
            &["additional_strength".into()],
            &["ring".into()],
            &base,
            &root.join("out"),
        )
        .and_then(|_| read_json(&base.join("mods.json")));
        fs::remove_dir_all(root).unwrap();
        result
    }

    fn sample_mod() -> Value {
        json!({
            "_index": 11, "Id": "SampleMod", "Name": "Named", "ModType": 9,
            "Domain": 1, "GenerationType": 2, "Level": 1,
            "Stat1": 0, "Stat1Value": [5, 8], "Tags": [0],
            "SpawnWeight_Tags": [0], "SpawnWeight_Values": [100]
        })
    }

    #[test]
    fn families_resolve_all_sparse_permuted_indices_independently_of_group() {
        let mut raw = sample_mod();
        raw["Families"] = json!([40, 0, 7]);
        for (name, table) in [
            (
                "permuted",
                r#"[{"_index":7,"Id":"FamilySeven"},{"_index":40,"Id":"FamilyForty"},{"_index":0,"Id":"FamilyZero"}]"#,
            ),
            (
                "reordered",
                r#"[{"_index":0,"Id":"FamilyZero"},{"_index":7,"Id":"FamilySeven"},{"_index":40,"Id":"FamilyForty"}]"#,
            ),
        ] {
            let output = adapt_sample(name, json!([raw]), Some(table)).unwrap();
            assert_eq!(
                output[0]["families"],
                json!(["FamilyForty", "FamilyZero", "FamilySeven"]),
                "{name}"
            );
            assert_eq!(output[0]["group"], "TierGroup");
            assert_eq!(output[0]["mod_type"], 9);
            assert_eq!(output[0]["stats"][0]["stat_id"], "additional_strength");
            assert_eq!(
                output[0]["spawn_weights"],
                json!([{"tag":"ring","weight":100}])
            );
        }
    }

    #[test]
    fn families_distinguish_unknown_from_explicitly_empty_without_a_lookup_table() {
        for (name, field) in [
            ("absent", None),
            ("null", Some(Value::Null)),
            ("empty", Some(json!([]))),
        ] {
            let mut raw = sample_mod();
            if let Some(field) = field {
                raw["Families"] = field;
            }
            let output = adapt_sample(name, json!([raw]), None).unwrap();
            if name == "empty" {
                assert_eq!(output[0].get("families"), Some(&json!([])));
            } else {
                assert_eq!(output[0].get("families"), None, "{name}");
            }
            assert_eq!(output[0]["group"], "TierGroup");
        }
    }

    #[test]
    fn families_reject_dangling_references_with_source_and_target_context() {
        for (name, table) in [
            ("missing-table", None),
            ("empty-table", Some("[]")),
            ("sparse-hole", Some(r#"[{"_index":40,"Id":"Unrelated"}]"#)),
            ("empty-id", Some(r#"[{"_index":7,"Id":""}]"#)),
            ("malformed-table", Some("{}")),
        ] {
            let mut raw = sample_mod();
            raw["Families"] = json!([7]);
            let error = adapt_sample(name, json!([raw]), table).unwrap_err();
            for context in ["SampleMod", "Families", "ModFamily", "7"] {
                assert!(error.contains(context), "{name}: {error}");
            }
        }
    }

    #[test]
    fn families_validate_references_before_filtering_placeholder_mods() {
        let raw = json!([{"_index":11,"Id":"Shell","Families":[7]}]);
        let error = adapt_sample("filtered-dangling", raw, Some("[]")).unwrap_err();
        assert!(
            error.contains("Shell") && error.contains("Families"),
            "{error}"
        );
    }

    #[test]
    fn families_reject_partial_resolution() {
        let mut raw = sample_mod();
        raw["Families"] = json!([7, 99]);
        let error = adapt_sample(
            "partial-dangling",
            json!([raw]),
            Some(r#"[{"_index":7,"Id":"Valid"}]"#),
        )
        .unwrap_err();
        for context in ["SampleMod", "Families", "99", "ModFamily"] {
            assert!(error.contains(context), "{error}");
        }
    }

    #[test]
    fn families_reject_duplicate_lookup_indices() {
        let mut raw = sample_mod();
        raw["Families"] = json!([7]);
        let error = adapt_sample(
            "duplicate-index",
            json!([raw]),
            Some(r#"[{"_index":7,"Id":"First"},{"_index":7,"Id":"Second"}]"#),
        )
        .unwrap_err();
        assert!(
            error.contains("ModFamily") && error.contains("7"),
            "{error}"
        );
    }
}
