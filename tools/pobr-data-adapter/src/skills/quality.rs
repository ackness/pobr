//! Official gem-quality tables -> the existing quality domain.
//!
//! A reviewed, versioned receipt pins all input bytes and the supported effect
//! scope. Neither vendor Lua nor an old output is consulted during generation.
//! Stat-set scope columns are retained as diagnostics: current consumers apply
//! quality effect-wide, matching the pinned migration reference. Adding scope
//! semantics or additional effects is a separate calculation change.

use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use pobr_data::catalog::{GemQualityStatDef, QualityStat};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub(crate) struct QualityArgs {
    pub raw: PathBuf,
    pub source: PathBuf,
    pub out: PathBuf,
    pub patch: String,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Receipt {
    schema: String,
    poe_version: String,
    provenance: serde_json::Value,
    inputs: BTreeMap<String, String>,
    compatibility: Compatibility,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Compatibility {
    reference_vendor_commit: String,
    reference_sha256: String,
    scope_policy: String,
    effects: Vec<String>,
    excluded_effects: BTreeMap<String, String>,
}

#[derive(Deserialize)]
struct IndexedId {
    #[serde(rename = "_index")]
    index: usize,
    #[serde(rename = "Id")]
    id: String,
}

#[derive(Deserialize)]
struct RawQuality {
    #[serde(rename = "GrantedEffect")]
    effect: usize,
    #[serde(rename = "Stats")]
    stats: Vec<usize>,
    #[serde(rename = "StatsValuesPermille")]
    values: Vec<i32>,
    #[serde(rename = "ApplyToStatSets")]
    sets: Vec<u32>,
    #[serde(rename = "AltStats")]
    alt_stats: Vec<usize>,
    #[serde(rename = "AltStatValuesPermille")]
    alt_values: Vec<i32>,
    #[serde(rename = "AltApplyToStatSets")]
    alt_sets: Vec<u32>,
}

#[derive(Serialize)]
struct Document<'a> {
    #[serde(rename = "_meta")]
    meta: Metadata<'a>,
    effects: Vec<GemQualityStatDef>,
}

#[derive(Serialize)]
struct Metadata<'a> {
    schema: &'static str,
    generator: &'static str,
    poe_version: &'a str,
    source_receipt_sha256: String,
    provenance: &'a serde_json::Value,
    inputs: &'a BTreeMap<String, String>,
    compatibility: &'a Compatibility,
    /// Preserve the official columns without adding new runtime semantics.
    unapplied_stat_set_scopes: BTreeMap<String, Scope>,
    regen_command: String,
}

#[derive(Serialize)]
struct Scope {
    main: Vec<u32>,
    alt: Vec<u32>,
}

fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn checked_table<T: for<'de> Deserialize<'de>>(
    raw: &Path,
    inputs: &BTreeMap<String, String>,
    name: &str,
) -> Result<T, String> {
    let expected = inputs
        .get(name)
        .ok_or_else(|| format!("source receipt is missing {name}"))?;
    let bytes = fs::read(raw.join(name)).map_err(|e| format!("{name}: {e}"))?;
    if sha256(&bytes) != *expected {
        return Err(format!(
            "{name}: input fingerprint differs from the reviewed source receipt"
        ));
    }
    serde_json::from_slice(&bytes).map_err(|e| format!("{name}: {e}"))
}

fn index(rows: Vec<IndexedId>, table: &str) -> Result<BTreeMap<usize, String>, String> {
    let mut ids = BTreeSet::new();
    let mut by_index = BTreeMap::new();
    for row in rows {
        if row.id.is_empty()
            || !ids.insert(row.id.clone())
            || by_index.insert(row.index, row.id).is_some()
        {
            return Err(format!("{table}: empty or duplicate ID / row index"));
        }
    }
    Ok(by_index)
}

fn build(args: &QualityArgs) -> Result<String, String> {
    // Patch is used as a directory component and in the portable regen command.
    if args.patch.is_empty()
        || args.patch == "."
        || args.patch == ".."
        || !args
            .patch
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"._-".contains(&c))
    {
        return Err("invalid patch directory component".into());
    }
    let receipt_bytes = fs::read(&args.source).map_err(|e| format!("source receipt: {e}"))?;
    let receipt: Receipt =
        serde_json::from_slice(&receipt_bytes).map_err(|e| format!("source receipt: {e}"))?;
    if receipt.schema != "gem-quality-source/v1" || receipt.poe_version != args.patch {
        return Err(
            "source receipt schema / game version does not match the requested version".into(),
        );
    }
    if receipt.compatibility.scope_policy != "preserve_effect_wide_quality" {
        return Err("unsupported quality scope policy".into());
    }
    let enabled: BTreeSet<_> = receipt.compatibility.effects.iter().cloned().collect();
    let excluded = &receipt.compatibility.excluded_effects;
    if enabled.is_empty()
        || enabled.len() != receipt.compatibility.effects.len()
        || enabled
            .iter()
            .any(|id| id.is_empty() || excluded.contains_key(id))
        || excluded
            .iter()
            .any(|(id, reason)| id.is_empty() || reason.trim().is_empty())
    {
        return Err("invalid or overlapping quality compatibility scope".into());
    }
    let effects = index(
        checked_table(&args.raw, &receipt.inputs, "GrantedEffects.json")?,
        "GrantedEffects",
    )?;
    let stats = index(
        checked_table(&args.raw, &receipt.inputs, "Stats.json")?,
        "Stats",
    )?;
    let rows: Vec<RawQuality> =
        checked_table(&args.raw, &receipt.inputs, "GrantedEffectQualityStats.json")?;
    let mut seen = BTreeSet::new();
    let mut result = BTreeMap::new();
    let mut scopes = BTreeMap::new();
    for row in rows {
        let id = effects
            .get(&row.effect)
            .ok_or_else(|| format!("quality: dangling GrantedEffect {}", row.effect))?;
        if !seen.insert(id.clone()) {
            return Err(format!("quality: duplicate effect {id}"));
        }
        if !enabled.contains(id) && !excluded.contains_key(id) {
            return Err(format!(
                "quality: effect {id} has no reviewed compatibility decision"
            ));
        }
        let mut quality = Vec::new();
        for (keys, values, alt) in [
            (&row.stats, &row.values, false),
            (&row.alt_stats, &row.alt_values, true),
        ] {
            if keys.len() != values.len() {
                return Err(format!(
                    "quality {id}: stat/value length mismatch (alt={alt})"
                ));
            }
            for (&key, &value) in keys.iter().zip(values) {
                let stat = stats
                    .get(&key)
                    .ok_or_else(|| format!("quality {id}: dangling stat {key}"))?;
                quality.push(QualityStat {
                    stat: stat.clone(),
                    per_quality_rate: f64::from(value) / 1000.0,
                    alt,
                });
            }
        }
        if !row.sets.is_empty() || !row.alt_sets.is_empty() {
            scopes.insert(
                id.clone(),
                Scope {
                    main: row.sets,
                    alt: row.alt_sets,
                },
            );
        }
        if enabled.contains(id) {
            if quality.is_empty() {
                return Err(format!(
                    "quality {id}: reviewed effect has no quality stats"
                ));
            }
            result.insert(
                id.clone(),
                GemQualityStatDef {
                    effect_id: id.clone(),
                    stats: quality,
                },
            );
        }
    }
    for id in enabled.iter().chain(excluded.keys()) {
        if !seen.contains(id) {
            return Err(format!(
                "quality: reviewed effect {id} disappeared from the input"
            ));
        }
    }
    let doc = Document {
        meta: Metadata {
            schema: "gem_quality_stats/v1",
            generator: "pobr-data-adapter --gem-quality",
            poe_version: &args.patch,
            source_receipt_sha256: sha256(&receipt_bytes),
            provenance: &receipt.provenance,
            inputs: &receipt.inputs,
            compatibility: &receipt.compatibility,
            unapplied_stat_set_scopes: scopes,
            regen_command: format!(
                "cargo run -p pobr-data-adapter -- --gem-quality pipeline/tables/English --quality-source pipeline/gem-quality/{}.json --out data --patch {}",
                args.patch, args.patch
            ),
        },
        effects: result.into_values().collect(),
    };
    let json = serde_json::to_string_pretty(&doc).map_err(|e| e.to_string())?;
    Ok(json + "\n")
}

pub(crate) fn run(args: QualityArgs) -> Result<String, String> {
    // Complete all reads and validation before touching the destination.
    let json = build(&args)?;
    let dir = args.out.join(&args.patch).join("overlay");
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let dest = dir.join("gem_quality_stats.json");
    let temp = dir.join(format!(".gem-quality-{}.tmp", std::process::id()));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp)
        .map_err(|e| format!("{}: {e}", temp.display()))?;
    let result = file
        .write_all(json.as_bytes())
        .and_then(|()| file.sync_all());
    drop(file);
    let result = result.and_then(|()| fs::rename(&temp, &dest));
    if let Err(error) = result {
        let _ = fs::remove_file(&temp);
        return Err(format!("{}: {error}", dest.display()));
    }
    Ok(format!(
        "gem quality: verified official inputs -> {}",
        dest.display()
    ))
}

#[cfg(test)]
mod tests;
