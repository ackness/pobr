use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use pobr_data::prelude::*;
use regex::Regex;

pub mod extract_gem_effects;
pub mod extract_lua;
pub mod extract_parser_rules;
pub mod extract_quality;
pub mod extract_stat_map;
pub mod extract_stat_set_labels;

pub fn collect_catalog(pob_root: &Path) -> io::Result<PobCatalog> {
    let modules = pob_root.join("src").join("Modules");
    let mut display_stats = Vec::new();
    let mut output_keys = SourceIndex::default();
    let mut breakdowns = SourceIndex::default();

    if let Some(content) = read_optional(&modules.join("BuildDisplayStats.lua"))? {
        collect_display_stats(&content, &mut display_stats, &mut output_keys);
    }

    if let Some(content) = read_optional(&modules.join("CalcSections.lua"))? {
        collect_section_refs(
            &content,
            "CalcSections.lua",
            &mut output_keys,
            &mut breakdowns,
        );
    }

    for file_name in [
        "CalcOffence.lua",
        "CalcDefence.lua",
        "Calcs.lua",
        "CalcPerform.lua",
    ] {
        if let Some(content) = read_optional(&modules.join(file_name))? {
            collect_output_writes(&content, file_name, &mut output_keys);
        }
    }

    if let Some(content) = read_optional(&modules.join("Data.lua"))? {
        collect_power_stat_list(&content, &mut output_keys);
    }

    display_stats.sort_by(|left, right| {
        left.id
            .cmp(&right.id)
            .then_with(|| left.label.cmp(&right.label))
            .then_with(|| left.format.cmp(&right.format))
    });

    Ok(PobCatalog {
        display_stats,
        output_keys: output_keys.into_output_entries(),
        breakdowns: breakdowns.into_breakdown_entries(),
    })
}

pub fn write_catalog(catalog: &PobCatalog, path: &Path) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(catalog).map_err(io::Error::other)?;
    fs::write(path, format!("{json}\n"))
}

pub fn read_catalog(path: &Path) -> io::Result<PobCatalog> {
    let content = fs::read_to_string(path)?;
    serde_json::from_str(&content).map_err(io::Error::other)
}

fn collect_display_stats(
    content: &str,
    display_stats: &mut Vec<DisplayStatDefinition>,
    output_keys: &mut SourceIndex,
) {
    for line in content.lines() {
        if !line.contains("stat =") {
            continue;
        }

        let Some(stat) = capture_field(line, "stat") else {
            continue;
        };
        let id = if let Some(child_stat) = capture_field(line, "childStat") {
            format!("{stat}.{child_stat}")
        } else {
            stat
        };
        let label = capture_field(line, "label");
        let format = capture_field(line, "fmt");
        let lower_is_better = line.contains("lowerIsBetter = true");
        let hide_stat = line.contains("hideStat = true");
        let comparison_visible =
            line.contains("compPercent = true") || line.contains("overCapStat =");

        output_keys.add(&id, "BuildDisplayStats.lua");

        display_stats.push(DisplayStatDefinition {
            id: DisplayStatId::from(id.clone()),
            pob_key: Some(id.clone()),
            label,
            category: infer_category(&id),
            value_type: infer_value_type(&id, format.as_deref()),
            format,
            default_visible: !hide_stat,
            comparison_visible,
            higher_is_better: if lower_is_better {
                Some(false)
            } else if comparison_visible {
                Some(true)
            } else {
                None
            },
            breakdown_policy: BreakdownPolicy::Optional,
            parity_status: ParityStatus::Planned,
        });
    }
}

fn collect_section_refs(
    content: &str,
    source_file: &str,
    output_keys: &mut SourceIndex,
    breakdowns: &mut SourceIndex,
) {
    let output_ref = Regex::new(r":output:([A-Za-z0-9_.:]+)").expect("valid output regex");
    for captures in output_ref.captures_iter(content) {
        output_keys.add(&captures[1], source_file);
    }

    let have_output = Regex::new(r#"haveOutput\s*=\s*"([^"]+)""#).expect("valid haveOutput regex");
    for captures in have_output.captures_iter(content) {
        output_keys.add(&captures[1], source_file);
    }

    let breakdown = Regex::new(r#"breakdown\s*=\s*"([^"]+)""#).expect("valid breakdown regex");
    for captures in breakdown.captures_iter(content) {
        breakdowns.add(&captures[1], source_file);
    }
}

fn collect_output_writes(content: &str, source_file: &str, output_keys: &mut SourceIndex) {
    let dotted =
        Regex::new(r#"(?:^|[^A-Za-z0-9_])(?:actor\.|env\.player\.|env\.enemy\.)?output\.([A-Za-z_][A-Za-z0-9_]*)\s*="#)
            .expect("valid dotted output regex");
    for captures in dotted.captures_iter(content) {
        output_keys.add(&captures[1], source_file);
    }

    let indexed = Regex::new(r#"output\["([^"]+)"\]\s*="#).expect("valid indexed output regex");
    for captures in indexed.captures_iter(content) {
        output_keys.add(&captures[1], source_file);
    }
}

fn collect_power_stat_list(content: &str, output_keys: &mut SourceIndex) {
    let Some(start) = content.find("powerStatList") else {
        return;
    };
    let rest = &content[start..];
    let Some(open) = rest.find('{') else {
        return;
    };
    let Some(close) = rest[open + 1..].find('}') else {
        return;
    };
    let list = &rest[open + 1..open + 1 + close];
    let quoted = Regex::new(r#""([^"]+)""#).expect("valid quoted string regex");
    for captures in quoted.captures_iter(list) {
        output_keys.add(&captures[1], "Data.lua");
    }
}

fn capture_field(line: &str, field: &str) -> Option<String> {
    let pattern = format!(r#"{field}\s*=\s*"([^"]+)""#);
    let regex = Regex::new(&pattern).expect("valid field regex");
    regex.captures(line).map(|captures| captures[1].into())
}

fn infer_category(id: &str) -> DisplayStatCategory {
    let lower = id.to_ascii_lowercase();
    if lower.contains("resist") {
        DisplayStatCategory::Resistance
    } else if lower.contains("cost") {
        DisplayStatCategory::Cost
    } else if lower.starts_with("req") || matches!(id, "Str" | "Dex" | "Int" | "Omni" | "ReqOmni") {
        DisplayStatCategory::Requirement
    } else if lower.contains("regen")
        || lower.contains("recovery")
        || lower.contains("leech")
        || lower.contains("recoup")
    {
        DisplayStatCategory::Recovery
    } else if lower.contains("degen") {
        DisplayStatCategory::Degen
    } else if lower.contains("dps")
        || lower.contains("damage")
        || lower.contains("crit")
        || lower.contains("speed")
        || lower.contains("hitchance")
        || lower.contains("averagehit")
        || lower.contains("averagedamage")
    {
        DisplayStatCategory::Offence
    } else if lower.contains("life")
        || lower.contains("mana")
        || lower.contains("energyshield")
        || lower == "rage"
    {
        DisplayStatCategory::Resource
    } else if lower.contains("block")
        || lower.contains("dodge")
        || lower.contains("suppression")
        || lower.contains("avoid")
    {
        DisplayStatCategory::Avoidance
    } else if lower.contains("armour")
        || lower.contains("evasion")
        || lower.contains("ward")
        || lower.contains("ehp")
        || lower.contains("maximumhittaken")
        || lower.contains("damagereduction")
    {
        DisplayStatCategory::Defence
    } else if lower.contains("minion") {
        DisplayStatCategory::Minion
    } else if lower.contains("enemy") {
        DisplayStatCategory::Enemy
    } else {
        DisplayStatCategory::Misc
    }
}

fn infer_value_type(id: &str, format: Option<&str>) -> StatValueType {
    if let Some(format) = format {
        if format.contains("%%") || format.contains('%') {
            return StatValueType::Percent;
        }
        if format.contains('s') {
            return StatValueType::TimeSeconds;
        }
    }

    let lower = id.to_ascii_lowercase();
    if lower.contains("chance")
        || lower.contains("resist")
        || lower.ends_with("percent")
        || lower.contains("effectmod")
    {
        StatValueType::Percent
    } else {
        StatValueType::Number
    }
}

fn read_optional(path: &Path) -> io::Result<Option<String>> {
    match fs::read_to_string(path) {
        Ok(content) => Ok(Some(content)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

#[derive(Debug, Default)]
struct SourceIndex {
    entries: BTreeMap<String, BTreeSet<String>>,
}

impl SourceIndex {
    fn add(&mut self, key: &str, source_file: &str) {
        self.entries
            .entry(key.into())
            .or_default()
            .insert(source_file.into());
    }

    fn into_output_entries(self) -> Vec<PobOutputCatalogEntry> {
        self.entries
            .into_iter()
            .map(|(key, source_files)| PobOutputCatalogEntry {
                key: PobOutputKey::from(key),
                source_files: source_files.into_iter().collect(),
                parity_status: ParityStatus::Planned,
            })
            .collect()
    }

    fn into_breakdown_entries(self) -> Vec<PobBreakdownCatalogEntry> {
        self.entries
            .into_iter()
            .map(|(id, source_files)| PobBreakdownCatalogEntry {
                id: DisplayStatId::from(id),
                source_files: source_files.into_iter().collect(),
                parity_status: ParityStatus::Planned,
            })
            .collect()
    }
}

pub fn modules_path(pob_root: &Path) -> PathBuf {
    pob_root.join("src").join("Modules")
}

/// Kind of difference between two catalogs at the output-key level.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffKind {
    /// A key present in `actual` but absent from `expected`.
    Added,
    /// A key present in `expected` but absent from `actual`.
    Removed,
    /// A key present in both, with a different [`ParityStatus`].
    StatusChanged,
    /// A key present in both with the same status, but different source files.
    SourceChanged,
}

/// A single discrepancy between an expected and an actual catalog.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogDiff {
    pub kind: DiffKind,
    pub key: String,
    pub detail: String,
}

/// Compare two catalogs' output keys and report every discrepancy.
///
/// Results are sorted by key so callers get a deterministic, diffable report.
/// Within a single key, at most one diff is produced (Added / Removed take
/// precedence, then StatusChanged, then SourceChanged).
pub fn diff_catalogs(expected: &PobCatalog, actual: &PobCatalog) -> Vec<CatalogDiff> {
    let expected_index = index_output_keys(expected);
    let actual_index = index_output_keys(actual);

    let mut keys: BTreeSet<&str> = BTreeSet::new();
    keys.extend(expected_index.keys().copied());
    keys.extend(actual_index.keys().copied());

    let mut diffs = Vec::new();
    for key in keys {
        match (expected_index.get(key), actual_index.get(key)) {
            (None, Some(_)) => diffs.push(CatalogDiff {
                kind: DiffKind::Added,
                key: key.to_string(),
                detail: format!("`{key}` present in actual but missing from expected"),
            }),
            (Some(_), None) => diffs.push(CatalogDiff {
                kind: DiffKind::Removed,
                key: key.to_string(),
                detail: format!("`{key}` present in expected but missing from actual"),
            }),
            (Some(left), Some(right)) => {
                if left.parity_status != right.parity_status {
                    diffs.push(CatalogDiff {
                        kind: DiffKind::StatusChanged,
                        key: key.to_string(),
                        detail: format!(
                            "parity status changed: {:?} -> {:?}",
                            left.parity_status, right.parity_status
                        ),
                    });
                } else if left.source_files != right.source_files {
                    diffs.push(CatalogDiff {
                        kind: DiffKind::SourceChanged,
                        key: key.to_string(),
                        detail: format!(
                            "source files changed: {:?} -> {:?}",
                            left.source_files, right.source_files
                        ),
                    });
                }
            }
            (None, None) => unreachable!("key originated from one of the two indexes"),
        }
    }

    diffs
}

fn index_output_keys(catalog: &PobCatalog) -> BTreeMap<&str, &PobOutputCatalogEntry> {
    catalog
        .output_keys
        .iter()
        .map(|entry| (entry.key.as_str(), entry))
        .collect()
}

/// Write `catalog` to `path` as a self-contained JSON fixture.
///
/// "Self-contained" means it captures the full catalog snapshot (no external
/// references), so it can later be compared against a freshly scanned catalog
/// without needing the original PoB source tree. Creates parent dirs as needed.
pub fn write_self_contained_fixture(catalog: &PobCatalog, path: &Path) -> io::Result<()> {
    write_catalog(catalog, path)
}

/// Load the fixture at `fixture_path` and diff it against `actual`.
///
/// The fixture is treated as the expected baseline; any drift in `actual` is
/// returned as [`CatalogDiff`] entries (empty when the catalogs match).
pub fn check_against_fixture(
    fixture_path: &Path,
    actual: &PobCatalog,
) -> io::Result<Vec<CatalogDiff>> {
    let expected = read_catalog(fixture_path)?;
    Ok(diff_catalogs(&expected, actual))
}

/// Describe a key present in PoBR's display catalog but absent from the PoB fixture.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MissingPobKey {
    /// The PoBR display stat id (doubles as the pob_key we map to).
    pub pobr_id: String,
    /// The PoB output key we tried to find in the fixture.
    pub pob_key: String,
}

/// Cross-check PoBR's declared display stat definitions against a PoB fixture catalog.
///
/// For every `DisplayStatDefinition` in `pobr_catalog` that has a `pob_key` **and** is
/// `ParityStatus::Computed`, verify that the `pob_key` appears in the PoB
/// fixture's `output_keys`.  Returns the list of missing keys; empty means full
/// parity.
///
/// This is the CI gate: if PoBR claims to compute a stat but PoB doesn't know that
/// key at all, something is wrong with the mapping.
pub fn check_pobr_parity(
    pob_fixture: &PobCatalog,
    pobr_display_defs: &[DisplayStatDefinition],
) -> Vec<MissingPobKey> {
    let pob_key_set: BTreeSet<&str> = pob_fixture
        .output_keys
        .iter()
        .map(|e| e.key.as_str())
        .chain(pob_fixture.display_stats.iter().map(|s| s.id.as_str()))
        .collect();

    pobr_display_defs
        .iter()
        .filter(|def| def.parity_status == ParityStatus::Computed)
        .filter_map(|def| {
            let pob_key = def.pob_key.as_ref()?.as_str().to_string();
            if pob_key_set.contains(pob_key.as_str()) {
                None
            } else {
                Some(MissingPobKey {
                    pobr_id: def.id.as_str().to_string(),
                    pob_key,
                })
            }
        })
        .collect()
}
