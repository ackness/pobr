//! Unique templates are selected by the same parser used for build calculation.
use std::collections::BTreeMap;

use pobr_core::item_text::parse_pob_xml_item;
use pobr_item::{ItemDraft, LineBucket, ModLineDraft, annotations::parse_mod_line};
use serde::{Deserialize, Serialize};

use super::ApiError;
use crate::state;

#[derive(Serialize)]
struct UniqueBase {
    name: String,
    item_class: String,
}

pub fn unique_catalog_json() -> Result<String, String> {
    (|| {
        let game = state::game_data().map_err(ApiError::not_initialized)?;
        let data = state::build_data().map_err(ApiError::not_initialized)?;
        let uniques = game
            .uniques()
            .map_err(|e| e.to_string())?
            .ok_or_else(|| ApiError::bad_request("missing_unique_data"))?;
        let entries: Vec<_> = uniques
            .uniques
            .iter()
            .enumerate()
            .map(|(id, unique)| {
                let bases: Vec<_> = unique
                    .raw
                    .lines()
                    .skip(1)
                    .filter_map(|line| {
                        let (_, name) = parse_mod_line(line);
                        data.base_items.get(&name).map(|base| UniqueBase {
                            name,
                            item_class: base.item_class.clone(),
                        })
                    })
                    .collect();
                serde_json::json!({ "id": id, "name": unique.name, "item_type": unique.item_type,
                "raw": unique.raw, "bases": bases })
            })
            .collect();
        serde_json::to_string(&entries).map_err(|e| ApiError::bad_request(e.to_string()))
    })()
    .map_err(ApiError::into_json)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct UniqueRequest {
    id: usize,
    #[serde(default)]
    variant: Option<u32>,
    #[serde(default)]
    alts: Vec<u32>,
    #[serde(default)]
    version: Option<u32>,
    #[serde(default)]
    groups: BTreeMap<u32, u32>,
    #[serde(default)]
    rolls: BTreeMap<usize, f64>,
}

const ALT_SUFFIXES: [&str; 5] = ["", " Two", " Three", " Four", " Five"];

pub fn unique_item_draft_json(request: &str) -> Result<String, String> {
    unique_draft(request).map_err(ApiError::into_json)
}

fn unique_draft(request: &str) -> Result<String, ApiError> {
    if request.len() > 131_072 {
        return Err(ApiError::bad_request("invalid_request"));
    }
    let req: UniqueRequest =
        serde_json::from_str(request).map_err(|_| ApiError::bad_request("invalid_request"))?;
    let game = state::game_data().map_err(ApiError::not_initialized)?;
    let data = state::build_data().map_err(ApiError::not_initialized)?;
    let uniques = game
        .uniques()
        .map_err(|e| e.to_string())?
        .ok_or_else(|| ApiError::bad_request("missing_unique_data"))?;
    let unique = uniques
        .uniques
        .get(req.id)
        .ok_or_else(|| ApiError::bad_request("unknown_unique"))?;
    let raw: Vec<_> = unique.raw.lines().map(str::trim).collect();
    let names = |key: &str| {
        raw.iter()
            .filter_map(|line| line.strip_prefix(key))
            .map(str::trim)
            .collect::<Vec<_>>()
    };
    let variants = names("Variant:");
    let versions = names("Version:");
    let selected =
        |requested: Option<u32>, key: &str, values: &[&str]| -> Result<Option<u32>, ApiError> {
            let explicit = raw.iter().find_map(|line| {
                line.strip_prefix(key)
                    .and_then(|value| value.trim().parse().ok())
            });
            let default = values
                .iter()
                .rposition(|name| *name == "Current")
                .map_or(values.len() as u32, |i| i as u32 + 1);
            let value = requested.or(explicit).unwrap_or(default);
            if values.is_empty() && requested.is_none() {
                return Ok(None);
            }
            if value == 0 || value as usize > values.len() {
                return Err(ApiError::bad_request("invalid_variant"));
            }
            Ok(Some(value))
        };
    let variant = selected(req.variant, "Selected Variant:", &variants)?;
    let version = selected(req.version, "Selected Version:", &versions)?;
    let alt_count = ALT_SUFFIXES
        .iter()
        .filter(|suffix| {
            raw.iter()
                .any(|line| *line == format!("Has Alt Variant{suffix}: true"))
        })
        .count();
    if req.alts.len() != alt_count
        || req
            .alts
            .iter()
            .any(|value| *value == 0 || *value as usize > variants.len())
    {
        return Err(ApiError::bad_request("invalid_variant"));
    }
    if req.rolls.iter().any(|(index, value)| {
        *index >= raw.len() || !value.is_finite() || !(0.0..=1.0).contains(value)
    }) {
        return Err(ApiError::bad_request("invalid_roll"));
    }
    // Catalogue index columns can precede the base or contain a variant annotation.
    // Resolve actual base-name lines against the engine's base map, never guess from prose.
    let base_lines: Vec<_> = raw
        .iter()
        .enumerate()
        .skip(1)
        .filter_map(|(index, line)| {
            let (annotations, name) = parse_mod_line(line);
            data.base_items
                .contains_key(&name)
                .then_some((index, annotations, name))
        })
        .collect();
    let base = base_lines
        .iter()
        .find(|(_, annotations, _)| {
            annotations
                .variants
                .as_ref()
                .is_none_or(|ids| variant.is_some_and(|value| ids.contains(&value)))
        })
        .ok_or_else(|| ApiError::bad_request("unique_base_unavailable"))?;
    let mut seed = vec![
        "Rarity: UNIQUE".to_string(),
        unique.name.clone(),
        base.2.clone(),
    ];
    for (index, line) in raw.iter().enumerate().skip(1) {
        if base_lines
            .iter()
            .any(|(base_index, _, _)| *base_index == index)
            || line.starts_with("Selected Variant:")
            || line.starts_with("Selected Alt Variant")
            || line.starts_with("Selected Version:")
            || line.starts_with("Selected Variant Group:")
        {
            continue;
        }
        // Only effect ranges get annotations; headers stay visible to both parsers.
        let has_range = pobr_core::apply_range::apply_range(line, 0.0, None, 1.0)
            != pobr_core::apply_range::apply_range(line, 1.0, None, 1.0);
        seed.push(if has_range {
            format!(
                "{{range:{}}}{line}",
                req.rolls.get(&index).copied().unwrap_or(0.5)
            )
        } else {
            line.to_string()
        });
    }
    if let Some(value) = variant {
        seed.push(format!("Selected Variant: {value}"));
    }
    if let Some(value) = version {
        seed.push(format!("Selected Version: {value}"));
    }
    for (i, value) in req.alts.iter().enumerate() {
        seed.push(format!("Selected Alt Variant{}: {value}", ALT_SUFFIXES[i]));
    }
    let annotation_ids = |line: &str, key: &str| -> Option<Vec<u32>> {
        line.split_once(&format!("{{{key}:"))?
            .1
            .split_once('}')
            .map(|(values, _)| {
                values
                    .split(',')
                    .filter_map(|value| value.parse().ok())
                    .collect()
            })
    };
    let mut used = std::collections::BTreeSet::new();
    for (group, value) in &req.groups {
        let valid = raw.iter().any(|line| {
            annotation_ids(line, "group").is_some_and(|ids| ids.contains(group))
                && annotation_ids(line, "variant").is_some_and(|ids| ids.contains(value))
                && annotation_ids(line, "version")
                    .is_none_or(|ids| version.is_some_and(|v| ids.contains(&v)))
        });
        if !valid || !used.insert(*value) {
            return Err(ApiError::bad_request("invalid_variant"));
        }
        seed.push(format!("Selected Variant Group: {group}={value}"));
    }
    let seed = seed.join("\n");
    let item = parse_pob_xml_item(&seed).map_err(|_| ApiError::bad_request("invalid_item"))?;
    let mut draft = ItemDraft::parse(&seed).map_err(|_| ApiError::bad_request("invalid_item"))?;
    // New catalogue items freeze the chosen version and roll values. Other versions
    // remain in the catalogue, not as inactive effects in the saved item's display.
    draft.lines = [
        (LineBucket::Implicit, item.implicit_texts),
        (LineBucket::Enchant, item.enchant_texts),
        (LineBucket::Explicit, item.modifier_texts),
    ]
    .into_iter()
    .flat_map(|(bucket, lines)| {
        lines.into_iter().map(move |text| ModLineDraft {
            text,
            annotations: Default::default(),
            bucket,
        })
    })
    .collect();
    if let Some(value) = version {
        draft
            .metadata_lines
            .push(format!("Note: {}", versions[value as usize - 1]));
    }
    if !req.groups.is_empty() {
        let mut choices = req.groups.values().copied();
        draft.variant.selected = choices.next();
        draft.variant.alts = [None; 5];
        for (slot, value) in draft.variant.alts.iter_mut().zip(choices) {
            *slot = Some(value);
        }
    }
    draft.implicit_count = None;
    serde_json::to_string(&serde_json::json!({ "text": draft.build_raw(), "base": base.2, "required_level": draft.header.level_req }))
        .map_err(|e| ApiError::bad_request(e.to_string()))
}
