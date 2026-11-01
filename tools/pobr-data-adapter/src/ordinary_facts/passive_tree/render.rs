//! Bounded CSD rendering to the existing ordinary English-text channel.
//!
//! This does not interpret game mechanics. Unknown syntax/handlers reject the
//! entire node, so callers can require an exact, declared compatibility binding.

mod number;
#[cfg(test)]
mod tests;

use number::Number;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

const SCOPES: [&str; 2] = [
    "Data/StatDescriptions/passive_skill_stat_descriptions.csd",
    "Data/StatDescriptions/stat_descriptions.csd",
];

pub(in crate::ordinary_facts) struct Renderer<'a> {
    descriptions: BTreeMap<String, Vec<(&'a Value, String)>>,
    equivalents: BTreeMap<String, Vec<Value>>,
}

impl<'a> Renderer<'a> {
    pub fn new(document: &'a Value) -> Result<Self, String> {
        Self::scoped(document, &SCOPES)
    }

    /// Item socketables use the global item-stat scope, without passive overrides.
    pub fn global(document: &'a Value) -> Result<Self, String> {
        Self::scoped(document, &SCOPES[1..])
    }

    fn scoped(document: &'a Value, scopes: &[&str]) -> Result<Self, String> {
        let mut descriptions: BTreeMap<String, Vec<(&Value, String)>> = BTreeMap::new();
        let mut equivalents = BTreeMap::new();
        for &path in scopes {
            let file = &document["files"][path];
            if file["path"] != path {
                return Err(format!("missing CSD scope {path}"));
            }
            let records = file["records"].as_array().ok_or("missing CSD records")?;
            let includes: Vec<_> = records
                .iter()
                .filter(|r| r["kind"] == "include")
                .map(|r| r["target"].as_str())
                .collect();
            let expected = if path == SCOPES[0] {
                vec![Some(SCOPES[1])]
            } else {
                vec![]
            };
            if includes != expected {
                return Err("unsupported passive CSD include graph".into());
            }
            let mut own: BTreeMap<String, Vec<_>> = BTreeMap::new();
            for (index, record) in records.iter().enumerate() {
                match record["kind"].as_str() {
                    Some("description" | "no_description") => {
                        let mut seen = BTreeSet::new();
                        for id in record["ids"].as_array().ok_or("missing description ids")? {
                            let id = id.as_str().ok_or("invalid description id")?;
                            if !seen.insert(id) {
                                return Err("duplicate CSD descriptor id".into());
                            }
                            own.entry(id.into()).or_default().push((
                                record,
                                format!("/files/{}/records/{index}", super::prepared::token(path)),
                            ));
                        }
                    }
                    Some("include" | "blank") => {}
                    _ => return Err(format!("unsupported CSD record in {path}")),
                }
            }
            // Explicit child scope owns a stat even when its handler is unsupported.
            for (id, mut records) in own {
                if records.len() > 1 && records.iter().all(|(r, _)| r["kind"] == "description") {
                    let signature = english_semantics(records[0].0)?;
                    let same = records
                        .iter()
                        .skip(1)
                        .map(|(r, _)| english_semantics(r))
                        .collect::<Result<Vec<_>, _>>()?
                        .iter()
                        .all(|s| *s == signature);
                    if same {
                        equivalents.insert(records[0].1.clone(), records.iter().map(|(r, p)|
                            json!({"pointer":p,"raw_slice_sha256":r["raw_slice_sha256"]})).collect());
                        records.truncate(1);
                    }
                }
                descriptions.entry(id).or_insert(records);
            }
        }
        Ok(Self {
            descriptions,
            equivalents,
        })
    }

    pub fn render(&self, stats: &[(String, i64)]) -> Result<(Vec<String>, Vec<Value>), String> {
        let values: BTreeMap<_, _> = stats.iter().map(|(id, v)| (id.as_str(), *v)).collect();
        if values.len() != stats.len() {
            return Err("duplicate source stat occurrence".into());
        }
        let mut consumed = BTreeSet::new();
        let mut lines = Vec::new();
        let mut evidence = Vec::new();
        for (id, _) in stats {
            if consumed.contains(id.as_str()) {
                continue;
            }
            let records = self
                .descriptions
                .get(id)
                .ok_or_else(|| format!("no CSD description for {id}"))?;
            let [(record, pointer)] = records.as_slice() else {
                return Err(format!("ambiguous CSD descriptor ownership for {id}"));
            };
            if record["kind"] != "description" {
                return Err(format!("no_description for {id} is not an effectless law"));
            }
            let ids: Vec<_> = record["ids"]
                .as_array()
                .ok_or("missing descriptor ids")?
                .iter()
                .map(|v| v.as_str().ok_or("invalid descriptor id"))
                .collect::<Result<_, _>>()?;
            // Every compound member must select this same unique descriptor.
            // Otherwise a parent compound could consume a child-owned stat before
            // the child's unsupported handler or ambiguity is examined.
            for member in &ids {
                let owners = self
                    .descriptions
                    .get(*member)
                    .ok_or_else(|| format!("no CSD description for {member}"))?;
                if !matches!(owners.as_slice(), [(_, owner)] if owner == pointer) {
                    return Err(format!("conflicting CSD compound ownership for {member}"));
                }
            }
            let raw_values: Vec<_> = ids
                .iter()
                .map(|id| {
                    values
                        .get(id)
                        .copied()
                        .ok_or_else(|| format!("incomplete compound CSD member {id}"))
                })
                .collect::<Result<_, _>>()?;
            if ids.iter().any(|id| consumed.contains(id)) {
                return Err("overlapping CSD compound coverage".into());
            }
            let groups: Vec<_> = record["groups"]
                .as_array()
                .ok_or("missing language groups")?
                .iter()
                .filter(|g| {
                    g["language_origin"] == "implicit-default" && g["declared_language"].is_null()
                })
                .collect();
            let [group] = groups.as_slice() else {
                return Err("ambiguous default CSD language".into());
            };
            let branches = group["branches"].as_array().ok_or("missing CSD branches")?;
            let mut selected = None;
            for (i, branch) in branches.iter().enumerate() {
                let restrictions = branch["restrictions_raw"]
                    .as_array()
                    .ok_or("missing CSD restrictions")?;
                // Both callers render ordinary passive/item effects, never gem quality.
                // Unknown restrictions remain an error, not a reason to try another line.
                if restrictions == &[json!("gem_quality")] {
                    continue;
                }
                if !restrictions.is_empty() {
                    return Err(format!("unsupported CSD restrictions for {id}"));
                }
                let conditions = branch["condition_tokens_raw"]
                    .as_array()
                    .ok_or("missing CSD conditions")?;
                if conditions.len() != raw_values.len() {
                    return Err("CSD condition cardinality mismatch".into());
                }
                let matches = conditions
                    .iter()
                    .zip(&raw_values)
                    .map(|(c, v)| condition(c.as_str().ok_or("invalid CSD condition")?, *v))
                    .collect::<Result<Vec<_>, String>>()?;
                if matches.iter().all(|x| *x) {
                    selected = Some((i, branch));
                    break;
                }
            }
            let (branch_index, branch) =
                selected.ok_or_else(|| format!("no CSD branch for {id}"))?;
            if branch["restrictions_raw"]
                .as_array()
                .is_none_or(|a| !a.is_empty())
                || branch["template_syntax_diagnostics"]
                    .as_array()
                    .is_none_or(|a| !a.is_empty())
            {
                return Err(format!("unsupported CSD restrictions/syntax for {id}"));
            }
            let mut formatted_values: Vec<_> =
                raw_values.iter().copied().map(Number::new).collect();
            let handlers = branch["quantifiers_lexical"]
                .as_array()
                .ok_or("missing CSD handlers")?;
            let mut handler_tokens = Vec::new();
            for q in handlers {
                handler_tokens.push(q["name_raw"].clone());
                handler_tokens.extend(
                    q["argument_tokens_raw"]
                        .as_array()
                        .ok_or("invalid CSD handler args")?
                        .iter()
                        .cloned(),
                );
            }
            if branch["quantifier_tokens_raw"] != json!(handler_tokens) {
                return Err("unaccounted CSD handler tokens".into());
            }
            number::apply(&mut formatted_values, handlers)?;
            let text = template(
                branch["template_raw"]
                    .as_str()
                    .ok_or("missing CSD template")?,
                &formatted_values,
            )?;
            if text.trim().is_empty() {
                return Err("empty CSD effect description".into());
            }
            lines.extend(text.replace("\\n", "\n").lines().map(str::to_owned));
            consumed.extend(ids.iter().copied());
            evidence.push(
                json!({"pointer":pointer,"raw_slice_sha256":record["raw_slice_sha256"],
                "ids":ids,"values":raw_values,"branch":branch_index,"span":branch["span"],
                "handlers":branch["quantifiers_lexical"],
                "formatted_values":formatted_values.iter().map(|v|v.display(false)).collect::<Result<Vec<_>,_>>()?,
                "equivalent_english_descriptors":self.equivalents.get(pointer).cloned().unwrap_or_default()}),
            );
        }
        Ok((lines, evidence))
    }
}

fn condition(token: &str, value: i64) -> Result<bool, String> {
    if token == "#" {
        return Ok(true);
    }
    if let Some((min, max)) = token.split_once('|') {
        let min = if min == "#" {
            i64::MIN
        } else {
            min.parse().map_err(|_| "unsupported CSD lower limit")?
        };
        let max = if max == "#" {
            i64::MAX
        } else {
            max.parse().map_err(|_| "unsupported CSD upper limit")?
        };
        if min > max {
            return Err("inverted CSD limits".into());
        }
        Ok((min..=max).contains(&value))
    } else {
        Ok(value
            == token
                .parse::<i64>()
                .map_err(|_| "unsupported CSD condition")?)
    }
}

fn template(text: &str, values: &[Number]) -> Result<String, String> {
    let mut out = String::new();
    let mut rest = text;
    while let Some((before, after)) = rest.split_once('{') {
        if before.contains('}') {
            return Err("unmatched CSD brace".into());
        }
        out.push_str(before);
        let (token, tail) = after.split_once('}').ok_or("unclosed CSD placeholder")?;
        let (index, format) = token.split_once(':').unwrap_or((token, ""));
        let value = values
            .get(
                index
                    .parse::<usize>()
                    .map_err(|_| "invalid CSD placeholder index")?,
            )
            .ok_or("CSD placeholder out of bounds")?;
        match format {
            "" | "d" => out.push_str(&value.display(false)?),
            "+d" => out.push_str(&value.display(true)?),
            _ => return Err(format!("unsupported CSD placeholder format {format}")),
        }
        rest = tail;
    }
    if rest.contains('}') {
        return Err("unmatched CSD brace".into());
    }
    out.push_str(rest);
    Ok(out)
}

/// Equivalent owners must agree in all English semantics, including unknown fields.
/// Only source positions and other languages are excluded; every original owner
/// remains in the receipt. Different wording/conditions/handlers stay ambiguous.
fn english_semantics(record: &Value) -> Result<Value, String> {
    let mut signature = record.clone();
    let object = signature.as_object_mut().ok_or("invalid CSD descriptor")?;
    object.remove("raw_slice_sha256");
    let groups = object
        .get_mut("groups")
        .and_then(Value::as_array_mut)
        .ok_or("missing CSD groups")?;
    groups
        .retain(|g| g["language_origin"] == "implicit-default" && g["declared_language"].is_null());
    if groups.len() != 1 {
        return Err("ambiguous default CSD language".into());
    }
    strip_positions(&mut signature);
    Ok(signature)
}
fn strip_positions(value: &mut Value) {
    match value {
        Value::Object(object) => {
            for key in [
                "span",
                "language_declaration_span",
                "template_character_span",
            ] {
                object.remove(key);
            }
            for value in object.values_mut() {
                strip_positions(value);
            }
        }
        Value::Array(values) => values.iter_mut().for_each(strip_positions),
        _ => {}
    }
}
