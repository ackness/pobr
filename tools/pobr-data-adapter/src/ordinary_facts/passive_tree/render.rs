//! Bounded CSD rendering to the existing ordinary English-text channel.
//!
//! This does not interpret game mechanics. Unknown syntax/handlers reject the
//! entire node, so callers can require an exact, declared compatibility binding.

mod catalog;
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

pub(in crate::ordinary_facts) type LookupValues = BTreeMap<String, BTreeMap<i64, String>>;

pub(in crate::ordinary_facts) struct Renderer<'a> {
    lookups: LookupValues,
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

    /// Attach already source-verified relational formatter names.
    pub fn with_lookup_values(mut self, values: &LookupValues) -> Self {
        self.lookups = values.clone();
        self
    }

    /// Coalesce owners only for consumers that discard display markup.
    /// All other English fields, including unknown fields, must agree exactly.
    pub fn with_plain_display_equivalence(mut self) -> Result<Self, String> {
        for records in self.descriptions.values_mut() {
            if records.len() < 2 || records.iter().any(|(r, _)| r["kind"] != "description") {
                continue;
            }
            let signature = plain_display_semantics(records[0].0)?;
            let same = records
                .iter()
                .skip(1)
                .map(|(record, _)| plain_display_semantics(record))
                .collect::<Result<Vec<_>, _>>()?
                .iter()
                .all(|other| *other == signature);
            if same {
                self.equivalents.insert(
                    records[0].1.clone(),
                    records
                        .iter()
                        .map(|(r, p)| {
                            json!({
                                "pointer":p,"raw_slice_sha256":r["raw_slice_sha256"],
                                "equivalence":"ordinary-plain-display/v1"
                            })
                        })
                        .collect(),
                );
                records.truncate(1);
            }
        }
        Ok(self)
    }

    /// Resolve an explicit CSD scope and its literal include closure, child first.
    pub fn scope(document: &'a Value, path: &str) -> Result<Self, String> {
        fn visit(
            document: &Value,
            path: &str,
            active: &mut BTreeSet<String>,
            paths: &mut Vec<String>,
        ) -> Result<(), String> {
            if !active.insert(path.into()) {
                return Err(format!("cyclic CSD include {path}"));
            }
            if paths.iter().any(|p| p == path) {
                active.remove(path);
                return Ok(());
            }
            paths.push(path.into());
            let records = document["files"][path]["records"]
                .as_array()
                .ok_or_else(|| format!("missing CSD scope {path}"))?;
            for record in records.iter().filter(|r| r["kind"] == "include") {
                visit(
                    document,
                    record["target"].as_str().ok_or("invalid CSD include")?,
                    active,
                    paths,
                )?;
            }
            active.remove(path);
            Ok(())
        }
        let mut paths = Vec::new();
        visit(document, path, &mut BTreeSet::new(), &mut paths)?;
        Self::scoped(
            document,
            &paths.iter().map(String::as_str).collect::<Vec<_>>(),
        )
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
            if includes
                .iter()
                .any(|p| p.is_none_or(|p| !scopes.contains(&p)))
            {
                return Err(format!("incomplete CSD include closure for {path}"));
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
            lookups: LookupValues::new(),
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
                if restrictions
                    .iter()
                    .all(|r| r == "gem_quality" || r == "table_only")
                    && !restrictions.is_empty()
                {
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
            let (numeric, lookups) = split_handlers(handlers, raw_values.len(), &self.lookups)?;
            number::apply(&mut formatted_values, &numeric)?;
            validate_lookup_formats(&formatted_values, &lookups)?;
            let source_template = branch["template_raw"]
                .as_str()
                .ok_or("missing CSD template")?;
            let text = if lookups.iter().all(Option::is_none) {
                template(source_template, &formatted_values)?
            } else {
                placeholders(source_template, |index, format| {
                    let number = formatted_values
                        .get(index)
                        .ok_or("CSD placeholder out of bounds")?;
                    if let Some(handler) = lookups.get(index).and_then(Option::as_ref) {
                        if !format.is_empty() {
                            return Err("numeric format on CSD lookup literal".into());
                        }
                        self.lookups[handler]
                            .get(&raw_values[index])
                            .filter(|s| !s.trim().is_empty())
                            .cloned()
                            .ok_or_else(|| {
                                format!("unknown CSD lookup key {handler}/{}", raw_values[index])
                            })
                    } else {
                        match format {
                            "" | "d" | "-d" => number.display(false),
                            "+d" => number.display(true),
                            _ => Err(format!("unsupported CSD placeholder format {format}")),
                        }
                    }
                })?
            };
            if text.trim().is_empty() {
                return Err("empty CSD effect description".into());
            }
            lines.extend(text.replace("\\n", "\n").lines().map(str::to_owned));
            consumed.extend(ids.iter().copied());
            evidence.push(
                json!({"pointer":pointer,"raw_slice_sha256":record["raw_slice_sha256"],
                "ids":ids,"values":raw_values,"branch":branch_index,"span":branch["span"],
                "handlers":branch["quantifiers_lexical"],"lookups":lookups,
                "formatted_values":formatted_values.iter().map(|v|v.display(false)).collect::<Result<Vec<_>,_>>()?,
                "equivalent_english_descriptors":self.equivalents.get(pointer).cloned().unwrap_or_default()}),
            );
        }
        Ok((lines, evidence))
    }
}

fn split_handlers(
    handlers: &[Value],
    count: usize,
    values: &LookupValues,
) -> Result<(Vec<Value>, Vec<Option<String>>), String> {
    let mut numeric = Vec::new();
    let mut lookups = vec![None; count];
    for handler in handlers {
        let name = handler["name_raw"]
            .as_str()
            .ok_or("invalid CSD handler name")?;
        if matches!(
            name,
            "passive_hash"
                | "passive_keystone_index"
                | "specific_skill"
                | "mod_value_to_item_class"
                | "ultimatum_wager_type_hash"
                | "tree_expansion_jewel_passive"
                | "mages_legacy_index"
        ) {
            if !values.contains_key(name) {
                numeric.push(handler.clone());
                continue;
            }
            let args = handler["argument_tokens_raw"]
                .as_array()
                .ok_or("invalid CSD handler args")?;
            let [arg] = args.as_slice() else {
                return Err("unsupported CSD handler arity".into());
            };
            let index = arg
                .as_str()
                .and_then(|s| s.parse::<usize>().ok())
                .and_then(|i| i.checked_sub(1))
                .ok_or("invalid CSD lookup index")?;
            let slot = lookups
                .get_mut(index)
                .ok_or("CSD handler index out of bounds")?;
            if slot.is_some() {
                return Err("multiple CSD lookups for one member".into());
            }
            *slot = Some(name.into());
        } else {
            numeric.push(handler.clone());
        }
    }
    Ok((numeric, lookups))
}
fn validate_lookup_formats(numbers: &[Number], lookups: &[Option<String>]) -> Result<(), String> {
    if numbers
        .iter()
        .zip(lookups)
        .any(|(n, l)| l.is_some() && n.format != Default::default())
    {
        return Err("mixed numeric/relational CSD formatting".into());
    }
    Ok(())
}

fn condition(token: &str, value: i64) -> Result<bool, String> {
    if let Some(excluded) = token.strip_prefix('!') {
        return Ok(value
            != excluded
                .parse::<i64>()
                .map_err(|_| "unsupported CSD exclusion")?);
    }
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
    placeholders(text, |index, format| {
        let value = values.get(index).ok_or("CSD placeholder out of bounds")?;
        match format {
            "" | "d" | "-d" => value.display(false),
            "+d" => value.display(true),
            _ => Err(format!("unsupported CSD placeholder format {format}")),
        }
    })
}

/// Representative catalog text uses one as its capture marker in display units.
pub(in crate::ordinary_facts) fn representative(text: &str, value: i64) -> Result<String, String> {
    placeholders(text, |_, format| Number::new(value).display(format == "+d"))
}

/// Format source ClientStrings using explicit raw arguments; substitutions are
/// never reparsed as template syntax. Escaped braces follow the source syntax.
pub(in crate::ordinary_facts) fn format_template(
    text: &str,
    args: &[&str],
) -> Result<String, String> {
    placeholders(text, |index, format| {
        let value = args.get(index).ok_or("template argument out of bounds")?;
        match format {
            "" => Ok((*value).into()),
            "d" | "+d" | "-d" => {
                let number = value
                    .parse::<i64>()
                    .map_err(|_| "integer template argument required")?;
                Number::new(number).display(format == "+d")
            }
            _ => Err(format!("unsupported source placeholder format {format}")),
        }
    })
}

fn placeholders(
    text: &str,
    mut replace: impl FnMut(usize, &str) -> Result<String, String>,
) -> Result<String, String> {
    let mut out = String::new();
    let mut rest = text;
    let mut next_index = 0;
    while !rest.is_empty() {
        if let Some(tail) = rest.strip_prefix("{{") {
            out.push('{');
            rest = tail;
            continue;
        }
        if let Some(tail) = rest.strip_prefix("}}") {
            out.push('}');
            rest = tail;
            continue;
        }
        if let Some(after) = rest.strip_prefix('{') {
            let (token, tail) = after.split_once('}').ok_or("unclosed CSD placeholder")?;
            let (index, format) = token.split_once(':').unwrap_or((token, ""));
            let index = if index.is_empty() {
                next_index
            } else {
                index
                    .parse::<usize>()
                    .map_err(|_| "invalid CSD placeholder index")?
            };
            next_index = index
                .checked_add(1)
                .ok_or("CSD placeholder index overflow")?;
            out.push_str(&replace(index, format)?);
            rest = tail;
        } else {
            let ch = rest.chars().next().ok_or("empty template suffix")?;
            if ch == '}' {
                return Err("unmatched CSD brace".into());
            }
            out.push(ch);
            rest = &rest[ch.len_utf8()..];
        }
    }
    Ok(out.replace("%%", "%"))
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
fn plain_display_semantics(record: &Value) -> Result<Value, String> {
    let mut signature = english_semantics(record)?;
    let branches = signature["groups"][0]["branches"]
        .as_array_mut()
        .ok_or("missing CSD branches")?;
    for branch in branches {
        let template = branch["template_raw"]
            .as_str()
            .ok_or("missing CSD template")?;
        branch["template_raw"] = json!(pobr_core::mod_parser::engine::strip_display_markup(
            template
        ));
    }
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
