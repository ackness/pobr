//! Branch-preserving projection to the ordinary catalog; raw units stay explicit.
use super::*;
use pobr_data::catalog::stat_descriptions::{DescriptionBranch, StatCondition, StatDescriptor};

impl Renderer<'_> {
    /// Project a complete effective descriptor. Unsupported semantics reject as a whole.
    pub fn catalog(&self, id: &str) -> Result<(StatDescriptor, Value), String> {
        let owners = self
            .descriptions
            .get(id)
            .ok_or_else(|| format!("no CSD description for {id}"))?;
        let [(record, pointer)] = owners.as_slice() else {
            return Err(format!("ambiguous CSD descriptor ownership for {id}"));
        };
        if record["kind"] != "description" {
            return Err(format!("no_description for {id} is not an effectless law"));
        }
        let ids = record["ids"]
            .as_array()
            .ok_or("missing descriptor ids")?
            .iter()
            .map(|v| v.as_str().map(str::to_owned).ok_or("invalid descriptor id"))
            .collect::<Result<Vec<_>, _>>()?;
        if ids.is_empty() {
            return Err("empty descriptor ids".into());
        }
        for member in &ids {
            if !matches!(self.descriptions.get(member).map(Vec::as_slice), Some([(_, owner)]) if owner == pointer)
            {
                return Err(format!("conflicting CSD compound ownership for {member}"));
            }
        }
        let groups = record["groups"]
            .as_array()
            .ok_or("missing CSD groups")?
            .iter()
            .filter(|g| {
                g["language_origin"] == "implicit-default" && g["declared_language"].is_null()
            })
            .collect::<Vec<_>>();
        let [group] = groups.as_slice() else {
            return Err("ambiguous default CSD language".into());
        };
        let mut branches = Vec::new();
        let mut evidence = Vec::new();
        for (index, branch) in group["branches"]
            .as_array()
            .ok_or("missing CSD branches")?
            .iter()
            .enumerate()
        {
            let restrictions = branch["restrictions_raw"]
                .as_array()
                .ok_or("missing CSD restrictions")?;
            if !restrictions.is_empty() {
                if restrictions
                    .iter()
                    .all(|r| r == "gem_quality" || r == "table_only")
                {
                    continue;
                }
                return Err(format!("unsupported CSD restrictions for {id}"));
            }
            if branch["template_syntax_diagnostics"]
                .as_array()
                .is_none_or(|d| !d.is_empty())
            {
                return Err(format!("unsupported CSD syntax for {id}"));
            }
            let conditions = branch["condition_tokens_raw"]
                .as_array()
                .ok_or("missing CSD conditions")?
                .iter()
                .map(|v| parse_condition(v.as_str().ok_or("invalid CSD condition")?))
                .collect::<Result<Vec<_>, String>>()?;
            if conditions.len() != ids.len() {
                return Err("CSD condition cardinality mismatch".into());
            }
            let handlers = branch["quantifiers_lexical"]
                .as_array()
                .ok_or("missing CSD handlers")?;
            let mut tokens = Vec::new();
            for handler in handlers {
                tokens.push(handler["name_raw"].clone());
                tokens.extend(
                    handler["argument_tokens_raw"]
                        .as_array()
                        .ok_or("invalid CSD handler args")?
                        .iter()
                        .cloned(),
                );
            }
            if branch["quantifier_tokens_raw"] != json!(tokens) {
                return Err("unaccounted CSD handler tokens".into());
            }
            // Zero here is only a symbolic coefficient probe, never a missing stat value.
            let mut numbers = ids.iter().map(|_| Number::new(0)).collect::<Vec<_>>();
            let (numeric, lookups) = split_handlers(handlers, ids.len(), &self.lookups)?;
            number::apply(&mut numbers, &numeric)?;
            validate_lookup_formats(&numbers, &lookups)?;
            let text = placeholders(
                branch["template_raw"]
                    .as_str()
                    .ok_or("missing CSD template")?,
                |i, format| {
                    if i >= ids.len() {
                        return Err("CSD placeholder out of bounds".into());
                    }
                    match format {
                        "" => Ok(format!("{{{i}}}")),
                        "d" | "+d" | "-d" => Ok(format!("{{{i}:{format}}}")),
                        _ => Err(format!("unsupported CSD placeholder format {format}")),
                    }
                },
            )?;
            let template =
                pobr_core::mod_parser::engine::strip_display_markup(&text.replace("\\n", "\n"));
            if template.trim().is_empty() {
                return Err("empty CSD effect description".into());
            }
            branches.push(DescriptionBranch {
                template,
                conditions,
                lookups: if lookups.iter().all(Option::is_none) {
                    Vec::new()
                } else {
                    lookups
                },
                formats: numbers.into_iter().map(|n| n.format).collect(),
            });
            evidence.push(json!({"branch":index,"span":branch["span"],"handlers":handlers}));
        }
        if branches.is_empty() {
            return Err("no ordinary CSD branches".into());
        }
        Ok((
            StatDescriptor {
                member_stats: ids,
                branches,
            },
            json!({"pointer":pointer,
            "raw_slice_sha256":record["raw_slice_sha256"],"branches":evidence,
            "equivalent_english_descriptors":self.equivalents.get(pointer).cloned().unwrap_or_default()}),
        ))
    }
}

fn parse_condition(token: &str) -> Result<StatCondition, String> {
    let mut out = StatCondition::default();
    if token == "#" {
        return Ok(out);
    }
    if let Some(v) = token.strip_prefix('!') {
        out.not = Some(v.parse().map_err(|_| "unsupported CSD exclusion")?);
    } else if let Some((min, max)) = token.split_once('|') {
        out.min = if min == "#" {
            None
        } else {
            Some(min.parse().map_err(|_| "unsupported CSD lower limit")?)
        };
        out.max = if max == "#" {
            None
        } else {
            Some(max.parse().map_err(|_| "unsupported CSD upper limit")?)
        };
        if out.min.zip(out.max).is_some_and(|(a, b)| a > b) {
            return Err("inverted CSD limits".into());
        }
    } else {
        let v = token.parse().map_err(|_| "unsupported CSD condition")?;
        out.min = Some(v);
        out.max = Some(v);
    }
    Ok(out)
}
