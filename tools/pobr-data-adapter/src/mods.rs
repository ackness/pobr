//! Mods + Stats domain adapter: raw `Mods.json` / `Stats.json` -> PoBR's minimal JSON.
//!
//! - `Stats.json` -> `stats.json` (the stat registry: id / is_local / semantic / category).
//! - `Mods.json` -> `mods.json` (the affix pool: merges the Stat1..4 foreign
//!   keys with the Stat1Value..4Value rolled ranges, resolves the Tags
//!   foreign key, and keeps domain / generation_type / mod_type / level).
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

use crate::{RawNamed, read_json, resolve, write_pretty};

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
        eprintln!("⚠ pobr-data-adapter：ModType.json 缺失/不可读——mods.json 的 group 字段整列缺省");
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
    let tw_names = read_json::<Vec<RawNamed>>(&tw.join("Mods.json"))?;
    let tw_by_index: BTreeMap<usize, String> = tw_names
        .into_iter()
        .filter_map(|r| r.name.map(|n| (r.index, n)))
        .collect();

    let total = raw_mods.len();
    let mut mods: Vec<ModDef> = Vec::new();
    let mut i18n_zh: BTreeMap<String, String> = BTreeMap::new();

    for (index, raw) in raw_mods.into_iter().enumerate() {
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
