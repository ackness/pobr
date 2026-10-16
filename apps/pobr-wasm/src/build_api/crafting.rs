//! Game metadata for manual crafting, and the shared lossless item-draft writer.
use pobr_data::catalog::{ModDef, SpawnWeight};
use pobr_item::ItemDraft;
use serde::Serialize;

use super::ApiError;
use crate::state;

#[derive(Serialize)]
struct CraftBase {
    id: String,
    name: String,
    item_class: String,
    tags: Vec<String>,
    mod_domain: u32,
}

#[derive(Serialize)]
struct CraftMod {
    id: String,
    domain: u32,
    kind: &'static str,
    level: u32,
    group: String,
    spawn_weights: Vec<SpawnWeight>,
}

impl CraftMod {
    fn from_game(value: ModDef) -> Option<Self> {
        Some(Self {
            kind: match value.generation_type? {
                1 => "prefix",
                2 => "suffix",
                _ => return None,
            },
            id: value.id,
            domain: value.domain,
            level: value.level,
            group: value.group?,
            spawn_weights: value.spawn_weights,
        })
    }
}

pub fn item_crafting_catalog_json() -> Result<String, String> {
    (|| {
        let game = state::game_data().map_err(ApiError::not_initialized)?;
        // The shared loader supplies inherited class tags absent from leaf base records.
        let bases: Vec<_> = game
            .base_items()
            .map_err(|e| e.to_string())?
            .into_iter()
            .map(|base| CraftBase {
                id: base.id,
                name: base.name,
                item_class: base.item_class,
                tags: base.tags,
                mod_domain: base.mod_domain,
            })
            .collect();
        let mods: Vec<_> = game
            .mods()
            .map_err(|e| e.to_string())?
            .into_iter()
            .filter_map(CraftMod::from_game)
            .collect();
        serde_json::to_string(&serde_json::json!({ "bases": bases, "mods": mods }))
            .map_err(|e| ApiError::bad_request(e.to_string()))
    })()
    .map_err(ApiError::into_json)
}

/// Canonical text for preview, library storage and export uses the same draft writer.
pub fn normalize_item_draft_json(text: &str) -> Result<String, String> {
    (|| {
        if text.len() > 131_072 {
            return Err(ApiError::bad_request("item_too_large"));
        }
        let draft = ItemDraft::parse(text).map_err(|_| ApiError::bad_request("invalid_item"))?;
        let data = state::build_data().map_err(ApiError::not_initialized)?;
        if !data.base_items.contains_key(&draft.header.base_name)
            || !matches!(
                draft.header.rarity.as_str(),
                "NORMAL" | "MAGIC" | "RARE" | "UNIQUE" | "RELIC"
            )
            || draft
                .header
                .item_level
                .is_some_and(|level| !(1..=100).contains(&level))
            || draft.header.quality.is_some_and(|quality| quality > 100)
        {
            return Err(ApiError::bad_request("invalid_item"));
        }
        serde_json::to_string(&serde_json::json!({ "text": draft.build_raw() }))
            .map_err(|e| ApiError::bad_request(e.to_string()))
    })()
    .map_err(ApiError::into_json)
}
