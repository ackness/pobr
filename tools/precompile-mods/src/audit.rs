//! Source-wide parser audit, separate from the historical build coverage ratchet.
//! A parsed sample proves text recognition, not a calculation consumer or uptime.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use pobr_core::apply_range::apply_range;
use pobr_core::item_text::{parse_pob_xml_item, strip_pob_annotations};
use pobr_core::mod_parser::{ParseStatus, parse_mod_engine_diag};
use serde::{Deserialize, Serialize};
use serde_json::Value;

type Corpus = BTreeMap<String, BTreeSet<String>>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Parsed,
    RecognizedEmpty,
    Partial,
    Unsupported,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Entry {
    pub sources: BTreeSet<String>,
    pub status: Status,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vendor_status: Option<Status>,
    pub mod_names: BTreeSet<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Audit {
    pub schema: String,
    pub vendor_commit: String,
    #[serde(default)]
    pub source_metadata: BTreeMap<String, Value>,
    pub source_counts: BTreeMap<String, usize>,
    pub summary: BTreeMap<String, usize>,
    pub entries: BTreeMap<String, Entry>,
}

#[derive(Debug, Default, Serialize)]
pub struct Delta {
    pub regressions: BTreeMap<String, BTreeSet<String>>,
    pub new_gaps: BTreeSet<String>,
    pub resolved: BTreeSet<String>,
    pub removed: BTreeSet<String>,
}

fn read_json(path: &Path) -> Result<Value, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    serde_json::from_slice(&bytes).map_err(|e| format!("{}: {e}", path.display()))
}

fn add(corpus: &mut Corpus, text: &str, source: String) {
    let text = strip_pob_annotations(text);
    // Low/high samples exercise roll boundaries; no template becomes a rule.
    for roll in [0.0, 1.0] {
        let text = apply_range(&text, roll, None, 1.0);
        for line in text.replace("\\n", "\n").lines() {
            let line = line.trim();
            if !line.is_empty() {
                corpus
                    .entry(line.into())
                    .or_default()
                    .insert(source.clone());
            }
        }
    }
}

/// Render numeric description placeholders, retaining unsupported formats for review.
fn sample(template: &str) -> Option<String> {
    let mut result = String::new();
    let mut rest = template;
    while let Some(start) = rest.find('{') {
        result.push_str(&rest[..start]);
        let end = rest[start..].find('}')? + start;
        let (index, format) = rest[start + 1..end]
            .split_once(':')
            .unwrap_or((&rest[start + 1..end], ""));
        let index: usize = if index.is_empty() {
            0
        } else {
            index.parse().ok()?
        };
        if index > 9 || !matches!(format, "" | "d" | "+d") {
            return None;
        }
        if format == "+d" {
            result.push('+');
        }
        result.push_str(&(17 + index * 12).to_string());
        rest = &rest[end + 1..];
    }
    result.push_str(rest);
    Some(result.replace("%%", "%"))
}

fn collect(data: &Path) -> Result<(Corpus, BTreeMap<String, usize>), String> {
    let mut corpus = Corpus::new();
    let mut counts = BTreeMap::new();
    let descriptions = read_json(&data.join("overlay/stat_descriptions.json"))?;
    let scopes = descriptions["scopes"]
        .as_object()
        .ok_or("missing description scopes")?;
    for (scope_name, scope) in scopes {
        for (stat, lines) in scope["single"]
            .as_object()
            .ok_or("missing single descriptions")?
        {
            for line in lines.as_array().ok_or("invalid stat lines")? {
                add(
                    &mut corpus,
                    line.as_str().ok_or("invalid stat line")?,
                    format!("stat:{scope_name}:{stat}"),
                );
            }
        }
        if let Some(compounds) = scope["compound"].as_object() {
            for (stat, compound) in compounds {
                if let Some(text) = compound["template"].as_str().and_then(sample) {
                    add(&mut corpus, &text, format!("compound:{scope_name}:{stat}"));
                } else {
                    *counts.entry("unsampled_compounds".into()).or_default() += 1;
                }
            }
        }
        *counts.entry("unrendered_stats".into()).or_default() +=
            scope["unrendered"].as_array().map_or(0, Vec::len);
    }
    let catalog = read_json(&data.join("overlay/trade_catalog.json"))?;
    for group in ["mods", "search_mods", "bases"] {
        for row in catalog[group]
            .as_array()
            .ok_or_else(|| format!("missing trade {group}"))?
        {
            let id = row["id"]
                .as_str()
                .or_else(|| row["name"].as_str())
                .ok_or("missing catalog id")?;
            let field = if group == "bases" {
                "implicits"
            } else {
                "lines"
            };
            for line in row[field].as_array().into_iter().flatten() {
                add(
                    &mut corpus,
                    line.as_str().ok_or("invalid catalog line")?,
                    format!("{group}:{id}"),
                );
            }
        }
    }
    let uniques = read_json(&data.join("overlay/uniques.json"))?;
    for row in uniques["uniques"].as_array().ok_or("missing uniques")? {
        let raw = row["raw"].as_str().ok_or("missing unique text")?;
        // Strip variant gates before ingest so archived and current variants are audited.
        let raw = raw
            .lines()
            .map(strip_pob_annotations)
            .collect::<Vec<_>>()
            .join("\n");
        let item = parse_pob_xml_item(&format!("Rarity: UNIQUE\n{raw}"))
            .map_err(|e| format!("invalid unique {}: {e}", row["name"]))?;
        for text in item
            .implicit_texts
            .iter()
            .chain(&item.modifier_texts)
            .chain(&item.enchant_texts)
        {
            add(
                &mut corpus,
                text,
                format!(
                    "unique:{}",
                    row["name"].as_str().ok_or("missing unique name")?
                ),
            );
        }
    }
    let tree = read_json(&data.join("base/passive_tree.json"))?;
    // The adapter stores node arrays; accept the documented wrapper too.
    let nodes = tree
        .as_array()
        .or_else(|| tree["nodes"].as_array())
        .ok_or("missing passive nodes")?;
    for row in nodes {
        for text in row["stats"].as_array().into_iter().flatten() {
            add(
                &mut corpus,
                text.as_str().ok_or("invalid passive text")?,
                format!("passive:{}", row["skill"]),
            );
        }
    }
    // These English templates are the actual output of CN import translation.
    let translations_path = data.join("i18n/zh-CN/stat_lines.json");
    if translations_path.is_file() {
        let translations = read_json(&translations_path)?;
        for pair in translations
            .as_array()
            .ok_or("invalid translation templates")?
        {
            if let Some(text) = pair["en"].as_str().and_then(sample) {
                add(
                    &mut corpus,
                    &text,
                    format!(
                        "translation:{}",
                        pair["src"].as_str().ok_or("missing source template")?
                    ),
                );
            } else {
                *counts.entry("unsampled_translations".into()).or_default() += 1;
            }
        }
    } else {
        counts.insert("missing_translation_catalog".into(), 1);
    }
    for sources in corpus.values() {
        for domain in sources
            .iter()
            .filter_map(|s| s.split(':').next())
            .collect::<BTreeSet<_>>()
        {
            *counts.entry(domain.into()).or_default() += 1;
        }
    }
    Ok((corpus, counts))
}

pub fn write_corpus(data: &Path, path: &Path) -> Result<(), String> {
    let (corpus, _) = collect(data)?;
    std::fs::write(
        path,
        corpus.keys().cloned().collect::<Vec<_>>().join("\n") + "\n",
    )
    .map_err(|e| format!("{}: {e}", path.display()))
}

fn vendor_results(path: &Path) -> Result<BTreeMap<String, Status>, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut results = BTreeMap::new();
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let row: Value =
            serde_json::from_str(line).map_err(|e| format!("invalid oracle row: {e}"))?;
        let text = row["line"].as_str().ok_or("oracle row lacks line")?;
        let status = if row["unsupported"]
            .as_bool()
            .ok_or("oracle row lacks status")?
        {
            Status::Unsupported
        } else if row["leftover"]
            .as_str()
            .is_some_and(|s| !s.trim().is_empty())
        {
            Status::Partial
        } else {
            Status::Parsed
        };
        if results.insert(text.into(), status).is_some() {
            return Err("duplicate oracle line".into());
        }
    }
    Ok(results)
}

pub fn build(data: &Path, oracle: Option<&Path>) -> Result<Audit, String> {
    let (corpus, source_counts) = collect(data)?;
    let rules = crate::parsed::compile_parser_rules(data)?;
    let vendor = oracle.map(vendor_results).transpose()?;
    let mut entries = BTreeMap::new();
    let mut summary = BTreeMap::new();
    for (text, sources) in corpus {
        let (outcome, diag) = parse_mod_engine_diag(&text, &rules);
        let status = if diag.dropped_pre_flag_tags > 0
            || outcome
                .unparsed
                .as_ref()
                .is_some_and(|s| !s.trim().is_empty())
                && outcome.status == ParseStatus::Parsed
        {
            Status::Partial
        } else if outcome.status == ParseStatus::Unsupported {
            Status::Unsupported
        } else if outcome.mods.is_empty() {
            Status::RecognizedEmpty
        } else {
            Status::Parsed
        };
        let vendor_status = match &vendor {
            Some(rows) => Some(
                *rows
                    .get(&text)
                    .ok_or_else(|| format!("oracle omitted {text:?}"))?,
            ),
            None => None,
        };
        let class = match (status, vendor_status) {
            (Status::Parsed, _) => "parsed",
            (Status::RecognizedEmpty, _) => "recognized_empty",
            (_, Some(Status::Parsed)) => "pobr_gap",
            (_, Some(_)) => "upstream_gap",
            _ => "uncompared_gap",
        };
        *summary.entry(class.into()).or_default() += 1;
        entries.insert(
            text,
            Entry {
                sources,
                status,
                vendor_status,
                mod_names: outcome.mods.iter().map(|m| m.name.to_string()).collect(),
            },
        );
    }
    let mut source_metadata = BTreeMap::new();
    for file in [
        "overlay/mod_parser_rules.json",
        "overlay/stat_descriptions.json",
        "overlay/trade_catalog.json",
        "overlay/uniques.json",
        "generated/special_vendor.json",
        "i18n/zh-CN/_meta.json",
    ] {
        let path = data.join(file);
        if path.is_file() {
            let document = read_json(&path)?;
            let meta = if file.ends_with("/_meta.json") {
                &document
            } else {
                &document["_meta"]
            };
            let selected: serde_json::Map<String, Value> =
                ["vendor_commit", "source_commit", "source_generated_at"]
                    .iter()
                    .filter_map(|key| meta.get(key).map(|value| (key.to_string(), value.clone())))
                    .collect();
            source_metadata.insert(file.into(), Value::Object(selected));
        }
    }
    let rules = read_json(&data.join("overlay/mod_parser_rules.json"))?;
    Ok(Audit {
        schema: "modifier-audit/v1".into(),
        vendor_commit: rules["_meta"]["vendor_commit"]
            .as_str()
            .unwrap_or("unknown")
            .into(),
        source_metadata,
        source_counts,
        summary,
        entries,
    })
}

pub fn compare(current: &Audit, previous: &Audit) -> Delta {
    let mut delta = Delta::default();
    let previous_sources: BTreeSet<_> = previous
        .entries
        .iter()
        .filter(|(_, entry)| entry.status == Status::Parsed)
        .flat_map(|(text, entry)| {
            entry
                .sources
                .iter()
                .filter(|source| {
                    current
                        .entries
                        .get(text)
                        .is_none_or(|now| !now.sources.contains(*source))
                })
                .cloned()
        })
        .collect();
    for (text, entry) in &current.entries {
        let old = previous.entries.get(text);
        if entry.status != Status::Parsed {
            let regressions = if old.is_some_and(|e| e.status == Status::Parsed) {
                entry.sources.clone()
            } else if old.is_none() {
                entry
                    .sources
                    .intersection(&previous_sources)
                    .cloned()
                    .collect()
            } else {
                BTreeSet::new()
            };
            if !regressions.is_empty() {
                delta.regressions.insert(text.clone(), regressions);
            }
            if old.is_none() {
                delta.new_gaps.insert(text.clone());
            }
        } else if old.is_some_and(|e| e.status != Status::Parsed) {
            delta.resolved.insert(text.clone());
        }
    }
    delta.removed = previous
        .entries
        .keys()
        .filter(|text| !current.entries.contains_key(*text))
        .cloned()
        .collect();
    delta
}

pub fn run(
    data: &Path,
    out: &Path,
    oracle: Option<&Path>,
    baseline: Option<&Path>,
) -> Result<(), String> {
    let current = build(data, oracle)?;
    let previous = baseline
        .map(|path| -> Result<Audit, String> {
            let value = read_json(path)?;
            let audit: Audit =
                serde_json::from_value(value).map_err(|e| format!("invalid baseline: {e}"))?;
            if audit.schema != current.schema {
                return Err("incompatible audit baseline schema".into());
            }
            Ok(audit)
        })
        .transpose()?;
    std::fs::write(out, crate::parsed::serialize_pretty_stable(&current)?)
        .map_err(|e| format!("{}: {e}", out.display()))?;
    eprintln!(
        "modifier audit: {} samples; {:?}",
        current.entries.len(),
        current.summary
    );
    if let Some(previous) = previous {
        let delta = compare(&current, &previous);
        let delta_path = out.with_extension("delta.json");
        std::fs::write(&delta_path, crate::parsed::serialize_pretty_stable(&delta)?)
            .map_err(|e| format!("{}: {e}", delta_path.display()))?;
        eprintln!(
            "modifier audit: {} regressions, {} new gaps, {} resolved, {} removed",
            delta.regressions.len(),
            delta.new_gaps.len(),
            delta.resolved.len(),
            delta.removed.len()
        );
        if !delta.regressions.is_empty() {
            return Err(format!(
                "parser regressions; inspect {}",
                delta_path.display()
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot(rows: &[(&str, &str, Status)]) -> Audit {
        Audit {
            schema: "modifier-audit/v1".into(),
            vendor_commit: "synthetic".into(),
            source_metadata: BTreeMap::new(),
            source_counts: BTreeMap::new(),
            summary: BTreeMap::new(),
            entries: rows
                .iter()
                .map(|(text, source, status)| {
                    (
                        text.to_string(),
                        Entry {
                            sources: BTreeSet::from([source.to_string()]),
                            status: *status,
                            vendor_status: None,
                            mod_names: BTreeSet::new(),
                        },
                    )
                })
                .collect(),
        }
    }

    #[test]
    fn audit_distinguishes_regressions_new_mechanics_and_removed_sources() {
        let before = snapshot(&[
            ("old wording", "stat:stable_id", Status::Parsed),
            ("known bonus", "unique:existing", Status::Parsed),
            ("resolved wording", "stat:resolved", Status::Unsupported),
            ("removed bonus", "stat:removed", Status::Parsed),
            ("conditional", "stat:condition", Status::Parsed),
        ]);
        let after = snapshot(&[
            ("new wording", "stat:stable_id", Status::Unsupported),
            ("known bonus", "unique:existing", Status::Parsed),
            ("new mechanic", "unique:existing", Status::Unsupported),
            ("resolved wording", "stat:resolved", Status::Parsed),
            ("conditional", "stat:condition", Status::Partial),
        ]);
        let delta = compare(&after, &before);
        assert_eq!(
            delta
                .regressions
                .keys()
                .map(String::as_str)
                .collect::<Vec<_>>(),
            ["conditional", "new wording"]
        );
        assert!(delta.new_gaps.contains("new mechanic"));
        assert!(delta.resolved.contains("resolved wording"));
        assert!(delta.removed.contains("removed bonus"));
        assert!(compare(&before, &before).regressions.is_empty());
    }

    #[test]
    fn templates_preserve_constants_positions_and_unsupported_formats() {
        assert_eq!(
            sample("Adds {1} to {0} Damage per 10 Strength"),
            Some("Adds 29 to 17 Damage per 10 Strength".into())
        );
        assert_eq!(
            sample("{0:+d}%% Resistance\\n{1:d} Life"),
            Some("+17% Resistance\\n29 Life".into())
        );
        assert!(sample("Grants {skill_name}").is_none());
        let mut corpus = Corpus::new();
        add(
            &mut corpus,
            "{variant:2}(3-7)% increased Speed",
            "unique:sample".into(),
        );
        assert_eq!(
            corpus.keys().map(String::as_str).collect::<Vec<_>>(),
            ["3% increased Speed", "7% increased Speed"]
        );
    }
}
